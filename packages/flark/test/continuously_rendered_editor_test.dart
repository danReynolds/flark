import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'focus preserves inline projection and hidden-boundary topology',
    () async {
      const source = 'Anchor.\n\nbefore **bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final row = controller.rows.last;
      final passive = controller.surfaceRow(row);
      final boldStart = source.indexOf('bold');
      final boldEnd = boldStart + 'bold'.length;
      final boldDisplayStart = passive.text.indexOf('bold');
      final boldDisplayEnd = boldDisplayStart + 'bold'.length;

      controller.activateRow(row, boldStart + 2);
      final active = controller.surfaceRow(row);
      expect(active.text, passive.text);
      expect(
        active.runs.map((run) => run.text),
        passive.runs.map((run) => run.text),
      );
      expect(
        active.runs.any(
          (run) => run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayStart,
          affinity: TextAffinity.downstream,
        ),
        boldStart,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayStart,
          affinity: TextAffinity.upstream,
        ),
        boldStart - 2,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayEnd,
          affinity: TextAffinity.upstream,
        ),
        boldEnd,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayEnd,
          affinity: TextAffinity.downstream,
        ),
        boldEnd + 2,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'Backspace edits visible graphemes and never hidden delimiters',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      final boldStart = source.indexOf('bold');

      controller.activateRow(row, boldStart);
      controller.deleteBackward();
      expect(controller.visibleSource, source);

      controller.activateRow(row, boldStart + 1);
      controller.deleteBackward();
      expect(controller.visibleSource, '**old** after\n');
      await _settle(controller);
      expect(controller.surfaceRow(controller.rows.first).text, 'old after');
    },
    skip: libraryPath == null,
  );

  test(
    'macOS upstream caret remains valid for a semantic Return',
    () async {
      const source = '- one\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);
      controller.updateEditingValue(
        controller.inputValue.copyWith(
          selection: TextSelection.collapsed(
            offset: controller.inputValue.selection.extentOffset,
            affinity: TextAffinity.upstream,
          ),
        ),
      );
      final canonical = await controller.resolveCanonicalSelection();
      expect(canonical!.affinity, FlarkCoreAffinity.upstream);

      controller.insertNewline();
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '- one\n- \n');
      expect(controller.status, isNot(FlarkEditorStatus.faulted));
    },
    skip: libraryPath == null,
  );

  test(
    'task Return continues unchecked and Backspace lifts the whole prefix',
    () async {
      const source = '- [x] done\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '- [x] done\n- [ ] \n');

      await controller.undo();
      expect(controller.visibleSource, source);
      final restored = controller.rows.first;
      controller.activateRow(restored, restored.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'done\n');
      expect(controller.surfaceRow(controller.rows.first).leadingText, '');
    },
    skip: libraryPath == null,
  );

  test(
    'depth-two list Return continues and empty Return or Backspace outdents',
    () async {
      const source = '- parent\n  - child';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.last;
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '- parent\n  - child\n  - ');

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '- parent\n  - child\n- ');

      await controller.undo();
      await controller.undo();
      expect(controller.visibleSource, source);
      row = controller.rows.last;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '- parent\n- child');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '- ');
    },
    skip: libraryPath == null,
  );

  test(
    'uniform deeper lists continue and outdent exactly one level at a time',
    () async {
      const source = '- root\n  - child\n    - leaf';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.last;
      expect(row.listItem?.nestingDepth, 3);
      expect(controller.surfaceRow(row).leadingText, '    - ');
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.visibleSource, '$source\n    - ');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '    - ');

      controller.insertNewline();
      await _settle(controller);
      expect(controller.visibleSource, '$source\n  - ');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '  - ');

      controller.insertNewline();
      await _settle(controller);
      expect(controller.visibleSource, '$source\n- ');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '- ');

      await controller.undo();
      await controller.undo();
      await controller.undo();
      expect(controller.visibleSource, source);
      row = controller.rows.last;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.visibleSource, '- root\n  - child\n  - leaf');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '  - ');
    },
    skip: libraryPath == null,
  );

  test(
    'nonuniform list containers use parser-authored marker columns',
    () async {
      const source = '10. root\n     - child';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.last;
      expect(row.listItem?.nestingDepth, 2);
      expect(row.listItem?.markerOffset, 1);
      expect(row.listItem?.markerColumn, 5);
      expect(controller.surfaceRow(row).leadingText, '     - ');
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.visibleSource, '$source\n     - ');
      expect(
        controller.surfaceRow(controller.rows.last).leadingText,
        '     - ',
      );

      controller.insertNewline();
      await _settle(controller);
      expect(controller.visibleSource, '$source\n - ');
      row = controller.rows.last;
      expect(row.listItem?.markerColumn, 1);
      expect(controller.surfaceRow(row).leadingText, ' - ');
    },
    skip: libraryPath == null,
  );

  test(
    'block quote Return continues, empty Return exits, and Backspace lifts',
    () async {
      const source = '> alpha';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> alpha\n> ');

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> alpha\n\n');

      await controller.undo();
      await controller.undo();
      expect(controller.visibleSource, source);
      row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'alpha');
      expect(controller.surfaceRow(controller.rows.first).leadingText, '');
    },
    skip: libraryPath == null,
  );

  test(
    'nested block quotes continue and outdent one parser-owned level',
    () async {
      const source = '> > alpha';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.first;
      expect(row.blockQuote?.nestingDepth, 2);
      expect(controller.surfaceRow(row).leadingText, '│ │ ');
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> > alpha\n> > ');

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> > alpha\n> ');
      expect(controller.rows.last.blockQuote?.nestingDepth, 1);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> > alpha\n\n');

      expect(await controller.undo(), isTrue);
      expect(await controller.undo(), isTrue);
      expect(await controller.undo(), isTrue);
      await _settle(controller);
      expect(controller.visibleSource, source);
      row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> alpha');
      expect(controller.rows.first.blockQuote?.nestingDepth, 1);
      expect(controller.surfaceRow(controller.rows.first).leadingText, '│ ');
    },
    skip: libraryPath == null,
  );

  test(
    'multiline nested quote outdents only the active rendered line',
    () async {
      const source = '> > first\n> > second\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      expect(row.blockQuote?.nestingDepth, 2);
      expect(row.projectionSegments, isNotNull);
      controller.activateRow(row, source.indexOf('second'));

      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> > first\n\n> second\n');
      expect(
        controller.rows.map((row) => row.blockQuote?.nestingDepth),
        containsAllInOrder([2, 1]),
      );
    },
    skip: libraryPath == null,
  );

  test(
    'indented code Return and Backspace preserve the rendered surface',
    () async {
      const source = '    one\n    two\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.first;
      expect(row.codeBlock?.style, FlarkCodeBlockStyle.indented);
      expect(controller.surfaceRow(row).text, 'one\ntwo\n');
      controller.activateRow(row, source.indexOf('two') + 2);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '    one\n    tw\n    o\n');
      row = controller.rows.first;
      expect(controller.surfaceRow(row).text, 'one\ntw\no\n');
      expect(controller.surfaceRow(row).text, isNot(contains('    ')));

      controller.activateRow(row, controller.visibleSource.indexOf('o\n'));
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, source);
      expect(controller.surfaceRow(controller.rows.first).text, 'one\ntwo\n');

      row = controller.rows.first;
      controller.activateRow(row, source.indexOf('one'));
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'one\n    two\n');
      expect(controller.rows.first.codeBlock, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'thematic break stays rendered on focus and deletes as one atom',
    () async {
      const source = 'before\n\n---\nnext\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      var atom = controller.rows.firstWhere((row) => row.thematicBreak);
      controller.activateRow(atom, source.indexOf('---') + 2);
      expect(controller.inputValue.text, isEmpty);
      expect(controller.globalCaretOffset, atom.editableUtf16!.start);
      expect(controller.surfaceRow(atom).text, isEmpty);
      expect(controller.surfaceRow(atom).thematicBreak, isTrue);

      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'before\n\nnext\n');
      expect(controller.rows.any((row) => row.thematicBreak), isFalse);

      expect(await controller.undo(), isTrue);
      await _settle(controller);
      expect(controller.visibleSource, source);
      atom = controller.rows.firstWhere((row) => row.thematicBreak);
      controller.activateRow(atom, atom.editableUtf16!.start);
      controller.deleteForward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'before\n\nnext\n');
    },
    skip: libraryPath == null,
  );

  test(
    'typing at a thematic atom boundary stays literal and invalidates the atom',
    () async {
      const source = '---\nnext\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final atom = controller.rows.firstWhere((row) => row.thematicBreak);
      controller.activateRow(atom, atom.editableUtf16!.start);
      controller.replaceSelection('x');
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'x---\nnext\n');
      expect(controller.rows.any((row) => row.thematicBreak), isFalse);
    },
    skip: libraryPath == null,
  );

  test(
    'Return at a thematic atom boundary inserts a plain line before it',
    () async {
      const source = '---\nnext\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final atom = controller.rows.firstWhere((row) => row.thematicBreak);
      controller.activateRow(atom, atom.editableUtf16!.start);
      controller.insertNewline();
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '\n---\nnext\n');
      expect(controller.rows.any((row) => row.thematicBreak), isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'ATX heading Return creates plain space and Backspace lifts the prefix',
    () async {
      const source = '## Head';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      var row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '## Head\n\n');

      await controller.undo();
      expect(controller.visibleSource, source);
      row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, 'Head');
      expect(controller.surfaceRow(controller.rows.first).kind, 5);
    },
    skip: libraryPath == null,
  );

  test(
    'an empty ATX heading exposes its real caret and exits on Return',
    () async {
      const source = '# ';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      expect(row.editableUtf16!.start, 2);
      expect(row.editableUtf16!.end, 2);
      controller.activateRow(row, row.editableUtf16!.start);

      controller.insertNewline();
      await _settle(controller);
      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '\n');
    },
    skip: libraryPath == null,
  );

  test(
    'parser-authorized ordinary edits retain projection while pending',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 2);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.semanticsCurrent, isFalse);
      final pending = controller.surfaceRow(row);
      expect(pending.text, 'boxld after');
      expect(pending.text, isNot(contains('**')));
      expect(
        pending.runs.any(
          (run) =>
              run.text == 'boxld' &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
      await _settle(controller);
      expect(controller.visibleSource, '**boxld** after\n');
      final settled = controller.surfaceRow(controller.rows.first);
      expect(settled.active, isTrue);
      expect(settled.text, 'boxld after');
    },
    skip: libraryPath == null,
  );

  test(
    'composition inside strong text is exact while pending and undoes as one unit',
    () async {
      const source = 'Before **β😀** and _em_.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      final globalCaret = source.indexOf('😀');
      controller.activateRow(row, globalCaret);
      final before = controller.inputValue;
      final localCaret = before.selection.extentOffset;
      final composedText = before.text.replaceRange(
        localCaret,
        localCaret,
        'に',
      );

      controller.updateEditingValue(
        TextEditingValue(
          text: composedText,
          selection: TextSelection.collapsed(offset: localCaret + 1),
          composing: TextRange(start: localCaret, end: localCaret + 1),
        ),
      );
      final pendingDeadline = DateTime.now().add(const Duration(seconds: 5));
      while (controller.pendingEdits != 0 &&
          DateTime.now().isBefore(pendingDeadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }

      expect(controller.lastError, isNull);
      expect(
        controller.visibleSource,
        source.replaceRange(globalCaret, globalCaret, 'に'),
      );
      expect(
        controller.inputValue.composing,
        TextRange(start: localCaret, end: localCaret + 1),
      );
      final composingSurface = controller.surfaceRow(row);
      expect(composingSurface.kind, 0);
      expect(composingSurface.text, 'Before **βに😀** and _em_.\n');
      expect(composingSurface.runs, hasLength(1));
      expect(composingSurface.runs.single.sourceExact, isTrue);
      expect(composingSurface.runs.single.styles, isEmpty);

      controller.updateEditingValue(
        controller.inputValue.copyWith(composing: TextRange.empty),
      );
      await _settle(controller);
      expect(controller.inputValue.composing, TextRange.empty);
      expect(
        controller.surfaceRow(controller.rows.first).text,
        'Before βに😀 and em.',
      );
      expect(controller.surfaceRow(controller.rows.first).kind, 5);
      expect(
        controller
            .surfaceRow(controller.rows.first)
            .runs
            .any(
              (run) =>
                  run.text == 'βに😀' &&
                  run.styles.contains(FlarkSurfaceInlineStyle.strong),
            ),
        isTrue,
      );

      expect(await controller.undo(), isTrue);
      await _settle(controller);
      expect(controller.visibleSource, source);
      expect(
        controller.surfaceRow(controller.rows.first).text,
        'Before β😀 and em.',
      );
      expect(controller.canUndo, isFalse);
    },
    skip: libraryPath == null,
  );

  test(
    'plain-text edits at inline content edges retain projection',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 'bold'.length);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.surfaceRow(row).text, 'boldx after');
      expect(controller.surfaceRow(row).text, isNot(contains('**')));
    },
    skip: libraryPath == null,
  );

  test(
    'shorter styled selection replacement is exact while pending',
    () async {
      const source = 'Before **bold** after.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, 10);
      controller.extendSelectionTo(12, activeOrdinal: row.ordinal);

      controller.replaceSelection('X');

      final pending = controller.surfaceRow(row);
      expect(pending.kind, 0);
      expect(pending.text, 'Before **bXd** after.\n');
      expect(pending.runs, hasLength(1));
      expect(pending.runs.single.sourceExact, isTrue);
      expect(pending.runs.single.styles, isEmpty);
      await _settle(controller);
      expect(controller.visibleSource, 'Before **bXd** after.\n');
      final recertified = controller.surfaceRow(controller.rows.first);
      expect(recertified.kind, 5);
      expect(recertified.text, 'Before bXd after.');
      expect(
        recertified.runs.any(
          (run) =>
              run.text == 'bXd' &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'history restores an in-row selection without narrowing the paint window',
    () async {
      const source = 'Before **bold** after.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, 10);
      controller.extendSelectionTo(12, activeOrdinal: row.ordinal);
      controller.replaceSelection('X');
      await _settle(controller);

      await controller.undo();

      expect(controller.inputValue.text, source);
      expect(
        controller.surfaceRow(controller.rows.first).text,
        'Before bold after.',
      );
      expect(controller.surfaceRow(controller.rows.first).text, isNot('ol'));
    },
    skip: libraryPath == null,
  );

  test(
    'plain heading safe burst stays projected and preserves its sibling',
    () async {
      const source = '# Heading\n\nPlain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final heading = controller.rows.first;
      final caret = source.indexOf('Heading') + 'Head'.length;
      controller.activateRow(heading, caret);
      final observed = <({int firstKind, int lastKind, String firstText})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 3) return;
        final first = controller.surfaceRow(rows.first);
        final last = controller.surfaceRow(rows.last);
        observed.add((
          firstKind: first.kind,
          lastKind: last.kind,
          firstText: first.text,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      for (final inserted in ['x', ' ', 'y', ' ', 'Z2']) {
        final before = controller.inputValue;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: inserted,
            insertionOffset: before.selection.extentOffset,
            selection: TextSelection.collapsed(
              offset: before.selection.extentOffset + inserted.length,
            ),
            composing: TextRange.empty,
          ),
        ]);
        capture();
      }

      await _settle(controller);
      expect(observed, isNotEmpty);
      expect(observed.every((state) => state.lastKind == 12), isTrue);
      expect(
        observed.every(
          (state) => state.firstKind == 12 && !state.firstText.startsWith('# '),
        ),
        isTrue,
        reason:
            'every immediate frame must retain parser-authorized heading '
            'presentation during an alternating word/space burst: $observed',
      );
      final recertified = controller.surfaceRow(controller.rows.first);
      expect(recertified.kind, 12);
      expect(recertified.text, 'Headx y Z2ing');
      expect(controller.visibleSource, startsWith('# Headx y Z2ing'));
    },
    skip: libraryPath == null,
  );

  test(
    'plain heading backspace keeps its parser-authored shell and sibling',
    () async {
      const source = '# Heading\n\nPlain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final heading = controller.rows.first;
      controller.activateRow(heading, heading.editableUtf16!.end);
      final observed = <({int firstKind, int lastKind, String firstText})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 3) return;
        observed.add((
          firstKind: controller.surfaceRow(rows.first).kind,
          lastKind: controller.surfaceRow(rows.last).kind,
          firstText: controller.surfaceRow(rows.first).text,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.deleteBackward();
      capture();
      await _settle(controller);

      expect(observed, isNotEmpty);
      expect(observed.every((state) => state.lastKind == 12), isTrue);
      expect(
        observed.every(
          (state) => state.firstKind == 12 && state.firstText == 'Headin',
        ),
        isTrue,
        reason:
            'the complete ATX edit cell keeps the block shell while its '
            'current content is exact',
      );
      final recertified = controller.surfaceRow(controller.rows.first);
      expect(recertified.kind, 12);
      expect(recertified.text, 'Headin');
      expect(controller.visibleSource, startsWith('# Headin\n'));
    },
    skip: libraryPath == null,
  );

  test(
    'plain paragraph backspace fails closed locally and preserves its sibling',
    () async {
      const source = 'Plain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final paragraph = controller.rows.first;
      controller.activateRow(paragraph, paragraph.editableUtf16!.end);
      final observed = <({int paragraphKind, int siblingKind})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 2) return;
        observed.add((
          paragraphKind: controller.surfaceRow(rows.first).kind,
          siblingKind: controller.surfaceRow(rows.last).kind,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.deleteBackward();
      capture();
      await _settle(controller);

      expect(observed, isNotEmpty);
      expect(observed.every((state) => state.siblingKind == 12), isTrue);
      expect(observed.any((state) => state.paragraphKind == 0), isTrue);
      expect(controller.surfaceRow(controller.rows.first).kind, 5);
      expect(controller.visibleSource, startsWith('Plain paragraph\n'));
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Return at the dogfood paragraph boundary owns a visible empty block',
    (tester) async {
      const source =
          '''This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

## Start here

1. Click here.
''';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 900,
            height: 600,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );

      Future<void> settleEdits() async {
        for (
          var turn = 0;
          turn < 8 && controller.pendingEdits != 0;
          turn += 1
        ) {
          await tester.pump();
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 10)),
          );
          await tester.pump();
        }
        expect(controller.pendingEdits, 0);
        expect(controller.lastError, isNull);
      }

      final boundary = source.indexOf('\n\n##');
      final paragraph = controller.rows.firstWhere(
        (row) => row.kind == 5 && row.editableUtf16!.end == boundary,
      );
      controller.activateRow(paragraph, boundary);
      await tester.pump();
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump();
      controller.insertNewline();
      expect(controller.visibleSource, source);
      expect(controller.pendingEdits, 1);
      await tester.pump();
      await settleEdits();
      await tester.pump();

      RenderFlarkSurface surface() =>
          tester.renderObject(find.byType(FlarkRenderSurfaceWidget));
      expect(controller.rows.map((row) => controller.surfaceRow(row).kind), [
        0,
        12,
        5,
      ]);
      var emptyBlock = surface().debugPaintedPlan.singleWhere(
        (entry) =>
            entry.neutral && entry.sourceStart == controller.globalCaretOffset,
      );
      expect(emptyBlock.text, '\n');
      expect(emptyBlock.active, isTrue);

      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      expect(controller.rows.map((row) => controller.surfaceRow(row).kind), [
        5,
        12,
        5,
      ]);
      emptyBlock = surface().debugPaintedPlan.singleWhere(
        (entry) =>
            entry.neutral && entry.sourceStart == controller.globalCaretOffset,
      );
      expect(emptyBlock.text, '\n');
      expect(emptyBlock.active, isTrue);
      expect(controller.inputValue.text, '\n');
      expect(controller.inputValue.selection.extentOffset, 0);

      final afterFirstReturn = source.replaceRange(boundary, boundary, '\n\n');
      expect(controller.visibleSource, afterFirstReturn);
      final firstNeutralCount = surface().debugPaintedPlan
          .where((entry) => entry.neutral)
          .length;

      controller.insertNewline();
      await settleEdits();
      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      final afterSecondReturn = afterFirstReturn.replaceRange(
        boundary + 2,
        boundary + 2,
        '\n',
      );
      expect(controller.visibleSource, afterSecondReturn);
      expect(
        surface().debugPaintedPlan.where((entry) => entry.neutral).length,
        firstNeutralCount + 1,
      );

      controller.deleteBackward();
      await settleEdits();
      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      expect(controller.visibleSource, afterFirstReturn);
      expect(controller.globalCaretOffset, boundary + 2);
      expect(
        surface().debugPaintedPlan.where((entry) => entry.neutral).length,
        firstNeutralCount,
      );

      final beforeTyping = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: beforeTyping.text,
          textInserted: 'x',
          insertionOffset: beforeTyping.selection.extentOffset,
          selection: const TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(controller.rows.map((row) => controller.surfaceRow(row).kind), [
        5,
        12,
        5,
      ]);
      final pendingTextBlock = surface().debugPaintedPlan.singleWhere(
        (entry) => entry.neutral && entry.active,
      );
      expect(pendingTextBlock.text, 'x\n');

      await settleEdits();
      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      final activeTexts = controller.rows
          .map(controller.surfaceRow)
          .where((row) => row.active)
          .map((row) => row.text)
          .toList(growable: false);
      expect(
        activeTexts,
        contains('x'),
        reason:
            'caret=${controller.globalCaretOffset}; '
            'input=${controller.inputValue}; '
            'rows=${controller.rows.map((row) => (row.ordinal, controller.surfaceRow(row).text, controller.surfaceRow(row).active)).toList()}',
      );
      expect(
        controller.surfaceRow(controller.rows.first).text,
        isNot(contains('**')),
      );
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  test(
    'parser-proved asterisk insertion stays rendered while pending',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 2);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '*',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      final pending = controller.surfaceRow(row);
      expect(pending.kind, 5);
      expect(pending.text, 'bo*ld after');
      expect(pending.text, isNot(contains('**')));
      expect(
        pending.runs.where((run) => run.text.contains('bo*ld')).single.styles,
        contains(FlarkSurfaceInlineStyle.strong),
      );
      await _settle(controller);
      final recertified = controller.surfaceRow(controller.rows.first);
      expect(recertified.kind, 5);
      expect(recertified.text, 'bo*ld after');
      expect(
        recertified.runs
            .where((run) => run.text.contains('bo*ld'))
            .single
            .styles,
        contains(FlarkSurfaceInlineStyle.strong),
      );
    },
    skip: libraryPath == null,
  );

  test(
    'completed inline syntax projects after parser certification',
    () async {
      const source = '*bold\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('\n'));
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '*',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.surfaceRow(row).text, contains('*bold*'));
      await _settle(controller);
      expect(controller.surfaceRow(controller.rows.first).text, 'bold');
    },
    skip: libraryPath == null,
  );

  test(
    'platform selections inside hidden markers normalize to legal stops',
    () async {
      const source = '**bold**\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      controller.activateRow(controller.rows.first, 2);

      controller.updateEditingValue(
        controller.inputValue.copyWith(
          selection: const TextSelection.collapsed(offset: 1),
        ),
      );
      expect(controller.inputValue.selection.extentOffset, 2);
      expect(controller.globalCaretOffset, 2);
    },
    skip: libraryPath == null,
  );

  testWidgets('editor and read-only view share one render plan', (
    tester,
  ) async {
    const source = '# Heading\n\nbefore **bold** after\n';
    final controller = (await tester.runAsync(
      () => FlarkEditorController.open(source, libraryPath: libraryPath!),
    ))!;
    addTearDown(controller.close);
    await tester.runAsync(controller.continueParsing);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Row(
          children: [
            Expanded(child: FlarkEditor(controller: controller)),
            Expanded(child: FlarkMarkdownView(controller: controller)),
          ],
        ),
      ),
    );
    await tester.pump();

    final surfaces = tester
        .renderObjectList<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        )
        .toList();
    expect(surfaces, hasLength(2));
    expect(surfaces[0].debugRenderPlanHash, surfaces[1].debugRenderPlanHash);
    expect(find.byType(EditableText), findsNothing);
  }, skip: libraryPath == null);

  testWidgets(
    'trackpad scrolling never changes the canonical selection',
    (tester) async {
      final source = List<String>.generate(
        20,
        (index) => 'Paragraph $index with enough text.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);
      await tester.runAsync(() async {
        controller.activateRow(controller.rows.first, 4);
        await controller.resolveCanonicalSelection();
      });
      final selectionBefore = controller.inputValue.selection;
      final globalCaretBefore = controller.globalCaretOffset;

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 240,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );

      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: tester.getCenter(find.byType(FlarkEditor)),
          scrollDelta: const Offset(0, 300),
        ),
      );
      await tester.pump();

      expect(surface.scrollOffset, greaterThan(0));
      expect(controller.inputValue.selection, selectionBefore);
      expect(controller.globalCaretOffset, globalCaretBefore);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );
}

Future<void> _settle(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  await controller.continueParsing();
}
