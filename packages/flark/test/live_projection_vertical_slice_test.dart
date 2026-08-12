import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark_core/flark_core.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'a structural edit immediately drops stale semantics and keeps exact source',
    () async {
      const source = '# Heading\n\nDistant.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.rows.first.kind, 12);
      final distant = controller.rows.last;

      controller.activateRow(
        controller.rows.first,
        controller.rows.first.editableUtf16!.start,
      );
      controller.deleteBackward();

      expect(controller.semanticsCurrent, isFalse);
      expect(controller.visibleSource, 'Heading\n\nDistant.\n');
      final active = controller.surfaceRow(controller.rows.first);
      expect(active.text, startsWith('Heading'));
      expect(active.kind, 0);
      final neutralDistant = controller.surfaceRow(distant);
      expect(neutralDistant.text, 'Distant.\n');
      expect(neutralDistant.kind, 0);
      expect(neutralDistant.globalUtf16Start, 9);

      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (controller.pendingEdits != 0 &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }
      await controller.continueParsing();

      expect(controller.lastError, isNull);
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.visibleSource, 'Heading\n\nDistant.\n');
      expect(controller.rows.first.kind, isNot(12));
    },
    skip: libraryPath == null,
  );

  test(
    'unchanged rows stay semantic while the edited structural range is pending',
    () async {
      final padding = List<String>.filled(40, 'stable').join(' ');
      final source = List<String>.generate(
        64,
        (index) =>
            'Paragraph ${index.toString().padLeft(3, '0')} $padding.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final first = controller.rows.first;
      final target = controller.rows.firstWhere(
        (row) => controller.surfaceRow(row).text.startsWith('Paragraph 024'),
      );
      expect(controller.surfaceRow(first).kind, isNot(0));
      expect(controller.surfaceRow(target).kind, isNot(0));

      controller.activateRow(target, target.sourceUtf16.start);
      controller.replaceSelection('# ');
      expect(controller.surfaceRow(first).kind, 0);
      expect(controller.surfaceRow(target).kind, 0);

      // pendingEdits reaches zero at admission, before the post-edit page is
      // installed; the mixed-partition assertions need the installed viewport
      // for the edited revision, so wait for that page rather than the ack.
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while ((controller.pendingEdits != 0 ||
              controller.viewport?.revision != controller.revision) &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }

      expect(controller.lastError, isNull);
      expect(controller.viewport?.revision, controller.revision);
      expect(
        controller.viewport?.certification,
        FlarkCertification.mixedCurrent,
      );
      expect(controller.visibleSource, contains('# Paragraph 024'));
      expect(controller.surfaceRow(first).kind, isNot(0));
      expect(controller.surfaceRow(target).kind, 0);

      await controller.continueParsing();
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.rows.any((row) => row.kind == 12), isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'ATX levels survive the native boundary and Backspace demotes cleanly',
    () async {
      const source = '# First\n\n### Third\n\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final first = controller.rows[0];
      final third = controller.rows[1];
      expect(first.headingLevel, 1);
      expect(third.headingLevel, 3);
      expect(controller.surfaceRow(first).text, 'First');
      expect(controller.surfaceRow(first).active, isTrue);
      expect(controller.surfaceRow(third).text, 'Third');

      controller.activateRow(third, third.editableUtf16!.start);
      controller.deleteBackward();
      expect(controller.visibleSource, '# First\n\nThird\n\n');
      expect(controller.surfaceRow(third).kind, 0);

      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (controller.pendingEdits != 0 &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }
      await controller.continueParsing();

      expect(controller.lastError, isNull);
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.rows[0].headingLevel, 1);
      expect(controller.rows[1].kind, 5);
      expect(controller.surfaceRow(controller.rows[1]).text, 'Third');
    },
    skip: libraryPath == null,
  );

  test(
    'list projection continues, demotes, and exits from parser-owned facts',
    () async {
      final bullet = await FlarkEditorController.open(
        '- alpha\n- beta\n',
        libraryPath: libraryPath!,
      );
      await bullet.continueParsing();
      addTearDown(bullet.close);

      final first = bullet.rows[0];
      final second = bullet.rows[1];
      expect(first.listItem?.markerStyle, FlarkListMarkerStyle.hyphen);
      expect(bullet.surfaceRow(first).leadingText, '- ');
      expect(bullet.surfaceRow(second).leadingText, '- ');

      bullet.activateRow(second, second.editableUtf16!.start);
      bullet.deleteBackward();
      await _settle(bullet);
      expect(bullet.visibleSource, '- alpha\n\nbeta\n');
      expect(bullet.rows[0].listItem, isNotNull);
      expect(bullet.rows[1].listItem, isNull);

      final ordered = await FlarkEditorController.open(
        '9) alpha\n',
        libraryPath: libraryPath,
      );
      await ordered.continueParsing();
      addTearDown(ordered.close);
      final orderedRow = ordered.rows.single;
      ordered.activateRow(orderedRow, orderedRow.editableUtf16!.end);
      ordered.insertNewline();
      await _settle(ordered);
      expect(ordered.visibleSource, '9) alpha\n10) \n');
      expect(
        ordered.rows.map((row) => row.listItem?.markerValue),
        contains(10),
      );
      final continuedEmpty = ordered.rows.firstWhere(
        (row) => row.listItem?.markerValue == 10,
      );
      expect(
        continuedEmpty.listItem?.simpleContinuation,
        isTrue,
        reason: 'terminal path depth ${continuedEmpty.pathDepth}',
      );
      ordered.activateRow(continuedEmpty, continuedEmpty.editableUtf16!.start);
      ordered.insertNewline();
      await _settle(ordered);
      expect(ordered.visibleSource, '9) alpha\n\n');

      final empty = await FlarkEditorController.open(
        '- alpha\n- ',
        libraryPath: libraryPath,
      );
      await empty.continueParsing();
      addTearDown(empty.close);
      final emptyRow = empty.rows.last;
      expect(emptyRow.kind, 14);
      empty.activateRow(emptyRow, emptyRow.editableUtf16!.start);
      empty.insertNewline();
      await _settle(empty);
      expect(empty.visibleSource, '- alpha\n\n');
      expect(empty.lastError, isNull);
      expect(empty.semanticsCurrent, isTrue);

      final tasks = await FlarkEditorController.open(
        '- [ ] todo\n- [X] done\n',
        libraryPath: libraryPath,
      );
      await tasks.continueParsing();
      addTearDown(tasks.close);
      expect(tasks.rows[0].listItem?.taskChecked, isFalse);
      expect(tasks.rows[1].listItem?.taskChecked, isTrue);
      tasks.activateRow(tasks.rows[1], tasks.rows[1].editableUtf16!.end);
      expect(tasks.surfaceRow(tasks.rows[0]).leadingText, '☐ ');
      tasks.insertNewline();
      await _settle(tasks);
      expect(tasks.visibleSource, '- [ ] todo\n- [X] done\n- [ ] \n');
      expect(tasks.rows.last.listItem?.taskChecked, isFalse);
    },
    skip: libraryPath == null,
  );

  test(
    'platform Return uses list continuation and visual block split recipes',
    () async {
      final ordered = await FlarkEditorController.open(
        '9) alpha\n',
        libraryPath: libraryPath!,
      );
      await ordered.continueParsing();
      addTearDown(ordered.close);
      final orderedRow = ordered.rows.single;
      ordered.activateRow(orderedRow, orderedRow.editableUtf16!.end);
      var before = ordered.inputValue;
      ordered.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '\n',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await _settle(ordered);
      expect(ordered.visibleSource, '9) alpha\n10) \n');
      expect(ordered.resyncCount, 0);

      final separated = await FlarkEditorController.open(
        'Paragraph.\n1. item\n',
        libraryPath: libraryPath,
      );
      await separated.continueParsing();
      addTearDown(separated.close);
      final paragraph = separated.rows.firstWhere((row) => row.kind == 5);
      separated.activateRow(paragraph, paragraph.editableUtf16!.end);
      before = separated.inputValue;
      separated.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '\n',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await _settle(separated);
      expect(separated.visibleSource, 'Paragraph.\n\n\n1. item\n');
      expect(separated.resyncCount, 0);
      expect(separated.rows.any((row) => row.listItem != null), isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'block structures project and edit from parser-owned facts',
    () async {
      final quote = await FlarkEditorController.open(
        '> quote\n\nParagraph.\n',
        libraryPath: libraryPath!,
      );
      await quote.continueParsing();
      addTearDown(quote.close);

      final quoteRow = quote.rows.firstWhere((row) => row.blockQuote != null);
      final paragraph = quote.rows.firstWhere(
        (row) => row.blockQuote == null && row.kind == 5,
      );
      quote.activateRow(paragraph, paragraph.editableUtf16!.start);
      final passiveQuote = quote.surfaceRow(quoteRow);
      expect(passiveQuote.leadingText, '│ ');
      expect(passiveQuote.text, 'quote');
      expect(passiveQuote.blockQuoteDepth, 1);

      quote.activateRow(quoteRow, quoteRow.editableUtf16!.start);
      quote.deleteBackward();
      expect(quote.visibleSource, 'quote\n\nParagraph.\n');
      await _settle(quote);
      expect(quote.rows.first.blockQuote, isNull);

      final continuation = await FlarkEditorController.open(
        '> quote\n',
        libraryPath: libraryPath,
      );
      await continuation.continueParsing();
      addTearDown(continuation.close);
      final continuedQuote = continuation.rows.single;
      continuation.activateRow(
        continuedQuote,
        continuedQuote.editableUtf16!.end,
      );
      continuation.insertNewline();
      expect(continuation.visibleSource, '> quote\n> \n');
      await _settle(continuation);

      final structures = await FlarkEditorController.open(
        'Paragraph.\n\n```dart\ncode\n```\n\n    indented\n\n---\n',
        libraryPath: libraryPath,
      );
      await structures.continueParsing();
      addTearDown(structures.close);
      final fenced = structures.rows.firstWhere(
        (row) => row.codeBlock?.style == FlarkCodeBlockStyle.fencedBacktick,
      );
      final indented = structures.rows.firstWhere(
        (row) => row.codeBlock?.style == FlarkCodeBlockStyle.indented,
      );
      final thematic = structures.rows.firstWhere((row) => row.thematicBreak);
      expect(structures.surfaceRow(fenced).text, 'code\n');
      expect(structures.surfaceRow(fenced).codeBlock?.closed, isTrue);
      expect(structures.surfaceRow(indented).text, 'indented\n');
      expect(structures.surfaceRow(thematic).text, isEmpty);
      expect(structures.surfaceRow(thematic).thematicBreak, isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'inline Markdown stays marker-free and source-mapped while active',
    () async {
      const source =
          'Active.\n\n*em🌍* **strong** `code` [link](https://example.com) '
          '<https://a.test>\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final inlineRow = controller.rows.last;
      final passive = controller.surfaceRow(inlineRow);
      expect(passive.active, isFalse);
      expect(passive.text, isNot(contains('*')));
      expect(passive.text, isNot(contains('`')));
      expect(passive.text, isNot(contains('https://example.com')));
      expect(passive.text, contains('em🌍 strong code link https://a.test'));

      final emphasis = passive.runs.singleWhere(
        (run) => run.styles.contains(FlarkSurfaceInlineStyle.emphasis),
      );
      expect(emphasis.text, 'em🌍');
      expect(emphasis.sourceUtf16Start, source.indexOf('em'));
      expect(passive.sourceOffsetForTextOffset(0), source.indexOf('em'));
      expect(
        passive.runs.any(
          (run) =>
              run.text == 'strong' &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
      expect(
        passive.runs.any(
          (run) =>
              run.text == 'code' &&
              run.styles.contains(FlarkSurfaceInlineStyle.code),
        ),
        isTrue,
      );
      expect(
        passive.runs.where(
          (run) => run.styles.contains(FlarkSurfaceInlineStyle.link),
        ),
        hasLength(2),
      );

      controller.activateRow(inlineRow, passive.sourceOffsetForTextOffset(0));
      final active = controller.surfaceRow(inlineRow);
      expect(active.active, isTrue);
      expect(active.text, passive.text);
      expect(active.runs, hasLength(passive.runs.length));
      expect(
        active.runs.any(
          (run) => run.styles.contains(FlarkSurfaceInlineStyle.emphasis),
        ),
        isTrue,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'passive projection applies parser-authored replacements and references',
    () async {
      const source =
          'Active.\n\n\\* &ngE; ` a ` [ref][id] ![alt](image.png)\n\n[id]: /target\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final inlineRow = controller.rows.firstWhere(
        (row) =>
            row.inlineFacts?.any(
              (fact) => fact.kind == FlarkInlineFactKind.replacement,
            ) ??
            false,
      );
      final passive = controller.surfaceRow(inlineRow);
      expect(passive.text, contains('* ≧̸ a ref alt'));
      expect(passive.text, isNot(contains('&ngE;')));
      expect(passive.text, isNot(contains('image.png')));
      expect(passive.text, isNot(contains('[id]')));

      final replacement = passive.runs.singleWhere((run) => run.text == '≧̸');
      expect(replacement.sourceExact, isFalse);
      final replacementTextOffset = passive.text.indexOf(replacement.text);
      final replacementSourceOffset = source.indexOf('&ngE;');
      expect(
        passive.sourceOffsetForTextOffset(replacementTextOffset),
        replacementSourceOffset,
      );
      expect(
        passive.sourceOffsetForTextOffset(
          replacementTextOffset + replacement.text.length,
        ),
        replacementSourceOffset + '&ngE;'.length,
      );
      expect(
        passive.runs.any(
          (run) =>
              run.text == 'ref' &&
              run.styles.contains(FlarkSurfaceInlineStyle.link),
        ),
        isTrue,
      );

      controller.activateRow(
        inlineRow,
        passive.sourceOffsetForTextOffset(replacementTextOffset),
      );
      expect(controller.surfaceRow(inlineRow).text, contains('≧̸'));

      const codeSource = 'Active.\n\n`a\r\nb`\n';
      final code = await FlarkEditorController.open(
        codeSource,
        libraryPath: libraryPath,
      );
      await code.continueParsing();
      addTearDown(code.close);
      expect(code.surfaceRow(code.rows.last).text, startsWith('a b'));
    },
    skip: libraryPath == null,
  );

  test(
    'passive tables use parser-owned cells while active editing stays exact',
    () async {
      const tableSource =
          '| f\\|oo | bar |\n| :--- | ---: |\n| `x\\|y` | **baz** |\n';
      const source = 'Active.\n\n$tableSource';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final row = controller.rows.firstWhere((row) => row.table != null);
      expect(row.table?.columnCount, 2);
      final passive = controller.surfaceRow(row);
      expect(passive.text, 'f|oo │ bar\nx|y │ baz');
      expect(
        passive.runs
            .where((run) => run.styles.contains(FlarkSurfaceInlineStyle.code))
            .map((run) => run.text)
            .join(),
        'x|y',
      );
      expect(
        passive.runs.any(
          (run) =>
              run.text == 'baz' &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );

      controller.activateRow(row, row.sourceUtf16.start);
      final active = controller.surfaceRow(row);
      expect(active.text, tableSource);
      expect(active.text, contains('| :--- | ---: |'));
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
  expect(controller.lastError, isNull);
  expect(controller.semanticsCurrent, isTrue);
}
