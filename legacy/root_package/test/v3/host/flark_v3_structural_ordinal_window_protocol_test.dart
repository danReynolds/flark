import 'package:flark/flark_adapter.dart';
import 'package:test/test.dart';

void main() {
  final source = FlarkV3SourceVersion(
    documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
    revision: 7,
    metric: FlarkV3SourceMetric(bytes: 1000, utf16: 900),
    contentHash: FlarkV3ContentHash128(5, 6, 7, 8),
  );
  final budget = FlarkV3HostStructuralOrdinalWindowBudget(
    maximumEntries: 96,
    maximumStoragePagesVisited: 8,
    maximumTreeNodesVisited: 128,
    maximumPackedEntriesInspected: 1024,
  );
  final query = FlarkV3HostStructuralOrdinalWindowQuery(
    sourceVersion: source,
    startBlockOrdinal: FlarkV3ProtocolU64.fromU32(100),
    budget: budget,
  );

  test('accepts one exact bounded window and its terminal empty cut', () {
    final window = FlarkV3HostStructuralOrdinalWindow(
      sourceVersion: source,
      totalBlockCount: FlarkV3ProtocolU64.fromU32(500),
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(100),
      nextBlockOrdinal: FlarkV3ProtocolU64.fromU32(196),
      startSource: FlarkV3SourceMetric(bytes: 200, utf16: 180),
      nextSource: FlarkV3SourceMetric(bytes: 400, utf16: 360),
      work: FlarkV3HostStructuralOrdinalWindowWorkReceipt(
        storagePagesVisited: 2,
        treeNodesVisited: 50,
        packedEntriesInspected: 607,
        summaryNodesSkipped: 22,
      ),
      complete: false,
    );
    expect(window.binds(query), isTrue);

    final terminalQuery = FlarkV3HostStructuralOrdinalWindowQuery(
      sourceVersion: source,
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(500),
      budget: budget,
    );
    final terminal = FlarkV3HostStructuralOrdinalWindow(
      sourceVersion: source,
      totalBlockCount: FlarkV3ProtocolU64.fromU32(500),
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(500),
      nextBlockOrdinal: FlarkV3ProtocolU64.fromU32(500),
      startSource: source.metric,
      nextSource: source.metric,
      work: FlarkV3HostStructuralOrdinalWindowWorkReceipt(
        storagePagesVisited: 2,
        treeNodesVisited: 40,
        packedEntriesInspected: 320,
        summaryNodesSkipped: 18,
      ),
      complete: true,
    );
    expect(terminal.binds(terminalQuery), isTrue);
  });

  test(
    'rejects oversized, nonadvancing, inverted, and false-complete cuts',
    () {
      FlarkV3HostStructuralOrdinalWindow candidate({
        int next = 196,
        FlarkV3SourceMetric? nextSource,
        bool complete = false,
      }) => FlarkV3HostStructuralOrdinalWindow(
        sourceVersion: source,
        totalBlockCount: FlarkV3ProtocolU64.fromU32(500),
        startBlockOrdinal: FlarkV3ProtocolU64.fromU32(100),
        nextBlockOrdinal: FlarkV3ProtocolU64.fromU32(next),
        startSource: FlarkV3SourceMetric(bytes: 200, utf16: 180),
        nextSource: nextSource ?? FlarkV3SourceMetric(bytes: 400, utf16: 360),
        work: FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
        complete: complete,
      );

      expect(candidate(next: 197).binds(query), isFalse);
      expect(candidate(next: 100).binds(query), isFalse);
      expect(
        candidate(
          nextSource: FlarkV3SourceMetric(bytes: 199, utf16: 179),
        ).binds(query),
        isFalse,
      );
      expect(candidate(complete: true).binds(query), isFalse);
    },
  );

  test('typed failures enforce their canonical work and total rules', () {
    FlarkV3HostStructuralOrdinalWindowFailure failure({
      required FlarkV3HostStructuralOrdinalWindowFailureReason reason,
      FlarkV3ProtocolU64? total,
      FlarkV3HostStructuralOrdinalWindowWorkReceipt? work,
    }) => FlarkV3HostStructuralOrdinalWindowFailure(
      sourceVersion: source,
      totalBlockCount: total ?? FlarkV3ProtocolU64.zero,
      startBlockOrdinal: query.startBlockOrdinal,
      reason: reason,
      work: work ?? FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
    );

    expect(
      failure(
        reason:
            FlarkV3HostStructuralOrdinalWindowFailureReason.ordinalOutOfRange,
        total: FlarkV3ProtocolU64.fromU32(500),
      ).binds(query),
      isTrue,
    );
    expect(
      failure(
        reason: FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable,
        total: FlarkV3ProtocolU64.fromU32(500),
      ).binds(query),
      isFalse,
    );
    expect(
      failure(
        reason: FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable,
        work: FlarkV3HostStructuralOrdinalWindowWorkReceipt(
          storagePagesVisited: 2,
          treeNodesVisited: 50,
          packedEntriesInspected: 607,
          summaryNodesSkipped: 22,
        ),
      ).binds(query),
      isTrue,
    );
    expect(
      failure(
        reason: FlarkV3HostStructuralOrdinalWindowFailureReason.treeNodeLimit,
        total: FlarkV3ProtocolU64.fromU32(500),
        work: FlarkV3HostStructuralOrdinalWindowWorkReceipt(
          storagePagesVisited: 0,
          treeNodesVisited: 1,
          packedEntriesInspected: 0,
          summaryNodesSkipped: 0,
        ),
      ).binds(query),
      isFalse,
    );
  });

  test('entry budgets stop at the locked transport maximum', () {
    expect(
      () => FlarkV3HostStructuralOrdinalWindowBudget(
        maximumEntries: 4097,
        maximumStoragePagesVisited: 1,
        maximumTreeNodesVisited: 1,
        maximumPackedEntriesInspected: 1,
      ),
      throwsRangeError,
    );
  });
}
