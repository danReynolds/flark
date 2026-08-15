@TestOn('browser')
library;

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

void main() {
  test(
    'Web host locates a bounded middle ordinal window and terminal EOF cut',
    () async {
      const blockCount = 8191;
      final markdown = List.generate(
        blockCount,
        (index) => '# p$index',
      ).join('\n');
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(const Duration(seconds: 20));
      addTearDown(() => runtime.close().timeout(const Duration(seconds: 10)));
      await runtime.initialReady.timeout(const Duration(seconds: 30));
      if (!runtime.status.structureCurrent) {
        await runtime.statuses
            .firstWhere((status) => status.structureCurrent)
            .timeout(const Duration(seconds: 30));
      }

      final status = runtime.status;
      final middle =
          runtime.queryBlockOrdinalWindow(
                FlarkV3DocumentOrdinalWindowDemand(
                  sourceRevision: status.sourceRevision,
                  structureGeneration: status.structureGeneration,
                  startBlockOrdinal: 4095,
                ),
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
      expect(middle.storagePagesVisited, lessThanOrEqualTo(8));
      expect(middle.treeNodesVisited, lessThanOrEqualTo(128));
      expect(middle.packedEntriesInspected, lessThanOrEqualTo(1024));

      final terminal =
          runtime.queryBlockOrdinalWindow(
                FlarkV3DocumentOrdinalWindowDemand(
                  sourceRevision: status.sourceRevision,
                  structureGeneration: status.structureGeneration,
                  startBlockOrdinal: blockCount,
                ),
              )
              as FlarkV3ExactDocumentOrdinalWindow;
      expect(terminal.complete, isTrue);
      expect(terminal.nextBlockOrdinal, blockCount);
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
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}
