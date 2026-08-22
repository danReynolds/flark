@TestOn('vm')
library;

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

const _pageBudget = FlarkV3DocumentBlockRangeBudget(
  maximumEncodedBytes: 32 + 2 * 160,
  maximumBlockCount: 2,
);

void main() {
  test(
    'native range pages and visible materialization stay bounded',
    () async {
      final markdown = List.generate(
        32,
        (index) => index == 17 ? 'Unicode é\r\ncontinuation $index' : 'p$index',
      ).join('\n\n');
      final runtime = await _openRuntime(markdown);
      addTearDown(() => runtime.close());

      expect(
        () => runtime.queryBlockRange(1, 1),
        throwsRangeError,
        reason: 'caret-only positions use the point-query lane',
      );
      expect(
        () => FlarkV3VisibleBlockDemand(
          sourceRevision: runtime.sourceRevision,
          structureGeneration: runtime.status.structureGeneration,
          startUtf16: 0,
          endUtf16: runtime.sourceLengthUtf16,
          maximumBlocks:
              FlarkV3VisibleBlockDemand.maximumMaterializedBlocks + 1,
        ),
        throwsRangeError,
      );

      final first =
          runtime.queryBlockRange(
                0,
                runtime.sourceLengthUtf16,
                budget: _pageBudget,
              )
              as FlarkV3DocumentStructuralBlockRange;
      expect(first.blocks, hasLength(2));
      expect(first.complete, isFalse);
      expect(first.continuation, isNotNull);
      expect(first.coveredSource.startUtf16, 0);
      expect(first.structureGeneration, runtime.status.structureGeneration);
      expect(
        first.continuation!.structureGeneration,
        first.structureGeneration,
      );

      final second =
          runtime.continueBlockRange(first.continuation!, budget: _pageBudget)
              as FlarkV3DocumentStructuralBlockRange;
      expect(second.blocks, hasLength(2));
      expect(second.structureGeneration, first.structureGeneration);
      expect(second.blocks.first.ordinal, first.blocks.last.ordinal + 1);
      expect(second.coveredSource.startUtf16, first.coveredSource.endUtf16);

      final materializer = FlarkV3VisibleBlockSetMaterializer(runtime);
      final demand = FlarkV3VisibleBlockDemand(
        sourceRevision: runtime.sourceRevision,
        structureGeneration: runtime.status.structureGeneration,
        startUtf16: 0,
        endUtf16: runtime.sourceLengthUtf16,
        maximumBlocks: 4,
      );
      final firstSnapshot =
          materializer.advance(demand, budget: _pageBudget)
              as FlarkV3ExactVisibleBlockSet;
      expect(firstSnapshot.blocks, hasLength(2));
      expect(firstSnapshot.demandCovered, isFalse);
      expect(firstSnapshot.truncated, isFalse);

      final secondSnapshot =
          materializer.advance(demand, budget: _pageBudget)
              as FlarkV3ExactVisibleBlockSet;
      expect(secondSnapshot.blocks, hasLength(4));
      expect(secondSnapshot.demandCovered, isFalse);
      expect(secondSnapshot.truncated, isTrue);
      expect(
        firstSnapshot.blocks,
        hasLength(2),
        reason: 'earlier immutable snapshots are not mutated while paging',
      );

      final originalLengthUtf16 = runtime.sourceLengthUtf16;
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: originalLengthUtf16 - 2,
            endUtf16: originalLengthUtf16,
            replacement: '',
          ),
        ),
      );
      expect(runtime.sourceLengthUtf16, originalLengthUtf16 - 2);
      final stale = runtime.continueBlockRange(
        first.continuation!,
        budget: _pageBudget,
      );
      expect(stale, isA<FlarkV3DocumentPendingBlockRange>());
      expect(
        (stale as FlarkV3DocumentPendingBlockRange).reason,
        FlarkV3DocumentPendingReason.sourceChanged,
      );
      final staleVisible = materializer.advance(demand, budget: _pageBudget);
      expect(staleVisible, isA<FlarkV3PendingVisibleBlockSet>());
      expect(
        (staleVisible as FlarkV3PendingVisibleBlockSet).reason,
        FlarkV3DocumentPendingReason.sourceChanged,
        reason:
            'a stale revision-bound range may extend past the shorter current '
            'source and must be rejected as stale before current coordinates '
            'are validated',
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'only an empty document admits an empty structural range',
    () async {
      final runtime = await _openRuntime('');
      addTearDown(() => runtime.close());

      final page =
          runtime.queryBlockRange(0, 0) as FlarkV3DocumentStructuralBlockRange;
      expect(page.blocks, isEmpty);
      expect(page.complete, isTrue);
      expect(page.coveredSource.startUtf16, 0);
      expect(page.coveredSource.endUtf16, 0);

      final visible =
          FlarkV3VisibleBlockSetMaterializer(runtime).advance(
                FlarkV3VisibleBlockDemand(
                  sourceRevision: runtime.sourceRevision,
                  structureGeneration: runtime.status.structureGeneration,
                  startUtf16: 0,
                  endUtf16: 0,
                ),
                budget: _pageBudget,
              )
              as FlarkV3ExactVisibleBlockSet;
      expect(visible.blocks, isEmpty);
      expect(visible.demandCovered, isTrue);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

Future<FlarkV3DocumentRuntime> _openRuntime(String markdown) async {
  final runtime = await FlarkV3DocumentRuntime.open(
    markdown,
    nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
  );
  await runtime.initialReady.timeout(const Duration(seconds: 10));
  if (!runtime.status.structureCurrent) {
    await runtime.statuses
        .firstWhere((status) => status.structureCurrent)
        .timeout(const Duration(seconds: 10));
  }
  return runtime;
}
