import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3InlineFactKind,
        FlarkV3InlineFactsDisposition,
        FlarkV3InlineMarkerPolicy,
        FlarkV3InlineProjection,
        FlarkV3LeafProjectionDemandDisposition,
        FlarkV3SourceDocument,
        FlarkV3SourceProjectionAffinity,
        FlarkV3SourceProjectionPieceKind;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

const _openTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);
const _largeDocumentTimeout = Duration(seconds: 60);

typedef _ExpectedSpan = ({
  int startUtf8,
  int endUtf8,
  int startUtf16,
  int endUtf16,
});

typedef _SemanticFixture = ({
  String name,
  String markdown,
  int initialRevision,
  int queryPositionUtf16,
  _ExpectedSpan source,
  _ExpectedSpan projectedSource,
});

const _fixtures = <_SemanticFixture>[
  (
    name: 'empty document',
    markdown: '',
    initialRevision: 0,
    queryPositionUtf16: 0,
    source: (startUtf8: 0, endUtf8: 0, startUtf16: 0, endUtf16: 0),
    projectedSource: (startUtf8: 0, endUtf8: 0, startUtf16: 0, endUtf16: 0),
  ),
  (
    name: 'paragraph after a leading reference definition',
    markdown: '[x]: /target\nCafé 😀 [x]\n',
    initialRevision: 1,
    queryPositionUtf16: 13,
    source: (startUtf8: 0, endUtf8: 28, startUtf16: 0, endUtf16: 25),
    projectedSource: (startUtf8: 13, endUtf8: 28, startUtf16: 13, endUtf16: 25),
  ),
];

