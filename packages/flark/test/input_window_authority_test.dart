import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  Future<FlarkEditorController> open(String source) async {
    final controller = await FlarkEditorController.open(
      source,
      libraryPath: libraryPath!,
    );
    await controller.continueParsing();
    return controller;
  }

  Future<void> settle(FlarkEditorController controller) async {
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    while ((controller.pendingEdits != 0 ||
            controller.viewport?.revision != controller.revision) &&
        DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
    expect(controller.lastError, isNull);
  }

  TextEditingDeltaInsertion insertion(
    TextEditingValue value,
    int offset,
    String text,
  ) => TextEditingDeltaInsertion(
    oldText: value.text,
    textInserted: text,
    insertionOffset: offset,
    selection: TextSelection.collapsed(offset: offset + text.length),
    composing: TextRange.empty,
  );

  Future<void> typePlatformText(
    FlarkEditorController controller,
    String text,
  ) async {
    for (final rune in text.runes) {
      final before = controller.inputValue;
      controller.applyDeltas([
        insertion(
          before,
          before.selection.extentOffset,
          String.fromCharCode(rune),
        ),
      ]);
      await Future<void>.delayed(const Duration(milliseconds: 35));
    }
  }

  test(
    'an attached window carries a truthful serialized shadow',
    () async {
      final controller = await open('# Flark\n\nA quick paragraph.\n');
      addTearDown(controller.close);

      expect(controller.inputWindowState, FlarkInputWindowState.synchronized);
      final shadow = controller.inputWindowShadow;
      expect(shadow.connectionEpoch, greaterThan(0));
      expect(shadow.windowEpoch, greaterThanOrEqualTo(1));
      expect(
        shadow.windowTextSha256,
        flarkWindowTextSha256(controller.inputValue.text),
      );

      // A valid platform batch stays on the connection and advances the
      // window epoch and text identity.
      final before = controller.inputWindowShadow;
      controller.applyDeltas([insertion(controller.inputValue, 0, 'Live ')]);
      expect(controller.inputValue.text, startsWith('Live '));
      final after = controller.inputWindowShadow;
      expect(after.connectionEpoch, before.connectionEpoch);
      expect(after.windowEpoch, before.windowEpoch + 1);
      expect(
        after.windowTextSha256,
        flarkWindowTextSha256(controller.inputValue.text),
      );
      expect(controller.resyncCount, 0);
      await settle(controller);
    },
    skip: libraryPath == null,
  );

  test(
    'a broken delta chain applies nothing and retires the connection',
    () async {
      final controller = await open('# Flark\n\nA quick paragraph.\n');
      addTearDown(controller.close);
      await settle(controller);

      final value = controller.inputValue;
      final revisionBefore = controller.revision;
      final textBefore = value.text;
      final epochBefore = controller.connectionEpoch;

      final first = insertion(value, 0, 'A');
      // The second delta lies about its old text, so the batch chain breaks.
      final second = TextEditingDeltaInsertion(
        oldText: textBefore,
        textInserted: 'B',
        insertionOffset: 0,
        selection: const TextSelection.collapsed(offset: 1),
        composing: TextRange.empty,
      );
      controller.applyDeltas([first, second]);

      expect(controller.inputValue.text, textBefore);
      expect(controller.pendingEdits, 0);
      expect(controller.revision, revisionBefore);
      expect(
        controller.lastResyncReason,
        FlarkInputResyncReason.deltaChainMismatch,
      );
      expect(controller.resyncCount, 1);
      expect(controller.connectionEpoch, isNot(epochBefore));
      expect(controller.windowEpoch, 1);
      expect(controller.inputWindowState, FlarkInputWindowState.synchronized);
    },
    skip: libraryPath == null,
  );

  test(
    'an accepted multi-delta callback commits exactly one revision',
    () async {
      final controller = await open('# Flark\n\nA quick paragraph.\n');
      addTearDown(controller.close);
      await settle(controller);

      final before = controller.inputValue;
      final revisionBefore = controller.revision;
      final first = insertion(before, 0, 'A');
      final afterFirst = first.apply(before);
      final second = insertion(afterFirst, 1, 'B');

      controller.applyDeltas([first, second]);

      expect(controller.inputValue.text, startsWith('AB'));
      expect(controller.pendingEdits, 1);
      await settle(controller);
      expect(controller.revision, revisionBefore + 1);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.inputValue.text, before.text);
    },
    skip: libraryPath == null,
  );

  test(
    'typing behind semantic Return reconciles provisional coordinates in FIFO order',
    () async {
      final body = List<String>.filled(4000, 'a').join();
      final controller = await open('9) $body\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.end);
      final revisionBefore = controller.revision;
      final before = controller.inputValue;
      final newline = insertion(before, before.selection.extentOffset, '\n');
      final provisionalAfter = newline.apply(before);
      final successor = insertion(
        provisionalAfter,
        provisionalAfter.selection.extentOffset,
        'x',
      );

      controller.applyDeltas([newline]);
      controller.applyDeltas([successor]);

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(controller.semanticSuccessorHighWatermark, 1);
      expect(controller.lastSemanticReconciliationMicros, lessThan(16000));
      expect(controller.revision, revisionBefore + 2);
      expect(controller.visibleSource, '9) $body\n10) x\n');
      expect(controller.globalCaretOffset, 4009);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, '9) $body\n10) \n');
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, '9) $body\n');
    },
    skip: libraryPath == null,
  );

  test(
    'repeated Return then typing keeps one truthful projected window',
    () async {
      final controller = await open(
        'A quick paragraph with **bold text**.\n\nTrailing paragraph.\n',
      );
      addTearDown(controller.close);
      final target = controller.rows.first;
      controller.activateRow(target, target.editableUtf16!.end);
      final frames = <({int burst, String presentation})>[];
      var activeBurst = 0;
      void captureFrame() {
        frames.add((
          burst: activeBurst,
          presentation: controller.rows
              .map((row) {
                final surface = controller.surfaceRow(row);
                return '${surface.leadingText}${surface.text}';
              })
              .join('\n'),
        ));
      }

      controller.addListener(captureFrame);
      addTearDown(() => controller.removeListener(captureFrame));

      for (var index = 0; index < 40; index += 1) {
        activeBurst = index + 1;
        final before = controller.inputValue;
        final offset = before.selection.extentOffset;
        final newline = insertion(before, offset, '\n');
        final provisional = newline.apply(before);
        final successor = insertion(
          provisional,
          provisional.selection.extentOffset,
          'x',
        );

        controller.applyDeltas([newline]);
        expect(
          controller.resyncCount,
          0,
          reason: 'newline dispatch in burst ${index + 1}',
        );
        controller.observePlatformNewlineAction();
        controller.applyDeltas([successor]);
        expect(
          controller.resyncCount,
          0,
          reason: 'successor capture in burst ${index + 1}',
        );
        await settle(controller);

        expect(
          controller.resyncCount,
          0,
          reason:
              'burst ${index + 1}: ${controller.lastResyncReason.name}; '
              'input=${controller.inputValue.text}/'
              '${controller.inputValue.selection}',
        );
        expect(
          controller.inputWindowShadow.windowTextSha256,
          flarkWindowTextSha256(controller.inputValue.text),
        );
        expect(
          controller.rows,
          isNotEmpty,
          reason: 'burst ${index + 1} dropped the rendered row surface',
        );
        expect(
          controller.rows
              .map((row) => controller.surfaceRow(row).text)
              .join('\n'),
          isNot(contains('**')),
          reason: 'burst ${index + 1} exposed predecessor source markers',
        );
      }
      expect(
        frames.where((frame) => frame.presentation.contains('**')),
        isEmpty,
        reason:
            'a parser-proved structural burst exposed source markers: '
            '${frames.where((frame) => frame.presentation.contains('**')).map((frame) => frame.burst).toList()}',
      );
      expect(
        frames.where((frame) => frame.presentation.isEmpty),
        isEmpty,
        reason: 'the rendered surface disappeared during a structural burst',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'action-only Return keeps rapid successors across input-window refresh timing',
    () async {
      const source = '''# Flark dogfood

This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

## Start here
''';
      const expected = '''# Flark dogfood

This is the real **Rust → Dart → Flutter** editor parapid

secondth. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

## Start here
''';

      for (final successorDelay in <Duration>[
        Duration.zero,
        const Duration(milliseconds: 100),
      ]) {
        final controller = await open(source);
        addTearDown(controller.close);
        final paragraph = controller.rows.firstWhere((row) => row.kind == 5);
        controller.activateRow(paragraph, source.indexOf('th. Use'));
        await typePlatformText(controller, 'rapid');
        await settle(controller);

        controller.observePlatformNewlineAction();
        if (successorDelay > Duration.zero) {
          await Future<void>.delayed(successorDelay);
        }
        await typePlatformText(controller, 'second');
        await settle(controller);

        expect(
          controller.visibleSource,
          startsWith(expected.trimRight()),
          reason: '${successorDelay.inMilliseconds}ms successor delay',
        );
        expect(controller.sourceUtf16Length, expected.length);
        expect(controller.globalCaretOffset, 82);
        expect(controller.resyncCount, 0);
      }
    },
    skip: libraryPath == null,
  );

  test(
    'selector Backspace waits behind semantic Return without losing either command',
    () async {
      final controller = await open('9) alpha\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.end);
      final before = controller.inputValue;
      final revisionBefore = controller.revision;

      controller.applyDeltas([
        insertion(before, before.selection.extentOffset, '\n'),
      ]);
      controller.deleteBackward();

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(controller.semanticSuccessorHighWatermark, 1);
      expect(controller.revision, revisionBefore + 2);
      expect(controller.visibleSource, '9) alpha\n\n\n');
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, '9) alpha\n10) \n');
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, '9) alpha\n');
    },
    skip: libraryPath == null,
  );

  test(
    'typing behind semantic Backspace maps exactly until certification',
    () async {
      final controller = await open('Before **bold**.\n\nAfter.\n');
      addTearDown(controller.close);
      final after = controller.rows.last;
      controller.activateRow(after, after.editableUtf16!.start);

      controller.deleteBackward();
      final beforeTyping = controller.inputValue;
      controller.applyDeltas([insertion(beforeTyping, 0, 'x')]);

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(controller.semanticSuccessorHighWatermark, 1);
      expect(controller.visibleSource, 'Before **bold**.xAfter.\n');
      expect(controller.globalCaretOffset, 'Before **bold**.x'.length);
      final pendingSurfaces = controller.rows
          .map((row) => controller.surfaceRow(row))
          .toList(growable: false);
      expect(
        pendingSurfaces.any(
          (surface) => surface.kind == 0 && surface.text.contains('**'),
        ),
        isTrue,
        reason: 'the affected structural range did not fail closed',
      );
      await controller.continueParsing();
      expect(controller.semanticsCurrent, isTrue);
      expect(
        controller.rows
            .map((row) => controller.surfaceRow(row).text)
            .join('\n'),
        isNot(contains('**')),
        reason: 'fresh parser certification did not restore projection',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'deferred Backspace falls back to a literal grapheme after semantic miss',
    () async {
      final controller = await open('Before.\n\nAfter.\n');
      addTearDown(controller.close);
      final after = controller.rows.last;
      controller.activateRow(after, after.editableUtf16!.start);

      controller.deleteBackward();
      controller.deleteBackward();

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(
        controller.visibleSource,
        'BeforeAfter.\n',
        reason:
            'revision=${controller.revision} pending=${controller.pendingEdits} '
            'status=${controller.status} error=${controller.lastError}',
      );
      expect(controller.revision, 3);
      expect(controller.semanticSuccessorHighWatermark, 1);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, 'Before.After.\n');
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.visibleSource, 'Before.\n\nAfter.\n');
    },
    skip: libraryPath == null,
  );

  test(
    'typing behind handled-no-change Backspace keeps the current window',
    () async {
      final controller = await open('alpha\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.start);

      controller.deleteBackward();
      final beforeTyping = controller.inputValue;
      controller.applyDeltas([insertion(beforeTyping, 0, 'x')]);

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(controller.semanticSuccessorHighWatermark, 1);
      expect(controller.visibleSource, 'xalpha\n');
      expect(controller.globalCaretOffset, 1);
    },
    skip: libraryPath == null,
  );

  test(
    'full-value successor reconciles behind semantic Return',
    () async {
      final controller = await open('9) alpha\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.end);
      final before = controller.inputValue;
      final newline = insertion(before, before.selection.extentOffset, '\n');
      final provisional = newline.apply(before);

      controller.updateEditingValue(provisional);
      controller.updateEditingValue(
        provisional.copyWith(
          text:
              '${provisional.text.substring(0, provisional.selection.extentOffset)}x${provisional.text.substring(provisional.selection.extentOffset)}',
          selection: TextSelection.collapsed(
            offset: provisional.selection.extentOffset + 1,
          ),
        ),
      );
      controller.observePlatformNewlineAction();

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(controller.visibleSource, '9) alpha\n10) x\n');
      expect(controller.semanticSuccessorHighWatermark, 1);
    },
    skip: libraryPath == null,
  );

  test(
    'typing behind empty-list exit maps across distinct structural splices',
    () async {
      final controller = await open('- one\n- \n');
      addTearDown(controller.close);
      final empty = controller.rows.last;
      controller.activateRow(empty, empty.editableUtf16!.end);
      final before = controller.inputValue;
      final newline = insertion(before, before.selection.extentOffset, '\n');
      final provisional = newline.apply(before);

      controller.applyDeltas([newline]);
      controller.observePlatformNewlineAction();
      controller.applyDeltas([
        insertion(provisional, provisional.selection.extentOffset, 'plain'),
      ]);

      expect(controller.resyncCount, 0);
      await settle(controller);
      expect(
        controller.resyncCount,
        0,
        reason: controller.lastResyncReason.name,
      );
      expect(controller.visibleSource, '- one\n\nplain\n');
      expect(controller.globalCaretOffset, 12);
      expect(controller.semanticSuccessorHighWatermark, 1);
    },
    skip: libraryPath == null,
  );

  test(
    'semantic successor queue fails closed at its declared cap',
    () async {
      final controller = await open('9) alpha\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.end);
      final before = controller.inputValue;
      final newline = insertion(before, before.selection.extentOffset, '\n');
      var provisional = newline.apply(before);

      controller.applyDeltas([newline]);
      for (var index = 0; index < 8; index += 1) {
        final successor = insertion(
          provisional,
          provisional.selection.extentOffset,
          'x',
        );
        controller.applyDeltas([successor]);
        provisional = successor.apply(provisional);
      }

      expect(controller.resyncCount, 1);
      expect(
        controller.lastResyncReason,
        FlarkInputResyncReason.successorQueueOverflow,
      );
      expect(controller.semanticSuccessorHighWatermark, 7);
      await settle(controller);
      expect(controller.visibleSource, '9) alpha\n10) \n');
    },
    skip: libraryPath == null,
  );

  test(
    'a stale first delta resynchronizes without mutation',
    () async {
      final controller = await open('# Flark\n\nA quick paragraph.\n');
      addTearDown(controller.close);
      await settle(controller);

      final textBefore = controller.inputValue.text;
      final stale = TextEditingDeltaInsertion(
        oldText: 'not the window text',
        textInserted: 'X',
        insertionOffset: 0,
        selection: const TextSelection.collapsed(offset: 1),
        composing: TextRange.empty,
      );
      controller.applyDeltas([stale]);

      expect(controller.inputValue.text, textBefore);
      expect(controller.pendingEdits, 0);
      expect(
        controller.lastResyncReason,
        FlarkInputResyncReason.oldTextMismatch,
      );
      expect(controller.windowEpoch, 1);
    },
    skip: libraryPath == null,
  );

  test(
    'a rejected composing callback commits the accepted prefix and unpins parsing',
    () async {
      final controller = await open('base\n');
      addTearDown(controller.close);
      await settle(controller);

      controller.updateEditingValue(
        const TextEditingValue(
          text: 'kbase\n',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 1),
        ),
      );
      await settle(controller);
      expect(
        controller.inputValue.composing,
        const TextRange(start: 0, end: 1),
      );

      controller.applyDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'stale composing window',
          textInserted: 'x',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 0, end: 2),
        ),
      ]);

      expect(controller.visibleSource, 'kbase\n');
      expect(controller.inputValue.composing, TextRange.empty);
      expect(
        controller.lastResyncReason,
        FlarkInputResyncReason.oldTextMismatch,
      );
      await settle(controller);
      expect(controller.viewport?.revision, controller.revision);
      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, 'base\n');
    },
    skip: libraryPath == null,
  );

  test(
    'an out-of-window range resynchronizes without mutation',
    () async {
      final controller = await open('# Flark\n\nA quick paragraph.\n');
      addTearDown(controller.close);
      await settle(controller);

      final value = controller.inputValue;
      final bad = TextEditingDeltaDeletion(
        oldText: value.text,
        deletedRange: TextRange(start: 0, end: value.text.length + 4),
        selection: const TextSelection.collapsed(offset: 0),
        composing: TextRange.empty,
      );
      controller.applyDeltas([bad]);

      expect(controller.inputValue.text, value.text);
      expect(
        controller.lastResyncReason,
        FlarkInputResyncReason.rangeOutOfWindow,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'host-originated exposure changes retire the connection',
    () async {
      final controller = await open('# First\n\nSecond paragraph here.\n');
      addTearDown(controller.close);
      await settle(controller);

      final epochBefore = controller.connectionEpoch;
      controller.activateRow(controller.rows.last, 0);
      expect(controller.connectionEpoch, isNot(epochBefore));
      expect(controller.windowEpoch, 1);
      expect(controller.inputWindowState, FlarkInputWindowState.synchronized);

      // Platform full-value fallback stays on the new connection.
      final connection = controller.connectionEpoch;
      final windowEpoch = controller.windowEpoch;
      final value = controller.inputValue;
      controller.updateEditingValue(
        TextEditingValue(
          text: 'Z${value.text}',
          selection: const TextSelection.collapsed(offset: 1),
        ),
      );
      expect(controller.connectionEpoch, connection);
      expect(controller.windowEpoch, greaterThan(windowEpoch));
      await settle(controller);
    },
    skip: libraryPath == null,
  );

  test(
    'resynchronizations mint strictly increasing connection epochs',
    () async {
      final controller = await open('# Flark\n\nBody.\n');
      addTearDown(controller.close);
      await settle(controller);

      final epochs = <int>[controller.connectionEpoch];
      for (var round = 0; round < 3; round += 1) {
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: 'wrong window text $round',
            textInserted: 'X',
            insertionOffset: 0,
            selection: const TextSelection.collapsed(offset: 1),
            composing: TextRange.empty,
          ),
        ]);
        epochs.add(controller.connectionEpoch);
      }
      expect(controller.resyncCount, 3);
      for (var index = 1; index < epochs.length; index += 1) {
        expect(epochs[index], greaterThan(epochs[index - 1]));
      }
    },
    skip: libraryPath == null,
  );

  test(
    'an oversized selection is anchored canonically and replaced atomically',
    () async {
      final padding = List<String>.filled(70, 'stable').join(' ');
      final source = List<String>.generate(
        64,
        (index) =>
            'Paragraph ${index.toString().padLeft(3, '0')} $padding.\n\n',
      ).join();
      expect(source.length, greaterThan(20 * 1024));
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      final lengthBefore = controller.sourceUtf16Length;
      const start = 10;
      const end = 20000;
      final generation = await controller.selectOversizedRangeUtf16(start, end);
      expect(controller.hasOversizedSelection, isTrue);
      expect(controller.canonicalSelectionGeneration, generation);
      expect(controller.inputValue.selection.isCollapsed, isTrue);
      expect(controller.inputValue.text.length, lessThanOrEqualTo(16 * 1024));

      final revisionBefore = controller.revision;
      controller.replaceSelection('X');
      final deadline = DateTime.now().add(const Duration(seconds: 10));
      while ((controller.revision == revisionBefore ||
              controller.pendingEdits != 0) &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }
      expect(controller.lastError, isNull);
      expect(controller.hasOversizedSelection, isFalse);
      expect(controller.sourceUtf16Length, lengthBefore - (end - start) + 1);
      expect(controller.globalCaretOffset, start + 1);

      final undone = await controller.undo();
      expect(undone, isTrue);
      expect(controller.sourceUtf16Length, lengthBefore);
      await settle(controller);
    },
    skip: libraryPath == null,
  );

  test(
    'a surrogate selection echo cannot retarget an oversized selection',
    () async {
      const source = 'First **bold**.\n\nSecond line.\n';
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      await controller.selectOversizedRangeUtf16(0, source.length);
      final surrogate = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaNonTextUpdate(
          oldText: surrogate.text,
          selection: surrogate.selection,
          composing: TextRange.empty,
        ),
      ]);

      final exact = await controller.resolveCanonicalSelection();
      expect(exact, isNotNull);
      expect(exact!.base, 0);
      expect(exact.extent, source.length);
      expect(controller.globalSelectionBase, 0);
      expect(controller.globalSelectionExtent, source.length);

      controller.applyDeltas([
        insertion(
          controller.inputValue,
          controller.inputValue.selection.extentOffset,
          'X',
        ),
      ]);
      await settle(controller);
      expect(await controller.readSource(), 'X');
      expect(controller.resyncCount, 0);

      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(await controller.readSource(), source);
      expect(controller.globalSelectionBase, 0);
      expect(controller.globalSelectionExtent, source.length);
    },
    skip: libraryPath == null,
  );

  test(
    'a full-value mutation replaces an oversized selection once',
    () async {
      const source = 'First **bold**.\n\nSecond line.\n';
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      await controller.selectOversizedRangeUtf16(0, source.length);
      controller.updateEditingValue(
        const TextEditingValue(
          text: 'Y',
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      await settle(controller);

      expect(await controller.readSource(), 'Y');
      expect(controller.revision, 2);
      expect(controller.resyncCount, 0);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(await controller.readSource(), source);
      expect(controller.globalSelectionBase, 0);
      expect(controller.globalSelectionExtent, source.length);
    },
    skip: libraryPath == null,
  );

  test(
    'an oversized replacement window never starts inside a Unicode scalar',
    () async {
      final source = List<String>.filled(20000, 's').join();
      final replacement = '😀${List<String>.filled(16383, 'a').join()}';
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      await controller.selectOversizedRangeUtf16(0, source.length);
      controller.replaceSelection(replacement);
      await settle(controller);

      expect(await controller.readSource(), replacement);
      expect(
        controller.inputValue.text,
        List<String>.filled(16383, 'a').join(),
      );
      expect(controller.inputValue.text.runes.length, 16383);
      expect(controller.inputValue.selection.extentOffset, 16383);
      expect(controller.resyncCount, 0);
    },
    skip: libraryPath == null,
  );

  test(
    'an oversized Backspace delta suppresses its duplicate selector',
    () async {
      const source = 'Select this whole line';
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      await controller.selectOversizedRangeUtf16(0, source.length);
      final surrogate = controller.inputValue;
      final caret = surrogate.selection.extentOffset;
      expect(caret, greaterThan(0));
      controller.applyDeltas([
        TextEditingDeltaDeletion(
          oldText: surrogate.text,
          deletedRange: TextRange(start: caret - 1, end: caret),
          selection: TextSelection.collapsed(offset: caret - 1),
          composing: TextRange.empty,
        ),
      ]);
      controller.observePlatformDeleteBackwardAction();
      await settle(controller);

      expect(await controller.readSource(), '');
      expect(controller.revision, 2);
      expect(controller.resyncCount, 0);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(await controller.readSource(), source);
      expect(controller.globalSelectionBase, 0);
      expect(controller.globalSelectionExtent, source.length);
    },
    skip: libraryPath == null,
  );

  test(
    'selection remains core-owned while crossing viewport pages',
    () async {
      final source = List<String>.generate(
        180,
        (index) => 'Paragraph ${index.toString().padLeft(3, '0')} body.\n\n',
      ).join();
      final controller = await open(source);
      addTearDown(controller.close);
      await settle(controller);

      final firstRow = controller.rows.first;
      final selectionBase =
          controller.surfaceRow(firstRow).globalUtf16Start + 2;
      controller.activateRow(firstRow, selectionBase);
      expect(
        (await controller.resolveCanonicalSelection())!.extent,
        selectionBase,
      );

      expect(await controller.nextViewportPage(), isTrue);
      final targetRow = controller.rows.last;
      final targetSurface = controller.surfaceRow(targetRow);
      final selectionExtent =
          targetSurface.globalUtf16Start +
          (targetSurface.text.length >= 4 ? 4 : targetSurface.text.length);
      controller.extendSelectionTo(
        selectionExtent,
        activeOrdinal: targetRow.ordinal,
      );

      final exact = await controller.resolveCanonicalSelection();
      expect((exact!.base, exact.extent), (selectionBase, selectionExtent));
      expect(controller.hasOversizedSelection, isTrue);
      expect(controller.surfaceRow(targetRow).selection, isNotNull);
      expect(
        await controller.readSelectedText(),
        source.substring(selectionBase, selectionExtent),
      );

      final revisionBefore = controller.revision;
      controller.replaceSelection('X');
      await settle(controller);
      expect(controller.revision, revisionBefore + 1);
      expect(await controller.undo(), isTrue);
      await settle(controller);
      expect(controller.sourceUtf16Length, source.length);
    },
    skip: libraryPath == null,
  );
}
