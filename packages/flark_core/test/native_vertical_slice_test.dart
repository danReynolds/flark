import 'dart:convert';
import 'dart:io';

import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'native history tokens support multi-level undo and redo',
    () async {
      const source = 'alpha beta 🌍\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);

      final first = await document.applyEditUtf16(0, 5, 'A');
      expect(first.historyDisposition, FlarkCoreHistoryDisposition.retained);
      expect(first.historyToken, isNotNull);

      final second = await document.applyEditUtf16(2, 6, 'BETA!');
      expect(second.revision, 3);
      expect(second.historyToken, isNotNull);
      expect(await document.readSource(), 'A BETA! 🌍\n');

      final undoSecond = await document.replayHistory(second.historyToken!);
      expect(undoSecond.revision, 4);
      expect(await document.readSource(), 'A beta 🌍\n');

      final undoFirst = await document.replayHistory(first.historyToken!);
      expect(undoFirst.revision, 5);
      expect(await document.readSource(), source);
      expect(undoFirst.sourceUtf16Length, source.length);
      expect(undoFirst.sourceByteLength, utf8.encode(source).length);

      final redoneFirst = await document.replayHistory(undoFirst.historyToken!);
      expect(redoneFirst.revision, 6);
      expect(await document.readSource(), 'A beta 🌍\n');

      final redoneSecond = await document.replayHistory(
        undoSecond.historyToken!,
      );
      expect(redoneSecond.revision, 7);
      expect(await document.readSource(), 'A BETA! 🌍\n');

      await document.releaseHistory(redoneSecond.historyToken!);
      await document.releaseHistory(redoneFirst.historyToken!);
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'native history tokens replay three adjacent insertions',
    () async {
      final document = await FlarkCoreDocument.open(
        'start\n',
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);

      final first = await document.applyEditUtf16(0, 0, 'a');
      final second = await document.applyEditUtf16(1, 1, 'b');
      final third = await document.applyEditUtf16(2, 2, 'c');

      final undoThird = await document.replayHistory(third.historyToken!);
      final undoSecond = await document.replayHistory(second.historyToken!);
      final undoFirst = await document.replayHistory(first.historyToken!);
      expect(await document.readSource(), 'start\n');

      final redoFirst = await document.replayHistory(undoFirst.historyToken!);
      final redoSecond = await document.replayHistory(undoSecond.historyToken!);
      final redoThird = await document.replayHistory(undoThird.historyToken!);
      expect(await document.readSource(), 'abcstart\n');

      await document.releaseHistory(redoThird.historyToken!);
      await document.releaseHistory(redoSecond.historyToken!);
      await document.releaseHistory(redoFirst.historyToken!);
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'bulk edits atomically paste 32 KiB and undo a large deletion',
    () async {
      const source = 'before\nafter\n';
      final paste = List.filled(32 * 1024, 'p').join();
      final pasted = source.replaceRange(7, 7, paste);
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);

      final pasteReceipt = await document.applyEditUtf16(7, 7, paste);
      expect(pasteReceipt.revision, 2);
      expect(
        pasteReceipt.historyDisposition,
        FlarkCoreHistoryDisposition.retained,
      );
      expect(await document.readSource(), pasted);

      final undonePaste = await document.replayHistory(
        pasteReceipt.historyToken!,
      );
      expect(await document.readSource(), source);
      final redonePaste = await document.replayHistory(
        undonePaste.historyToken!,
      );
      expect(await document.readSource(), pasted);

      final deleted = pasted.replaceRange(7, 7 + 8 * 1024, '');
      final deleteReceipt = await document.applyEditUtf16(7, 7 + 8 * 1024, '');
      expect(deleteReceipt.revision, 5);
      expect(await document.readSource(), deleted);
      final undoneDelete = await document.replayHistory(
        deleteReceipt.historyToken!,
      );
      expect(await document.readSource(), pasted);

      await document.releaseHistory(redonePaste.historyToken!);
      await document.releaseHistory(undoneDelete.historyToken!);
    },
    skip: libraryPath == null,
  );

  test(
    'persistent Dart actor opens, edits, pumps, and queries the Rust runtime',
    () async {
      const source = '# Flark\n\nA quick paragraph. 🌍\n\n- one\n- two\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);

      await document.pumpUntilReady();
      final initial = await document.queryViewport(maxRows: 1);
      expect(initial.isCertified, isTrue);
      expect(initial.rows, isNotEmpty);
      expect(initial.rows.first.kind, 12);
      expect(initial.rows.first.headingLevel, 1);
      expect(initial.rows.first.headingStyle, FlarkHeadingStyle.atx);
      expect(initial.continuation, isNonZero);
      final next = await document.queryViewportNext(initial, maxRows: 1);
      expect(next.requestedBytes.start, initial.requestedBytes.start);
      expect(next.requestedBytes.end, initial.requestedBytes.end);
      expect(next.coveredBytes.start, initial.coveredBytes.end);
      expect(next.coveredBytes.end, greaterThan(next.coveredBytes.start));
      await document.releaseViewportContinuation(next);
      final invalidatedByEdit = await document.queryViewport(maxRows: 1);

      final quick = source.indexOf('quick');
      final edit = await document.applyEditUtf16(
        quick,
        quick + 'quick'.length,
        'fast',
      );
      expect(edit.revision, 2);
      await document.releaseViewportContinuation(invalidatedByEdit);

      final pending = await document.queryViewport();
      expect(pending.isCertified, isFalse);
      expect(pending.neutralSource, contains('A fast paragraph. 🌍'));

      await document.pumpUntilReady();
      final world = (await document.readSource()).indexOf('🌍');
      await document.applyEditUtf16(world, world + '🌍'.length, 'world');
      await document.pumpUntilReady();

      expect(await document.readSource(), contains('A fast paragraph. world'));
      final current = await document.queryViewport();
      expect(current.revision, 3);
      expect(current.isCertified, isTrue);
      expect(current.rows.first.sourceUtf16.start, 0);

      await document.dispose();
      await expectLater(document.queryViewport(), throwsStateError);
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'live query decodes a current-source mixed certification partition',
    () async {
      final source = List<String>.generate(
        240,
        (index) =>
            'Paragraph ${index.toString().padLeft(3, '0')} has stable source '
            'for restart coverage.\n\n',
      ).join();
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final editStart = source.indexOf('Paragraph 120');
      await document.applyEditUtf16(
        editStart,
        editStart + 'Paragraph'.length,
        'Heading',
      );
      final currentSource = source.replaceRange(
        editStart,
        editStart + 'Paragraph'.length,
        'Heading',
      );
      final live = await document.queryViewport(endByte: editStart + 2048);

      expect(live.certification, FlarkCertification.mixedCurrent);
      expect(
        live.certificationRanges.any((range) => range.isCertified),
        isTrue,
      );
      expect(
        live.certificationRanges.any((range) => !range.isCertified),
        isTrue,
      );
      expect(live.neutralSource, currentSource.substring(0, editStart + 2048));
    },
    skip: libraryPath == null,
  );

  test(
    'list marker facts and exact prefix geometry cross the Dart boundary',
    () async {
      const source = '- alpha\n9) beta\n42) \n- [ ] todo\n- [X] done\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final viewport = await document.queryViewport();
      expect(viewport.coveredUtf16.start, 0);
      expect(viewport.coveredUtf16.end, source.length);
      final items = viewport.rows
          .map((row) => row.listItem)
          .whereType<FlarkListItemPresentation>()
          .toList();
      expect(items, hasLength(5));
      expect(items[0].markerStyle, FlarkListMarkerStyle.hyphen);
      expect(items[0].prefixUtf16.start, 0);
      expect(items[0].prefixUtf16.end, 2);
      expect(items[0].simpleContinuation, isTrue);
      expect(items[0].startsList, isTrue);
      expect(items[1].markerStyle, FlarkListMarkerStyle.orderedParenthesis);
      expect(items[1].markerValue, 9);
      expect(items[2].markerValue, 42);
      expect(items[2].startsList, isFalse);
      expect(items[3].taskChecked, isFalse);
      expect(items[4].taskChecked, isTrue);
      expect(items[4].prefixUtf16.end, source.indexOf('done'));
    },
    skip: libraryPath == null,
  );

  test(
    'block structure facts cross the Dart boundary without source inference',
    () async {
      const source = '> quote\n\n```dart\ncode\n```\n\n    indented\n\n---\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final viewport = await document.queryViewport();
      final quote = viewport.rows.firstWhere((row) => row.blockQuote != null);
      expect(quote.blockQuote?.prefixUtf16.start, 0);
      expect(quote.blockQuote?.prefixUtf16.end, 2);
      expect(quote.blockQuote?.nestingDepth, 1);
      expect(quote.blockQuote?.simpleContinuation, isTrue);

      final fenced = viewport.rows.firstWhere(
        (row) => row.codeBlock?.style == FlarkCodeBlockStyle.fencedBacktick,
      );
      expect(fenced.codeBlock?.minimumClosingLength, 3);
      expect(fenced.codeBlock?.closed, isTrue);
      expect(
        source.substring(
          fenced.editableUtf16!.start,
          fenced.editableUtf16!.end,
        ),
        'code\n',
      );

      final indented = viewport.rows.firstWhere(
        (row) => row.codeBlock?.style == FlarkCodeBlockStyle.indented,
      );
      expect(
        source.substring(
          indented.editableUtf16!.start,
          indented.editableUtf16!.end,
        ),
        'indented\n',
      );
      expect(viewport.rows.any((row) => row.thematicBreak), isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'inline facts cross with exact marker and visible-content geometry',
    () async {
      const source =
          '*em🌍* **strong** `code` [link](https://example.com) '
          '<https://a.test>\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final row = (await document.queryViewport()).rows.single;
      final facts = row.inlineFacts!;
      expect(facts.map((fact) => fact.kind), [
        FlarkInlineFactKind.emphasis,
        FlarkInlineFactKind.strong,
        FlarkInlineFactKind.code,
        FlarkInlineFactKind.directLink,
        FlarkInlineFactKind.autolinkUri,
      ]);
      expect(
        facts.map(
          (fact) =>
              source.substring(fact.sourceUtf16.start, fact.sourceUtf16.end),
        ),
        [
          '*em🌍*',
          '**strong**',
          '`code`',
          '[link](https://example.com)',
          '<https://a.test>',
        ],
      );
      expect(
        facts.map(
          (fact) =>
              source.substring(fact.contentUtf16.start, fact.contentUtf16.end),
        ),
        ['em🌍', 'strong', 'code', 'link', 'https://a.test'],
      );

      await document.applyEditUtf16(0, source.length, '*x &amp; y*\n');
      await document.pumpUntilReady();
      final transformed =
          (await document.queryViewport()).rows.single.inlineFacts!;
      expect(transformed.map((fact) => fact.kind), [
        FlarkInlineFactKind.emphasis,
        FlarkInlineFactKind.replacement,
      ]);
      expect(transformed.last.replacement, '&');
    },
    skip: libraryPath == null,
  );
}