void main() {
  group('public runtime native/Web semantic parity', () {
    for (final fixture in _fixtures) {
      test(fixture.name, () async {
        final runtime = await openFlarkV3PublicRuntimeForTest(
          fixture.markdown,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          final current = await _awaitCurrent(
            runtime,
            revision: fixture.initialRevision,
          );
          _expectCurrentStatus(current, revision: fixture.initialRevision);
          expect(runtime.sourceRevision, fixture.initialRevision);
          expect(runtime.sourceLengthUtf16, fixture.source.endUtf16);
          expect(runtime.exportMarkdown(), fixture.markdown);

          final result = runtime.queryAtUtf16(fixture.queryPositionUtf16);
          if (fixture.markdown.isEmpty) {
            expect(result, isA<FlarkV3DocumentSourceGapQuery>());
            final gap = result as FlarkV3DocumentSourceGapQuery;
            expect(gap.sourceRevision, 0);
            expect(gap.structureRevision, 0);
            expect(gap.reason, FlarkV3DocumentQueryGapReason.unavailableFacts);
            _expectSpan(gap.range, fixture.source);

            final range = runtime.queryBlockRange(0, 0);
            expect(range, isA<FlarkV3DocumentSourceGapBlockRange>());
            final empty = range as FlarkV3DocumentSourceGapBlockRange;
            expect(empty.sourceRevision, 0);
            expect(empty.structureRevision, 0);
            expect(
              empty.reason,
              FlarkV3DocumentQueryGapReason.undecodableClosure,
            );
            _expectSpan(empty.requestedSource, fixture.source);
          } else {
            expect(result, isA<FlarkV3RecursiveGreenPointQuery>());
            final point = result as FlarkV3RecursiveGreenPointQuery;
            expect(point.sourceRevision, fixture.initialRevision);
            expect(point.structureRevision, fixture.initialRevision);
            expect(point.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(point.ancestry.map((frame) => frame.kind), const [
              FlarkV3RecursiveGreenKind.document,
              FlarkV3RecursiveGreenKind.paragraph,
            ]);
            expect(point.inlineFacts, isNull);

            final range = runtime.queryBlockRange(
              point.pointUtf16,
              point.pointUtf16 + 1,
            );
            expect(range, isA<FlarkV3RecursiveGreenRowRange>());
            final rows = range as FlarkV3RecursiveGreenRowRange;
            expect(rows.sourceRevision, fixture.initialRevision);
            expect(rows.structureRevision, fixture.initialRevision);
            expect(rows.selectedRow, isNotNull);
            final row = rows.selectedRow!;
            expect(row.frameId, point.owner.frameId);
            expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(row.selected, isTrue);
            expect(row.inlineCapable, isTrue);
            expect(
              row.presentationKind,
              FlarkV3RecursiveGreenRowPresentationKind.inline,
            );
            expect(
              row.editCapability,
              FlarkV3RecursiveGreenRowEditCapability.contiguous,
            );
            _expectSpan(row.physicalSource, fixture.source);
            expect(row.editableSource, isNotNull);
            _expectSpan(row.editableSource!, (
              startUtf8: fixture.projectedSource.startUtf8,
              endUtf8: fixture.projectedSource.endUtf8 - 1,
              startUtf16: fixture.projectedSource.startUtf16,
              endUtf16: fixture.projectedSource.endUtf16 - 1,
            ));
            expect(row.path.map((frame) => frame.kind), const [
              FlarkV3RecursiveGreenKind.document,
              FlarkV3RecursiveGreenKind.paragraph,
            ]);
            expect(runtime.readSourceRange(0, 13), '[x]: /target\n');
          }

          final firstClose = runtime.close();
          final repeatedClose = runtime.close();
          expect(identical(repeatedClose, firstClose), isTrue);
          await firstClose.timeout(_closeTimeout);
          closed = true;
          _expectClosedStatus(
            runtime.status,
            revision: fixture.initialRevision,
          );
          expect(
            () => runtime.queryAtUtf16(fixture.queryPositionUtf16),
            throwsStateError,
          );
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      }, timeout: const Timeout(Duration(minutes: 1)));
    }

    test(
      'tight bullet list clean and live-edit paths converge on one exact terminal item',
      () async {
        const initial = 'prefix 😀\r\n\r\n  - α😀\r\n  - β';
        const target = 'prefix 😀\r\n\r\n  - α😀\r\n  - ';
        final editStart = initial.lastIndexOf('β');
        final liveRuntime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final cleanRuntime = await openFlarkV3PublicRuntimeForTest(
          target,
        ).timeout(_openTimeout);
        var liveClosed = false;
        var cleanClosed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(liveRuntime, revision: 1),
            revision: 1,
          );
          _expectCurrentStatus(
            await _awaitCurrent(cleanRuntime, revision: 1),
            revision: 1,
          );

          final receipt = liveRuntime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: '',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(
            liveRuntime.exportMarkdown(),
            target,
            reason:
                'canonical source must be exact before parser recertification',
          );
          _expectCurrentStatus(
            await _awaitCurrent(liveRuntime, revision: 2),
            revision: 2,
          );

          FlarkV3RecursiveGreenRenderableRow exactTerminalItem(
            FlarkV3DocumentRuntime runtime, {
            required int revision,
          }) {
            final query = runtime.queryAtUtf16(target.length);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
            final point = query as FlarkV3RecursiveGreenPointQuery;
            expect(point.sourceRevision, revision);
            expect(point.structureRevision, revision);
            expect(
              point.owner.kind,
              FlarkV3RecursiveGreenKind.terminalEmptyItem,
            );
            expect(point.inlineFacts, isNull);

            final range = runtime.queryBlockRange(
              target.length - 1,
              target.length,
            );
            expect(range, isA<FlarkV3RecursiveGreenRowRange>());
            final rows = range as FlarkV3RecursiveGreenRowRange;
            expect(rows.sourceRevision, revision);
            expect(rows.structureRevision, revision);
            expect(rows.selectedRow, isNotNull);
            final row = rows.selectedRow!;
            expect(row.frameId, point.owner.frameId);
            expect(
              point.ancestry.map((frame) => frame.kind),
              row.path.map((frame) => frame.kind),
            );
            return row;
          }

          void expectTerminalList(FlarkV3RecursiveGreenRenderableRow row) {
            expect(row.kind, FlarkV3RecursiveGreenKind.terminalEmptyItem);
            expect(row.selected, isTrue);
            expect(row.inlineCapable, isFalse);
            expect(
              row.presentationKind,
              FlarkV3RecursiveGreenRowPresentationKind.inline,
            );
            expect(
              row.editCapability,
              FlarkV3RecursiveGreenRowEditCapability.contiguous,
            );
            expect(row.editableSource, isNotNull);
            _expectSpan(row.editableSource!, (
              startUtf8: 31,
              endUtf8: 31,
              startUtf16: 26,
              endUtf16: 26,
            ));
            _expectSpan(row.presentationPhysicalSource, (
              startUtf8: 27,
              endUtf8: 31,
              startUtf16: 22,
              endUtf16: 26,
            ));
            expect(row.path.map((frame) => frame.kind), const [
              FlarkV3RecursiveGreenKind.document,
              FlarkV3RecursiveGreenKind.list,
              FlarkV3RecursiveGreenKind.item,
              FlarkV3RecursiveGreenKind.terminalEmptyItem,
            ]);
            final list =
                row.path
                        .singleWhere(
                          (frame) =>
                              frame.kind == FlarkV3RecursiveGreenKind.list,
                        )
                        .fact!
                    as FlarkV3RecursiveGreenListPathFact;
            expect(list.style, FlarkV3RecursiveGreenListStyle.bullet);
            expect(list.bulletMarker, FlarkV3BulletListMarker.hyphen);
            expect(list.tight, isTrue);
            expect(list.start, 1);
            final item =
                row.path
                        .singleWhere(
                          (frame) =>
                              frame.kind == FlarkV3RecursiveGreenKind.item,
                        )
                        .fact!
                    as FlarkV3RecursiveGreenItemPathFact;
            expect(item.markerOffset, 2);
            expect(item.padding, 2);
          }

          final live = exactTerminalItem(liveRuntime, revision: 2);
          final clean = exactTerminalItem(cleanRuntime, revision: 1);
          expectTerminalList(live);
          expectTerminalList(clean);
          expect(
            live.path.map((frame) => frame.kind),
            clean.path.map((frame) => frame.kind),
          );
          expect(liveRuntime.exportMarkdown(), cleanRuntime.exportMarkdown());

          await liveRuntime.close().timeout(_closeTimeout);
          liveClosed = true;
          await cleanRuntime.close().timeout(_closeTimeout);
          cleanClosed = true;
        } finally {
          if (!liveClosed) await liveRuntime.close().timeout(_closeTimeout);
          if (!cleanClosed) await cleanRuntime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'nested list topology and tightness recertify across a structural edit',
      () async {
        const initial = '* foo\n  * bar\n\n  baz\n';
        const edited = '* foo\n  * bar\n\n  * βaz\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );

          FlarkV3RecursiveGreenRenderableRow exactRowAt(int pointUtf16) {
            final point = runtime.queryAtUtf16(pointUtf16);
            expect(point, isA<FlarkV3RecursiveGreenPointQuery>());
            final green = point as FlarkV3RecursiveGreenPointQuery;
            expect(green.sourceRevision, runtime.sourceRevision);
            expect(green.structureRevision, runtime.sourceRevision);

            final range = runtime.queryBlockRange(pointUtf16, pointUtf16 + 1);
            expect(range, isA<FlarkV3RecursiveGreenRowRange>());
            final rows = range as FlarkV3RecursiveGreenRowRange;
            expect(rows.sourceRevision, runtime.sourceRevision);
            expect(rows.structureRevision, runtime.sourceRevision);
            expect(rows.selectedRow, isNotNull);
            final row = rows.selectedRow!;
            expect(
              green.ancestry.map((frame) => frame.kind),
              row.path.map((frame) => frame.kind),
            );
            return row;
          }

          void expectNestedParagraph(
            FlarkV3RecursiveGreenRenderableRow row, {
            required List<bool> listTightness,
          }) {
            expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(
              row.editCapability,
              FlarkV3RecursiveGreenRowEditCapability.contiguous,
            );
            expect(row.path.map((frame) => frame.kind), const [
              FlarkV3RecursiveGreenKind.document,
              FlarkV3RecursiveGreenKind.list,
              FlarkV3RecursiveGreenKind.item,
              FlarkV3RecursiveGreenKind.list,
              FlarkV3RecursiveGreenKind.item,
              FlarkV3RecursiveGreenKind.paragraph,
            ]);
            expect(
              row.path
                  .where(
                    (frame) => frame.kind == FlarkV3RecursiveGreenKind.list,
                  )
                  .map(
                    (frame) =>
                        (frame.fact as FlarkV3RecursiveGreenListPathFact).tight,
                  ),
              listTightness,
            );
          }

          expectNestedParagraph(
            exactRowAt(initial.indexOf('bar') + 1),
            listTightness: const [false, true],
          );
          final lazy = exactRowAt(initial.indexOf('baz') + 1);
          expect(lazy.path.map((frame) => frame.kind), const [
            FlarkV3RecursiveGreenKind.document,
            FlarkV3RecursiveGreenKind.list,
            FlarkV3RecursiveGreenKind.item,
            FlarkV3RecursiveGreenKind.paragraph,
          ]);

          final editStart = initial.indexOf('baz');
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 3,
                replacement: '* βaz',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(runtime.exportMarkdown(), edited);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );

          expectNestedParagraph(
            exactRowAt(edited.indexOf('β') + 1),
            listTightness: const [true, false],
          );
          expect(runtime.exportMarkdown(), edited);

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'expanded list topology stays exact while task syntax remains literal',
      () async {
        const fixtures = <String, String>{
          'loose': '- first\n\n- second\n',
          'task': '- [x] task\n',
          'nested': '- first\n  - nested\n',
          'multi-block item': '- first\n\n  second\n',
        };
        for (final MapEntry(key: name, value: markdown) in fixtures.entries) {
          final runtime = await openFlarkV3PublicRuntimeForTest(
            markdown,
          ).timeout(_openTimeout);
          var closed = false;
          try {
            _expectCurrentStatus(
              await _awaitCurrent(runtime, revision: 1),
              revision: 1,
            );
            final pointUtf16 = switch (name) {
              'loose' => markdown.indexOf('second') + 1,
              'nested' => markdown.indexOf('nested') + 1,
              'multi-block item' => markdown.indexOf('second') + 1,
              _ => markdown.indexOf('[x]') + 1,
            };
            final query = runtime.queryAtUtf16(pointUtf16);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>(), reason: name);
            final point = query as FlarkV3RecursiveGreenPointQuery;
            expect(point.sourceRevision, 1, reason: name);
            expect(point.structureRevision, 1, reason: name);
            expect(
              point.owner.kind,
              FlarkV3RecursiveGreenKind.paragraph,
              reason: name,
            );
            expect(
              point.inlineFacts,
              isNull,
              reason: '$name has no undemanded inline authority',
            );

            final range = runtime.queryBlockRange(pointUtf16, pointUtf16 + 1);
            expect(range, isA<FlarkV3RecursiveGreenRowRange>(), reason: name);
            final rows = range as FlarkV3RecursiveGreenRowRange;
            expect(rows.sourceRevision, 1, reason: name);
            expect(rows.structureRevision, 1, reason: name);
            expect(rows.selectedRow, isNotNull, reason: name);
            final row = rows.selectedRow!;
            expect(row.frameId, point.owner.frameId, reason: name);
            expect(row.kind, FlarkV3RecursiveGreenKind.paragraph, reason: name);
            expect(row.selected, isTrue, reason: name);
            expect(
              row.editCapability,
              FlarkV3RecursiveGreenRowEditCapability.contiguous,
              reason: name,
            );
            expect(row.path.map((frame) => frame.kind), switch (name) {
              'nested' => const [
                FlarkV3RecursiveGreenKind.document,
                FlarkV3RecursiveGreenKind.list,
                FlarkV3RecursiveGreenKind.item,
                FlarkV3RecursiveGreenKind.list,
                FlarkV3RecursiveGreenKind.item,
                FlarkV3RecursiveGreenKind.paragraph,
              ],
              _ => const [
                FlarkV3RecursiveGreenKind.document,
                FlarkV3RecursiveGreenKind.list,
                FlarkV3RecursiveGreenKind.item,
                FlarkV3RecursiveGreenKind.paragraph,
              ],
            }, reason: name);
            final expectedTightness = switch (name) {
              'loose' || 'multi-block item' => const [false],
              'nested' => const [true, true],
              _ => const [true],
            };
            expect(
              row.path
                  .where(
                    (frame) => frame.kind == FlarkV3RecursiveGreenKind.list,
                  )
                  .map(
                    (frame) =>
                        (frame.fact as FlarkV3RecursiveGreenListPathFact).tight,
                  ),
              expectedTightness,
              reason: name,
            );
            final editableSource = row.editableSource;
            expect(editableSource, isNotNull, reason: name);
            expect(
              runtime.readSourceRange(
                editableSource!.startUtf16,
                editableSource.endUtf16,
              ),
              switch (name) {
                'loose' => 'second',
                'nested' => 'nested',
                'multi-block item' => 'second',
                _ => '[x] task',
              },
              reason: name == 'task'
                  ? 'task-list extension syntax must remain literal content'
                  : name,
            );
            expect(runtime.exportMarkdown(), markdown, reason: name);
            expect(
              runtime.readSourceRange(0, markdown.length),
              markdown,
              reason: name,
            );
            await runtime.close().timeout(_closeTimeout);
            closed = true;
          } finally {
            if (!closed) await runtime.close().timeout(_closeTimeout);
          }
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'block quote joins recursive row and marker-free sidecar authority',
      () async {
        const markdown = '\uFEFF   > α😀\r\n> β\rlazy😀\u0000';
        const projectedMarkdown = 'α😀\r\nβ\rlazy😀\u0000';
        const source = (startUtf8: 0, endUtf8: 30, startUtf16: 0, endUtf16: 22);
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          leafProjectionDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          expect(runtime.exportMarkdown(), markdown);
          expect(runtime.sourceLengthUtf16, source.endUtf16);

          final initialQuery = lease.queryAtUtf16(6);
          expect(initialQuery, isA<FlarkV3RecursiveGreenPointQuery>());
          final initial = initialQuery as FlarkV3RecursiveGreenPointQuery;
          expect(initial.sourceRevision, 1);
          expect(initial.structureRevision, 1);
          expect(initial.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(initial.ancestry.map((ancestor) => ancestor.kind), const [
            FlarkV3RecursiveGreenKind.document,
            FlarkV3RecursiveGreenKind.blockQuote,
            FlarkV3RecursiveGreenKind.paragraph,
          ]);
          expect(initial.inlineFacts, isNull);
          expect(initial.blockQuoteProjection, isNull);
          expect(initial.projectedInlineFacts, isNull);

          final rowRange = runtime.queryBlockRange(6, 7);
          expect(rowRange, isA<FlarkV3RecursiveGreenRowRange>());
          final rows = rowRange as FlarkV3RecursiveGreenRowRange;
          expect(rows.sourceRevision, 1);
          expect(rows.structureRevision, 1);
          expect(rows.selectedRow, isNotNull);
          final row = rows.selectedRow!;
          expect(row.frameId, initial.owner.frameId);
          expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(row.selected, isTrue);
          expect(row.inlineCapable, isFalse);
          expect(
            row.presentationKind,
            FlarkV3RecursiveGreenRowPresentationKind.inline,
          );
          expect(
            row.editCapability,
            FlarkV3RecursiveGreenRowEditCapability.unavailable,
          );
          expect(row.editableSource, isNull);
          _expectSpan(row.physicalSource, (
            startUtf8: 8,
            endUtf8: 30,
            startUtf16: 6,
            endUtf16: 22,
          ));
          expect(
            row.path.map((frame) => frame.kind),
            initial.ancestry.map((frame) => frame.kind),
          );
          _expectSpan(
            row.path
                .singleWhere(
                  (frame) => frame.kind == FlarkV3RecursiveGreenKind.blockQuote,
                )
                .physicalSource,
            (startUtf8: 3, endUtf8: 30, startUtf16: 1, endUtf16: 22),
          );

          final beforePresentation =
              runtime.status.leafProjectionPresentationGeneration;
          final beforeOutcome =
              runtime.status.leafProjectionAttemptOutcomeGeneration;
          final settled = _awaitStatus(
            runtime,
            (status) =>
                status.leafProjectionPresentationGeneration >
                    beforePresentation ||
                status.leafProjectionAttemptOutcomeGeneration > beforeOutcome ||
                status.state == FlarkV3DocumentRuntimeState.faulted,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(6, query: initial),
            FlarkV3LeafProjectionDemandDisposition.scheduled,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(6, query: initial),
            FlarkV3LeafProjectionDemandDisposition.coalesced,
          );
          final settledStatus = await settled;
          expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
          expect(
            settledStatus.leafProjectionAttemptOutcomeGeneration,
            beforeOutcome + 1,
          );
          expect(
            settledStatus.leafProjectionPresentationGeneration,
            beforePresentation + 1,
          );

          final refinedQuery = lease.queryAtUtf16(6);
          expect(refinedQuery, isA<FlarkV3RecursiveGreenPointQuery>());
          final refined = refinedQuery as FlarkV3RecursiveGreenPointQuery;
          expect(refined.owner.frameId, initial.owner.frameId);
          final payload = refined.blockQuoteProjection;
          expect(
            payload,
            isNotNull,
            reason: 'the sidecar must carry the demanded quote-line recipe',
          );
          expect(refined.paragraphSource, isNotNull);
          _expectSpan(refined.paragraphSource!, source);
          expect(refined.inlineFacts, isNull);
          expect(refined.projectedInlineFacts, isNull);
          expect(payload!.projectedUtf8Length, 20);
          expect(payload.projectedUtf16Length, 14);
          expect(payload.sourceRevision, 1);
          expect(payload.sourceVersion.metric.bytes, source.endUtf8);
          expect(payload.sourceVersion.metric.utf16, source.endUtf16);
          _expectSpan(payload.source, source);

          expect(payload.records, hasLength(3));
          _expectBlockQuoteRecord(
            payload.records[0],
            relativeStartUtf8: 0,
            physical: (startUtf8: 0, endUtf8: 16, startUtf16: 0, endUtf16: 11),
            hidden: (startUtf8: 0, endUtf8: 8, startUtf16: 0, endUtf16: 6),
            content: (startUtf8: 8, endUtf8: 14, startUtf16: 6, endUtf16: 9),
            lineEnding: (
              startUtf8: 14,
              endUtf8: 16,
              startUtf16: 9,
              endUtf16: 11,
            ),
            kind: FlarkV3BlockQuoteLineProjectionKind.marked,
          );
          _expectBlockQuoteRecord(
            payload.records[1],
            relativeStartUtf8: 16,
            physical: (
              startUtf8: 16,
              endUtf8: 21,
              startUtf16: 11,
              endUtf16: 15,
            ),
            hidden: (startUtf8: 16, endUtf8: 18, startUtf16: 11, endUtf16: 13),
            content: (startUtf8: 18, endUtf8: 20, startUtf16: 13, endUtf16: 14),
            lineEnding: (
              startUtf8: 20,
              endUtf8: 21,
              startUtf16: 14,
              endUtf16: 15,
            ),
            kind: FlarkV3BlockQuoteLineProjectionKind.marked,
          );
          _expectBlockQuoteRecord(
            payload.records[2],
            relativeStartUtf8: 21,
            physical: (
              startUtf8: 21,
              endUtf8: 30,
              startUtf16: 15,
              endUtf16: 22,
            ),
            hidden: (startUtf8: 21, endUtf8: 21, startUtf16: 15, endUtf16: 15),
            content: (startUtf8: 21, endUtf8: 30, startUtf16: 15, endUtf16: 22),
            lineEnding: (
              startUtf8: 30,
              endUtf8: 30,
              startUtf16: 22,
              endUtf16: 22,
            ),
            kind: FlarkV3BlockQuoteLineProjectionKind.lazyContinuation,
          );

          final projection = payload.toSourceProjection();
          expect(projection.isCertified, isTrue);
          expect(
            projection.certifiedSourceVersion,
            same(payload.sourceVersion),
          );
          expect(projection.sourceText, markdown);
          expect(projection.displayText, projectedMarkdown);
          expect(projection.displayLengthUtf16, 14);
          expect(projection.pieces.map((piece) => piece.kind), const [
            FlarkV3SourceProjectionPieceKind.hide,
            FlarkV3SourceProjectionPieceKind.copy,
            FlarkV3SourceProjectionPieceKind.hide,
            FlarkV3SourceProjectionPieceKind.copy,
            FlarkV3SourceProjectionPieceKind.copy,
          ]);
          expect(projection.sourceToDisplayOffset(4), 0);
          expect(projection.sourceToDisplayOffset(12), 5);
          expect(
            projection.displayToSourceOffset(
              5,
              affinity: FlarkV3SourceProjectionAffinity.upstream,
            ),
            11,
          );
          expect(
            projection.displayToSourceOffset(
              5,
              affinity: FlarkV3SourceProjectionAffinity.downstream,
            ),
            13,
          );

          final beforeProjectedPresentation =
              runtime.status.leafProjectionPresentationGeneration;
          final beforeProjectedOutcome =
              runtime.status.leafProjectionAttemptOutcomeGeneration;
          final projectedSettled = _awaitStatus(
            runtime,
            (status) =>
                status.leafProjectionPresentationGeneration >
                    beforeProjectedPresentation ||
                status.leafProjectionAttemptOutcomeGeneration >
                    beforeProjectedOutcome ||
                status.state == FlarkV3DocumentRuntimeState.faulted,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(6, query: refined),
            FlarkV3LeafProjectionDemandDisposition.scheduled,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(6, query: refined),
            FlarkV3LeafProjectionDemandDisposition.coalesced,
          );
          final projectedStatus = await projectedSettled;
          expect(projectedStatus.state, FlarkV3DocumentRuntimeState.open);
          expect(
            projectedStatus.leafProjectionAttemptOutcomeGeneration,
            beforeProjectedOutcome + 1,
          );
          expect(
            projectedStatus.leafProjectionPresentationGeneration,
            beforeProjectedPresentation + 1,
          );

          final projectedQuery = lease.queryAtUtf16(6);
          expect(projectedQuery, isA<FlarkV3RecursiveGreenPointQuery>());
          final projected = projectedQuery as FlarkV3RecursiveGreenPointQuery;
          expect(projected.owner.frameId, initial.owner.frameId);
          expect(projected.blockQuoteProjection, isNotNull);
          expect(projected.inlineFacts, isNull);
          final projectedInline = projected.projectedInlineFacts;
          expect(projectedInline, isNotNull);
          expect(
            projectedInline!.disposition,
            FlarkV3ProjectedInlineFactsDisposition.authoritative,
          );
          expect(projectedInline.sourceRevision, 1);
          _expectSpan(projectedInline.physicalSource, source);
          expect(projectedInline.projectedSource.startUtf8, 0);
          expect(projectedInline.projectedSource.endUtf8, 20);
          expect(projectedInline.projectedSource.startUtf16, 0);
          expect(projectedInline.projectedSource.endUtf16, 14);
          expect(projectedInline.facts, isEmpty);
          expect(runtime.exportMarkdown(), markdown);
          expect(
            lease.ensureActiveProjectionAtUtf16(6, query: projected),
            FlarkV3LeafProjectionDemandDisposition.notApplicable,
          );

          lease.release();
          final firstClose = runtime.close();
          expect(identical(runtime.close(), firstClose), isTrue);
          await firstClose.timeout(_closeTimeout);
          closed = true;
          _expectClosedStatus(runtime.status, revision: 1);
          expect(() => runtime.queryAtUtf16(6), throwsStateError);
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'thematic break publishes exact atomic facts and an empty marker-free projection',
      () async {
        const markdown = '  * * * \r\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          expect(runtime.exportMarkdown(), markdown);
          expect(runtime.sourceLengthUtf16, markdown.length);
          _expectThematicBreak(runtime, runtime.queryAtUtf16(4), revision: 1);

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'live Paragraph to thematic break to Paragraph revisions stay exact',
      () async {
        const initialParagraph = 'alpha\n';
        const thematicBreak = '  * * * \r\n';
        const finalParagraph = 'omega\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initialParagraph,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          _expectRecursiveGreenParagraphRow(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 1,
            source: _asciiSpan(0, initialParagraph.length),
          );

          final thematicReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 0,
                endUtf16: initialParagraph.length,
                replacement: thematicBreak,
              ),
            ),
          );
          expect(thematicReceipt.changed, isTrue);
          expect(thematicReceipt.sourceRevision, 2);
          expect(runtime.exportMarkdown(), thematicBreak);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          _expectThematicBreak(runtime, runtime.queryAtUtf16(4), revision: 2);

          final paragraphReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: const FlarkV3SourceEdit(
                startUtf16: 0,
                endUtf16: thematicBreak.length,
                replacement: finalParagraph,
              ),
            ),
          );
          expect(paragraphReceipt.changed, isTrue);
          expect(paragraphReceipt.sourceRevision, 3);
          expect(runtime.exportMarkdown(), finalParagraph);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 3),
            revision: 3,
          );
          _expectRecursiveGreenParagraphRow(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 3,
            source: _asciiSpan(0, finalParagraph.length),
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'segmented points keep exact public semantics and converge after joining',
      () async {
        const initial = 'p\n\n**q**';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.upstream,
            revision: 1,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, 2),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 1,
            kind: FlarkV3DocumentStructureKind.unknown,
            unknownReason: FlarkV3DocumentUnknownReason.blankBoundary,
            source: _asciiSpan(2, 3),
            visibleSource: _asciiSpan(2, 2),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 3,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 1,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(3, 8),
          );

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 2,
                endUtf16: 3,
                replacement: '',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(runtime.exportMarkdown(), 'p\n**q**');
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, 7),
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      '100,000 Paragraphs keep a middle edit exact and document-local',
      () async {
        const paragraphCount = 100_000;
        const middleIndex = paragraphCount ~/ 2;
        final paragraphs = List<String>.generate(
          paragraphCount,
          (index) =>
              'Paragraph ${index.toString().padLeft(6, '0')} is canonical.',
          growable: false,
        );
        final markdown = paragraphs.join('\n\n');
        final middleMarkdown = paragraphs[middleIndex];
        final middleStart = markdown.indexOf(middleMarkdown);
        final editStart = middleStart + middleMarkdown.indexOf('canonical');
        final lastStart = markdown.lastIndexOf(paragraphs.last);

        final coldClock = Stopwatch()..start();
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_largeDocumentTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 1,
              timeout: _largeDocumentTimeout,
            ),
            revision: 1,
          );
          coldClock.stop();
          expect(runtime.sourceLengthUtf16, markdown.length);
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: middleStart + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 1,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(
              middleStart,
              middleStart + middleMarkdown.length + 1,
            ),
          );

          final applyClock = Stopwatch()..start();
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: 'C',
              ),
            ),
          );
          applyClock.stop();
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);

          final replacementClock = Stopwatch()..start();
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 2,
              timeout: _largeDocumentTimeout,
            ),
            revision: 2,
          );
          replacementClock.stop();

          expect(
            runtime.readSourceRange(
              middleStart,
              middleStart + middleMarkdown.length,
            ),
            middleMarkdown.replaceFirst('canonical', 'Canonical'),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, paragraphs.first.length + 1),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: middleStart + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(
              middleStart,
              middleStart + middleMarkdown.length + 1,
            ),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: lastStart + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(lastStart, markdown.length),
          );
          expect(
            applyClock.elapsed,
            lessThan(const Duration(milliseconds: 50)),
            reason:
                'the foreground edit is one persistent-source cut, independent '
                'of the 100,000-block parser workload',
          );

          // ignore: avoid_print
          print(
            'flark_v3_100k_paragraphs '
            'source_bytes=${markdown.length} '
            'cold_us=${coldClock.elapsedMicroseconds} '
            'apply_us=${applyClock.elapsedMicroseconds} '
            'replacement_us=${replacementClock.elapsedMicroseconds}',
          );

          await runtime.close().timeout(const Duration(seconds: 30));
          closed = true;
        } finally {
          if (!closed) {
            await runtime.close().timeout(const Duration(seconds: 30));
          }
        }
      },
      timeout: const Timeout(Duration(minutes: 3)),
    );

    test(
      'segmented BOF length change stays exact across local revisions',
      () async {
        const paragraphCount = 4096;
        const middleIndex = paragraphCount ~/ 2;
        const replacement = 'expanded';
        final paragraphs = List<String>.generate(
          paragraphCount,
          (index) =>
              'Paragraph ${index.toString().padLeft(4, '0')} ${'a' * 32}.',
          growable: false,
        );
        final source = paragraphs.join('\n\n');
        final firstEnd = paragraphs.first.length + 1;
        final middleStart = source.indexOf(paragraphs[middleIndex]);
        final middleEnd = middleStart + paragraphs[middleIndex].length + 1;
        final lastStart = source.lastIndexOf(paragraphs.last);
        final editStart = source.indexOf('aaaaaaaa') + 4;
        final coordinateDelta = replacement.length - 1;

        final runtime = await openFlarkV3PublicRuntimeForTest(
          source,
        ).timeout(_largeDocumentTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 1,
              timeout: _largeDocumentTimeout,
            ),
            revision: 1,
          );

          final firstEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: replacement,
              ),
            ),
          );
          expect(firstEdit.changed, isTrue);
          expect(firstEdit.sourceRevision, 2);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 2,
              timeout: _largeDocumentTimeout,
            ),
            revision: 2,
          );
          expect(runtime.sourceLengthUtf16, source.length + coordinateDelta);
          expect(
            runtime.readSourceRange(editStart, editStart + replacement.length),
            replacement,
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, firstEnd + coordinateDelta),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: middleStart + coordinateDelta + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(
              middleStart + coordinateDelta,
              middleEnd + coordinateDelta,
            ),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: lastStart + coordinateDelta + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(
              lastStart + coordinateDelta,
              source.length + coordinateDelta,
            ),
          );

          final secondEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: 'E',
              ),
            ),
          );
          expect(secondEdit.changed, isTrue);
          expect(secondEdit.sourceRevision, 3);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 3,
              timeout: _largeDocumentTimeout,
            ),
            revision: 3,
          );
          expect(runtime.readSourceRange(editStart, editStart + 1), 'E');
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: editStart,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 3,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, firstEnd + coordinateDelta),
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 2)),
    );

    test(
      'segmented EOF length change stays exact across local revisions',
      () async {
        const paragraphCount = 4096;
        const middleIndex = paragraphCount ~/ 2;
        const replacement = 'expanded';
        final paragraphs = List<String>.generate(
          paragraphCount,
          (index) =>
              'Paragraph ${index.toString().padLeft(4, '0')} ${'a' * 32}.',
          growable: false,
        );
        final source = paragraphs.join('\n\n');
        final firstEnd = paragraphs.first.length + 1;
        final middleStart = source.indexOf(paragraphs[middleIndex]);
        final middleEnd = middleStart + paragraphs[middleIndex].length + 1;
        final lastStart = source.lastIndexOf(paragraphs.last);
        final editStart = source.lastIndexOf('aaaaaaaa') + 4;
        final coordinateDelta = replacement.length - 1;

        final runtime = await openFlarkV3PublicRuntimeForTest(
          source,
        ).timeout(_largeDocumentTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 1,
              timeout: _largeDocumentTimeout,
            ),
            revision: 1,
          );

          final firstEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: replacement,
              ),
            ),
          );
          expect(firstEdit.changed, isTrue);
          expect(firstEdit.sourceRevision, 2);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 2,
              timeout: _largeDocumentTimeout,
            ),
            revision: 2,
          );
          expect(runtime.sourceLengthUtf16, source.length + coordinateDelta);
          expect(
            runtime.readSourceRange(editStart, editStart + replacement.length),
            replacement,
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(0, firstEnd),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: middleStart + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(middleStart, middleEnd),
          );
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: lastStart + 4,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 2,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(lastStart, source.length + coordinateDelta),
          );

          final secondEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: 'E',
              ),
            ),
          );
          expect(secondEdit.changed, isTrue);
          expect(secondEdit.sourceRevision, 3);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 3,
              timeout: _largeDocumentTimeout,
            ),
            revision: 3,
          );
          expect(runtime.readSourceRange(editStart, editStart + 1), 'E');
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: editStart,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 3,
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: _asciiSpan(lastStart, source.length + coordinateDelta),
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 2)),
    );

    test(
      '4,096 Paragraphs keep interior ATX and fence edits exact and local',
      () async {
        const paragraphCount = 4096;
        final source = StringBuffer();
        for (var index = 0; index < paragraphCount; index += 1) {
          source
            ..writeln(
              'Paragraph ${index.toString().padLeft(4, '0')} '
              '${List.filled(64, 'a').join()}',
            )
            ..writeln(
              'Continuation ${index.toString().padLeft(4, '0')} '
              '${List.filled(64, 'b').join()}',
            )
            ..writeln();
          if (index == paragraphCount ~/ 2 - 1) {
            source
              ..writeln('## mixed **heading**')
              ..writeln()
              ..writeln('```dart')
              ..writeln('let value = 1;')
              ..writeln('```')
              ..writeln();
          }
        }
        final initial = source.toString();
        final headingStart = initial.indexOf('## mixed');
        final headingEnd = initial.indexOf('\n', headingStart) + 1;
        final headingEdit = initial.indexOf('heading');
        final fenceStart = initial.indexOf('```dart');
        final fenceBodyStart = initial.indexOf('\n', fenceStart) + 1;
        final fenceEdit = initial.indexOf('value = 1') + 'value = '.length;
        final closingFence = initial.indexOf('\n```\n\n', fenceEdit) + 1;
        final closedFenceEnd = closingFence + 4;
        final lastParagraph = initial.lastIndexOf('Paragraph 4095');
        expect(headingEdit, greaterThan(0));
        expect(fenceEdit, greaterThan(headingEdit));
        expect(closingFence, greaterThan(fenceEdit));

        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_largeDocumentTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 1,
              timeout: _largeDocumentTimeout,
            ),
            revision: 1,
          );

          (FlarkV3RecursiveGreenPointQuery, FlarkV3RecursiveGreenRenderableRow)
          exactRowAt({
            required int pointUtf16,
            required int revision,
            required FlarkV3RecursiveGreenKind kind,
          }) {
            final query = runtime.queryAtUtf16(pointUtf16);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
            final point = query as FlarkV3RecursiveGreenPointQuery;
            expect(point.sourceRevision, revision);
            expect(point.structureRevision, revision);
            expect(point.owner.kind, kind);
            expect(point.inlineFacts, isNull);
            final range = runtime.queryBlockRange(
              point.source.startUtf16,
              point.source.endUtf16,
            );
            expect(range, isA<FlarkV3RecursiveGreenRowRange>());
            final rows = range as FlarkV3RecursiveGreenRowRange;
            expect(rows.sourceRevision, revision);
            expect(rows.structureRevision, revision);
            expect(rows.selectedRow, isNotNull);
            final row = rows.selectedRow!;
            expect(row.frameId, point.owner.frameId);
            expect(row.kind, kind);
            expect(row.selected, isTrue);
            expect(
              point.ancestry.map((frame) => frame.kind),
              row.path.map((frame) => frame.kind),
            );
            expect(
              row.path.indexWhere((frame) => frame.isRowOwner),
              point.ownerIndex,
            );
            expect(
              point.source.startUtf16,
              greaterThanOrEqualTo(row.physicalSource.startUtf16),
            );
            expect(
              point.source.endUtf16,
              lessThanOrEqualTo(row.physicalSource.endUtf16),
            );
            return (point, row);
          }

          final (_, initialHeading) = exactRowAt(
            pointUtf16: headingEdit + 1,
            revision: 1,
            kind: FlarkV3RecursiveGreenKind.heading,
          );
          _expectSpan(
            initialHeading.physicalSource,
            _asciiSpan(headingStart, headingEnd),
          );
          expect(initialHeading.editableSource, isNotNull);
          _expectSpan(
            initialHeading.editableSource!,
            _asciiSpan(headingStart + 3, headingEnd - 1),
          );
          final initialHeadingFact = initialHeading.path.last.fact;
          expect(
            initialHeadingFact,
            isA<FlarkV3RecursiveGreenHeadingPathFact>(),
          );
          expect(
            (initialHeadingFact! as FlarkV3RecursiveGreenHeadingPathFact).level,
            2,
          );

          final headingApplyClock = Stopwatch()..start();
          final headingReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: headingEdit,
                endUtf16: headingEdit + 1,
                replacement: 'H',
              ),
            ),
          );
          headingApplyClock.stop();
          expect(headingReceipt.changed, isTrue);
          final headingCurrentClock = Stopwatch()..start();
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 2,
              timeout: _largeDocumentTimeout,
            ),
            revision: 2,
          );
          headingCurrentClock.stop();
          final (_, editedHeading) = exactRowAt(
            pointUtf16: headingEdit + 1,
            revision: 2,
            kind: FlarkV3RecursiveGreenKind.heading,
          );
          _expectSpan(
            editedHeading.physicalSource,
            _asciiSpan(headingStart, headingEnd),
          );
          expect(editedHeading.editableSource, isNotNull);
          _expectSpan(
            editedHeading.editableSource!,
            _asciiSpan(headingStart + 3, headingEnd - 1),
          );
          expect(
            runtime.readSourceRange(
              headingEdit,
              headingEdit + 'Heading'.length,
            ),
            'Heading',
          );

          final fenceApplyClock = Stopwatch()..start();
          final fenceReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: FlarkV3SourceEdit(
                startUtf16: fenceEdit,
                endUtf16: fenceEdit + 1,
                replacement: '2',
              ),
            ),
          );
          fenceApplyClock.stop();
          expect(fenceReceipt.changed, isTrue);
          final fenceCurrentClock = Stopwatch()..start();
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 3,
              timeout: _largeDocumentTimeout,
            ),
            revision: 3,
          );
          fenceCurrentClock.stop();
          final (_, editedFence) = exactRowAt(
            pointUtf16: fenceEdit,
            revision: 3,
            kind: FlarkV3RecursiveGreenKind.fencedCode,
          );
          _expectSpan(
            editedFence.physicalSource,
            _asciiSpan(fenceStart, closedFenceEnd),
          );
          expect(editedFence.editableSource, isNotNull);
          _expectSpan(
            editedFence.editableSource!,
            _asciiSpan(fenceBodyStart, closingFence),
          );
          expect(
            editedFence.path.last.fact,
            isA<FlarkV3RecursiveGreenCodePathFact>(),
          );
          expect(runtime.readSourceRange(fenceEdit, fenceEdit + 1), '2');

          for (final point in [
            4,
            headingEdit + 1,
            fenceEdit,
            lastParagraph + 4,
          ]) {
            exactRowAt(
              pointUtf16: point,
              revision: 3,
              kind: point == headingEdit + 1
                  ? FlarkV3RecursiveGreenKind.heading
                  : point == fenceEdit
                  ? FlarkV3RecursiveGreenKind.fencedCode
                  : FlarkV3RecursiveGreenKind.paragraph,
            );
          }
          expect(
            headingApplyClock.elapsed,
            lessThan(const Duration(milliseconds: 50)),
          );
          expect(
            fenceApplyClock.elapsed,
            lessThan(const Duration(milliseconds: 50)),
          );

          final unclosedReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 3,
              operation: FlarkV3SourceEdit(
                startUtf16: closingFence,
                endUtf16: closingFence + 3,
                replacement: '',
              ),
            ),
          );
          expect(unclosedReceipt.changed, isTrue);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 4,
              timeout: _largeDocumentTimeout,
            ),
            revision: 4,
          );
          final (unclosedPoint, unclosedFence) = exactRowAt(
            pointUtf16: fenceEdit,
            revision: 4,
            kind: FlarkV3RecursiveGreenKind.fencedCode,
          );
          expect(unclosedFence.editableSource, isNotNull);
          _expectSpan(
            unclosedFence.physicalSource,
            _asciiSpan(fenceStart, runtime.sourceLengthUtf16),
          );
          _expectSpan(
            unclosedFence.editableSource!,
            _asciiSpan(fenceBodyStart, runtime.sourceLengthUtf16),
          );
          final (swallowedSuffix, swallowedRow) = exactRowAt(
            pointUtf16: lastParagraph + 4,
            revision: 4,
            kind: FlarkV3RecursiveGreenKind.fencedCode,
          );
          expect(swallowedSuffix.owner.frameId, unclosedPoint.owner.frameId);
          expect(swallowedRow.frameId, unclosedFence.frameId);
          expect(
            runtime.exportMarkdown(),
            initial
                .replaceRange(headingEdit, headingEdit + 1, 'H')
                .replaceRange(fenceEdit, fenceEdit + 1, '2')
                .replaceRange(closingFence, closingFence + 3, ''),
          );

          // ignore: avoid_print
          print(
            'flark_v3_4096_mixed_blocks '
            'source_bytes=${initial.length} '
            'heading_apply_us=${headingApplyClock.elapsedMicroseconds} '
            'heading_current_us=${headingCurrentClock.elapsedMicroseconds} '
            'fence_apply_us=${fenceApplyClock.elapsedMicroseconds} '
            'fence_current_us=${fenceCurrentClock.elapsedMicroseconds}',
          );

          await runtime.close().timeout(const Duration(seconds: 30));
          closed = true;
        } finally {
          if (!closed) {
            await runtime.close().timeout(const Duration(seconds: 30));
          }
        }
      },
      timeout: const Timeout(Duration(minutes: 3)),
    );

    test(
      'escaped punctuation facts preserve parser preorder and recertify after edit',
      () async {
        const initial = '**\\***\n';
        const edited = '**\\_**\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          expect(runtime.exportMarkdown(), initial);

          Future<FlarkV3RecursiveGreenPointQuery> demandInline({
            required int revision,
          }) async {
            final query = lease.queryAtUtf16(3);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
            final paragraph = query as FlarkV3RecursiveGreenPointQuery;
            expect(paragraph.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(paragraph.sourceRevision, revision);
            expect(paragraph.structureRevision, revision);
            expect(paragraph.inlineFacts, isNull);

            final beforePresentation =
                runtime.status.inlinePresentationGeneration;
            final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > beforePresentation ||
                  status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(3, query: paragraph),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.inlinePresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.inlineAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );

            final refined = lease.queryAtUtf16(3);
            expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
            final result = refined as FlarkV3RecursiveGreenPointQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.owner.frameId, paragraph.owner.frameId);
            expect(result.paragraphSource, isNotNull);
            expect(result.inlineSource, isNotNull);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertifiedFacts(
            FlarkV3RecursiveGreenPointQuery result, {
            required int revision,
          }) {
            _expectSpan(result.paragraphSource!, _asciiSpan(0, 7));
            _expectSpan(result.inlineSource!, _asciiSpan(0, 6));
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, 6));
            expect(inline.facts.map((fact) => fact.kind), [
              FlarkV3InlineFactKind.strong,
              FlarkV3InlineFactKind.escapedPunctuation,
            ]);

            final strong = inline.facts[0];
            _expectSpan(strong.source, _asciiSpan(0, 6));
            _expectSpan(strong.opener, _asciiSpan(0, 2));
            _expectSpan(strong.content, _asciiSpan(2, 4));
            _expectSpan(strong.closer, _asciiSpan(4, 6));

            final escaped = inline.facts[1];
            _expectSpan(escaped.source, _asciiSpan(2, 4));
            _expectSpan(escaped.opener, _asciiSpan(2, 3));
            _expectSpan(escaped.content, _asciiSpan(3, 4));
            _expectSpan(escaped.closer, _asciiSpan(4, 4));
            expect(escaped.linkAnnotation, isNull);
            expect(escaped.normalizesCodeLineEndings, isFalse);
            expect(escaped.trimsOneCodeEdgeSpace, isFalse);
          }

          expectCertifiedFacts(await demandInline(revision: 1), revision: 1);

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 3,
                endUtf16: 4,
                replacement: '_',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(
            runtime.exportMarkdown(),
            edited,
            reason:
                'the canonical source must change before parser '
                'recertification',
          );
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          expect(
            (lease.queryAtUtf16(3) as FlarkV3RecursiveGreenPointQuery)
                .inlineFacts,
            isNull,
            reason:
                'revision-one inline authority must not survive the source '
                'edit',
          );

          expectCertifiedFacts(await demandInline(revision: 2), revision: 2);
          expect(runtime.exportMarkdown(), edited);

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'hard line break facts preserve marker-free semantics across marker edits',
      () async {
        const initial = '*a  \nb*\n';
        const edited = '*a\\\nb*\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var exactSource = FlarkV3SourceDocument.fromString('')
            .apply(
              FlarkV3SourceTransaction.single(
                baseRevision: 0,
                operation: const FlarkV3SourceEdit(
                  startUtf16: 0,
                  endUtf16: 0,
                  replacement: initial,
                ),
              ),
            )
            .document;
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          expect(runtime.exportMarkdown(), initial);
          expect(exactSource.revision, 1);
          expect(exactSource.toString(), initial);

          Future<FlarkV3RecursiveGreenPointQuery> demandInline({
            required int revision,
          }) async {
            final query = lease.queryAtUtf16(1);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
            final paragraph = query as FlarkV3RecursiveGreenPointQuery;
            expect(paragraph.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(paragraph.sourceRevision, revision);
            expect(paragraph.structureRevision, revision);
            expect(paragraph.inlineFacts, isNull);

            final beforePresentation =
                runtime.status.inlinePresentationGeneration;
            final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > beforePresentation ||
                  status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(1, query: paragraph),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.inlinePresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.inlineAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );

            final refined = lease.queryAtUtf16(1);
            expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
            final result = refined as FlarkV3RecursiveGreenPointQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.owner.frameId, paragraph.owner.frameId);
            expect(result.paragraphSource, isNotNull);
            expect(result.inlineSource, isNotNull);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertified(
            FlarkV3RecursiveGreenPointQuery result, {
            required int revision,
            required bool backslashMarker,
            required String source,
          }) {
            _expectSpan(result.paragraphSource!, _asciiSpan(0, source.length));
            _expectSpan(result.inlineSource!, _asciiSpan(0, source.length - 1));
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, source.length - 1));
            expect(inline.facts.map((fact) => fact.kind), [
              FlarkV3InlineFactKind.emphasis,
              FlarkV3InlineFactKind.hardLineBreak,
            ]);

            final emphasis = inline.facts[0];
            final emphasisEnd = source.length - 1;
            _expectSpan(emphasis.source, _asciiSpan(0, emphasisEnd));
            _expectSpan(emphasis.opener, _asciiSpan(0, 1));
            _expectSpan(emphasis.content, _asciiSpan(1, emphasisEnd - 1));
            _expectSpan(
              emphasis.closer,
              _asciiSpan(emphasisEnd - 1, emphasisEnd),
            );

            final hardBreak = inline.facts[1];
            final contentStart = backslashMarker ? 3 : 4;
            final factEnd = contentStart + 1;
            _expectSpan(hardBreak.source, _asciiSpan(2, factEnd));
            _expectSpan(hardBreak.opener, _asciiSpan(2, contentStart));
            _expectSpan(hardBreak.content, _asciiSpan(contentStart, factEnd));
            _expectSpan(hardBreak.closer, _asciiSpan(factEnd, factEnd));
            expect(hardBreak.linkAnnotation, isNull);
            expect(hardBreak.normalizesCodeLineEndings, isFalse);
            expect(hardBreak.trimsOneCodeEdgeSpace, isFalse);

            final projection = FlarkV3InlineProjection.fromValidatedFacts(
              sourceDocument: exactSource,
              expectedSource: inline.sourceVersion,
              facts: inline,
              markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
            );
            expect(
              projection.sourceText,
              source.substring(0, source.length - 1),
            );
            expect(
              projection.sourceProjection.sourceText,
              source.substring(0, source.length - 1),
            );
            expect(projection.displayText, 'a\nb');
            expect(projection.sourceProjection.displayText, 'a\nb');
            final replacementPieces = projection.sourceProjection.pieces.where(
              (piece) => piece.kind == FlarkV3SourceProjectionPieceKind.replace,
            );
            expect(replacementPieces, hasLength(1));
            expect(replacementPieces.single.displayText, '\n');
          }

          final revisionOne = await demandInline(revision: 1);
          expectCertified(
            revisionOne,
            revision: 1,
            backslashMarker: false,
            source: initial,
          );
          final beforeCacheHit = runtime.status.inlinePresentationGeneration;
          expect(
            lease.ensureActiveProjectionAtUtf16(1, query: revisionOne),
            FlarkV3LeafProjectionDemandDisposition.notApplicable,
          );
          expect(
            runtime.status.inlinePresentationGeneration,
            beforeCacheHit,
            reason: 'a current exact inline cache hit must schedule no work',
          );

          const markerEdit = FlarkV3SourceEdit(
            startUtf16: 2,
            endUtf16: 4,
            replacement: '\\',
          );
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: markerEdit,
            ),
          );
          exactSource = exactSource
              .apply(
                FlarkV3SourceTransaction.single(
                  baseRevision: 1,
                  operation: markerEdit,
                ),
              )
              .document;
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(runtime.exportMarkdown(), edited);
          expect(exactSource.revision, 2);
          expect(exactSource.toString(), edited);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          expect(
            (lease.queryAtUtf16(1) as FlarkV3RecursiveGreenPointQuery)
                .inlineFacts,
            isNull,
            reason:
                'the marker edit must invalidate revision-one inline '
                'authority before recertification',
          );

          final revisionTwo = await demandInline(revision: 2);
          expectCertified(
            revisionTwo,
            revision: 2,
            backslashMarker: true,
            source: edited,
          );
          expect(runtime.exportMarkdown(), edited);

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'character references carry parser-cooked replacement values across edits',
      () async {
        const initial = '*&amp; &ngE;*\n';
        const edited = '*&copy; &ngE;*\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var exactSource = FlarkV3SourceDocument.fromString('')
            .apply(
              FlarkV3SourceTransaction.single(
                baseRevision: 0,
                operation: const FlarkV3SourceEdit(
                  startUtf16: 0,
                  endUtf16: 0,
                  replacement: initial,
                ),
              ),
            )
            .document;
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );

          Future<FlarkV3RecursiveGreenPointQuery> demandInline({
            required int revision,
          }) async {
            final query = lease.queryAtUtf16(2);
            expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
            final paragraph = query as FlarkV3RecursiveGreenPointQuery;
            expect(paragraph.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
            expect(paragraph.sourceRevision, revision);
            expect(paragraph.structureRevision, revision);
            expect(paragraph.inlineFacts, isNull);

            final beforePresentation =
                runtime.status.inlinePresentationGeneration;
            final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > beforePresentation ||
                  status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(2, query: paragraph),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.inlinePresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.inlineAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );

            final refined = lease.queryAtUtf16(2);
            expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
            final result = refined as FlarkV3RecursiveGreenPointQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.owner.frameId, paragraph.owner.frameId);
            expect(result.paragraphSource, isNotNull);
            expect(result.inlineSource, isNotNull);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertified(
            FlarkV3RecursiveGreenPointQuery result, {
            required int revision,
            required String source,
            required String firstValue,
          }) {
            _expectSpan(result.paragraphSource!, _asciiSpan(0, source.length));
            _expectSpan(result.inlineSource!, _asciiSpan(0, source.length - 1));
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, source.length - 1));
            expect(inline.facts.map((fact) => fact.kind), [
              FlarkV3InlineFactKind.emphasis,
              FlarkV3InlineFactKind.characterReference,
              FlarkV3InlineFactKind.characterReference,
            ]);

            final firstStart = source.indexOf('&');
            final firstEnd = source.indexOf(';', firstStart) + 1;
            final secondStart = source.indexOf('&', firstEnd);
            final secondEnd = source.indexOf(';', secondStart) + 1;
            final references = inline.facts.skip(1).toList();
            expect(references.map((fact) => fact.characterReferenceValue), [
              firstValue,
              '≧\u{338}',
            ]);
            for (final (index, fact) in references.indexed) {
              final start = index == 0 ? firstStart : secondStart;
              final end = index == 0 ? firstEnd : secondEnd;
              _expectSpan(fact.source, _asciiSpan(start, end));
              _expectSpan(fact.content, _asciiSpan(start, end));
              _expectSpan(fact.opener, _asciiSpan(start, start));
              _expectSpan(fact.closer, _asciiSpan(end, end));
            }

            final visible = FlarkV3InlineProjection.fromValidatedFacts(
              sourceDocument: exactSource,
              expectedSource: inline.sourceVersion,
              facts: inline,
            );
            expect(visible.displayText, source.substring(0, source.length - 1));
            final markerFree = FlarkV3InlineProjection.fromValidatedFacts(
              sourceDocument: exactSource,
              expectedSource: inline.sourceVersion,
              facts: inline,
              markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
            );
            expect(
              markerFree.sourceText,
              source.substring(0, source.length - 1),
            );
            expect(markerFree.displayText, '$firstValue ≧\u{338}');
            expect(
              markerFree.sourceProjection.pieces.where(
                (piece) =>
                    piece.kind == FlarkV3SourceProjectionPieceKind.replace,
              ),
              hasLength(2),
            );
          }

          expectCertified(
            await demandInline(revision: 1),
            revision: 1,
            source: initial,
            firstValue: '&',
          );

          const edit = FlarkV3SourceEdit(
            startUtf16: 1,
            endUtf16: 6,
            replacement: '&copy;',
          );
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(baseRevision: 1, operation: edit),
          );
          exactSource = exactSource
              .apply(
                FlarkV3SourceTransaction.single(
                  baseRevision: 1,
                  operation: edit,
                ),
              )
              .document;
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(runtime.exportMarkdown(), edited);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          expect(
            (lease.queryAtUtf16(2) as FlarkV3RecursiveGreenPointQuery)
                .inlineFacts,
            isNull,
            reason:
                'the source edit must revoke the old cooked replacement before '
                'recertification',
          );
          expectCertified(
            await demandInline(revision: 2),
            revision: 2,
            source: edited,
            firstValue: '©',
          );

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'demanded Paragraph facts remain current after the host sidecar moves',
      () async {
        const markdown =
            '*first*\n\n'
            '**bold _em_** and `code`.\n\n'
            '`tail`';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          final middleStart = markdown.indexOf('**bold');
          final middleEnd = markdown.lastIndexOf('\n\n') + 1;
          final middlePoint = markdown.indexOf('bold') + 1;

          final query = lease.queryAtUtf16(middlePoint);
          expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
          final middle = query as FlarkV3RecursiveGreenPointQuery;
          expect(middle.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(middle.sourceRevision, 1);
          expect(middle.structureRevision, 1);
          expect(
            middle.inlineFacts,
            isNull,
            reason:
                'the persistent block root remains structure-only before '
                'selected-leaf demand',
          );

          final beforeInline = runtime.status.inlinePresentationGeneration;
          final committed = _awaitStatus(
            runtime,
            (status) =>
                status.inlinePresentationGeneration > beforeInline ||
                status.state == FlarkV3DocumentRuntimeState.faulted,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(middlePoint, query: middle),
            FlarkV3LeafProjectionDemandDisposition.scheduled,
          );
          final committedStatus = await committed;
          expect(committedStatus.state, FlarkV3DocumentRuntimeState.open);
          expect(
            committedStatus.inlinePresentationGeneration,
            beforeInline + 1,
          );

          final refined = lease.queryAtUtf16(middlePoint);
          expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
          final refinedMiddle = refined as FlarkV3RecursiveGreenPointQuery;
          expect(refinedMiddle.owner.frameId, middle.owner.frameId);
          expect(refinedMiddle.paragraphSource, isNotNull);
          expect(refinedMiddle.inlineSource, isNotNull);
          _expectSpan(
            refinedMiddle.paragraphSource!,
            _asciiSpan(middleStart, middleEnd),
          );
          _expectSpan(
            refinedMiddle.inlineSource!,
            _asciiSpan(middleStart, middleEnd - 1),
          );
          final inline = refinedMiddle.inlineFacts;
          expect(inline, isNotNull);
          expect(
            inline!.disposition,
            FlarkV3InlineFactsDisposition.authoritative,
          );
          expect(inline.facts.map((fact) => fact.kind), [
            FlarkV3InlineFactKind.strong,
            FlarkV3InlineFactKind.emphasis,
            FlarkV3InlineFactKind.code,
          ]);
          _expectSpan(inline.source, _asciiSpan(middleStart, middleEnd - 1));

          final strong = inline.facts[0];
          _expectSpan(
            strong.source,
            _asciiSpan(middleStart, middleStart + '**bold _em_**'.length),
          );
          final emphasisStart = markdown.indexOf('_em_');
          _expectSpan(
            inline.facts[1].source,
            _asciiSpan(emphasisStart, emphasisStart + '_em_'.length),
          );
          final codeStart = markdown.indexOf('`code`');
          _expectSpan(
            inline.facts[2].source,
            _asciiSpan(codeStart, codeStart + '`code`'.length),
          );

          final first =
              lease.queryAtUtf16(1) as FlarkV3RecursiveGreenPointQuery;
          final tail =
              lease.queryAtUtf16(markdown.length - 2)
                  as FlarkV3RecursiveGreenPointQuery;
          expect(
            first.inlineFacts,
            isNull,
            reason: 'a selected-leaf sidecar cannot attach to its neighbor',
          );
          expect(
            tail.inlineFacts,
            isNull,
            reason: 'equal query authority is still block-identity scoped',
          );

          Future<FlarkV3RecursiveGreenPointQuery> demandLeaf(
            int positionUtf16,
            FlarkV3RecursiveGreenPointQuery point,
          ) async {
            final before = runtime.status.inlinePresentationGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > before ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(positionUtf16, query: point),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final status = await settled;
            expect(status.state, FlarkV3DocumentRuntimeState.open);
            final refined = lease.queryAtUtf16(positionUtf16);
            expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
            final result = refined as FlarkV3RecursiveGreenPointQuery;
            expect(result.owner.frameId, point.owner.frameId);
            expect(result.paragraphSource, isNotNull);
            expect(result.inlineSource, isNotNull);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          final refinedTail = await demandLeaf(markdown.length - 2, tail);
          expect(
            refinedTail.inlineFacts!.facts.single.kind,
            FlarkV3InlineFactKind.code,
          );
          final retainedMiddle =
              lease.queryAtUtf16(middlePoint)
                  as FlarkV3RecursiveGreenPointQuery;
          expect(
            retainedMiddle.inlineFacts,
            isNotNull,
            reason:
                'decoded current-ACK facts survive after the singleton host '
                'sidecar moves',
          );
          expect(
            retainedMiddle.inlineFacts!.facts.map((fact) => fact.kind),
            [
              FlarkV3InlineFactKind.strong,
              FlarkV3InlineFactKind.emphasis,
              FlarkV3InlineFactKind.code,
            ],
            reason:
                'decoded current-ACK facts survive after the singleton host '
                'sidecar moves',
          );
          final refinedFirst = await demandLeaf(1, first);
          expect(
            refinedFirst.inlineFacts!.facts.single.kind,
            FlarkV3InlineFactKind.emphasis,
          );
          expect(
            (lease.queryAtUtf16(markdown.length - 2)
                    as FlarkV3RecursiveGreenPointQuery)
                .inlineFacts!
                .facts
                .single
                .kind,
            FlarkV3InlineFactKind.code,
          );
          final cachedMiddle =
              lease.queryAtUtf16(middlePoint)
                  as FlarkV3RecursiveGreenPointQuery;
          final afterThreeDemands = runtime.status.inlinePresentationGeneration;
          expect(
            lease.ensureActiveProjectionAtUtf16(
              middlePoint,
              query: cachedMiddle,
            ),
            FlarkV3LeafProjectionDemandDisposition.notApplicable,
            reason: 'a cache hit must not move the host singleton again',
          );
          await Future<void>.delayed(const Duration(milliseconds: 10));
          expect(
            runtime.status.inlinePresentationGeneration,
            afterThreeDemands,
          );
          expect(runtime.exportMarkdown(), markdown);

          final edit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: markdown.length,
                endUtf16: markdown.length,
                replacement: '!',
              ),
            ),
          );
          expect(edit.changed, isTrue);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          expect(
            (lease.queryAtUtf16(middlePoint) as FlarkV3RecursiveGreenPointQuery)
                .inlineFacts,
            isNull,
            reason:
                'an edit clears old-ACK facts before the next exact revision '
                'can authorize presentation or editing',
          );
          expect(runtime.exportMarkdown(), '$markdown!');

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'ATX heading geometry and inline facts recertify after a live content edit',
      () async {
        const initial = '## **β😀** live _heading_ ###\r\n';
        const edited = '## **β😀!** live _heading_ ###\r\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );

          Future<FlarkV3RecursiveGreenPointQuery> demandInline(
            int positionUtf16,
            FlarkV3RecursiveGreenPointQuery structural,
          ) async {
            final beforePresentation =
                runtime.status.inlinePresentationGeneration;
            final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > beforePresentation ||
                  status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(
                positionUtf16,
                query: structural,
              ),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.inlinePresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.inlineAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );
            final refined = lease.queryAtUtf16(positionUtf16);
            expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
            final result = refined as FlarkV3RecursiveGreenPointQuery;
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          final initialPosition = initial.indexOf('live') + 1;
          final initialQuery = lease.queryAtUtf16(initialPosition);
          expect(initialQuery, isA<FlarkV3RecursiveGreenPointQuery>());
          final initialHeading =
              initialQuery as FlarkV3RecursiveGreenPointQuery;
          _expectAtxHeading(
            runtime,
            initialHeading,
            revision: 1,
            source: (startUtf8: 0, endUtf8: 34, startUtf16: 0, endUtf16: 31),
            content: (startUtf8: 3, endUtf8: 28, startUtf16: 3, endUtf16: 25),
          );
          expect(initialHeading.inlineFacts, isNull);

          final initialRefined = await demandInline(
            initialPosition,
            initialHeading,
          );
          final initialInline = initialRefined.inlineFacts!;
          expect(
            initialInline.disposition,
            FlarkV3InlineFactsDisposition.authoritative,
          );
          expect(initialInline.facts.map((fact) => fact.kind), [
            FlarkV3InlineFactKind.strong,
            FlarkV3InlineFactKind.emphasis,
          ]);
          _expectSpan(initialInline.source, (
            startUtf8: 3,
            endUtf8: 28,
            startUtf16: 3,
            endUtf16: 25,
          ));
          _expectSpan(initialInline.facts[0].source, (
            startUtf8: 3,
            endUtf8: 13,
            startUtf16: 3,
            endUtf16: 10,
          ));
          _expectSpan(initialInline.facts[0].content, (
            startUtf8: 5,
            endUtf8: 11,
            startUtf16: 5,
            endUtf16: 8,
          ));
          _expectSpan(initialInline.facts[1].source, (
            startUtf8: 19,
            endUtf8: 28,
            startUtf16: 16,
            endUtf16: 25,
          ));

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 8,
                endUtf16: 8,
                replacement: '!',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(
            runtime.exportMarkdown(),
            edited,
            reason:
                'the exact source authority must retain opening, inline, '
                'closing, and CRLF markers during pending recertification',
          );
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );

          final editedPosition = edited.indexOf('live') + 1;
          final editedQuery = lease.queryAtUtf16(editedPosition);
          expect(editedQuery, isA<FlarkV3RecursiveGreenPointQuery>());
          final editedHeading = editedQuery as FlarkV3RecursiveGreenPointQuery;
          _expectAtxHeading(
            runtime,
            editedHeading,
            revision: 2,
            source: (startUtf8: 0, endUtf8: 35, startUtf16: 0, endUtf16: 32),
            content: (startUtf8: 3, endUtf8: 29, startUtf16: 3, endUtf16: 26),
          );
          expect(
            editedHeading.inlineFacts,
            isNull,
            reason:
                'facts from the prior source/ACK cannot cross the edit; the '
                'new heading requires its own parser-certified sidecar',
          );

          final editedRefined = await demandInline(
            editedPosition,
            editedHeading,
          );
          final editedInline = editedRefined.inlineFacts!;
          expect(
            editedInline.disposition,
            FlarkV3InlineFactsDisposition.authoritative,
          );
          expect(editedInline.facts.map((fact) => fact.kind), [
            FlarkV3InlineFactKind.strong,
            FlarkV3InlineFactKind.emphasis,
          ]);
          _expectSpan(editedInline.source, (
            startUtf8: 3,
            endUtf8: 29,
            startUtf16: 3,
            endUtf16: 26,
          ));
          _expectSpan(editedInline.facts[0].source, (
            startUtf8: 3,
            endUtf8: 14,
            startUtf16: 3,
            endUtf16: 11,
          ));
          _expectSpan(editedInline.facts[0].content, (
            startUtf8: 5,
            endUtf8: 12,
            startUtf16: 5,
            endUtf16: 9,
          ));
          _expectSpan(editedInline.facts[1].source, (
            startUtf8: 20,
            endUtf8: 29,
            startUtf16: 17,
            endUtf16: 26,
          ));
          expect(runtime.exportMarkdown(), edited);

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'Setext heading keeps its structural CRLF hidden across a live edit',
      () async {
        const initial = '**β😀** live _heading_\r\n---\r\n';
        const edited = '**β😀!** live _heading_\r\n---\r\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );

          Future<FlarkV3RecursiveGreenPointQuery> demandInline(
            int positionUtf16,
            FlarkV3RecursiveGreenPointQuery structural,
          ) async {
            final beforePresentation =
                runtime.status.inlinePresentationGeneration;
            final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > beforePresentation ||
                  status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureActiveProjectionAtUtf16(
                positionUtf16,
                query: structural,
              ),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.inlinePresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.inlineAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );
            return lease.queryAtUtf16(positionUtf16)
                as FlarkV3RecursiveGreenPointQuery;
          }

          final initialPosition = initial.indexOf('live') + 1;
          final initialQuery =
              lease.queryAtUtf16(initialPosition)
                  as FlarkV3RecursiveGreenPointQuery;
          _expectSetextHeading(
            runtime,
            initialQuery,
            revision: 1,
            source: (startUtf8: 0, endUtf8: 32, startUtf16: 0, endUtf16: 29),
            content: (startUtf8: 0, endUtf8: 25, startUtf16: 0, endUtf16: 22),
          );
          final initialRefined = await demandInline(
            initialPosition,
            initialQuery,
          );
          expect(initialRefined.inlineFacts!.facts.map((fact) => fact.kind), [
            FlarkV3InlineFactKind.strong,
            FlarkV3InlineFactKind.emphasis,
          ]);
          _expectSpan(initialRefined.inlineFacts!.source, (
            startUtf8: 0,
            endUtf8: 25,
            startUtf16: 0,
            endUtf16: 22,
          ));

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 5,
                endUtf16: 5,
                replacement: '!',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(runtime.exportMarkdown(), edited);
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );

          final editedPosition = edited.indexOf('live') + 1;
          final editedQuery =
              lease.queryAtUtf16(editedPosition)
                  as FlarkV3RecursiveGreenPointQuery;
          _expectSetextHeading(
            runtime,
            editedQuery,
            revision: 2,
            source: (startUtf8: 0, endUtf8: 33, startUtf16: 0, endUtf16: 30),
            content: (startUtf8: 0, endUtf8: 26, startUtf16: 0, endUtf16: 23),
          );
          expect(
            editedQuery.inlineFacts,
            isNull,
            reason: 'the edited heading requires a fresh exact sidecar',
          );
          final editedRefined = await demandInline(editedPosition, editedQuery);
          expect(editedRefined.inlineFacts!.facts.map((fact) => fact.kind), [
            FlarkV3InlineFactKind.strong,
            FlarkV3InlineFactKind.emphasis,
          ]);
          _expectSpan(editedRefined.inlineFacts!.source, (
            startUtf8: 0,
            endUtf8: 26,
            startUtf16: 0,
            endUtf16: 23,
          ));
          expect(runtime.exportMarkdown(), edited);

          lease.release();
          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'oversized Paragraph remains exact without an unusable inline demand',
      () async {
        final markdown = List<String>.filled(9 * 1024, 'x').join();
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          final structural = lease.queryAtUtf16(markdown.length ~/ 2);
          expect(structural, isA<FlarkV3RecursiveGreenPointQuery>());
          final paragraph = structural as FlarkV3RecursiveGreenPointQuery;
          expect(paragraph.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(paragraph.inlineFacts, isNull);
          final row = _selectedRecursiveGreenRow(runtime, paragraph);
          expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
          _expectSpan(row.physicalSource, _asciiSpan(0, markdown.length));
          _expectSpan(row.editableSource!, _asciiSpan(0, markdown.length));
          final beforePresentation =
              runtime.status.inlinePresentationGeneration;
          final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
          expect(
            lease.ensureActiveProjectionAtUtf16(
              markdown.length ~/ 2,
              query: paragraph,
            ),
            FlarkV3LeafProjectionDemandDisposition.notApplicable,
          );
          await Future<void>.delayed(const Duration(milliseconds: 10));
          expect(
            runtime.status.inlinePresentationGeneration,
            beforePresentation,
          );
          expect(runtime.status.inlineAttemptOutcomeGeneration, beforeOutcome);
          expect(runtime.exportMarkdown(), markdown);

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'large definition prefix does not make its small Paragraph ineligible',
      () async {
        final definitions = StringBuffer();
        for (var index = 0; index < 1024; index += 1) {
          definitions.write('[r$index]: /u\n');
        }
        final tailStart = definitions.length;
        const tail = '**tail**';
        final markdown = '$definitions$tail';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          markdown,
        ).timeout(_openTimeout);
        final lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          final position = tailStart + 3;
          final structural = lease.queryAtUtf16(position);
          expect(structural, isA<FlarkV3RecursiveGreenPointQuery>());
          final paragraph = structural as FlarkV3RecursiveGreenPointQuery;
          expect(paragraph.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          final row = _selectedRecursiveGreenRow(runtime, paragraph);
          expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
          _expectSpan(row.physicalSource, _asciiSpan(0, markdown.length));
          _expectSpan(
            row.editableSource!,
            _asciiSpan(tailStart, markdown.length),
          );
          expect(paragraph.inlineFacts, isNull);

          final beforePresentation =
              runtime.status.inlinePresentationGeneration;
          final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
          final settled = _awaitStatus(
            runtime,
            (status) =>
                status.inlinePresentationGeneration > beforePresentation ||
                status.inlineAttemptOutcomeGeneration > beforeOutcome ||
                status.state == FlarkV3DocumentRuntimeState.faulted,
          );
          expect(
            lease.ensureActiveProjectionAtUtf16(position, query: paragraph),
            FlarkV3LeafProjectionDemandDisposition.scheduled,
            reason:
                'inline eligibility is bounded by the projected Paragraph, '
                'not its definition-bearing physical leaf',
          );
          final settledStatus = await settled;
          expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
          expect(
            settledStatus.inlinePresentationGeneration,
            beforePresentation + 1,
          );
          expect(
            settledStatus.inlineAttemptOutcomeGeneration,
            beforeOutcome + 1,
          );

          final refined = lease.queryAtUtf16(position);
          expect(refined, isA<FlarkV3RecursiveGreenPointQuery>());
          final inline =
              (refined as FlarkV3RecursiveGreenPointQuery).inlineFacts;
          expect(inline, isNotNull);
          expect(
            inline!.disposition,
            FlarkV3InlineFactsDisposition.authoritative,
          );
          expect(inline.facts, hasLength(1));
          expect(inline.facts.single.kind, FlarkV3InlineFactKind.strong);
          _expectSpan(
            inline.facts.single.source,
            _asciiSpan(tailStart, markdown.length),
          );
          expect(runtime.exportMarkdown(), markdown);

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          lease.release();
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'frozen reference prefix keeps length-changing tail edits exact',
      () async {
        const referenceCount = 2048;
        const paragraphCount = 2048;
        const editedParagraph = paragraphCount ~/ 2;
        const replacement = 'expanded';
        final markdown = StringBuffer();
        for (var index = 0; index < referenceCount; index += 1) {
          markdown.writeln('[ref-$index]: /target-$index');
        }
        final tailStart = markdown.length;
        final paragraphRanges = <({int start, int end})>[];
        for (var index = 0; index < paragraphCount; index += 1) {
          final start = markdown.length;
          markdown.writeln(
            'tail paragraph ${index.toString().padLeft(4, '0')} '
            '${'a' * 32}\n',
          );
          paragraphRanges.add((start: start, end: markdown.length - 1));
        }
        final source = markdown.toString();
        final editedRange = paragraphRanges[editedParagraph];
        final editStart = source.indexOf('aaaaaaaa', editedRange.start) + 4;
        final coordinateDelta = replacement.length - 1;

        final runtime = await openFlarkV3PublicRuntimeForTest(
          source,
        ).timeout(_largeDocumentTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 1,
              timeout: _largeDocumentTimeout,
            ),
            revision: 1,
          );
          final baseFirst =
              runtime.queryAtUtf16(tailStart + 1)
                  as FlarkV3RecursiveGreenPointQuery;
          expect(baseFirst.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          final baseFirstRow = _selectedRecursiveGreenRow(runtime, baseFirst);
          _expectSpan(
            baseFirstRow.physicalSource,
            _asciiSpan(0, paragraphRanges.first.end),
          );
          _expectSpan(
            baseFirstRow.editableSource!,
            _asciiSpan(tailStart, paragraphRanges.first.end - 1),
          );

          final firstEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: replacement,
              ),
            ),
          );
          expect(firstEdit.changed, isTrue);
          expect(firstEdit.sourceRevision, 2);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 2,
              timeout: _largeDocumentTimeout,
            ),
            revision: 2,
          );
          expect(
            runtime.readSourceRange(editStart, editStart + replacement.length),
            replacement,
          );
          expect(runtime.sourceLengthUtf16, source.length + coordinateDelta);

          final edited =
              runtime.queryAtUtf16(editStart + 1)
                  as FlarkV3RecursiveGreenPointQuery;
          expect(edited.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          final editedRow = _selectedRecursiveGreenRow(runtime, edited);
          _expectSpan(
            editedRow.physicalSource,
            _asciiSpan(editedRange.start, editedRange.end + coordinateDelta),
          );

          final lastBase = paragraphRanges.last;
          final last =
              runtime.queryAtUtf16(lastBase.start + coordinateDelta + 1)
                  as FlarkV3RecursiveGreenPointQuery;
          final lastRow = _selectedRecursiveGreenRow(runtime, last);
          _expectSpan(
            lastRow.physicalSource,
            _asciiSpan(
              lastBase.start + coordinateDelta,
              lastBase.end + coordinateDelta,
            ),
          );

          final secondBase = paragraphRanges[paragraphCount * 3 ~/ 4];
          final secondEditStart = secondBase.start + coordinateDelta + 24;
          final secondEdit = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: FlarkV3SourceEdit(
                startUtf16: secondEditStart,
                endUtf16: secondEditStart + 1,
                replacement: 'Q',
              ),
            ),
          );
          expect(secondEdit.changed, isTrue);
          expect(secondEdit.sourceRevision, 3);
          _expectCurrentStatus(
            await _awaitCurrent(
              runtime,
              revision: 3,
              timeout: _largeDocumentTimeout,
            ),
            revision: 3,
          );
          expect(
            runtime.readSourceRange(secondEditStart, secondEditStart + 1),
            'Q',
          );
          expect(
            (runtime.queryAtUtf16(secondEditStart)
                    as FlarkV3RecursiveGreenPointQuery)
                .owner
                .kind,
            FlarkV3RecursiveGreenKind.paragraph,
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 2)),
    );

    test(
      'bounded gap and edit converge to the same current structure',
      () async {
        const initial = 'alpha';
        const edited = 'alpha beta';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );

          final gapResult = runtime.queryAtUtf16(
            2,
            budget: const FlarkV3DocumentQueryBudget(maximumEncodedBytes: 1),
          );
          expect(gapResult, isA<FlarkV3DocumentSourceGapQuery>());
          final gap = gapResult as FlarkV3DocumentSourceGapQuery;
          expect(gap.sourceRevision, 1);
          expect(gap.structureRevision, 1);
          expect(gap.reason, FlarkV3DocumentQueryGapReason.encodedByteLimit);
          _expectSpan(gap.range, (
            startUtf8: 0,
            endUtf8: 5,
            startUtf16: 0,
            endUtf16: 5,
          ));

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: runtime.sourceRevision,
              operation: const FlarkV3SourceEdit(
                startUtf16: 5,
                endUtf16: 5,
                replacement: ' beta',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(runtime.sourceRevision, 2);
          expect(runtime.exportMarkdown(), edited);
          expect(runtime.readSourceRange(6, 10), 'beta');
          expect(runtime.status.sourceRevision, 2);
          expect(runtime.status.sourceCurrent, isFalse);
          expect(runtime.status.structureRevision, 1);
          expect(runtime.status.structureCurrent, isFalse);

          final current = await _awaitCurrent(runtime, revision: 2);
          _expectCurrentStatus(current, revision: 2);
          final result = runtime.queryAtUtf16(6);
          expect(result, isA<FlarkV3RecursiveGreenPointQuery>());
          final point = result as FlarkV3RecursiveGreenPointQuery;
          expect(point.sourceRevision, 2);
          expect(point.structureRevision, 2);
          expect(point.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(point.inlineFacts, isNull);
          final row = _selectedRecursiveGreenRow(runtime, point);
          expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
          expect(
            row.presentationKind,
            FlarkV3RecursiveGreenRowPresentationKind.inline,
          );
          expect(
            row.editCapability,
            FlarkV3RecursiveGreenRowEditCapability.contiguous,
          );
          _expectSpan(row.physicalSource, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));
          _expectSpan(row.editableSource!, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));

          await runtime.close().timeout(_closeTimeout);
          closed = true;
          _expectClosedStatus(runtime.status, revision: 2);
          expect(
            () => runtime.apply(
              FlarkV3SourceTransaction.single(
                baseRevision: runtime.sourceRevision,
                operation: const FlarkV3SourceEdit(
                  startUtf16: 0,
                  endUtf16: 0,
                  replacement: 'late ',
                ),
              ),
            ),
            throwsStateError,
          );
          expect(runtime.exportMarkdown(), edited);
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'fenced code stays exact and literal across a live body edit',
      () async {
        const initial = 'p\n\n```dart\né\n```\n';
        final runtime = await openFlarkV3PublicRuntimeForTest(
          initial,
        ).timeout(_openTimeout);
        var closed = false;
        try {
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 1),
            revision: 1,
          );
          _expectFence(
            runtime,
            runtime.queryAtUtf16(11),
            revision: 1,
            bodyEndUtf8: 14,
          );

          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 1,
              operation: const FlarkV3SourceEdit(
                startUtf16: 12,
                endUtf16: 12,
                replacement: ' **literal**',
              ),
            ),
          );
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, 2);
          expect(
            runtime.exportMarkdown(),
            'p\n\n```dart\né **literal**\n```\n',
          );

          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 2),
            revision: 2,
          );
          _expectFence(
            runtime,
            runtime.queryAtUtf16(12),
            revision: 2,
            bodyEndUtf8: 26,
          );

          final unclosedReceipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: 2,
              operation: const FlarkV3SourceEdit(
                startUtf16: 25,
                endUtf16: 29,
                replacement: '',
              ),
            ),
          );
          expect(unclosedReceipt.changed, isTrue);
          expect(unclosedReceipt.sourceRevision, 3);
          expect(runtime.exportMarkdown(), 'p\n\n```dart\né **literal**\n');
          _expectCurrentStatus(
            await _awaitCurrent(runtime, revision: 3),
            revision: 3,
          );
          _expectFence(
            runtime,
            runtime.queryAtUtf16(12),
            revision: 3,
            bodyEndUtf8: 26,
            closed: false,
          );

          await runtime.close().timeout(_closeTimeout);
          closed = true;
        } finally {
          if (!closed) await runtime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );
  });
}

