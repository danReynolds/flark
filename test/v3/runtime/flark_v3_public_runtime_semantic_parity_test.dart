import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3InlineDemandDisposition,
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
  FlarkV3DocumentStructureKind kind,
  FlarkV3DocumentUnknownReason? unknownReason,
  _ExpectedSpan source,
  _ExpectedSpan visibleSource,
  _ExpectedSpan projectedSource,
  int referenceDefinitionCount,
  int projectionRunCount,
});

const _fixtures = <_SemanticFixture>[
  (
    name: 'empty document',
    markdown: '',
    initialRevision: 0,
    queryPositionUtf16: 0,
    kind: FlarkV3DocumentStructureKind.empty,
    unknownReason: null,
    source: (startUtf8: 0, endUtf8: 0, startUtf16: 0, endUtf16: 0),
    visibleSource: (startUtf8: 0, endUtf8: 0, startUtf16: 0, endUtf16: 0),
    projectedSource: (startUtf8: 0, endUtf8: 0, startUtf16: 0, endUtf16: 0),
    referenceDefinitionCount: 0,
    projectionRunCount: 0,
  ),
  (
    name: 'paragraph after a leading reference definition',
    markdown: '[x]: /target\nCafé 😀 [x]\n',
    initialRevision: 1,
    queryPositionUtf16: 13,
    kind: FlarkV3DocumentStructureKind.paragraph,
    unknownReason: null,
    source: (startUtf8: 0, endUtf8: 28, startUtf16: 0, endUtf16: 25),
    visibleSource: (startUtf8: 13, endUtf8: 28, startUtf16: 13, endUtf16: 25),
    projectedSource: (startUtf8: 13, endUtf8: 28, startUtf16: 13, endUtf16: 25),
    referenceDefinitionCount: 1,
    projectionRunCount: 1,
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
          expect(result, isA<FlarkV3DocumentStructuralQuery>());
          _expectStructure(
            result as FlarkV3DocumentStructuralQuery,
            fixture,
            revision: fixture.initialRevision,
          );

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
        final liveLease = FlarkV3DocumentRuntimeAdapter.borrow(
          liveRuntime,
          leafProjectionDemandOwner: true,
        );
        final cleanLease = FlarkV3DocumentRuntimeAdapter.borrow(
          cleanRuntime,
          leafProjectionDemandOwner: true,
        );
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

          Future<FlarkV3DocumentStructuralQuery> demandTerminalItem(
            FlarkV3DocumentRuntime runtime,
            FlarkV3DocumentRuntimeAdapterLease lease, {
            required int revision,
          }) async {
            final initialQuery = lease.queryAtUtf16(target.length);
            expect(initialQuery, isA<FlarkV3DocumentStructuralQuery>());
            final structural = initialQuery as FlarkV3DocumentStructuralQuery;
            expect(
              structural.structure.kind,
              FlarkV3DocumentStructureKind.bulletList,
            );
            expect(structural.bulletListProjection, isNull);
            final beforePresentation =
                runtime.status.leafProjectionPresentationGeneration;
            final beforeOutcome =
                runtime.status.leafProjectionAttemptOutcomeGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.leafProjectionPresentationGeneration >
                      beforePresentation ||
                  status.leafProjectionAttemptOutcomeGeneration >
                      beforeOutcome ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureLeafProjectionAtUtf16(
                target.length,
                structuralQuery: structural,
              ),
              FlarkV3LeafProjectionDemandDisposition.scheduled,
            );
            final settledStatus = await settled;
            expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
            expect(
              settledStatus.leafProjectionPresentationGeneration,
              beforePresentation + 1,
            );
            expect(
              settledStatus.leafProjectionAttemptOutcomeGeneration,
              beforeOutcome + 1,
            );
            final refined = lease.queryAtUtf16(target.length);
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.bulletListProjection, isNotNull);
            return result;
          }

          void expectTerminalList(
            FlarkV3DocumentStructuralQuery result, {
            required int revision,
          }) {
            expect(result.sourceRevision, revision);
            expect(
              result.structure.kind,
              FlarkV3DocumentStructureKind.bulletList,
            );
            _expectSpan(result.structure.source, (
              startUtf8: 15,
              endUtf8: 31,
              startUtf16: 13,
              endUtf16: 26,
            ));
            _expectSpan(result.structure.visibleSource, (
              startUtf8: 15,
              endUtf8: 15,
              startUtf16: 13,
              endUtf16: 13,
            ));
            final facts = result.structure.bulletList!;
            expect(facts.marker, FlarkV3BulletListMarker.hyphen);
            expect(facts.tight, isTrue);
            expect(facts.itemCount, 2);
            expect(facts.paragraphCount, 1);
            expect(facts.terminalEmptyRelativeStartUtf8, 12);
            expect(facts.projectedUtf8Length, 8);
            expect(facts.projectedUtf16Length, 5);
            expect(result.projection.runCount, 2);

            final payload = result.bulletListProjection!;
            expect(payload.selectedItemOrdinal, 1);
            expect(payload.coversWholeList, isFalse);
            expect(payload.records, hasLength(1));
            expect(payload.selectedItem.isEmpty, isTrue);
            expect(payload.toSourceProjection().displayText, '');
            expect(payload.toSelectedItemSourceProjection().displayText, '');
            expect(payload.editingInputs.activeHiddenSourcePrefix, '  - ');
            expect(payload.editingInputs.activeRemovableSourcePrefix, '  - ');
            expect(
              payload.editingInputs.activeRemovableSourcePrefixOffsetUtf16,
              0,
            );
            expect(payload.editingInputs.continuationSourcePrefix, '  - ');
            expect(payload.editingInputs.canonicalLineEnding, '\r\n');
            expect(payload.editingInputs.emptyEnterExits, isTrue);
            expect(payload.editingInputs.backspaceAtStartRemovesPrefix, isTrue);
            expect(payload.pointPath.nodes, hasLength(2));
            expect(
              payload.pointPath.root.kind,
              FlarkV3DocumentPointPathNodeKind.list,
            );
            expect(
              payload.pointPath.selectedLeaf.kind,
              FlarkV3DocumentPointPathNodeKind.listItem,
            );
          }

          final live = await demandTerminalItem(
            liveRuntime,
            liveLease,
            revision: 2,
          );
          final clean = await demandTerminalItem(
            cleanRuntime,
            cleanLease,
            revision: 1,
          );
          expectTerminalList(live, revision: 2);
          expectTerminalList(clean, revision: 1);
          expect(
            live.bulletListProjection!.toSourceProjection().displayText,
            clean.bulletListProjection!.toSourceProjection().displayText,
          );
          expect(liveRuntime.exportMarkdown(), cleanRuntime.exportMarkdown());

          liveLease.release();
          cleanLease.release();
          await liveRuntime.close().timeout(_closeTimeout);
          liveClosed = true;
          await cleanRuntime.close().timeout(_closeTimeout);
          cleanClosed = true;
        } finally {
          liveLease.release();
          cleanLease.release();
          if (!liveClosed) await liveRuntime.close().timeout(_closeTimeout);
          if (!cleanClosed) await cleanRuntime.close().timeout(_closeTimeout);
        }
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'unsupported list families remain literal and never mint list authority',
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
            final query = runtime.queryAtUtf16(2);
            expect(query, isA<FlarkV3DocumentSourceGapQuery>(), reason: name);
            final gap = query as FlarkV3DocumentSourceGapQuery;
            expect(gap.sourceRevision, 1, reason: name);
            expect(gap.structureRevision, 1, reason: name);
            expect(
              gap.reason,
              FlarkV3DocumentQueryGapReason.undecodableClosure,
              reason: name,
            );
            _expectSpan(gap.range, _asciiSpan(0, markdown.length));
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
      'block quote demands one schema-4 path and projects marker-free source',
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
          expect(initialQuery, isA<FlarkV3DocumentStructuralQuery>());
          final initial = initialQuery as FlarkV3DocumentStructuralQuery;
          expect(initial.sourceRevision, 1);
          expect(initial.structureRevision, 1);
          expect(
            initial.structure.kind,
            FlarkV3DocumentStructureKind.blockQuote,
          );
          expect(initial.structure.referenceDefinitionCount, 0);
          _expectSpan(initial.structure.source, source);
          _expectSpan(initial.structure.visibleSource, _asciiSpan(0, 0));
          expect(
            initial.projection.kind,
            FlarkV3DocumentStructureKind.blockQuote,
          );
          _expectSpan(initial.projection.source, source);
          _expectSpan(initial.projection.projectedSource, _asciiSpan(0, 0));
          expect(initial.projection.runCount, 3);
          expect(initial.inlineFacts, isNull);
          expect(initial.indentedCodeProjection, isNull);
          expect(
            initial.pointPath,
            isNull,
            reason: 'canonical structure must precede the demanded path',
          );
          expect(initial.blockQuoteProjection, isNull);

          final facts = initial.structure.blockQuote;
          expect(facts, isNotNull);
          expect(facts!.lineCount, 3);
          expect(facts.childFirstLine, 0);
          expect(facts.childLineCount, 3);
          expect(facts.projectedUtf8Length, 20);
          expect(facts.projectedUtf16Length, 14);

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
            lease.ensureLeafProjectionAtUtf16(6, structuralQuery: initial),
            FlarkV3LeafProjectionDemandDisposition.scheduled,
          );
          expect(
            lease.ensureLeafProjectionAtUtf16(6, structuralQuery: initial),
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
          expect(refinedQuery, isA<FlarkV3DocumentStructuralQuery>());
          final refined = refinedQuery as FlarkV3DocumentStructuralQuery;
          expect(
            refined.structure.kind,
            FlarkV3DocumentStructureKind.blockQuote,
          );
          final pointPath = refined.pointPath;
          final payload = refined.blockQuoteProjection;
          expect(
            pointPath,
            isNotNull,
            reason: 'schema 4 must retain exact outer-to-inner ancestry',
          );
          expect(
            payload,
            isNotNull,
            reason: 'schema 4 must carry the demanded quote-line recipe',
          );
          expect(payload!.pointPath, same(pointPath));
          expect(payload.facts, same(refined.structure.blockQuote));
          expect(payload.sourceRevision, 1);
          expect(payload.sourceVersion.metric.bytes, source.endUtf8);
          expect(payload.sourceVersion.metric.utf16, source.endUtf16);
          _expectSpan(payload.source, source);

          final ancestor = pointPath!.blockQuoteAncestor;
          expect(ancestor.kind, FlarkV3DocumentPointPathNodeKind.blockQuote);
          expect(ancestor.depth, 0);
          expect(ancestor.parentIndex, isNull);
          expect(ancestor.isNoncontiguous, isFalse);
          expect(ancestor.isSelected, isFalse);
          _expectSpan(ancestor.source, source);
          expect(ancestor.firstRun, 0);
          expect(ancestor.runCount, 3);
          expect(ancestor.projectedUtf8Length, 20);
          expect(ancestor.projectedUtf16Length, 14);

          final selected = pointPath.selectedLeaf;
          expect(selected.kind, FlarkV3DocumentPointPathNodeKind.paragraph);
          expect(selected.depth, 1);
          expect(selected.parentIndex, 0);
          expect(selected.isNoncontiguous, isTrue);
          expect(selected.isSelected, isTrue);
          _expectSpan(selected.source, source);
          expect(selected.firstRun, 0);
          expect(selected.runCount, 3);
          expect(selected.projectedUtf8Length, 20);
          expect(selected.projectedUtf16Length, 14);

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
          expect(runtime.exportMarkdown(), markdown);
          expect(
            lease.ensureLeafProjectionAtUtf16(6, structuralQuery: refined),
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
          _expectThematicBreak(runtime.queryAtUtf16(4), revision: 1);

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
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 1,
            kind: FlarkV3DocumentStructureKind.paragraph,
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
          _expectThematicBreak(runtime.queryAtUtf16(4), revision: 2);

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
          _expectExactLiteralPoint(
            runtime,
            positionUtf16: 2,
            affinity: FlarkV3DocumentQueryAffinity.downstream,
            revision: 3,
            kind: FlarkV3DocumentStructureKind.paragraph,
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
        final headingEdit = initial.indexOf('heading');
        final fenceEdit = initial.indexOf('value = 1') + 'value = '.length;
        final closingFence = initial.indexOf('\n```\n\n', fenceEdit) + 1;
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

          final initialHeading = runtime.queryAtUtf16(headingEdit + 1);
          expect(initialHeading, isA<FlarkV3DocumentStructuralQuery>());
          expect(
            (initialHeading as FlarkV3DocumentStructuralQuery).structure.kind,
            FlarkV3DocumentStructureKind.heading,
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
          final editedHeading = runtime.queryAtUtf16(headingEdit + 1);
          expect(editedHeading, isA<FlarkV3DocumentStructuralQuery>());
          expect(
            (editedHeading as FlarkV3DocumentStructuralQuery).structure.kind,
            FlarkV3DocumentStructureKind.heading,
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
          final editedFence = runtime.queryAtUtf16(fenceEdit);
          expect(editedFence, isA<FlarkV3DocumentStructuralQuery>());
          expect(
            (editedFence as FlarkV3DocumentStructuralQuery).structure.kind,
            FlarkV3DocumentStructureKind.fencedCode,
          );
          expect(runtime.readSourceRange(fenceEdit, fenceEdit + 1), '2');

          for (final point in [
            4,
            headingEdit + 1,
            fenceEdit,
            lastParagraph + 4,
          ]) {
            final query = runtime.queryAtUtf16(point);
            expect(
              query,
              isA<FlarkV3DocumentStructuralQuery>(),
              reason: 'every retained or replaced block must remain queryable',
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
          final unclosedFence = runtime.queryAtUtf16(fenceEdit);
          expect(unclosedFence, isA<FlarkV3DocumentStructuralQuery>());
          final unclosed = (unclosedFence as FlarkV3DocumentStructuralQuery)
              .structure
              .fencedCode;
          expect(unclosed, isNotNull);
          expect(unclosed!.closed, isFalse);
          expect(unclosed.closingMarker, isNull);
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

          Future<FlarkV3DocumentStructuralQuery> demandInline({
            required int revision,
          }) async {
            final structural = lease.queryAtUtf16(3);
            expect(structural, isA<FlarkV3DocumentStructuralQuery>());
            final paragraph = structural as FlarkV3DocumentStructuralQuery;
            expect(
              paragraph.structure.kind,
              FlarkV3DocumentStructureKind.paragraph,
            );
            expect(paragraph.sourceRevision, revision);
            expect(paragraph.structureRevision, revision);
            _expectSpan(paragraph.structure.source, _asciiSpan(0, 7));
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
              lease.ensureInlineAtUtf16(3, structuralQuery: paragraph),
              FlarkV3InlineDemandDisposition.scheduled,
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
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertifiedFacts(
            FlarkV3DocumentStructuralQuery result, {
            required int revision,
          }) {
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, 7));
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
            (lease.queryAtUtf16(3) as FlarkV3DocumentStructuralQuery)
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

          Future<FlarkV3DocumentStructuralQuery> demandInline({
            required int revision,
          }) async {
            final structural = lease.queryAtUtf16(1);
            expect(structural, isA<FlarkV3DocumentStructuralQuery>());
            final paragraph = structural as FlarkV3DocumentStructuralQuery;
            expect(
              paragraph.structure.kind,
              FlarkV3DocumentStructureKind.paragraph,
            );
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
              lease.ensureInlineAtUtf16(1, structuralQuery: paragraph),
              FlarkV3InlineDemandDisposition.scheduled,
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
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertified(
            FlarkV3DocumentStructuralQuery result, {
            required int revision,
            required bool backslashMarker,
            required String source,
          }) {
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, source.length));
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
            expect(projection.sourceText, source);
            expect(projection.sourceProjection.sourceText, source);
            expect(projection.displayText, 'a\nb\n');
            expect(projection.sourceProjection.displayText, 'a\nb\n');
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
            lease.ensureInlineAtUtf16(1, structuralQuery: revisionOne),
            FlarkV3InlineDemandDisposition.notApplicable,
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
            (lease.queryAtUtf16(1) as FlarkV3DocumentStructuralQuery)
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

          Future<FlarkV3DocumentStructuralQuery> demandInline({
            required int revision,
          }) async {
            final structural = lease.queryAtUtf16(2);
            expect(structural, isA<FlarkV3DocumentStructuralQuery>());
            final paragraph = structural as FlarkV3DocumentStructuralQuery;
            expect(
              paragraph.structure.kind,
              FlarkV3DocumentStructureKind.paragraph,
            );
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
              lease.ensureInlineAtUtf16(2, structuralQuery: paragraph),
              FlarkV3InlineDemandDisposition.scheduled,
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
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.sourceRevision, revision);
            expect(result.structureRevision, revision);
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          void expectCertified(
            FlarkV3DocumentStructuralQuery result, {
            required int revision,
            required String source,
            required String firstValue,
          }) {
            final inline = result.inlineFacts!;
            expect(inline.sourceRevision, revision);
            expect(
              inline.disposition,
              FlarkV3InlineFactsDisposition.authoritative,
            );
            _expectSpan(inline.source, _asciiSpan(0, source.length));
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
            expect(visible.displayText, source);
            final markerFree = FlarkV3InlineProjection.fromValidatedFacts(
              sourceDocument: exactSource,
              expectedSource: inline.sourceVersion,
              facts: inline,
              markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
            );
            expect(markerFree.sourceText, source);
            expect(markerFree.displayText, '$firstValue ≧\u{338}\n');
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
            (lease.queryAtUtf16(2) as FlarkV3DocumentStructuralQuery)
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

          final structural = lease.queryAtUtf16(middlePoint);
          expect(structural, isA<FlarkV3DocumentStructuralQuery>());
          final middle = structural as FlarkV3DocumentStructuralQuery;
          expect(middle.structure.kind, FlarkV3DocumentStructureKind.paragraph);
          _expectSpan(
            middle.projection.projectedSource,
            _asciiSpan(middleStart, middleEnd),
          );
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
            lease.ensureInlineAtUtf16(middlePoint, structuralQuery: middle),
            FlarkV3InlineDemandDisposition.scheduled,
          );
          final committedStatus = await committed;
          expect(committedStatus.state, FlarkV3DocumentRuntimeState.open);
          expect(
            committedStatus.inlinePresentationGeneration,
            beforeInline + 1,
          );

          final refined = lease.queryAtUtf16(middlePoint);
          expect(refined, isA<FlarkV3DocumentStructuralQuery>());
          final inline =
              (refined as FlarkV3DocumentStructuralQuery).inlineFacts;
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
          _expectSpan(inline.source, _asciiSpan(middleStart, middleEnd));

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

          final first = lease.queryAtUtf16(1);
          final tail = lease.queryAtUtf16(markdown.length - 2);
          expect(
            (first as FlarkV3DocumentStructuralQuery).inlineFacts,
            isNull,
            reason: 'a selected-leaf sidecar cannot attach to its neighbor',
          );
          expect(
            (tail as FlarkV3DocumentStructuralQuery).inlineFacts,
            isNull,
            reason: 'equal query authority is still block-identity scoped',
          );

          Future<FlarkV3DocumentStructuralQuery> demandLeaf(
            int positionUtf16,
            FlarkV3DocumentStructuralQuery structural,
          ) async {
            final before = runtime.status.inlinePresentationGeneration;
            final settled = _awaitStatus(
              runtime,
              (status) =>
                  status.inlinePresentationGeneration > before ||
                  status.state == FlarkV3DocumentRuntimeState.faulted,
            );
            expect(
              lease.ensureInlineAtUtf16(
                positionUtf16,
                structuralQuery: structural,
              ),
              FlarkV3InlineDemandDisposition.scheduled,
            );
            final status = await settled;
            expect(status.state, FlarkV3DocumentRuntimeState.open);
            final refined = lease.queryAtUtf16(positionUtf16);
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          final refinedTail = await demandLeaf(markdown.length - 2, tail);
          expect(
            refinedTail.inlineFacts!.facts.single.kind,
            FlarkV3InlineFactKind.code,
          );
          final retainedMiddle =
              lease.queryAtUtf16(middlePoint) as FlarkV3DocumentStructuralQuery;
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
                    as FlarkV3DocumentStructuralQuery)
                .inlineFacts!
                .facts
                .single
                .kind,
            FlarkV3InlineFactKind.code,
          );
          final cachedMiddle =
              lease.queryAtUtf16(middlePoint) as FlarkV3DocumentStructuralQuery;
          final afterThreeDemands = runtime.status.inlinePresentationGeneration;
          expect(
            lease.ensureInlineAtUtf16(
              middlePoint,
              structuralQuery: cachedMiddle,
            ),
            FlarkV3InlineDemandDisposition.notApplicable,
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
            (lease.queryAtUtf16(middlePoint) as FlarkV3DocumentStructuralQuery)
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

          Future<FlarkV3DocumentStructuralQuery> demandInline(
            int positionUtf16,
            FlarkV3DocumentStructuralQuery structural,
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
              lease.ensureInlineAtUtf16(
                positionUtf16,
                structuralQuery: structural,
              ),
              FlarkV3InlineDemandDisposition.scheduled,
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
            expect(refined, isA<FlarkV3DocumentStructuralQuery>());
            final result = refined as FlarkV3DocumentStructuralQuery;
            expect(result.inlineFacts, isNotNull);
            return result;
          }

          final initialPosition = initial.indexOf('live') + 1;
          final initialQuery = lease.queryAtUtf16(initialPosition);
          expect(initialQuery, isA<FlarkV3DocumentStructuralQuery>());
          final initialHeading = initialQuery as FlarkV3DocumentStructuralQuery;
          _expectAtxHeading(
            initialHeading,
            revision: 1,
            source: (startUtf8: 0, endUtf8: 34, startUtf16: 0, endUtf16: 31),
            content: (startUtf8: 3, endUtf8: 28, startUtf16: 3, endUtf16: 25),
            opening: _asciiSpan(0, 2),
            closing: (startUtf8: 29, endUtf8: 32, startUtf16: 26, endUtf16: 29),
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
          expect(editedQuery, isA<FlarkV3DocumentStructuralQuery>());
          final editedHeading = editedQuery as FlarkV3DocumentStructuralQuery;
          _expectAtxHeading(
            editedHeading,
            revision: 2,
            source: (startUtf8: 0, endUtf8: 35, startUtf16: 0, endUtf16: 32),
            content: (startUtf8: 3, endUtf8: 29, startUtf16: 3, endUtf16: 26),
            opening: _asciiSpan(0, 2),
            closing: (startUtf8: 30, endUtf8: 33, startUtf16: 27, endUtf16: 30),
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

          Future<FlarkV3DocumentStructuralQuery> demandInline(
            int positionUtf16,
            FlarkV3DocumentStructuralQuery structural,
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
              lease.ensureInlineAtUtf16(
                positionUtf16,
                structuralQuery: structural,
              ),
              FlarkV3InlineDemandDisposition.scheduled,
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
                as FlarkV3DocumentStructuralQuery;
          }

          final initialPosition = initial.indexOf('live') + 1;
          final initialQuery =
              lease.queryAtUtf16(initialPosition)
                  as FlarkV3DocumentStructuralQuery;
          _expectSetextHeading(
            initialQuery,
            revision: 1,
            source: (startUtf8: 0, endUtf8: 32, startUtf16: 0, endUtf16: 29),
            content: (startUtf8: 0, endUtf8: 25, startUtf16: 0, endUtf16: 22),
            contentLineEnding: (
              startUtf8: 25,
              endUtf8: 27,
              startUtf16: 22,
              endUtf16: 24,
            ),
            underline: (
              startUtf8: 27,
              endUtf8: 30,
              startUtf16: 24,
              endUtf16: 27,
            ),
            underlineLineEnding: (
              startUtf8: 30,
              endUtf8: 32,
              startUtf16: 27,
              endUtf16: 29,
            ),
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
                  as FlarkV3DocumentStructuralQuery;
          _expectSetextHeading(
            editedQuery,
            revision: 2,
            source: (startUtf8: 0, endUtf8: 33, startUtf16: 0, endUtf16: 30),
            content: (startUtf8: 0, endUtf8: 26, startUtf16: 0, endUtf16: 23),
            contentLineEnding: (
              startUtf8: 26,
              endUtf8: 28,
              startUtf16: 23,
              endUtf16: 25,
            ),
            underline: (
              startUtf8: 28,
              endUtf8: 31,
              startUtf16: 25,
              endUtf16: 28,
            ),
            underlineLineEnding: (
              startUtf8: 31,
              endUtf8: 33,
              startUtf16: 28,
              endUtf16: 30,
            ),
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
          expect(structural, isA<FlarkV3DocumentStructuralQuery>());
          final paragraph = structural as FlarkV3DocumentStructuralQuery;
          expect(
            paragraph.structure.kind,
            FlarkV3DocumentStructureKind.paragraph,
          );
          expect(paragraph.inlineFacts, isNull);
          final beforePresentation =
              runtime.status.inlinePresentationGeneration;
          final beforeOutcome = runtime.status.inlineAttemptOutcomeGeneration;
          expect(
            lease.ensureInlineAtUtf16(
              markdown.length ~/ 2,
              structuralQuery: paragraph,
            ),
            FlarkV3InlineDemandDisposition.notApplicable,
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
          expect(structural, isA<FlarkV3DocumentStructuralQuery>());
          final paragraph = structural as FlarkV3DocumentStructuralQuery;
          expect(
            paragraph.structure.kind,
            FlarkV3DocumentStructureKind.paragraph,
          );
          expect(
            paragraph.structure.source.endUtf8 -
                paragraph.structure.source.startUtf8,
            greaterThan(8 * 1024),
            reason:
                'the physical leaf deliberately includes the large '
                'definition prefix',
          );
          _expectSpan(
            paragraph.projection.projectedSource,
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
            lease.ensureInlineAtUtf16(position, structuralQuery: paragraph),
            FlarkV3InlineDemandDisposition.scheduled,
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
          expect(refined, isA<FlarkV3DocumentStructuralQuery>());
          final inline =
              (refined as FlarkV3DocumentStructuralQuery).inlineFacts;
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
                  as FlarkV3DocumentStructuralQuery;
          expect(
            baseFirst.structure.kind,
            FlarkV3DocumentStructureKind.paragraph,
          );
          expect(baseFirst.structure.referenceDefinitionCount, referenceCount);
          _expectSpan(
            baseFirst.structure.source,
            _asciiSpan(0, paragraphRanges.first.end),
          );
          _expectSpan(
            baseFirst.structure.visibleSource,
            _asciiSpan(tailStart, paragraphRanges.first.end),
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
                  as FlarkV3DocumentStructuralQuery;
          expect(edited.structure.referenceDefinitionCount, 0);
          _expectSpan(
            edited.structure.source,
            _asciiSpan(editedRange.start, editedRange.end + coordinateDelta),
          );

          final lastBase = paragraphRanges.last;
          final last =
              runtime.queryAtUtf16(lastBase.start + coordinateDelta + 1)
                  as FlarkV3DocumentStructuralQuery;
          _expectSpan(
            last.structure.source,
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
                    as FlarkV3DocumentStructuralQuery)
                .structure
                .kind,
            FlarkV3DocumentStructureKind.paragraph,
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
          expect(result, isA<FlarkV3DocumentStructuralQuery>());
          final structure = result as FlarkV3DocumentStructuralQuery;
          expect(structure.sourceRevision, 2);
          expect(structure.structureRevision, 2);
          expect(
            structure.structure.kind,
            FlarkV3DocumentStructureKind.paragraph,
          );
          expect(structure.structure.unknownReason, isNull);
          expect(structure.structure.referenceDefinitionCount, 0);
          _expectSpan(structure.structure.source, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));
          _expectSpan(structure.structure.visibleSource, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));
          expect(
            structure.projection.kind,
            FlarkV3DocumentStructureKind.paragraph,
          );
          _expectSpan(structure.projection.source, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));
          _expectSpan(structure.projection.projectedSource, (
            startUtf8: 0,
            endUtf8: 10,
            startUtf16: 0,
            endUtf16: 10,
          ));
          expect(structure.projection.runCount, 1);

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
          _expectFence(runtime.queryAtUtf16(11), revision: 1, bodyEndUtf8: 14);

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
          _expectFence(runtime.queryAtUtf16(12), revision: 2, bodyEndUtf8: 26);

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
  FlarkV3DocumentQueryResult query, {
  required int revision,
  required int bodyEndUtf8,
  bool closed = true,
}) {
  expect(query, isA<FlarkV3DocumentStructuralQuery>());
  final result = query as FlarkV3DocumentStructuralQuery;
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(
    result.structure.kind,
    FlarkV3DocumentStructureKind.fencedCode,
    reason:
        'source=${result.structure.source.startUtf8}..'
        '${result.structure.source.endUtf8} '
        'unknown=${result.structure.unknownReason}',
  );
  expect(result.projection.kind, FlarkV3DocumentStructureKind.fencedCode);
  expect(result.inlineFacts, isNull);
  final fence = result.structure.fencedCode!;
  expect(fence.marker, FlarkV3CodeFenceMarker.backtick);
  expect(fence.openingIndent, 0);
  expect(fence.closed, closed);
  _expectSpan(fence.openingMarker, _asciiSpan(3, 6));
  _expectSpan(fence.rawInfoSource, _asciiSpan(6, 10));
  expect(fence.bodySource.startUtf8, 11);
  expect(fence.bodySource.endUtf8, bodyEndUtf8);
  if (closed) {
    expect(fence.closingMarker!.startUtf8, bodyEndUtf8);
    expect(fence.closingMarker!.endUtf8, bodyEndUtf8 + 3);
  } else {
    expect(fence.closingMarker, isNull);
    expect(result.structure.source.endUtf8, bodyEndUtf8);
  }
  _expectSpan(result.projection.projectedSource, (
    startUtf8: 11,
    endUtf8: bodyEndUtf8,
    startUtf16: 11,
    endUtf16: bodyEndUtf8 - 1,
  ));
}

void _expectThematicBreak(
  FlarkV3DocumentQueryResult query, {
  required int revision,
}) {
  expect(query, isA<FlarkV3DocumentStructuralQuery>());
  final result = query as FlarkV3DocumentStructuralQuery;
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.structure.kind, FlarkV3DocumentStructureKind.thematicBreak);
  expect(result.structure.unknownReason, isNull);
  expect(result.structure.referenceDefinitionCount, 0);
  expect(result.structure.heading, isNull);
  expect(result.structure.fencedCode, isNull);
  expect(result.structure.inlineContentSource, isNull);
  expect(result.structure.canCarryInlineFacts, isFalse);
  _expectSpan(result.structure.source, _asciiSpan(0, 10));
  _expectSpan(result.structure.visibleSource, _asciiSpan(0, 0));

  final thematicBreak = result.structure.thematicBreak;
  expect(thematicBreak, isNotNull);
  expect(thematicBreak!.marker, FlarkV3ThematicBreakMarker.asterisk);
  expect(thematicBreak.markerCount, 3);
  expect(thematicBreak.openingIndent, 2);
  expect(thematicBreak.hasBofBom, isFalse);
  _expectSpan(thematicBreak.markerEnvelope, _asciiSpan(2, 7));
  _expectSpan(thematicBreak.lineEnding, _asciiSpan(8, 10));

  expect(result.projection.kind, FlarkV3DocumentStructureKind.thematicBreak);
  _expectSpan(result.projection.source, _asciiSpan(0, 10));
  _expectSpan(result.projection.projectedSource, _asciiSpan(0, 0));
  expect(result.projection.runCount, 0);
  expect(
    result.inlineFacts,
    isNull,
    reason: 'an atomic marker-free block cannot carry inline facts',
  );
}

void _expectAtxHeading(
  FlarkV3DocumentStructuralQuery result, {
  required int revision,
  required _ExpectedSpan source,
  required _ExpectedSpan content,
  required _ExpectedSpan opening,
  required _ExpectedSpan closing,
}) {
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.structure.kind, FlarkV3DocumentStructureKind.heading);
  expect(result.structure.unknownReason, isNull);
  expect(result.structure.referenceDefinitionCount, 0);
  expect(result.structure.canCarryInlineFacts, isTrue);
  _expectSpan(result.structure.source, source);
  _expectSpan(result.structure.visibleSource, content);
  _expectSpan(result.structure.inlineContentSource!, content);

  final heading = result.structure.heading! as FlarkV3AtxHeadingFacts;
  expect(heading.level, 2);
  expect(heading.hasClosingMarker, isTrue);
  _expectSpan(heading.openingMarker, opening);
  _expectSpan(heading.contentSource, content);
  _expectSpan(heading.closingMarker!, closing);

  expect(result.projection.kind, FlarkV3DocumentStructureKind.heading);
  _expectSpan(result.projection.source, source);
  _expectSpan(result.projection.projectedSource, content);
  expect(result.projection.runCount, 1);
}

void _expectSetextHeading(
  FlarkV3DocumentStructuralQuery result, {
  required int revision,
  required _ExpectedSpan source,
  required _ExpectedSpan content,
  required _ExpectedSpan contentLineEnding,
  required _ExpectedSpan underline,
  required _ExpectedSpan underlineLineEnding,
}) {
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.structure.kind, FlarkV3DocumentStructureKind.heading);
  expect(result.structure.unknownReason, isNull);
  expect(result.structure.referenceDefinitionCount, 0);
  expect(result.structure.canCarryInlineFacts, isTrue);
  _expectSpan(result.structure.source, source);
  _expectSpan(result.structure.visibleSource, content);
  _expectSpan(result.structure.inlineContentSource!, content);

  final heading = result.structure.heading! as FlarkV3SetextHeadingFacts;
  expect(heading.level, 2);
  expect(heading.openingIndent, 0);
  _expectSpan(heading.contentSource, content);
  _expectSpan(heading.contentLineEnding, contentLineEnding);
  _expectSpan(heading.underlineMarker, underline);
  _expectSpan(heading.underlineLineEnding, underlineLineEnding);

  expect(result.projection.kind, FlarkV3DocumentStructureKind.heading);
  _expectSpan(result.projection.source, source);
  _expectSpan(result.projection.projectedSource, content);
  expect(result.projection.runCount, 1);
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

void _expectStructure(
  FlarkV3DocumentStructuralQuery result,
  _SemanticFixture fixture, {
  required int revision,
}) {
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.structure.kind, fixture.kind);
  expect(result.structure.unknownReason, fixture.unknownReason);
  expect(
    result.structure.referenceDefinitionCount,
    fixture.referenceDefinitionCount,
  );
  _expectSpan(result.structure.source, fixture.source);
  _expectSpan(result.structure.visibleSource, fixture.visibleSource);

  expect(result.projection.kind, fixture.kind);
  _expectSpan(result.projection.source, fixture.source);
  _expectSpan(result.projection.projectedSource, fixture.projectedSource);
  expect(result.projection.runCount, fixture.projectionRunCount);
  expect(
    result.inlineFacts,
    isNull,
    reason: 'canonical publication must remain structure-only',
  );
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
  expect(query, isA<FlarkV3DocumentStructuralQuery>());
  final result = query as FlarkV3DocumentStructuralQuery;
  expect(result.sourceRevision, revision);
  expect(result.structureRevision, revision);
  expect(result.structure.kind, kind);
  expect(result.structure.unknownReason, unknownReason);
  expect(result.structure.referenceDefinitionCount, 0);
  _expectSpan(result.structure.source, source);
  _expectSpan(result.structure.visibleSource, visibleSource ?? source);
  expect(result.projection.kind, kind);
  _expectSpan(result.projection.source, source);
  _expectSpan(result.projection.projectedSource, source);
  expect(result.projection.runCount, 1);
  expect(
    result.inlineFacts,
    isNull,
    reason: 'canonical publication must remain structure-only',
  );
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
