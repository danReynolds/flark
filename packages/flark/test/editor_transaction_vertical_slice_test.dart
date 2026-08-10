import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'bounded cross-row replacement is source-exact and undoable',
    () async {
      const source = '**alpha**\n\nmiddle 🌍\n\nlast\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final first = controller.rows.first;
      final middle = controller.rows[1];
      final last = controller.rows.last;
      final selectionStart = source.indexOf('alpha');
      final selectionEnd = source.indexOf('last') + 2;

      controller.activateRow(first, selectionStart);
      controller.extendSelectionTo(selectionEnd, activeOrdinal: last.ordinal);

      expect(controller.inputValue.selection.isCollapsed, isFalse);
      expect(controller.surfaceRow(first).selection, isNotNull);
      expect(controller.surfaceRow(middle).selection, isNotNull);
      expect(controller.surfaceRow(last).active, isTrue);
      expect(controller.surfaceRow(first).text, startsWith('**alpha**'));

      controller.replaceSelection('X');
      final replaced = source.replaceRange(selectionStart, selectionEnd, 'X');
      expect(controller.visibleSource, replaced);
      await _waitForTransactions(controller);

      expect(controller.lastError, isNull);
      expect(controller.revision, 2);
      expect(controller.canUndo, isTrue);

      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, source);
      expect(controller.revision, 3);
      expect(controller.inputValue.selection.isCollapsed, isFalse);

      expect(await controller.redo(), isTrue);
      expect(controller.visibleSource, replaced);
      expect(controller.revision, 4);
      expect(controller.globalCaretOffset, selectionStart + 1);
    },
    skip: libraryPath == null,
  );

  test(
    'ordinary delta insertion remains a bounded reversible transaction',
    () async {
      final controller = await FlarkEditorController.open(
        '# Flark\n\nA quick paragraph.\n',
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final before = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'Live ',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 5,
          ),
          composing: TextRange.empty,
        ),
      ]);

      await _waitForTransactions(controller);
      expect(controller.visibleSource, startsWith('Live # Flark'));
      expect(controller.canUndo, isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    '32 KiB paste and large deletion stay bounded and reversible',
    () async {
      const source = 'before\nafter\n';
      final paste = List.filled(32 * 1024, 'p').join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      controller.activateRow(controller.rows.first, 7);

      final beforePaste = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: beforePaste.text,
          textInserted: paste,
          insertionOffset: beforePaste.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: beforePaste.selection.extentOffset + paste.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      expect(controller.inputValue.text.length, lessThanOrEqualTo(16 * 1024));
      expect(controller.visibleSource.length, lessThanOrEqualTo(16 * 1024));
      expect(controller.globalCaretOffset, 7 + paste.length);
      expect(controller.rows, isEmpty);
      final optimisticPaint = controller.neutralSurfaceRow(
        globalUtf16Start: controller.visibleUtf16Start,
        text: controller.visibleSource,
        ordinal: 0,
      );
      expect(optimisticPaint.active, isTrue);
      expect(optimisticPaint.text.length, lessThanOrEqualTo(2 * 1024));
      expect(
        optimisticPaint.globalUtf16Start,
        lessThanOrEqualTo(controller.globalCaretOffset),
      );
      expect(
        optimisticPaint.globalUtf16Start + optimisticPaint.text.length,
        greaterThanOrEqualTo(controller.globalCaretOffset),
      );
      await _waitForTransactions(controller);

      expect(controller.lastError, isNull);
      expect(controller.revision, 2);
      expect(controller.sourceUtf16Length, source.length + paste.length);
      expect(controller.sourceByteLength, source.length + paste.length);

      expect(await controller.undo(), isTrue);
      expect(controller.sourceUtf16Length, source.length);
      expect(controller.visibleSource, source);
      expect(controller.globalCaretOffset, 7);

      expect(await controller.redo(), isTrue);
      expect(controller.sourceUtf16Length, source.length + paste.length);
      expect(controller.globalCaretOffset, 7 + paste.length);
      expect(controller.inputValue.text.length, lessThanOrEqualTo(16 * 1024));
      expect(controller.visibleSource.length, lessThanOrEqualTo(16 * 1024));

      controller.updateEditingValue(
        controller.inputValue.copyWith(
          selection: const TextSelection(baseOffset: 0, extentOffset: 8 * 1024),
          composing: TextRange.empty,
        ),
      );
      controller.replaceSelection('');
      await _waitForTransactions(controller);
      expect(
        controller.sourceUtf16Length,
        source.length + paste.length - 8 * 1024,
      );
      expect(await controller.undo(), isTrue);
      expect(controller.sourceUtf16Length, source.length + paste.length);
    },
    skip: libraryPath == null,
  );

  test('rapid typing is one reversible history group', () async {
    const source = 'start\n';
    final controller = await FlarkEditorController.open(
      source,
      libraryPath: libraryPath!,
    );
    addTearDown(controller.close);
    await controller.continueParsing();

    for (final character in ['a', 'b', 'c']) {
      final before = controller.inputValue;
      final offset = before.selection.extentOffset;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: character,
          insertionOffset: offset,
          selection: TextSelection.collapsed(offset: offset + 1),
          composing: TextRange.empty,
        ),
      ]);
    }
    await _waitForTransactions(controller);
    expect(controller.visibleSource, 'abc$source');

    expect(await controller.undo(), isTrue);
    expect(controller.visibleSource, source);
    expect(controller.globalCaretOffset, 0);

    expect(await controller.redo(), isTrue);
    expect(controller.visibleSource, 'abc$source');
    expect(controller.globalCaretOffset, 3);

    controller.insertNewline();
    await _waitForTransactions(controller);
    expect(await controller.undo(), isTrue);
    expect(controller.visibleSource, 'abc$source');
    expect(await controller.undo(), isTrue);
    expect(controller.visibleSource, source);
  }, skip: libraryPath == null);

  test(
    'IME composition stays active and is one reversible history group',
    () async {
      const source = 'start\n';
      const composed = 'e\u0301';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final initial = controller.inputValue;
      controller.updateEditingValue(
        TextEditingValue(
          text: 'e${initial.text}',
          selection: const TextSelection.collapsed(offset: 1),
          composing: const TextRange(start: 0, end: 1),
        ),
      );
      controller.updateEditingValue(
        TextEditingValue(
          text: '$composed${initial.text}',
          selection: const TextSelection.collapsed(offset: 2),
          composing: const TextRange(start: 0, end: 2),
        ),
      );
      await _waitForTransactions(controller);

      await controller.continueParsing();
      expect(
        controller.inputValue.composing,
        const TextRange(start: 0, end: 2),
      );
      expect(controller.visibleSource, '$composed$source');

      controller.updateEditingValue(
        controller.inputValue.copyWith(composing: TextRange.empty),
      );
      await controller.continueParsing();
      expect(controller.inputValue.composing, TextRange.empty);

      final beforeTyping = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: beforeTyping.text,
          textInserted: 'x',
          insertionOffset: beforeTyping.selection.extentOffset,
          selection: const TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      await _waitForTransactions(controller);
      expect(controller.visibleSource, '${composed}x$source');

      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, '$composed$source');
      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, source);

      expect(await controller.redo(), isTrue);
      expect(controller.visibleSource, '$composed$source');
      expect(await controller.redo(), isTrue);
      expect(controller.visibleSource, '${composed}x$source');
    },
    skip: libraryPath == null,
  );

  test(
    'backward and forward delete remove one grapheme cluster',
    () async {
      const family = '👨‍👩‍👧‍👦';
      const source = 'A${family}B\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      controller.activateRow(controller.rows.first, 1 + family.length);
      controller.deleteBackward();
      expect(controller.visibleSource, 'AB\n');
      await _waitForTransactions(controller);
      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, source);

      controller.activateRow(controller.rows.first, 1);
      controller.deleteForward();
      expect(controller.visibleSource, 'AB\n');
      await _waitForTransactions(controller);
      expect(await controller.undo(), isTrue);
      expect(controller.visibleSource, source);
    },
    skip: libraryPath == null,
  );
}

Future<void> _waitForTransactions(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  expect(controller.pendingEdits, 0);
}