void _expectFence(
  FlarkV3DocumentRuntime runtime,
  FlarkV3DocumentQueryResult query, {
  required int revision,
  required int bodyEndUtf8,
  bool closed = true,
}) {
  expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
  final result = query as FlarkV3RecursiveGreenPointQuery;
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.owner.kind, FlarkV3RecursiveGreenKind.fencedCode);
  expect(result.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.fencedCode,
  ]);
  expect(result.isIdentityEditableContent, isTrue);
  expect(result.inlineFacts, isNull);

  final range = runtime.queryBlockRange(
    result.pointUtf16,
    result.pointUtf16 + 1,
  );
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final rows = range as FlarkV3RecursiveGreenRowRange;
  expect(rows.sourceRevision, revision);
  expect(rows.structureRevision, revision);
  expect(rows.selectedRow, isNotNull);
  final row = rows.selectedRow!;
  expect(row.frameId, result.owner.frameId);
  expect(row.kind, FlarkV3RecursiveGreenKind.fencedCode);
  expect(row.literal, isTrue);
  expect(
    row.presentationKind,
    FlarkV3RecursiveGreenRowPresentationKind.fencedCode,
  );
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  expect(row.editableSource, isNotNull);

  final owner = row.path.singleWhere((frame) => frame.isRowOwner);
  expect(owner.frameId, row.frameId);
  expect(owner.fact, isA<FlarkV3RecursiveGreenCodePathFact>());
  final fence = owner.fact! as FlarkV3RecursiveGreenCodePathFact;
  expect(fence.marker, FlarkV3CodeFenceMarker.backtick);
  expect(fence.fenceOffsetColumns, 0);
  expect(fence.minimumClosingLength, BigInt.from(3));

  final bodyEndUtf16 = bodyEndUtf8 - 1;
  _expectSpan(row.editableSource!, (
    startUtf8: 11,
    endUtf8: bodyEndUtf8,
    startUtf16: 11,
    endUtf16: bodyEndUtf16,
  ));
  _expectSpan(row.physicalSource, (
    startUtf8: 3,
    endUtf8: closed ? bodyEndUtf8 + 4 : bodyEndUtf8,
    startUtf16: 3,
    endUtf16: closed ? bodyEndUtf16 + 4 : bodyEndUtf16,
  ));
}

