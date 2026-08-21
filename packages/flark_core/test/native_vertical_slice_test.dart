import 'dart:convert';
import 'dart:io';

import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'malformed host UTF-16 is rejected before an edit mutates the document',
    () async {
      final document = await FlarkCoreDocument.open(
        'alpha\n',
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      final malformed = String.fromCharCode(0xd800);

      await expectLater(
        document.applyEditUtf16(0, 0, malformed),
        throwsA(
          isA<FlarkCoreNativeException>()
              .having((error) => error.status, 'status', 0x020b)
              .having((error) => error.detail, 'invalid UTF-16 offset', 0),
        ),
      );
      expect(document.revision, 1);
      expect(await document.readSource(), 'alpha\n');
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

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
    'plain heading projection edit cell crosses the Dart worker boundary',
    () async {
      const source = '# Test is here\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final row = (await document.queryViewport()).rows.single;
      expect(row.kind, 12);
      expect(row.literalSafeEnvelopes, isEmpty);
      expect(
        row.projectionEditCells.map(
          (cell) => (
            cell.matcher,
            cell.affectedBytes.start,
            cell.affectedBytes.end,
            cell.affectedUtf16.start,
            cell.affectedUtf16.end,
            cell.triggerUtf16.start,
            cell.triggerUtf16.end,
            cell.retainBlockShell,
            cell.retainOutsideClosure,
            cell.presentClosureExact,
            cell.chainResultCell,
          ),
        ),
        [
          (
            FlarkProjectionEditMatcher.anyNoCrLfSplice,
            2,
            14,
            2,
            14,
            2,
            14,
            true,
            false,
            true,
            true,
          ),
        ],
      );
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'mixed heading dependency cell crosses the Dart worker boundary',
    () async {
      const source = '# **left** middle _right_\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final row = (await document.queryViewport()).rows.single;
      expect(row.inlineFacts?.map((fact) => fact.kind), [
        FlarkInlineFactKind.strong,
        FlarkInlineFactKind.emphasis,
      ]);
      expect(row.projectionEditCells, hasLength(1));
      final cell = row.projectionEditCells.single;
      expect(
        cell.matcher,
        FlarkProjectionEditMatcher.insertSingleAsciiSpaceAtPoint,
      );
      expect((cell.affectedUtf16.start, cell.affectedUtf16.end), (2, 10));
      expect((cell.triggerUtf16.start, cell.triggerUtf16.end), (4, 4));
      expect(cell.retainBlockShell, isTrue);
      expect(cell.retainOutsideClosure, isTrue);
      expect(cell.presentClosureExact, isTrue);
      expect(cell.chainResultCell, isFalse);
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'nonzero multibyte viewport rows use global UTF-16 coordinates',
    () async {
      const initial = 'Head.\n\nTrailing.\n';
      final document = await FlarkCoreDocument.open(
        initial,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final inserted = StringBuffer();
      for (var chunk = 0; chunk < 5; chunk += 1) {
        final paragraphs = List<String>.generate(
          9,
          (index) => '${'😀' * 100} $chunk-$index',
        ).join('\n\n');
        final structuredTail = chunk == 4
            ? 'Active.\n\n'
                  'alpha123\n\n'
                  '> **bold** and `code`\n'
                  '> tail\n\n'
                  '- [x] task with *emphasis*\n\n'
                  '| α | β |\n'
                  '| :--- | ---: |\n'
                  '| *x* | [y](https://example.test) |\n'
            : '';
        final addition = '$paragraphs\n\n$structuredTail';
        await document.applyEditUtf16(
          5 + inserted.length,
          5 + inserted.length,
          addition,
        );
        inserted.write(addition);
      }
      await document.pumpUntilReady();

      final source = 'Head.${inserted.toString()}\n\nTrailing.\n';
      final activeUtf16 = source.indexOf('Active.');
      final sourceBytes = utf8.encode(source);
      final activeByte = utf8.encode(source.substring(0, activeUtf16)).length;
      FlarkSourceRange utf16RangeForBytes(FlarkSourceRange bytes) =>
          FlarkSourceRange(
            utf8.decode(sourceBytes.sublist(0, bytes.start)).length,
            utf8.decode(sourceBytes.sublist(0, bytes.end)).length,
          );
      void expectGlobalRange(
        FlarkSourceRange bytes,
        FlarkSourceRange utf16,
        String label,
      ) {
        final expected = utf16RangeForBytes(bytes);
        expect(
          (utf16.start, utf16.end),
          (expected.start, expected.end),
          reason: '$label must use document-global UTF-16 coordinates',
        );
      }

      const unsnappedEnd = 16384;
      expect(
        sourceBytes[unsnappedEnd] & 0xc0,
        0x80,
        reason: 'fixture byte cap must split a multibyte scalar',
      );
      var alignedEnd = unsnappedEnd;
      while (sourceBytes[alignedEnd] & 0xc0 == 0x80) {
        alignedEnd -= 1;
      }
      final boundedViewport = await document.queryViewport(
        endByte: unsnappedEnd,
        maxRows: 32,
      );
      expect(
        (
          boundedViewport.requestedBytes.start,
          boundedViewport.requestedBytes.end,
        ),
        (0, alignedEnd),
      );
      expectGlobalRange(
        boundedViewport.coveredBytes,
        boundedViewport.coveredUtf16,
        'scalar-aligned covered range',
      );

      final viewport = await document.queryViewport(
        startByte: activeByte,
        maxRows: 32,
      );
      expect(viewport.isCertified, isTrue);
      expect(viewport.coveredUtf16.start, activeUtf16);
      expectGlobalRange(
        viewport.coveredBytes,
        viewport.coveredUtf16,
        'covered range',
      );

      var sawInlineFact = false;
      var sawLiteralSafeEnvelope = false;
      var sawListPrefix = false;
      var sawQuotePrefix = false;
      var sawProjectionSegment = false;
      var sawTableCell = false;
      for (final row in viewport.rows) {
        expectGlobalRange(row.sourceBytes, row.sourceUtf16, 'row source');
        if (row.editableBytes case final editableBytes?) {
          expectGlobalRange(
            editableBytes,
            row.editableUtf16!,
            'row editable range',
          );
        } else {
          expect(row.editableUtf16, isNull);
        }
        if (row.listItem case final listItem?) {
          expectGlobalRange(
            listItem.prefixBytes,
            listItem.prefixUtf16,
            'list prefix',
          );
          sawListPrefix = true;
        }
        if (row.blockQuote case final blockQuote?) {
          expectGlobalRange(
            blockQuote.prefixBytes,
            blockQuote.prefixUtf16,
            'blockquote prefix',
          );
          sawQuotePrefix = true;
        }
        for (final fact in row.inlineFacts ?? const <FlarkInlineFact>[]) {
          expectGlobalRange(fact.sourceBytes, fact.sourceUtf16, 'inline fact');
          expectGlobalRange(
            fact.contentBytes,
            fact.contentUtf16,
            'inline fact content',
          );
          sawInlineFact = true;
        }
        for (final envelope in row.literalSafeEnvelopes) {
          expectGlobalRange(
            envelope.sourceBytes,
            envelope.sourceUtf16,
            'literal-safe envelope',
          );
          sawLiteralSafeEnvelope = true;
        }
        for (final segment
            in row.projectionSegments ?? const <FlarkProjectionSegment>[]) {
          expectGlobalRange(
            segment.sourceBytes,
            segment.sourceUtf16,
            'projection segment',
          );
          sawProjectionSegment = true;
        }
        for (final tableRow
            in row.table?.rows ?? const <List<FlarkTableCellPresentation>>[]) {
          for (final cell in tableRow) {
            expectGlobalRange(cell.sourceBytes, cell.sourceUtf16, 'table cell');
            expectGlobalRange(
              cell.contentBytes,
              cell.contentUtf16,
              'table cell content',
            );
            sawTableCell = true;
          }
        }
      }
      expect(sawInlineFact, isTrue);
      expect(sawLiteralSafeEnvelope, isTrue);
      expect(sawListPrefix, isTrue);
      expect(sawQuotePrefix, isTrue);
      expect(sawProjectionSegment, isTrue);
      expect(sawTableCell, isTrue);
    },
    skip: libraryPath == null,
  );

  test(
    'projected multiline quote segments survive the native worker boundary',
    () async {
      const source = '> first\n> second\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final viewport = await document.queryViewport(maxRows: 1);
      expect(viewport.rows, hasLength(1));
      final row = viewport.rows.single;
      expect(
        row.editCapability,
        FlarkViewportRowEditCapability.projectedReserved,
      );
      expect((row.editableUtf16?.start, row.editableUtf16?.end), (2, 16));
      expect(
        row.projectionSegments
            ?.map(
              (segment) => (segment.sourceUtf16.start, segment.sourceUtf16.end),
            )
            .toList(growable: false),
        const [(2, 8), (10, 16)],
      );
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );

  test(
    'empty multiline quote row survives the native worker boundary',
    () async {
      const source = '> first\n> \n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final viewport = await document.queryViewport(maxRows: 8);
      expect(viewport.rows, hasLength(2));
      final row = viewport.rows.last;
      expect(row.kind, 15);
      expect((row.sourceUtf16.start, row.sourceUtf16.end), (11, 11));
      expect((row.editableUtf16?.start, row.editableUtf16?.end), (10, 10));
      expect(
        (row.blockQuote?.prefixUtf16.start, row.blockQuote?.prefixUtf16.end),
        (8, 10),
      );
      expect(row.blockQuote?.simpleContinuation, isTrue);
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
      expect(
        row.literalSafeEnvelopes.map((envelope) => envelope.editClass.name),
        ['asciiWordInsertion', 'asciiWordInsertion', 'asciiWordInsertion'],
      );
      expect(
        row.literalSafeEnvelopes.map(
          (envelope) => source.substring(
            envelope.sourceUtf16.start,
            envelope.sourceUtf16.end,
          ),
        ),
        ['strong', 'code', 'link'],
      );
      final emojiOffset = source.indexOf('🌍');
      expect(
        row.literalSafeEnvelopes.any(
          (envelope) =>
              envelope.sourceUtf16.start <= emojiOffset &&
              emojiOffset <= envelope.sourceUtf16.end,
        ),
        isFalse,
        reason: 'non-ASCII content must remain outside every word envelope',
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

  test(
    'bounded GFM table cells cross as typed Dart presentation',
    () async {
      const source =
          '| f\\|oo | bar |\n| :--- | ---: |\n| `x\\|y` | **baz** |\n';
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();

      final row = (await document.queryViewport()).rows.single;
      final table = row.table!;
      expect(table.rows, hasLength(2));
      expect(table.columnCount, 2);
      expect(table.rows.first.map((cell) => cell.alignment), [
        FlarkTableAlignment.left,
        FlarkTableAlignment.right,
      ]);
      expect(table.rows.first.every((cell) => cell.header), isTrue);
      expect(table.rows.last.every((cell) => !cell.header), isTrue);
      expect(
        source.substring(
          table.rows.first.first.contentUtf16.start,
          table.rows.first.first.contentUtf16.end,
        ),
        r'f\|oo',
      );
      expect(
        row.inlineFacts!.map((fact) => fact.kind),
        containsAll([
          FlarkInlineFactKind.code,
          FlarkInlineFactKind.strong,
          FlarkInlineFactKind.replacement,
        ]),
      );
      expect(
        row.inlineFacts!.any(
          (fact) => fact.kind == FlarkInlineFactKind.tableCell,
        ),
        isFalse,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'parser-cooked semantic targets resolve only from authoritative facts',
    () async {
      const source =
          '[direct](https://example.com "title") '
          '<me@example.com> www.example.com ![alt][img]\n\n'
          "[img]: /asset.png 'cap'\n";
      final document = await FlarkCoreDocument.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();
      final facts = (await document.queryViewport()).rows.first.inlineFacts!;

      Future<FlarkSemanticTarget> target(FlarkInlineFactKind kind) async =>
          (await document.querySemanticTarget(
            facts.firstWhere((fact) => fact.kind == kind),
          ))!;

      final direct = await target(FlarkInlineFactKind.directLink);
      expect(direct.kind, FlarkSemanticTargetKind.link);
      expect(direct.syntax, FlarkSemanticTargetSyntax.direct);
      expect(direct.destination, 'https://example.com');
      expect(direct.title, 'title');

      final email = await target(FlarkInlineFactKind.autolinkEmail);
      expect(email.destination, 'mailto:me@example.com');
      final www = await target(FlarkInlineFactKind.autolinkUri);
      expect(www.destination, 'http://www.example.com');

      final image = await target(FlarkInlineFactKind.referenceImage);
      expect(image.kind, FlarkSemanticTargetKind.image);
      expect(image.syntax, FlarkSemanticTargetSyntax.reference);
      expect(image.destination, '/asset.png');
      expect(image.title, 'cap');
      expect(
        source.substring(
          image.destinationSourceUtf16.start,
          image.destinationSourceUtf16.end,
        ),
        '/asset.png',
      );

      final strong = facts.firstWhere(
        (fact) => fact.kind == FlarkInlineFactKind.directLink,
      );
      final forged = FlarkInlineFact(
        kind: FlarkInlineFactKind.strong,
        flags: strong.flags,
        sourceBytes: FlarkSourceRange(
          strong.sourceBytes.start + 1,
          strong.sourceBytes.end,
        ),
        sourceUtf16: FlarkSourceRange(
          strong.sourceUtf16.start + 1,
          strong.sourceUtf16.end,
        ),
        contentBytes: strong.contentBytes,
        contentUtf16: strong.contentUtf16,
      );
      expect(await document.querySemanticTarget(forged), isNull);

      final edit = await document.applyEditUtf16(0, 0, 'x');
      expect(document.isReady, isFalse);
      expect(await document.querySemanticTarget(strong), isNull);
      if (edit.historyToken case final token?) {
        await document.releaseHistory(token);
      }
    },
    skip: libraryPath == null,
  );
}
