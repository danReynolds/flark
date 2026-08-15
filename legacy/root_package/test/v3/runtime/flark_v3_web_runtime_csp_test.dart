@TestOn('browser')
library;

import 'dart:convert';

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

void main() {
  test(
    'public runtime completes and reopens through external Worker under strict CSP',
    () async {
      expect(FlarkV3DocumentRuntime.platformSupport.supported, isTrue);

      final runtime = await FlarkV3DocumentRuntime.open(
        'alpha **beta**',
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(const Duration(seconds: 20));
      var closed = false;
      addTearDown(() async {
        if (!closed) await runtime.close().timeout(const Duration(seconds: 10));
      });

      await runtime.initialReady.timeout(const Duration(seconds: 20));
      final current = runtime.status.structureCurrent
          ? runtime.status
          : await runtime.statuses
                .firstWhere((status) => status.structureCurrent)
                .timeout(const Duration(seconds: 20));
      expect(current.sourceCurrent, isTrue);
      expect(current.structureRevision, runtime.sourceRevision);

      final query = runtime.queryAtUtf16(6);
      expect(query, isA<FlarkV3DocumentStructuralQuery>());
      final structure = query as FlarkV3DocumentStructuralQuery;
      expect(structure.structure.kind, FlarkV3DocumentStructureKind.paragraph);
      expect(structure.structure.source.startUtf16, 0);
      expect(structure.structure.source.endUtf16, runtime.sourceLengthUtf16);

      final oneLeaf = runtime.queryAtUtf16(
        6,
        budget: const FlarkV3DocumentQueryBudget(maximumLeafCount: 1),
      );
      expect(
        oneLeaf,
        isA<FlarkV3DocumentStructuralQuery>(),
        reason:
            'A sole-Paragraph point query visits exactly one semantic leaf.',
      );

      final gapCases =
          <(FlarkV3DocumentQueryBudget, FlarkV3DocumentQueryGapReason)>[
            (
              const FlarkV3DocumentQueryBudget(maximumEncodedBytes: 155),
              FlarkV3DocumentQueryGapReason.encodedByteLimit,
            ),
            (
              const FlarkV3DocumentQueryBudget(maximumTreeNodesVisited: 1),
              FlarkV3DocumentQueryGapReason.treeNodeLimit,
            ),
          ];
      for (final (budget, reason) in gapCases) {
        final result = runtime.queryAtUtf16(6, budget: budget);
        expect(
          result,
          isA<FlarkV3DocumentSourceGapQuery>(),
          reason:
              'budget bytes=${budget.maximumEncodedBytes}, '
              'depth=${budget.maximumOpenDepth}, '
              'leaves=${budget.maximumLeafCount}, '
              'nodes=${budget.maximumTreeNodesVisited}',
        );
        final gap = result as FlarkV3DocumentSourceGapQuery;
        expect(gap.reason, reason);
        expect(gap.range.startUtf8, 0);
        expect(gap.range.startUtf16, 0);
        expect(gap.range.endUtf8, utf8.encode(runtime.exportMarkdown()).length);
        expect(gap.range.endUtf16, runtime.sourceLengthUtf16);
      }

      await runtime.close().timeout(const Duration(seconds: 10));
      closed = true;
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);

      final reopened = await FlarkV3DocumentRuntime.open(
        'second **document**',
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(const Duration(seconds: 20));
      addTearDown(() => reopened.close().timeout(const Duration(seconds: 10)));
      await reopened.initialReady.timeout(const Duration(seconds: 20));
      final reopenedCurrent = reopened.status.structureCurrent
          ? reopened.status
          : await reopened.statuses
                .firstWhere((status) => status.structureCurrent)
                .timeout(const Duration(seconds: 20));
      expect(reopenedCurrent.sourceCurrent, isTrue);
      expect(reopened.queryAtUtf16(8), isA<FlarkV3DocumentStructuralQuery>());
      await reopened.close().timeout(const Duration(seconds: 10));
      expect(reopened.status.state, FlarkV3DocumentRuntimeState.closed);
    },
  );
}