void _expectThematicBreak(
  FlarkV3DocumentRuntime runtime,
  FlarkV3DocumentQueryResult query, {
  required int revision,
}) {
  expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
  final result = query as FlarkV3RecursiveGreenPointQuery;
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.owner.kind, FlarkV3RecursiveGreenKind.thematicBreak);
  expect(result.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.thematicBreak,
  ]);
  expect(result.paragraphSource, isNull);
  expect(result.inlineSource, isNull);
  expect(
    result.inlineFacts,
    isNull,
    reason: 'an atomic marker-free block cannot carry inline facts',
  );
  expect(result.blockQuoteProjection, isNull);
  expect(result.projectedInlineFacts, isNull);

  final range = runtime.queryBlockRange(
    result.pointUtf16,
    result.pointUtf16 + 1,
  );
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final rows = range as FlarkV3RecursiveGreenRowRange;
  expect(rows.sourceRevision, revision);
  expect(rows.structureRevision, revision);
  expect(rows.selectedRow, isNotNull);
  final row = rows.selectedRow!;
  expect(row.frameId, result.owner.frameId);
  expect(row.kind, FlarkV3RecursiveGreenKind.thematicBreak);
  expect(row.selected, isTrue);
  expect(row.inlineCapable, isFalse);
  expect(
    row.presentationKind,
    FlarkV3RecursiveGreenRowPresentationKind.thematicBreak,
  );
  expect(
    row.editCapability,
    FlarkV3RecursiveGreenRowEditCapability.contiguous,
    reason: 'an atomic marker-free row exposes only a collapsed edit boundary',
  );
  expect(row.editableSource, isNotNull);
  _expectSpan(row.editableSource!, _asciiSpan(0, 0));
  _expectSpan(row.physicalSource, _asciiSpan(0, 10));
  _expectSpan(row.presentationPhysicalSource, _asciiSpan(0, 10));
  expect(row.path.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.thematicBreak,
  ]);
  expect(row.path.last.isRowOwner, isTrue);
}

