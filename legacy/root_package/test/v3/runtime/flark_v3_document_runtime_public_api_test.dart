import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  test(
    'open owns the real source, parser, native host, publication, and close',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        '# Live\n\n**Markdown** with [a reference][ref].\n\n'
        '[ref]: https://example.com "Example"\n',
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      addTearDown(() async {
        await runtime.close().timeout(const Duration(seconds: 5));
      });

      await runtime.initialReady.timeout(const Duration(seconds: 5));
      final initialStructure = runtime.status.structureCurrent
          ? runtime.status
          : await runtime.statuses
                .firstWhere((status) => status.structureCurrent)
                .timeout(const Duration(seconds: 5));
      expect(initialStructure.structureRevision, runtime.sourceRevision);
      final initialQuery = runtime.queryAtUtf16(0);
      expect(initialQuery, isA<FlarkV3DocumentStructuralQuery>());
      final initialFacts = initialQuery as FlarkV3DocumentStructuralQuery;
      expect(initialFacts.structure.kind, FlarkV3DocumentStructureKind.heading);
      expect(initialFacts.structure.unknownReason, isNull);
      expect(initialFacts.structure.referenceDefinitionCount, 0);
      expect(
        (
          initialFacts.structure.source.startUtf8,
          initialFacts.structure.source.endUtf8,
          initialFacts.structure.source.startUtf16,
          initialFacts.structure.source.endUtf16,
        ),
        (0, 7, 0, 7),
      );
      expect(
        (
          initialFacts.structure.visibleSource.startUtf8,
          initialFacts.structure.visibleSource.endUtf8,
          initialFacts.structure.visibleSource.startUtf16,
          initialFacts.structure.visibleSource.endUtf16,
        ),
        (2, 6, 2, 6),
      );
      final heading = initialFacts.structure.heading! as FlarkV3AtxHeadingFacts;
      expect(heading.level, 1);
      expect(heading.hasClosingMarker, isFalse);
      expect(
        (
          heading.openingMarker.startUtf8,
          heading.openingMarker.endUtf8,
          heading.openingMarker.startUtf16,
          heading.openingMarker.endUtf16,
        ),
        (0, 1, 0, 1),
      );
      expect(
        (
          heading.contentSource.startUtf8,
          heading.contentSource.endUtf8,
          heading.contentSource.startUtf16,
          heading.contentSource.endUtf16,
        ),
        (2, 6, 2, 6),
      );
      expect(heading.closingMarker, isNull);
      expect(
        initialFacts.projection.kind,
        FlarkV3DocumentStructureKind.heading,
      );
      expect(
        (
          initialFacts.projection.source.startUtf8,
          initialFacts.projection.source.endUtf8,
          initialFacts.projection.source.startUtf16,
          initialFacts.projection.source.endUtf16,
        ),
        (0, 7, 0, 7),
      );
      expect(
        (
          initialFacts.projection.projectedSource.startUtf8,
          initialFacts.projection.projectedSource.endUtf8,
          initialFacts.projection.projectedSource.startUtf16,
          initialFacts.projection.projectedSource.endUtf16,
        ),
        (2, 6, 2, 6),
      );
      expect(initialFacts.projection.runCount, 1);
      expect(initialFacts.inlineFacts, isNull);
      expect(
        runtime.queryAtUtf16(
          0,
          budget: const FlarkV3DocumentQueryBudget(maximumEncodedBytes: 64),
        ),
        isA<FlarkV3DocumentSourceGapQuery>(),
      );

      final priorStructureRevision = initialStructure.structureRevision!;
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: 0,
            replacement: '> ',
          ),
        ),
      );
      expect(runtime.status.structureCurrent, isFalse);

      final editedStructure = await runtime.statuses
          .firstWhere(
            (status) =>
                status.structureCurrent &&
                status.structureRevision == runtime.sourceRevision,
          )
          .timeout(const Duration(seconds: 5));
      expect(
        editedStructure.structureRevision,
        greaterThan(priorStructureRevision),
      );
      expect(runtime.exportMarkdown(), startsWith('> # Live'));
      final editedFacts = runtime.queryAtUtf16(2);
      expect(editedFacts, isA<FlarkV3DocumentStructuralQuery>());
      expect(
        (editedFacts as FlarkV3DocumentStructuralQuery).structure.unknownReason,
        FlarkV3DocumentUnknownReason.unsupportedSource,
      );

      await runtime.close().timeout(const Duration(seconds: 5));
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}
