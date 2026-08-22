import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

void main() {
  test(
    'generic leaf demand preserves inline scheduling and stale outcomes',
    () async {
      final runtime = await openFlarkV3PublicRuntimeForTest('**bold**');
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final initial = _structural(lease.queryAtUtf16(3));
        expect(initial.structure.kind, FlarkV3DocumentStructureKind.paragraph);
        expect(initial.inlineFacts, isNull);

        final beforeGeneration =
            runtime.status.leafProjectionPresentationGeneration;
        final committed = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration > beforeGeneration ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(3, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(3, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );
        final committedStatus = await committed.timeout(
          const Duration(seconds: 5),
        );
        expect(committedStatus.state, FlarkV3DocumentRuntimeState.open);

        final refined = _structural(lease.queryAtUtf16(3));
        expect(refined.inlineFacts, isNotNull);
        expect(
          lease.ensureInlineAtUtf16(3, structuralQuery: refined),
          FlarkV3InlineDemandDisposition.notApplicable,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(3, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );

        runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: runtime.sourceRevision,
            operation: const FlarkV3SourceEdit(
              startUtf16: 8,
              endUtf16: 8,
              replacement: '!',
            ),
          ),
        );
        final editedRevision = runtime.sourceRevision;
        if (!runtime.status.structureCurrent) {
          await runtime.statuses
              .firstWhere(
                (status) =>
                    status.structureCurrent &&
                    status.structureRevision == editedRevision,
              )
              .timeout(const Duration(seconds: 5));
        }
        expect(
          lease.ensureLeafProjectionAtUtf16(3, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.stale,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand schedules indented code through the shared lane',
    () async {
      const markdown = '    code\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      final passive = FlarkV3DocumentRuntimeAdapter.borrow(runtime);
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final indented = _indentedQuery(
          revision: runtime.sourceRevision,
          sourceLength: markdown.length,
        );
        expect(indented.indentedCodeProjection, isNull);

        expect(
          () =>
              passive.ensureLeafProjectionAtUtf16(2, structuralQuery: indented),
          throwsStateError,
        );
        expect(
          lease.ensureInlineAtUtf16(2, structuralQuery: indented),
          FlarkV3InlineDemandDisposition.notApplicable,
          reason: 'the compatibility API must remain inline-only',
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(2, structuralQuery: indented),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(2, structuralQuery: indented),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );
      } finally {
        passive.release();
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand installs and retains a block quote path projection',
    () async {
      const markdown = '   > alpha\n> beta\nlazy\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final initial = _structural(lease.queryAtUtf16(5));
        expect(initial.structure.kind, FlarkV3DocumentStructureKind.blockQuote);
        expect(initial.pointPath, isNull);
        expect(initial.blockQuoteProjection, isNull);
        expect(
          lease.ensureInlineAtUtf16(5, structuralQuery: initial),
          FlarkV3InlineDemandDisposition.notApplicable,
          reason: 'the compatibility API must remain inline-only',
        );

        final beforeGeneration =
            runtime.status.leafProjectionPresentationGeneration;
        final committed = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration > beforeGeneration ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(5, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(5, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );
        final committedStatus = await committed.timeout(
          const Duration(seconds: 5),
        );
        expect(committedStatus.state, FlarkV3DocumentRuntimeState.open);

        final refined = _structural(lease.queryAtUtf16(5));
        expect(refined.pointPath, isNotNull);
        expect(refined.blockQuoteProjection, isNotNull);
        expect(
          refined.blockQuoteProjection!.pointPath,
          same(refined.pointPath),
          reason: 'cache resolution must retain the schema-4 path and payload',
        );
        expect(
          refined.blockQuoteProjection!.toSourceProjection().displayText,
          'alpha\nbeta\nlazy\n',
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(5, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand installs a selected tight bullet-list item',
    () async {
      const markdown = '- alpha\n- beta\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final initial = _structural(lease.queryAtUtf16(11));
        expect(initial.structure.kind, FlarkV3DocumentStructureKind.bulletList);
        expect(initial.structure.bulletList!.itemCount, 2);
        expect(initial.pointPath, isNull);
        expect(initial.bulletListProjection, isNull);
        expect(
          lease.ensureInlineAtUtf16(11, structuralQuery: initial),
          FlarkV3InlineDemandDisposition.notApplicable,
          reason: 'the compatibility API must remain inline-only',
        );

        final beforeGeneration =
            runtime.status.leafProjectionPresentationGeneration;
        final committed = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration > beforeGeneration ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(11, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(11, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );
        final committedStatus = await committed.timeout(
          const Duration(seconds: 5),
        );
        expect(committedStatus.state, FlarkV3DocumentRuntimeState.open);

        final refined = _structural(lease.queryAtUtf16(11));
        final payload = refined.bulletListProjection;
        expect(refined.pointPath, isNotNull);
        expect(payload, isNotNull);
        expect(payload!.pointPath, same(refined.pointPath));
        expect(payload.selectedItemOrdinal, 1);
        expect(payload.coversWholeList, isFalse);
        expect(payload.records, hasLength(1));
        expect(payload.toSourceProjection().displayText, 'beta\n');
        expect(payload.toSelectedItemSourceProjection().displayText, 'beta\n');

        final beforeInlineGeneration =
            runtime.status.leafProjectionPresentationGeneration;
        final inlineCommitted = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration >
                  beforeInlineGeneration ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(11, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        final inlineStatus = await inlineCommitted.timeout(
          const Duration(seconds: 5),
        );
        expect(inlineStatus.state, FlarkV3DocumentRuntimeState.open);
        final joined = _structural(lease.queryAtUtf16(11));
        expect(joined.bulletListProjection, isNotNull);
        expect(joined.inlineFacts, isNotNull);
        expect(
          lease.ensureLeafProjectionAtUtf16(11, structuralQuery: joined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand installs and joins a selected ordered-list item',
    () async {
      const markdown = '007) **alpha**\r\n9) beta\r\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final initialResult = lease.queryAtUtf16(8);
        if (initialResult case FlarkV3DocumentSourceGapQuery(
          :final reason,
          :final range,
        )) {
          fail(
            'Ordered-list initial query returned $reason for '
            '${range.startUtf8}..${range.endUtf8}.',
          );
        }
        final initial = _structural(initialResult);
        expect(
          initial.structure.kind,
          FlarkV3DocumentStructureKind.orderedList,
        );
        expect(initial.structure.orderedList!.itemCount, 2);
        expect(initial.structure.orderedList!.start, 7);
        expect(initial.pointPath, isNull);
        expect(initial.orderedListProjection, isNull);

        final beforeProjection =
            runtime.status.leafProjectionPresentationGeneration;
        final projectionCommitted = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration > beforeProjection ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(8, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(8, structuralQuery: initial),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );
        expect(
          (await projectionCommitted.timeout(const Duration(seconds: 5))).state,
          FlarkV3DocumentRuntimeState.open,
        );

        final refined = _structural(lease.queryAtUtf16(8));
        final payload = refined.orderedListProjection;
        expect(refined.pointPath, isNotNull);
        expect(payload, isNotNull);
        expect(payload!.pointPath, same(refined.pointPath));
        expect(payload.selectedItemOrdinal, 0);
        expect(payload.selectedMarkerText, '007)');
        expect(payload.editingInputs.continuationSourcePrefix, '008) ');
        expect(payload.editingInputs.canonicalLineEnding, '\r\n');
        expect(payload.toSourceProjection().displayText, '**alpha**\n');

        final beforeInline =
            runtime.status.leafProjectionPresentationGeneration;
        final inlineCommitted = runtime.statuses.firstWhere(
          (status) =>
              status.leafProjectionPresentationGeneration > beforeInline ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(8, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          (await inlineCommitted.timeout(const Duration(seconds: 5))).state,
          FlarkV3DocumentRuntimeState.open,
        );
        final joined = _structural(lease.queryAtUtf16(8));
        expect(joined.orderedListProjection, isNotNull);
        expect(joined.inlineFacts, isNotNull);
        expect(
          (
            joined.inlineFacts!.source.startUtf8,
            joined.inlineFacts!.source.endUtf8,
          ),
          (
            joined.orderedListProjection!.selectedItem.content.startUtf8,
            joined.orderedListProjection!.selectedItem.content.endUtf8,
          ),
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(8, structuralQuery: joined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand rejects inconsistent or oversized list summaries',
    () async {
      const markdown = '- item\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final revision = runtime.sourceRevision;
        expect(
          lease.ensureLeafProjectionAtUtf16(
            3,
            structuralQuery: _bulletListQuery(
              revision: revision,
              sourceLength: markdown.length,
              itemCount: 2,
              projectionRunCount: 1,
            ),
          ),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(
            3,
            structuralQuery: _bulletListQuery(
              revision: revision,
              sourceLength: 8 * 1024 + 1,
              itemCount: 1,
              projectionRunCount: 1,
            ),
          ),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'generic leaf demand refuses a block quote above the whole-leaf cap',
    () async {
      final markdown = '> ${'a' * (8 * 1024 - 1)}';
      final runtime = await openFlarkV3PublicRuntimeForTest(markdown);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        final blockQuote = _blockQuoteQuery(
          revision: runtime.sourceRevision,
          sourceLength: markdown.length,
          projectedLength: markdown.length - 2,
        );

        expect(
          lease.ensureInlineAtUtf16(3, structuralQuery: blockQuote),
          FlarkV3InlineDemandDisposition.notApplicable,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(3, structuralQuery: blockQuote),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
      } finally {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

FlarkV3DocumentStructuralQuery _structural(FlarkV3DocumentQueryResult query) {
  expect(query, isA<FlarkV3DocumentStructuralQuery>());
  return query as FlarkV3DocumentStructuralQuery;
}

FlarkV3DocumentStructuralQuery _indentedQuery({
  required int revision,
  required int sourceLength,
}) {
  final source = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: sourceLength,
    startUtf16: 0,
    endUtf16: sourceLength,
  );
  const hidden = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 0,
    startUtf16: 0,
    endUtf16: 0,
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: revision,
    structureRevision: revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: source,
      visibleSource: hidden,
      referenceDefinitionCount: 0,
      indentedCode: FlarkV3IndentedCodeFacts(
        deindentColumns: 4,
        hasBofBom: false,
        lineCount: 1,
        projectedUtf8Length: sourceLength - 4,
        projectedUtf16Length: sourceLength - 4,
        terminalLineEndingBytes: 1,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: source,
      projectedSource: hidden,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralQuery _blockQuoteQuery({
  required int revision,
  required int sourceLength,
  required int projectedLength,
}) {
  final source = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: sourceLength,
    startUtf16: 0,
    endUtf16: sourceLength,
  );
  const hidden = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 0,
    startUtf16: 0,
    endUtf16: 0,
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: revision,
    structureRevision: revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.blockQuote,
      source: source,
      visibleSource: hidden,
      referenceDefinitionCount: 0,
      blockQuote: FlarkV3BlockQuoteFacts(
        lineCount: 1,
        childFirstLine: 0,
        childLineCount: 1,
        projectedUtf8Length: projectedLength,
        projectedUtf16Length: projectedLength,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.blockQuote,
      source: source,
      projectedSource: hidden,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralQuery _bulletListQuery({
  required int revision,
  required int sourceLength,
  required int itemCount,
  required int projectionRunCount,
}) {
  final source = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: sourceLength,
    startUtf16: 0,
    endUtf16: sourceLength,
  );
  const hidden = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 0,
    startUtf16: 0,
    endUtf16: 0,
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: revision,
    structureRevision: revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.bulletList,
      source: source,
      visibleSource: hidden,
      referenceDefinitionCount: 0,
      bulletList: FlarkV3BulletListFacts(
        marker: FlarkV3BulletListMarker.hyphen,
        itemCount: itemCount,
        terminalEmptyRelativeStartUtf8: null,
        paragraphCount: itemCount,
        projectedUtf8Length: sourceLength - itemCount * 2,
        projectedUtf16Length: sourceLength - itemCount * 2,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.bulletList,
      source: source,
      projectedSource: hidden,
      runCount: projectionRunCount,
    ),
  );
}