void _expectRecursiveGreenParagraphRow(
  FlarkV3DocumentRuntime runtime, {
  required int positionUtf16,
  required FlarkV3DocumentQueryAffinity affinity,
  required int revision,
  required _ExpectedSpan source,
}) {
  final query = runtime.queryAtUtf16(positionUtf16, affinity: affinity);
  expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
  final point = query as FlarkV3RecursiveGreenPointQuery;
  expect(point.sourceRevision, revision);
  expect(point.structureRevision, revision);
  expect(point.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(point.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.paragraph,
  ]);
  expect(point.inlineFacts, isNull);

  final range = runtime.queryBlockRange(point.pointUtf16, point.pointUtf16 + 1);
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final rows = range as FlarkV3RecursiveGreenRowRange;
  expect(rows.sourceRevision, revision);
  expect(rows.structureRevision, revision);
  expect(rows.selectedRow, isNotNull);
  final row = rows.selectedRow!;
  expect(row.frameId, point.owner.frameId);
  expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(row.selected, isTrue);
  expect(row.inlineCapable, isTrue);
  expect(row.presentationKind, FlarkV3RecursiveGreenRowPresentationKind.inline);
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  _expectSpan(row.physicalSource, source);
  expect(row.editableSource, isNotNull);
  _expectSpan(row.editableSource!, (
    startUtf8: source.startUtf8,
    endUtf8: source.endUtf8 - 1,
    startUtf16: source.startUtf16,
    endUtf16: source.endUtf16 - 1,
  ));
  expect(row.path.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.paragraph,
  ]);
}

