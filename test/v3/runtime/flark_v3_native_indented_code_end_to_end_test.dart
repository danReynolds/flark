@TestOn('vm')
library;

import 'dart:convert';

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3LeafProjectionDemandDisposition,
        FlarkV3SourceProjectionPieceKind;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'real native runtime demands exact indented-code projection geometry',
    () async {
      const markdown = '\uFEFF\tα\r\n    \tβ\r      γ\u0000';
      const projectedMarkdown = 'α\r\n\tβ\r  γ\u0000';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
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
        expect(structural.sourceRevision, runtime.sourceRevision);
        expect(structural.structureRevision, runtime.sourceRevision);
        expect(structural.structure.referenceDefinitionCount, 0);
        _expectSpan(structural.structure.source, 0, 25, 0, 20);
        _expectSpan(structural.structure.visibleSource, 0, 0, 0, 0);
        expect(
          structural.projection.kind,
          FlarkV3DocumentStructureKind.indentedCode,
        );
        _expectSpan(structural.projection.source, 0, 25, 0, 20);
        _expectSpan(structural.projection.projectedSource, 0, 0, 0, 0);
        expect(structural.projection.runCount, 3);
        expect(structural.inlineFacts, isNull);
        expect(
          structural.indentedCodeProjection,
          isNull,
          reason:
              'exact structural facts must precede the separately demanded '
              'physical-line recipe',
        );

        final facts = structural.structure.indentedCode;
        expect(facts, isNotNull);
        expect(facts!.deindentColumns, 4);
        expect(facts.hasBofBom, isTrue);
        expect(facts.lineCount, 3);
        expect(facts.projectedUtf8Length, 13);
        expect(facts.projectedUtf16Length, 10);
        expect(facts.terminalLineEndingBytes, 0);
        expect(utf8.encode(markdown), hasLength(25));
        expect(markdown, hasLength(20));
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
          reason: 'repeat demand cannot consume the bounded retry early',
        );

        final settledStatus = await settled;
        expect(
          settledStatus.state,
          FlarkV3DocumentRuntimeState.open,
          reason:
              'the native endpoint must keep exact source live while '
              'fulfilling indented-code projection demand',
        );
        expect(
          settledStatus.leafProjectionAttemptOutcomeGeneration,
          initialOutcome + 1,
          reason: 'one demanded projection reaches one terminal host outcome',
        );
        expect(
          settledStatus.leafProjectionPresentationGeneration,
          initialPresentation + 1,
          reason: 'the successful payload becomes atomically queryable',
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
              'schema 3 must join the authoritative parser-authored line '
              'recipe to the selected exact block',
        );
        expect(payload!.sourceRevision, runtime.sourceRevision);
        expect(payload.sourceVersion.metric.bytes, 25);
        expect(payload.sourceVersion.metric.utf16, 20);
        _expectSpan(payload.source, 0, 25, 0, 20);
        expect(payload.facts, same(refined.structure.indentedCode));
        expect(payload.records, hasLength(3));

        _expectRecord(
          payload.records[0],
          relativeStartUtf8: 0,
          physical: const (
            utf8Start: 0,
            utf8End: 8,
            utf16Start: 0,
            utf16End: 5,
          ),
          hidden: const (utf8Start: 0, utf8End: 4, utf16Start: 0, utf16End: 2),
          content: const (utf8Start: 4, utf8End: 6, utf16Start: 2, utf16End: 3),
          lineEnding: const (
            utf8Start: 6,
            utf8End: 8,
            utf16Start: 3,
            utf16End: 5,
          ),
        );
        _expectRecord(
          payload.records[1],
          relativeStartUtf8: 8,
          physical: const (
            utf8Start: 8,
            utf8End: 16,
            utf16Start: 5,
            utf16End: 12,
          ),
          hidden: const (utf8Start: 8, utf8End: 12, utf16Start: 5, utf16End: 9),
          content: const (
            utf8Start: 12,
            utf8End: 15,
            utf16Start: 9,
            utf16End: 11,
          ),
          lineEnding: const (
            utf8Start: 15,
            utf8End: 16,
            utf16Start: 11,
            utf16End: 12,
          ),
        );
        _expectRecord(
          payload.records[2],
          relativeStartUtf8: 16,
          physical: const (
            utf8Start: 16,
            utf8End: 25,
            utf16Start: 12,
            utf16End: 20,
          ),
          hidden: const (
            utf8Start: 16,
            utf8End: 20,
            utf16Start: 12,
            utf16End: 16,
          ),
          content: const (
            utf8Start: 20,
            utf8End: 25,
            utf16Start: 16,
            utf16End: 20,
          ),
          lineEnding: const (
            utf8Start: 25,
            utf8End: 25,
            utf16Start: 20,
            utf16End: 20,
          ),
        );
        expect(
          payload.records.every((record) => !record.isInternalBlank),
          isTrue,
        );

        final projection = payload.toSourceProjection();
        expect(projection.isCertified, isTrue);
        expect(projection.certifiedSourceVersion, same(payload.sourceVersion));
        expect(projection.sourceStartUtf16, 0);
        expect(projection.sourceEndUtf16, 20);
        expect(projection.sourceText, markdown);
        expect(projection.displayText, projectedMarkdown);
        expect(projection.displayLengthUtf16, 10);
        expect(projection.pieces.map((piece) => piece.kind), const [
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.copy,
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.copy,
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.copy,
        ]);
        expect(
          lease.ensureLeafProjectionAtUtf16(10, structuralQuery: refined),
          FlarkV3LeafProjectionDemandDisposition.notApplicable,
          reason: 'an authoritative revision-bound payload needs no redemand',
        );
        expect(
          runtime.exportMarkdown(),
          markdown,
          reason: 'marker-free projection never rewrites exact source truth',
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

void _expectRecord(
  FlarkV3IndentedCodeLineProjectionRecord record, {
  required int relativeStartUtf8,
  required _ExpectedSpan physical,
  required _ExpectedSpan hidden,
  required _ExpectedSpan content,
  required _ExpectedSpan lineEnding,
}) {
  expect(record.relativeLineStartUtf8, relativeStartUtf8);
  _expectSpan(
    record.physicalSource,
    physical.utf8Start,
    physical.utf8End,
    physical.utf16Start,
    physical.utf16End,
  );
  _expectSpan(
    record.hiddenPrefix,
    hidden.utf8Start,
    hidden.utf8End,
    hidden.utf16Start,
    hidden.utf16End,
  );
  _expectSpan(
    record.content,
    content.utf8Start,
    content.utf8End,
    content.utf16Start,
    content.utf16End,
  );
  _expectSpan(
    record.lineEnding,
    lineEnding.utf8Start,
    lineEnding.utf8End,
    lineEnding.utf16Start,
    lineEnding.utf16End,
  );
}

void _expectSpan(
  FlarkV3SourceSpan span,
  int utf8Start,
  int utf8End,
  int utf16Start,
  int utf16End,
) {
  expect(span.startUtf8, utf8Start);
  expect(span.endUtf8, utf8End);
  expect(span.startUtf16, utf16Start);
  expect(span.endUtf16, utf16End);
}

typedef _ExpectedSpan = ({
  int utf8Start,
  int utf8End,
  int utf16Start,
  int utf16End,
});
