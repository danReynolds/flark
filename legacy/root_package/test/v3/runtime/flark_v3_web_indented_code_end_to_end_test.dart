@TestOn('browser')
library;

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3LeafProjectionDemandDisposition;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'real Web runtime demands exact marker-free indented-code projection',
    () async {
      const markdown = '\uFEFF\tα\r\n    \tβ\r      γ\u0000';
      const projectedMarkdown = 'α\r\n\tβ\r  γ\u0000';
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.open);
        expect(runtime.status.sourceCurrent, isTrue);
        expect(runtime.status.structureCurrent, isTrue);

        final schema1 = runtime.queryAtUtf16(10);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.indentedCode,
        );
        expect(structural.inlineFacts, isNull);
        expect(
          structural.indentedCodeProjection,
          isNull,
          reason:
              'variant 7 structure must not imply an authoritative line '
              'recipe before explicit demand',
        );
        final facts = structural.structure.indentedCode;
        expect(facts, isNotNull);
        expect(facts!.hasBofBom, isTrue);
        expect(facts.lineCount, 3);
        expect(facts.projectedUtf8Length, 13);
        expect(facts.projectedUtf16Length, 10);
        expect(runtime.exportMarkdown(), markdown);

        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          leafProjectionDemandOwner: true,
        );
        final initialPresentation =
            runtime.status.leafProjectionPresentationGeneration;
        final initialOutcome =
            runtime.status.leafProjectionAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.leafProjectionPresentationGeneration >
                  initialPresentation ||
              status.leafProjectionAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(10, structuralQuery: structural),
          FlarkV3LeafProjectionDemandDisposition.scheduled,
        );
        expect(
          lease.ensureLeafProjectionAtUtf16(10, structuralQuery: structural),
          FlarkV3LeafProjectionDemandDisposition.coalesced,
        );

        final settledStatus = await settled;
        expect(
          settledStatus.state,
          FlarkV3DocumentRuntimeState.open,
          reason:
              'the Worker/Wasm endpoint must remain open while fulfilling '
              'the selected leaf projection',
        );
        expect(
          settledStatus.leafProjectionAttemptOutcomeGeneration,
          initialOutcome + 1,
        );
        expect(
          settledStatus.leafProjectionPresentationGeneration,
          initialPresentation + 1,
        );

        final schema3 = lease.queryAtUtf16(10);
        expect(schema3, isA<FlarkV3DocumentStructuralQuery>());
        final refined = schema3 as FlarkV3DocumentStructuralQuery;
        expect(
          refined.structure.kind,
          FlarkV3DocumentStructureKind.indentedCode,
        );
        expect(refined.inlineFacts, isNull);
        final payload = refined.indentedCodeProjection;
        expect(
          payload,
          isNotNull,
          reason:
              'viewport schema 3 must join the demanded parser-authored '
              'physical-line recipe',
        );
        expect(payload!.sourceRevision, runtime.sourceRevision);
        expect(payload.records, hasLength(3));

        final projection = payload.toSourceProjection();
        expect(projection.isCertified, isTrue);
        expect(projection.sourceText, markdown);
        expect(projection.displayText, projectedMarkdown);
        expect(projection.displayLengthUtf16, 10);
        expect(
          lease.ensureLeafProjectionAtUtf16(10, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
        );
        expect(
          runtime.exportMarkdown(),
          markdown,
          reason:
              'the marker-free Web projection must not rewrite exact source',
        );
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );
}

Future<FlarkV3DocumentRuntimeStatus> _awaitStatus(
  FlarkV3DocumentRuntime runtime,
  bool Function(FlarkV3DocumentRuntimeStatus status) predicate,
) {
  final current = runtime.status;
  if (predicate(current)) {
    return Future<FlarkV3DocumentRuntimeStatus>.value(current);
  }
  return runtime.statuses.firstWhere(predicate).timeout(_functionalTimeout);
}