void _expectAtxHeading(
  FlarkV3DocumentRuntime runtime,
  FlarkV3RecursiveGreenPointQuery result, {
  required int revision,
  required _ExpectedSpan source,
  required _ExpectedSpan content,
}) {
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.owner.kind, FlarkV3RecursiveGreenKind.heading);
  expect(result.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.heading,
  ]);
  final row = _selectedRecursiveGreenRow(runtime, result);
  expect(row.kind, FlarkV3RecursiveGreenKind.heading);
  expect(row.presentationKind, FlarkV3RecursiveGreenRowPresentationKind.inline);
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  _expectSpan(row.physicalSource, source);
  _expectSpan(row.editableSource!, content);
  final owner = row.path.singleWhere((frame) => frame.isRowOwner);
  expect(owner.fact, isA<FlarkV3RecursiveGreenHeadingPathFact>());
  final heading = owner.fact! as FlarkV3RecursiveGreenHeadingPathFact;
  expect(heading.level, 2);
  expect(heading.style, FlarkV3RecursiveGreenHeadingStyle.atx);
}

void _expectSetextHeading(
  FlarkV3DocumentRuntime runtime,
  FlarkV3RecursiveGreenPointQuery result, {
  required int revision,
  required _ExpectedSpan source,
  required _ExpectedSpan content,
}) {
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.owner.kind, FlarkV3RecursiveGreenKind.heading);
  expect(result.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.heading,
  ]);
  final row = _selectedRecursiveGreenRow(runtime, result);
  expect(row.kind, FlarkV3RecursiveGreenKind.heading);
  expect(row.presentationKind, FlarkV3RecursiveGreenRowPresentationKind.inline);
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  _expectSpan(row.physicalSource, source);
  _expectSpan(row.editableSource!, content);
  final owner = row.path.singleWhere((frame) => frame.isRowOwner);
  expect(owner.fact, isA<FlarkV3RecursiveGreenHeadingPathFact>());
  final heading = owner.fact! as FlarkV3RecursiveGreenHeadingPathFact;
  expect(heading.level, 2);
  expect(heading.style, FlarkV3RecursiveGreenHeadingStyle.setext);
}

