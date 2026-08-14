import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark_core/flark_core.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'a structural edit waits for its receipt before adopting new source',
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

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.visibleSource, source);
      expect(controller.surfaceRow(controller.rows.first).text, 'Heading');
      expect(controller.surfaceRow(distant).text, 'Distant.');

      await _settle(controller);
      expect(controller.visibleSource, 'Heading\n\nDistant.\n');
      expect(controller.rows.first.kind, 5);
      expect(controller.surfaceRow(controller.rows.first).text, 'Heading');

      expect(controller.lastError, isNull);
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.visibleSource, 'Heading\n\nDistant.\n');
      expect(controller.rows.first.kind, isNot(12));
    },
    skip: libraryPath == null,
  );

  test(
    'cached rows keep a stable presentation while a structural edit is pending',
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
      expect(
        controller.surfaceRow(first).kind,
        isNot(0),
        reason: 'a local optimistic edit must not relay unchanged rows',
      );
      expect(
        controller.surfaceRow(target).kind,
        isNot(0),
        reason: 'the touched row retains its prior frame until a receipt lands',
      );

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
      expect(
        controller.surfaceRow(target).kind,
        isNot(0),
        reason:
            'the last valid touched-row frame remains until recertification',
      );

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
      expect(controller.visibleSource, source);
      expect(controller.surfaceRow(third).text, 'Third');

      await _settle(controller);
      expect(controller.visibleSource, '# First\n\nThird\n\n');
      final demoted = controller.rows[1];
      expect(demoted.kind, 5);
      expect(controller.surfaceRow(demoted).text, 'Third');

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
      expect(ordered.visibleSource, '9) alpha\n\n\n');

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
    'paragraph merge never publishes empty rows or completed inline markers',
    () async {
      final controller = await FlarkEditorController.open(
        'Before **bold**.\n\nAfter.\n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final frames = <String>[];
      void captureFrame() {
        frames.add(
          controller.rows.isEmpty
              ? '<empty>'
              : controller.rows
                    .map((row) {
                      final surface = controller.surfaceRow(row);
                      return '${surface.leadingText}${surface.text}';
                    })
                    .join('|'),
        );
      }

      controller.addListener(captureFrame);
      addTearDown(() => controller.removeListener(captureFrame));
      final after = controller.rows.last;
      controller.activateRow(after, after.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);

      expect(controller.visibleSource, 'Before **bold**.After.\n');
      expect(frames, isNot(contains('<empty>')));
      expect(
        frames.where((frame) => frame.contains('**')),
        isEmpty,
        reason: 'completed inline markers flashed in frames: $frames',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'list lift keeps completed inline projection through pending frames',
    () async {
      final controller = await FlarkEditorController.open(
        '- first\n- **bold**\n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final frames = <String>[];
      void captureFrame() {
        frames.add(
          controller.rows
              .map((row) {
                final surface = controller.surfaceRow(row);
                return '${surface.leadingText}${surface.text}';
              })
              .join('|'),
        );
      }

      controller.addListener(captureFrame);
      addTearDown(() => controller.removeListener(captureFrame));
      final second = controller.rows.last;
      controller.activateRow(second, second.editableUtf16!.start);
      controller.deleteBackward();
      await _settle(controller);

      expect(controller.visibleSource, '- first\n\n**bold**\n');
      expect(frames.where((frame) => frame.isEmpty), isEmpty);
      expect(
        frames.where((frame) => frame.contains('**')),
        isEmpty,
        reason: 'completed inline markers flashed in frames: $frames',
      );
      final lifted = controller.rows.last;
      final presentation = controller.surfaceRow(lifted);
      expect(presentation.leadingText, isEmpty);
      expect(presentation.text, 'bold');
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
      await _settle(quote);
      expect(quote.visibleSource, 'quote\n\nParagraph.\n');
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
      await _settle(continuation);
      expect(continuation.visibleSource, '> quote\n> \n');
      expect(continuation.rows.last.kind, 15);
      expect(
        (
          continuation.rows.last.editableUtf16?.start,
          continuation.rows.last.editableUtf16?.end,
        ),
        (10, 10),
      );

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
    'multiline block quote keeps repeated prefixes hidden while typing',
    () async {
      final controller = await FlarkEditorController.open(
        '> first\n> second\n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final row = controller.rows.single;
      expect(
        row.editCapability,
        FlarkViewportRowEditCapability.projectedReserved,
      );
      final initial = controller.surfaceRow(row);
      expect(initial.leadingText, '│ ');
      expect(initial.text, 'first\nsecond');
      expect(initial.runs, hasLength(2));
      expect(initial.sourceOffsetForTextOffset(6), 10);
      expect(
        initial.sourceOffsetForTextOffset(6, affinity: TextAffinity.upstream),
        8,
      );

      final frames = <String>[];
      void capture() => frames.add(controller.surfaceRow(row).text);
      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.activateRow(row, 10);
      controller.replaceSelection('X');
      expect(controller.visibleSource, '> first\n> Xsecond\n');
      expect(controller.surfaceRow(row).text, 'first\nXsecond');
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(
        controller.surfaceRow(controller.rows.single).text,
        'first\nXsecond',
      );
      expect(frames, isNotEmpty);
      expect(
        frames.where((frame) => frame.contains('>')),
        isEmpty,
        reason: 'a repeated quote marker flashed in frames: $frames',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'multiline block quote Return remains projected through certification',
    () async {
      final controller = await FlarkEditorController.open(
        '> first\n> second\n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      var row = controller.rows.single;
      final frames = <String>[];
      void capture() {
        final index = controller.rows.indexWhere(
          (candidate) => candidate.ordinal == row.ordinal,
        );
        if (index >= 0) {
          frames.add(controller.surfaceRow(controller.rows[index]).text);
        }
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.activateRow(row, 13);
      controller.insertNewline();
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> first\n> sec\n> ond\n');
      row = controller.rows.single;
      expect(controller.surfaceRow(row).text, 'first\nsec\nond');
      expect(
        frames.where((frame) => frame.contains('>')),
        isEmpty,
        reason: 'a quote marker flashed in frames: $frames',
      );

      final sourceBeforeBoundaryBackspace = controller.visibleSource;
      controller.activateRow(row, 16);
      controller.deleteBackward();
      expect(controller.visibleSource, sourceBeforeBoundaryBackspace);
    },
    skip: libraryPath == null,
  );

  test(
    'certified empty quote continuation exits without marker flash',
    () async {
      final controller = await FlarkEditorController.open(
        '> first\n> \n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final empty = controller.rows.last;
      expect(empty.kind, 15);
      expect((empty.editableUtf16?.start, empty.editableUtf16?.end), (10, 10));
      final initial = controller.surfaceRow(empty);
      expect(initial.leadingText, '│ ');
      expect(initial.text, isEmpty);

      final frames = <String>[];
      void capture() {
        frames.add(
          controller.rows
              .expand(controller.surfaceRowsFor)
              .map((surface) => '${surface.leadingText}${surface.text}')
              .join('|'),
        );
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.activateRow(empty, 10);
      controller.insertNewline();
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> first\n\n');
      expect(frames, isNotEmpty);
      expect(
        frames.where((frame) => frame.contains('>')),
        isEmpty,
        reason: 'an empty quote marker flashed in frames: $frames',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'multiline block quote Backspace lifts one line without marker flash',
    () async {
      final controller = await FlarkEditorController.open(
        '> first\n> second\n',
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();
      addTearDown(controller.close);

      final row = controller.rows.single;
      final frames = <String>[];
      void capture() {
        final index = controller.rows.indexWhere(
          (candidate) => candidate.ordinal == row.ordinal,
        );
        if (index < 0) return;
        frames.add(
          controller
              .surfaceRowsFor(controller.rows[index])
              .map((surface) => '${surface.leadingText}${surface.text}')
              .join('|'),
        );
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.activateRow(row, 10);
      final before = controller.inputValue;
      final caret = before.selection.extentOffset;
      controller.applyDeltas([
        TextEditingDeltaDeletion(
          oldText: before.text,
          deletedRange: TextRange(start: caret - 1, end: caret),
          selection: TextSelection.collapsed(offset: caret - 1),
          composing: TextRange.empty,
        ),
      ]);
      controller.observePlatformDeleteBackwardAction();
      controller.replaceSelection('X');
      await _settle(controller);

      expect(controller.lastError, isNull);
      expect(controller.visibleSource, '> first\n\nXsecond\n');
      expect(
        frames.where((frame) => frame.contains('>')),
        isEmpty,
        reason: 'a quote marker flashed in frames: $frames',
      );
      expect(
        frames,
        contains('│ first\n|\nXsecond'),
        reason: 'the mapped mixed quote/plain successor was never published',
      );
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
    'tables stay projected while one parser-owned cell is edited',
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

      final frames = <String>[];
      void capture() {
        final tables = controller.rows.where((row) => row.table != null);
        if (tables.isNotEmpty) {
          frames.add(controller.surfaceRow(tables.first).text);
        }
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      final firstCell = row.table!.rows.first.first;
      controller.activateRow(row, firstCell.contentUtf16.start + 4);
      final active = controller.surfaceRow(row);
      expect(active.text, passive.text);
      expect(active.text, isNot(contains('| :--- | ---: |')));

      controller.replaceSelection('X');
      final optimistic = controller.surfaceRow(row);
      expect(optimistic.text, 'f|oXo │ bar\nx|y │ baz');
      expect(optimistic.text, isNot(contains('| :--- | ---: |')));

      await _settle(controller);
      final settled = controller.rows.firstWhere((row) => row.table != null);
      expect(controller.surfaceRow(settled).text, 'f|oXo │ bar\nx|y │ baz');
      expect(controller.visibleSource, contains(r'f\|oXo'));

      controller.activateRow(
        settled,
        controller.visibleSource.indexOf('X') + 1,
      );
      controller.deleteBackward();
      expect(controller.surfaceRow(settled).text, passive.text);
      await _settle(controller);
      expect(controller.visibleSource, source);
      expect(
        frames.where((frame) => frame.contains(':---')),
        isEmpty,
        reason: 'table delimiter source flashed in frames: $frames',
      );
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
