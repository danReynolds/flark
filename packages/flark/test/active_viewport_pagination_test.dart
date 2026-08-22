import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'close drains an in-flight viewport page before native disposal',
    () async {
      final source = List<String>.generate(
        33,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      await controller.continueParsing();

      var pageSettled = false;
      final paging = controller.nextViewportPage().whenComplete(
        () => pageSettled = true,
      );
      expect(pageSettled, isFalse);

      await controller.close();

      expect(pageSettled, isTrue);
      expect(await paging, isTrue);
      expect(controller.status, FlarkEditorStatus.disposed);
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'an edit beyond the head page recertifies on the active viewport page',
    () async {
      final source = List<String>.generate(
        33,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      expect(controller.rows, hasLength(32));
      expect(controller.canPageForward, isTrue);
      expect(await controller.nextViewportPage(), isTrue);
      expect(controller.viewportPageIndex, 1);
      expect(controller.rows, hasLength(1));

      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.end);
      final before = controller.inputValue;
      final revisionBefore = controller.revision;
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
      await _settleReceipts(controller);

      expect(controller.revision, revisionBefore + 1);
      expect(controller.rows, hasLength(1));
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.debugProjectionContinuityActive, isFalse);
      expect(controller.surfaceRow(controller.rows.single).kind, 5);
      expect(controller.surfaceRow(controller.rows.single).text, contains('x'));
      expect(
        controller.viewport!.coveredUtf16.start,
        controller.visibleUtf16Start,
      );
      expect(
        controller.viewport!.coveredUtf16.end,
        controller.visibleUtf16Start + controller.visibleSource.length,
      );
      await controller.continueParsing();

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.viewportPageIndex, 1);
      expect(controller.rows, hasLength(1));
      expect(controller.surfaceRow(controller.rows.single).text, contains('x'));
      expect(controller.canPageBackward, isTrue);
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'rapid projected typing retains its row across the 16 KiB byte window',
    () async {
      const block = '## Section\n\nA quick paragraph with **bold text**.\n\n';
      final source = List<String>.filled(400, block).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final row = controller.rows.firstWhere(
        (candidate) =>
            candidate.inlineFacts?.any((fact) => fact.kind.name == 'strong') ??
            false,
      );
      final strong = row.inlineFacts!.firstWhere(
        (fact) => fact.kind.name == 'strong',
      );
      controller.activateRow(row, strong.contentUtf16.start + 2);
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.surfaceRow(row).active, isTrue);
      expect(
        row.literalSafeEnvelopes.isNotEmpty ||
            row.projectionEditCells.isNotEmpty,
        isTrue,
        reason:
            'literal envelopes=${row.literalSafeEnvelopes.length}, '
            'edit cells=${row.projectionEditCells.map((cell) => '${cell.matcher.name}:${cell.triggerUtf16.start}-${cell.triggerUtf16.end}').toList()}',
      );
      expect(
        row.literalSafeEnvelopes.any(
          (envelope) =>
              envelope.editClass.name == 'asciiWordInsertion' &&
              envelope.sourceUtf16.start <= strong.contentUtf16.start + 2 &&
              strong.contentUtf16.start + 2 <= envelope.sourceUtf16.end,
        ),
        isTrue,
        reason:
            'caret=${strong.contentUtf16.start + 2}; '
            'envelopes=${row.literalSafeEnvelopes.map((envelope) => '${envelope.editClass.name}:${envelope.sourceUtf16.start}-${envelope.sourceUtf16.end}').toList()}',
      );
      final lostGenerations = <String>[];
      void capture() {
        final surfaces = controller.rows.map(controller.surfaceRow).toList();
        final hasActiveProjection = surfaces.any(
          (surface) => surface.active && !surface.text.contains('**'),
        );
        if (!hasActiveProjection) {
          lostGenerations.add(
            'generation=${controller.sourceGeneration} '
            'rows=${controller.rows.length} '
            'continuity=${controller.debugProjectionContinuityActive} '
            'semantics=${controller.semanticsCurrent} '
            'active=${surfaces.where((surface) => surface.active).map((surface) => '${surface.kind}:${surface.text}').toList()}',
          );
        }
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      for (var index = 0; index < 140; index += 1) {
        final before = controller.inputValue;
        final offset = before.selection.extentOffset;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: index.isEven ? 'x' : 'y',
            insertionOffset: offset,
            selection: TextSelection.collapsed(offset: offset + 1),
            composing: TextRange.empty,
          ),
        ]);
        await Future<void>.delayed(Duration.zero);
      }
      await _settleReceipts(controller);

      expect(lostGenerations, isEmpty);
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'a Backspace crossing the active page start requeries from the head',
    () async {
      final source = List<String>.generate(
        33,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      expect(await controller.nextViewportPage(), isTrue);

      final row = controller.rows.single;
      controller.activateRow(row, row.editableUtf16!.start);
      controller.deleteBackward();
      await _settleReceipts(controller);
      await controller.continueParsing();

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.viewportPageIndex, 0);
      expect(controller.rows, hasLength(32));
      _expectRowsContained(controller);
      expect(controller.canPageBackward, isFalse);
      expect(
        await controller.readSource(),
        endsWith('Paragraph 31.Paragraph 32.\n\n'),
      );
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'a replacement crossing the active page start rejects its stale byte hint',
    () async {
      final source = List<String>.generate(
        33,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      expect(await controller.nextViewportPage(), isTrue);

      final pageStart = controller.viewport!.coveredUtf16.start;
      final selectionStart = pageStart - 2;
      final selectionEnd = pageStart + 2;
      await controller.selectOversizedRangeUtf16(selectionStart, selectionEnd);
      controller.replaceSelection('Z');
      await _settleReceipts(controller);
      await controller.continueParsing();

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.viewportPageIndex, 0);
      expect(controller.rows, hasLength(32));
      expect(
        await controller.readSource(),
        source.replaceRange(selectionStart, selectionEnd, 'Z'),
      );
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'an edit supersedes an in-flight page query without faulting',
    () async {
      final source = List<String>.generate(
        33,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final first = controller.rows.first;
      controller.activateRow(first, first.editableUtf16!.end);
      final before = controller.inputValue;
      final insertionOffset = before.selection.extentOffset;
      final globalInsertionOffset = controller.globalCaretOffset;
      final paging = controller.nextViewportPage();
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(offset: insertionOffset + 1),
          composing: TextRange.empty,
        ),
      ]);

      expect(await paging, isFalse);
      await _settleReceipts(controller);
      await controller.continueParsing();

      expect(controller.semanticsCurrent, isTrue);
      expect(
        await controller.readSource(),
        source.replaceRange(globalInsertionOffset, globalInsertionOffset, 'x'),
      );
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'a bounded active-page seek crosses more than four continuation pages',
    () async {
      const source = 'Head.\n\nTrailing.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final first = controller.rows.first;
      controller.activateRow(first, first.editableUtf16!.end);
      final before = controller.inputValue;
      final insertionOffset = before.selection.extentOffset;
      final insertedRows = List<String>.filled(161, '\nx\n').join();
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: insertedRows,
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(
            offset: insertionOffset + insertedRows.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await _settleReceipts(controller);
      final caret = controller.globalCaretOffset;
      await controller.continueParsing();

      expect(controller.semanticsCurrent, isTrue);
      expect(controller.viewportPageIndex, greaterThan(4));
      expect(controller.rows, isNotEmpty);
      expect(controller.visibleUtf16Start, lessThanOrEqualTo(caret));
      expect(
        controller.visibleUtf16Start + controller.visibleSource.length,
        greaterThanOrEqualTo(caret),
      );
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'navigation crosses a byte slice and a deep boundary edit recertifies',
    () async {
      final fixture = _deepPageBoundaryFixture();
      final controller = await FlarkEditorController.open(
        fixture.source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      expect(controller.viewport!.coveredBytes.end, 16 * 1024);
      expect(controller.viewport!.continuation, 0);
      expect(controller.canPageForward, isTrue);
      expect(await controller.nextViewportPage(), isTrue);
      final secondSliceStart = controller.viewport!.coveredBytes.start;
      expect(secondSliceStart, 16 * 1024);
      expect(controller.rows, hasLength(32));
      expect(await controller.nextViewportPage(), isTrue);
      final deepStart = controller.viewport!.coveredBytes.start;
      expect(deepStart, greaterThan(16 * 1024));

      final pageStart = controller.viewport!.coveredUtf16.start;
      final selectionStart = pageStart - 2;
      final selectionEnd = pageStart + 2;
      await controller.selectOversizedRangeUtf16(selectionStart, selectionEnd);
      controller.replaceSelection('Z');
      await _settleReceipts(controller);
      await controller.continueParsing();
      expect(
        controller.semanticsCurrent,
        isTrue,
        reason: _viewportDiagnostics(
          controller,
          controller.globalCaretOffset,
          needle: 'Z',
        ),
      );
      _expectRowsContained(controller);
      expect(
        await controller.readSource(),
        fixture.source.replaceRange(selectionStart, selectionEnd, 'Z'),
      );
      expect(controller.viewport!.coveredBytes.start, greaterThan(0));
      expect(controller.canPageBackward, isTrue);
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'a deep page-start semantic merge reanchors and can navigate backward',
    () async {
      final fixture = _deepPageBoundaryFixture();
      final controller = await FlarkEditorController.open(
        fixture.source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      expect(await controller.nextViewportPage(), isTrue);
      final sliceStart = controller.viewport!.coveredBytes.start;
      expect(sliceStart, 16 * 1024);
      expect(await controller.nextViewportPage(), isTrue);
      expect(controller.viewport!.coveredBytes.start, greaterThan(sliceStart));
      expect(controller.rows, hasLength(1));
      final quote = controller.rows.single;
      expect(quote.blockQuote, isNotNull);

      controller.activateRow(quote, quote.editableUtf16!.start);
      controller.deleteBackward();
      await _settleReceipts(controller);
      await controller.continueParsing();

      expect(
        controller.semanticsCurrent,
        isTrue,
        reason: _viewportDiagnostics(
          controller,
          controller.globalCaretOffset,
          needle: 'Quote',
        ),
      );
      _expectRowsContained(controller);
      for (final row in controller.rows) {
        controller.surfaceRow(row);
      }
      expect(await controller.readSource(), fixture.withoutQuotePrefix);
      final recertifiedStart = controller.viewport!.coveredBytes.start;
      expect(recertifiedStart, greaterThan(0));
      expect(controller.canPageBackward, isTrue);
      expect(await controller.previousViewportPage(), isTrue);
      expect(
        controller.viewport!.coveredBytes.start,
        lessThan(recertifiedStart),
      );
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'a multibyte active-page seek starts from exact retained source',
    () async {
      const source = 'Head.\n\nTrailing.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final first = controller.rows.first;
      controller.activateRow(first, first.editableUtf16!.end);
      final inserted = StringBuffer();
      for (var chunk = 0; chunk < 5; chunk += 1) {
        final paragraphs = List<String>.generate(
          9,
          (index) => '${'😀' * 100} $chunk-$index',
        ).join('\n\n');
        final addition = '$paragraphs\n\n${chunk == 4 ? 'Active.' : ''}';
        inserted.write(addition);
        final before = controller.inputValue;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: addition,
            insertionOffset: before.selection.extentOffset,
            selection: TextSelection.collapsed(
              offset: before.selection.extentOffset + addition.length,
            ),
            composing: TextRange.empty,
          ),
        ]);
      }
      await _settleReceipts(controller);
      final expected = 'Head.${inserted.toString()}\n\nTrailing.\n';
      final caret = controller.globalCaretOffset;
      expect(utf8.encode(expected).length, greaterThan(16 * 1024));

      await controller.continueParsing();

      expect(
        controller.semanticsCurrent,
        isTrue,
        reason: _viewportDiagnostics(controller, caret, needle: 'Active.'),
      );
      final directStart = controller.viewport!.coveredBytes.start;
      expect(directStart, greaterThan(0));
      expect(controller.viewportPageIndex, greaterThan(0));
      expect(controller.canPageBackward, isTrue);
      expect(controller.rows, isNotEmpty);
      expect(
        controller.rows.any(
          (row) => controller.surfaceRow(row).text.contains('Active.'),
        ),
        isTrue,
      );
      expect(controller.visibleUtf16Start, lessThanOrEqualTo(caret));
      expect(
        controller.visibleUtf16Start + controller.visibleSource.length,
        greaterThanOrEqualTo(caret),
      );
      expect(await controller.readSource(), expected);
      expect(controller.lastError, isNull);

      expect(await controller.previousViewportPage(), isTrue);
      expect(controller.viewport!.coveredBytes.start, lessThan(directStart));
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );

  test(
    'an oversized multibyte multiline block remains exact when it cannot fit',
    () async {
      const source = 'Head.\n\nTrailing.\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final first = controller.rows.first;
      controller.activateRow(first, first.editableUtf16!.end);
      final inserted = StringBuffer();
      for (var chunk = 0; chunk < 5; chunk += 1) {
        final lines = List<String>.filled(9, '😀' * 100).join('\n');
        final addition = '\n$lines${chunk == 4 ? '\ntail' : ''}';
        inserted.write(addition);
        final before = controller.inputValue;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: addition,
            insertionOffset: before.selection.extentOffset,
            selection: TextSelection.collapsed(
              offset: before.selection.extentOffset + addition.length,
            ),
            composing: TextRange.empty,
          ),
        ]);
      }
      await _settleReceipts(controller);
      final expected = 'Head.${inserted.toString()}\n\nTrailing.\n';
      final caret = controller.globalCaretOffset;
      expect(utf8.encode(expected).length, greaterThan(16 * 1024));

      await controller.continueParsing();

      expect(controller.semanticsCurrent, isFalse);
      expect(
        controller.rows,
        isEmpty,
        reason: _viewportDiagnostics(controller, caret, needle: 'tail'),
      );
      expect(controller.viewportPageIndex, 0);
      expect(controller.canPageBackward, isFalse);
      expect(controller.canPageForward, isFalse);
      expect(controller.globalCaretOffset, caret);
      expect(controller.inputValue.text, contains('tail'));
      expect(await controller.readSource(), expected);
      expect(controller.lastError, isNull);
    },
    skip: libraryPath == null,
  );
}

