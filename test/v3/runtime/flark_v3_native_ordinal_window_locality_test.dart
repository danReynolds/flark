@TestOn('vm')
library;

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

const _windowEntries = 97;
const _localityBudget = FlarkV3DocumentOrdinalWindowBudget(
  maximumEntries: _windowEntries,
  maximumStoragePagesVisited: 2,
  maximumTreeNodesVisited: 96,
  maximumPackedEntriesInspected: 1024,
);

void main() {
  test(
    'distant ordinal windows retain logarithmic bounded work at 50k entries',
    () async {
      final small = await _measureStructuralWork(4096);
      final large = await _measureStructuralWork(50000);

      expect(small.storagePagesVisited, lessThanOrEqualTo(2));
      expect(large.storagePagesVisited, lessThanOrEqualTo(2));
      expect(small.treeNodesVisited, lessThanOrEqualTo(96));
      expect(large.treeNodesVisited, lessThanOrEqualTo(96));
      expect(small.packedEntriesInspected, lessThanOrEqualTo(1024));
      expect(large.packedEntriesInspected, lessThanOrEqualTo(1024));
      expect(large.summaryNodesSkipped, greaterThan(0));
      expect(
        large.treeNodesVisited,
        lessThanOrEqualTo(small.treeNodesVisited + 32),
        reason:
            'A 12x larger structural sequence may add tree height, not prefix '
            'work. Crossing this envelope requires revisiting the locator '
            'budget and benchmark evidence.',
      );

      // ignore: avoid_print
      print(
        'flark_v3_ordinal_locality '
        '4096=${small.toReceipt()} 50000=${large.toReceipt()}',
      );
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

Future<_MaximumWork> _measureStructuralWork(int entryCount) async {
  final source = StringBuffer();
  for (var ordinal = 0; ordinal < entryCount; ordinal += 1) {
    if (ordinal != 0) source.write('\n');
    source
      ..write('# heading ')
      ..write(ordinal);
  }
  final runtime = await FlarkV3DocumentRuntime.open(
    source.toString(),
    nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
  );
  try {
    await runtime.initialReady.timeout(const Duration(minutes: 1));
    if (!runtime.status.structureCurrent) {
      await runtime.statuses
          .firstWhere((status) => status.structureCurrent)
          .timeout(const Duration(minutes: 1));
    }

    final status = runtime.status;
    final lastFull = entryCount - _windowEntries;
    final starts = <int>[
      0,
      entryCount ~/ 7,
      entryCount ~/ 4,
      entryCount ~/ 2,
      (entryCount * 3) ~/ 4,
      (entryCount * 6) ~/ 7,
      lastFull,
    ];
    final maximum = _MaximumWork();
    for (final start in starts) {
      final result = runtime.queryBlockOrdinalWindow(
        FlarkV3DocumentOrdinalWindowDemand(
          sourceRevision: status.sourceRevision,
          structureGeneration: status.structureGeneration,
          startBlockOrdinal: start,
        ),
        budget: _localityBudget,
      );
      expect(
        result,
        isA<FlarkV3ExactDocumentOrdinalWindow>(),
        reason: 'Distant ordinal $start of $entryCount must stay bounded.',
      );
      final exact = result as FlarkV3ExactDocumentOrdinalWindow;
      expect(exact.totalBlockCount, entryCount);
      expect(exact.startBlockOrdinal, start);
      expect(exact.nextBlockOrdinal, start + _windowEntries);
      maximum.observe(exact);
    }
    return maximum;
  } finally {
    await runtime.close().timeout(const Duration(seconds: 30));
  }
}

final class _MaximumWork {
  int storagePagesVisited = 0;
  int treeNodesVisited = 0;
  int packedEntriesInspected = 0;
  int summaryNodesSkipped = 0;

  void observe(FlarkV3ExactDocumentOrdinalWindow window) {
    storagePagesVisited = _max(storagePagesVisited, window.storagePagesVisited);
    treeNodesVisited = _max(treeNodesVisited, window.treeNodesVisited);
    packedEntriesInspected = _max(
      packedEntriesInspected,
      window.packedEntriesInspected,
    );
    summaryNodesSkipped = _max(summaryNodesSkipped, window.summaryNodesSkipped);
  }

  String toReceipt() =>
      '{storage:$storagePagesVisited,tree:$treeNodesVisited,'
      'packed:$packedEntriesInspected,skipped:$summaryNodesSkipped}';
}

int _max(int left, int right) => left > right ? left : right;