FlarkV3RecursiveGreenRenderableRow _selectedRecursiveGreenRow(
  FlarkV3DocumentRuntime runtime,
  FlarkV3RecursiveGreenPointQuery query,
) {
  final range = runtime.queryBlockRange(query.pointUtf16, query.pointUtf16 + 1);
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final rows = range as FlarkV3RecursiveGreenRowRange;
  expect(rows.sourceRevision, query.sourceRevision);
  expect(rows.structureRevision, query.structureRevision);
  expect(rows.selectedRow, isNotNull);
  final row = rows.selectedRow!;
  expect(row.frameId, query.owner.frameId);
  return row;
}

Future<FlarkV3DocumentRuntimeStatus> _awaitCurrent(
  FlarkV3DocumentRuntime runtime, {
  required int revision,
  Duration timeout = _openTimeout,
}) {
  final status = runtime.status;
  if (status.sourceRevision == revision &&
      status.sourceCurrent &&
      status.structureRevision == revision &&
      status.structureCurrent) {
    return Future<FlarkV3DocumentRuntimeStatus>.value(status);
  }
  return runtime.statuses
      .firstWhere(
        (candidate) =>
            candidate.sourceRevision == revision &&
            candidate.sourceCurrent &&
            candidate.structureRevision == revision &&
            candidate.structureCurrent,
      )
      .timeout(timeout);
}