({String source, String withoutQuotePrefix}) _deepPageBoundaryFixture() {
  final prefix = List<String>.filled(32, '${'p' * 510}\n\n').join();
  final headings = List<String>.generate(
    31,
    (index) => '# Heading $index\n\n',
  ).join();
  return (
    source: '$prefix${headings}Paragraph.\n> Quote\n',
    withoutQuotePrefix: '$prefix${headings}Paragraph.\nQuote\n',
  );
}

void _expectRowsContained(FlarkEditorController controller) {
  final viewport = controller.viewport!;
  final visibleEnd =
      controller.visibleUtf16Start + controller.visibleSource.length;
  for (final row in controller.rows) {
    expect(
      row.sourceBytes.start,
      greaterThanOrEqualTo(viewport.coveredBytes.start),
    );
    expect(row.sourceBytes.end, lessThanOrEqualTo(viewport.coveredBytes.end));
    expect(
      row.sourceUtf16.start,
      greaterThanOrEqualTo(controller.visibleUtf16Start),
    );
    expect(row.sourceUtf16.end, lessThanOrEqualTo(visibleEnd));
  }
}

String _viewportDiagnostics(
  FlarkEditorController controller,
  int caret, {
  required String needle,
}) {
  final viewport = controller.viewport;
  final ranges = viewport?.certificationRanges
      .map(
        (range) =>
            '${range.isCertified}:${range.sourceUtf16.start}-'
            '${range.sourceUtf16.end}',
      )
      .join(',');
  final rows = controller.rows
      .map((row) {
        final surface = controller.surfaceRow(row);
        return '${row.kind}:${row.sourceUtf16.start}-${row.sourceUtf16.end}/'
            'surface=${surface.kind}:${surface.globalUtf16Start}:'
            '${surface.text.length}:needle=${surface.text.contains(needle)}';
      })
      .join(',');
  return 'viewport=${viewport?.isCertified} bytes='
      '${viewport?.coveredBytes.start}:${viewport?.coveredBytes.end} utf16='
      '${viewport?.coveredUtf16.start}:${viewport?.coveredUtf16.end} '
      'ranges=$ranges rows=$rows visible=${controller.visibleUtf16Start}:'
      '${controller.visibleSource.length} caret=$caret';
}

Future<void> _settleReceipts(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while ((controller.pendingEdits != 0 ||
          controller.viewport?.revision != controller.revision) &&
      DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  expect(controller.pendingEdits, 0);
  expect(controller.viewport?.revision, controller.revision);
  expect(controller.lastError, isNull);
}
