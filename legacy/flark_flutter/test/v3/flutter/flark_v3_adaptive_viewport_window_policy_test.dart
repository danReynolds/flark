import 'package:flark_flutter/src/v3/flutter/flark_v3_managed_viewport_presentation_source.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_virtualized_live_surface.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('adaptive viewport window policy', () {
    test('halves a dense exact window through one entry', () {
      final policy = FlarkV3AdaptiveViewportWindowPolicy();
      var maximumBlocks = 64;
      final attempted = <int>[];

      while (true) {
        attempted.add(maximumBlocks);
        final half = maximumBlocks ~/ 2;
        final retry = policy.recordBudgetExceeded(
          sourceRevision: 3,
          structureGeneration: 5,
          failedStartOrdinal: 128 - half,
          failedNextOrdinal: 128 - half + maximumBlocks,
          failedMaximumBlocks: maximumBlocks,
          activeOrdinal: 128,
        );
        if (retry == null) break;
        expect(retry.centerOrdinal, 128);
        maximumBlocks = retry.maximumBlocks;
      }

      expect(attempted, <int>[64, 32, 16, 8, 4, 2, 1]);
    });

    test('keeps the reduced cap sticky only around the failed authority', () {
      final policy = FlarkV3AdaptiveViewportWindowPolicy();
      policy
        ..recordBudgetExceeded(
          sourceRevision: 3,
          structureGeneration: 5,
          failedStartOrdinal: 96,
          failedNextOrdinal: 160,
          failedMaximumBlocks: 64,
          activeOrdinal: 128,
        )
        ..recordBudgetExceeded(
          sourceRevision: 3,
          structureGeneration: 5,
          failedStartOrdinal: 112,
          failedNextOrdinal: 144,
          failedMaximumBlocks: 32,
          activeOrdinal: 128,
        );

      final nearby = policy.constrain(
        FlarkV3ViewportWindowDemand(centerOrdinal: 120, maximumBlocks: 64),
        sourceRevision: 3,
        structureGeneration: 5,
      );
      final distant = policy.constrain(
        FlarkV3ViewportWindowDemand(centerOrdinal: 200, maximumBlocks: 64),
        sourceRevision: 3,
        structureGeneration: 5,
      );
      final newAuthority = policy.constrain(
        FlarkV3ViewportWindowDemand(centerOrdinal: 120, maximumBlocks: 64),
        sourceRevision: 4,
        structureGeneration: 6,
      );

      expect(nearby.maximumBlocks, 16);
      expect(distant.maximumBlocks, 64);
      expect(newAuthority.maximumBlocks, 64);
    });

    test('shrinks from actual coverage at a document edge', () {
      final policy = FlarkV3AdaptiveViewportWindowPolicy();

      final retry = policy.recordBudgetExceeded(
        sourceRevision: 3,
        structureGeneration: 5,
        failedStartOrdinal: 0,
        failedNextOrdinal: 10,
        failedMaximumBlocks: 64,
        activeOrdinal: 4,
      );

      expect(retry, isNotNull);
      expect(retry!.maximumBlocks, 5);
      expect(retry.centerOrdinal, 4);
    });
  });
}
