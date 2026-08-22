@TestOn('vm')
library;

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  test(
    'native exact ordinal windows jump into a large document without a prefix crawl',
    () async {
      const blockCount = 8191;
      final markdown = List.generate(
        blockCount,
        (index) => '# p$index',
      ).join('\n');
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(runtime);
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 10));
      });
      await runtime.initialReady.timeout(const Duration(seconds: 30));
      if (!runtime.status.structureCurrent) {
        await runtime.statuses
            .firstWhere((status) => status.structureCurrent)
            .timeout(const Duration(seconds: 30));
      }

      final status = runtime.status;
      final middleDemand = FlarkV3DocumentOrdinalWindowDemand(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startBlockOrdinal: 4095,
      );
      final middle =
          lease.queryBlockOrdinalWindow(
                middleDemand,
                budget: const FlarkV3DocumentOrdinalWindowBudget(
                  maximumEntries: 97,
                ),
              )
              as FlarkV3ExactDocumentOrdinalWindow;
      expect(middle.totalBlockCount, blockCount);
      expect(middle.startBlockOrdinal, 4095);
      expect(middle.nextBlockOrdinal, 4192);
      expect(middle.coveredSource.startUtf16, markdown.indexOf('# p4095'));
      expect(middle.coveredSource.endUtf16, markdown.indexOf('# p4192'));
      expect(middle.complete, isFalse);
      expect(middle.storagePagesVisited, lessThanOrEqualTo(8));
      expect(middle.treeNodesVisited, lessThanOrEqualTo(128));
      expect(middle.packedEntriesInspected, lessThanOrEqualTo(1024));

      final terminalDemand = FlarkV3DocumentOrdinalWindowDemand(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startBlockOrdinal: blockCount,
      );
      final terminal =
          runtime.queryBlockOrdinalWindow(terminalDemand)
              as FlarkV3ExactDocumentOrdinalWindow;
      expect(terminal.startBlockOrdinal, blockCount);
      expect(terminal.nextBlockOrdinal, blockCount);
      expect(terminal.complete, isTrue);
      expect(terminal.coveredSource.startUtf16, markdown.length);
      expect(terminal.coveredSource.endUtf16, markdown.length);

      final outOfRange =
          runtime.queryBlockOrdinalWindow(
                FlarkV3DocumentOrdinalWindowDemand(
                  sourceRevision: status.sourceRevision,
                  structureGeneration: status.structureGeneration,
                  startBlockOrdinal: blockCount + 1,
                ),
              )
              as FlarkV3UnavailableDocumentOrdinalWindow;
      expect(
        outOfRange.reason,
        FlarkV3DocumentOrdinalWindowFailureReason.ordinalOutOfRange,
      );
      expect(outOfRange.totalBlockCount, blockCount);

      final wrongStructure =
          runtime.queryBlockOrdinalWindow(
                FlarkV3DocumentOrdinalWindowDemand(
                  sourceRevision: status.sourceRevision,
                  structureGeneration: status.structureGeneration + 1,
                  startBlockOrdinal: 0,
                ),
              )
              as FlarkV3UnavailableDocumentOrdinalWindow;
      expect(
        wrongStructure.reason,
        FlarkV3DocumentOrdinalWindowFailureReason.structureChanged,
      );

      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: runtime.sourceLengthUtf16,
            endUtf16: runtime.sourceLengthUtf16,
            replacement: '\n# new',
          ),
        ),
      );
      final stale =
          runtime.queryBlockOrdinalWindow(middleDemand)
              as FlarkV3UnavailableDocumentOrdinalWindow;
      expect(
        stale.reason,
        FlarkV3DocumentOrdinalWindowFailureReason.sourceChanged,
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}