Future<FlarkV3DocumentRuntimeStatus> _awaitStatus(
  FlarkV3DocumentRuntime runtime,
  bool Function(FlarkV3DocumentRuntimeStatus status) predicate,
) {
  final current = runtime.status;
  if (predicate(current)) {
    return Future<FlarkV3DocumentRuntimeStatus>.value(current);
  }
  return runtime.statuses.firstWhere(predicate).timeout(_openTimeout);
}

void _expectCurrentStatus(
  FlarkV3DocumentRuntimeStatus status, {
  required int revision,
}) {
  expect(status.state, FlarkV3DocumentRuntimeState.open);
  expect(status.sourceRevision, revision);
  expect(status.sourceCurrent, isTrue);
  expect(status.structureRevision, revision);
  expect(status.structureCurrent, isTrue);
}

void _expectClosedStatus(
  FlarkV3DocumentRuntimeStatus status, {
  required int revision,
}) {
  expect(status.state, FlarkV3DocumentRuntimeState.closed);
  expect(status.sourceRevision, revision);
  expect(status.sourceCurrent, isTrue);
  expect(status.structureRevision, revision);
  expect(status.structureCurrent, isTrue);
}

void _expectExactLiteralPoint(
  FlarkV3DocumentRuntime runtime, {
  required int positionUtf16,
  required FlarkV3DocumentQueryAffinity affinity,
  required int revision,
  required FlarkV3DocumentStructureKind kind,
  required _ExpectedSpan source,
  _ExpectedSpan? visibleSource,
  FlarkV3DocumentUnknownReason? unknownReason,
}) {
  final query = runtime.queryAtUtf16(positionUtf16, affinity: affinity);
  expect(query, isA<FlarkV3RecursiveGreenPointQuery>());
  final point = query as FlarkV3RecursiveGreenPointQuery;
  expect(point.sourceRevision, revision);
  expect(point.structureRevision, revision);
  expect(point.inlineFacts, isNull);

  if (kind == FlarkV3DocumentStructureKind.unknown) {
    expect(unknownReason, FlarkV3DocumentUnknownReason.blankBoundary);
    expect(point.owner.kind, FlarkV3RecursiveGreenKind.document);
    expect(point.coveragePart, FlarkV3RecursiveGreenCoveragePart.gap);
    expect(point.logicalAtom.kind, FlarkV3RecursiveGreenLogicalAtomKind.none);
    expect(point.ancestry.map((frame) => frame.kind), const [
      FlarkV3RecursiveGreenKind.document,
    ]);
    _expectSpan(point.source, source);
    expect(visibleSource, isNotNull);
    expect(visibleSource!.startUtf8, visibleSource.endUtf8);
    expect(visibleSource.startUtf16, visibleSource.endUtf16);
    return;
  }

  expect(kind, FlarkV3DocumentStructureKind.paragraph);
  expect(unknownReason, isNull);
  expect(point.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(point.ancestry.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.paragraph,
  ]);
  expect(point.source.startUtf8, greaterThanOrEqualTo(source.startUtf8));
  expect(point.source.endUtf8, lessThanOrEqualTo(source.endUtf8));
  expect(point.source.startUtf16, greaterThanOrEqualTo(source.startUtf16));
  expect(point.source.endUtf16, lessThanOrEqualTo(source.endUtf16));

  final range = runtime.queryBlockRange(
    point.source.startUtf16,
    point.source.endUtf16,
  );
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final rows = range as FlarkV3RecursiveGreenRowRange;
  expect(rows.sourceRevision, revision);
  expect(rows.structureRevision, revision);
  expect(rows.selectedRow, isNotNull);
  final row = rows.selectedRow!;
  expect(row.frameId, point.owner.frameId);
  expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(row.selected, isTrue);
  expect(row.inlineCapable, isTrue);
  expect(row.literal, isFalse);
  expect(row.presentationKind, FlarkV3RecursiveGreenRowPresentationKind.inline);
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  _expectSpan(row.physicalSource, source);
  expect(row.editableSource, isNotNull);
  final physicalText = runtime.readSourceRange(
    source.startUtf16,
    source.endUtf16,
  );
  final terminalLineEnding = physicalText.endsWith('\r\n')
      ? 2
      : physicalText.endsWith('\n') || physicalText.endsWith('\r')
      ? 1
      : 0;
  _expectSpan(row.editableSource!, (
    startUtf8: source.startUtf8,
    endUtf8: source.endUtf8 - terminalLineEnding,
    startUtf16: source.startUtf16,
    endUtf16: source.endUtf16 - terminalLineEnding,
  ));
  expect(row.path.map((frame) => frame.kind), const [
    FlarkV3RecursiveGreenKind.document,
    FlarkV3RecursiveGreenKind.paragraph,
  ]);
  expect(row.path.last.isRowOwner, isTrue);
}

_ExpectedSpan _asciiSpan(int start, int end) =>
    (startUtf8: start, endUtf8: end, startUtf16: start, endUtf16: end);

void _expectSpan(FlarkV3SourceSpan actual, _ExpectedSpan expected) {
  expect(actual.startUtf8, expected.startUtf8);
  expect(actual.endUtf8, expected.endUtf8);
  expect(actual.startUtf16, expected.startUtf16);
  expect(actual.endUtf16, expected.endUtf16);
}

void _expectBlockQuoteRecord(
  FlarkV3BlockQuoteLineProjectionRecord record, {
  required int relativeStartUtf8,
  required _ExpectedSpan physical,
  required _ExpectedSpan hidden,
  required _ExpectedSpan content,
  required _ExpectedSpan lineEnding,
  required FlarkV3BlockQuoteLineProjectionKind kind,
}) {
  expect(record.relativeLineStartUtf8, relativeStartUtf8);
  expect(record.kind, kind);
  _expectSpan(record.physicalSource, physical);
  _expectSpan(record.hiddenPrefix, hidden);
  _expectSpan(record.content, content);
  _expectSpan(record.lineEnding, lineEnding);
}
