import 'dart:async';

import 'package:flark/flark_v3.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_flutter_live_controller.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_visible_block_coordinator.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('advances exactly one visible-set page per scheduled frame', () async {
    final driver = _FakeVisibleBlockDriver(
      sourceRevision: 7,
      sourceLengthUtf16: 60,
      advance: (demand, call) => _exact(
        demand,
        blockCount: call + 1,
        coveredEndUtf16: (call + 1) * 10,
        demandCovered: call == 2,
      ),
    );
    final scheduler = _ManualFrameScheduler();
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(maximumBlockCount: 1),
    );
    addTearDown(() async {
      coordinator.dispose();
      await driver.close();
    });

    coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 30));
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.scheduled);
    expect(scheduler.pendingCount, 1);

    scheduler.flushOne();
    expect(driver.advanceCount, 1);
    expect(coordinator.boundedAdvanceCount, 1);
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.materializing);
    expect(coordinator.exactValue!.blocks, hasLength(1));
    expect(scheduler.pendingCount, 1);

    scheduler.flushOne();
    expect(driver.advanceCount, 2);
    expect(coordinator.boundedAdvanceCount, 2);
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.materializing);
    expect(coordinator.exactValue!.blocks, hasLength(2));
    expect(scheduler.pendingCount, 1);

    scheduler.flushOne();
    expect(driver.advanceCount, 3);
    expect(coordinator.boundedAdvanceCount, 3);
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
    expect(coordinator.exactValue!.blocks, hasLength(3));
    expect(coordinator.exactValue!.demandCovered, isTrue);
    expect(scheduler.pendingCount, 0);
  });

  test(
    'coalesces viewport replacement before doing host-facing work',
    () async {
      final driver = _FakeVisibleBlockDriver(
        sourceRevision: 3,
        sourceLengthUtf16: 100,
        advance: (demand, _) => _exact(demand, demandCovered: true),
      );
      final scheduler = _ManualFrameScheduler();
      final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
        driver: driver,
        frameScheduler: scheduler,
      );
      addTearDown(() async {
        coordinator.dispose();
        await driver.close();
      });

      expect(
        () => coordinator.requestVisibleSourceRange(
          const TextRange(start: 20, end: 20),
        ),
        throwsRangeError,
        reason: 'caret-only positions use the Dart point-query lane',
      );
      coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 20));
      coordinator.requestVisibleSourceRange(
        const TextRange(start: 40, end: 60),
      );

      expect(scheduler.pendingCount, 1);
      expect(driver.advanceCount, 0);
      scheduler.flushOne();
      expect(driver.advanceCount, 1);
      expect(driver.demands.single.startUtf16, 40);
      expect(driver.demands.single.endUtf16, 60);
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
    },
  );

  test(
    'runtime progress resumes a stranded scheduled or materializing quantum',
    () async {
      final driver = _FakeVisibleBlockDriver(
        sourceRevision: 5,
        sourceLengthUtf16: 40,
        advance: (demand, call) => _exact(
          demand,
          coveredEndUtf16: call == 0 ? 20 : demand.endUtf16,
          demandCovered: call != 0,
        ),
      )..isQueryable = false;
      final scheduler = _ManualFrameScheduler();
      final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
        driver: driver,
        frameScheduler: scheduler,
      );
      addTearDown(() async {
        coordinator.dispose();
        await driver.close();
      });

      coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 40));
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.scheduled);
      expect(scheduler.pendingCount, 0);

      driver
        ..isQueryable = true
        ..emitChange();
      expect(scheduler.pendingCount, 1);
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.materializing);
      expect(scheduler.pendingCount, 1);

      driver.isQueryable = false;
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.materializing);
      expect(scheduler.pendingCount, 0);

      driver
        ..isQueryable = true
        ..emitChange();
      expect(scheduler.pendingCount, 1);
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
      expect(driver.advanceCount, 2);
    },
  );

  test('pending waits for runtime progress and gap remains terminal', () async {
    final driver = _FakeVisibleBlockDriver(
      sourceRevision: 11,
      sourceLengthUtf16: 80,
      advance: (demand, call) {
        if (demand.startUtf16 == 40) {
          return FlarkV3SourceGapVisibleBlockSet(
            demand: demand,
            reason: FlarkV3DocumentQueryGapReason.treeNodeLimit,
          );
        }
        if (call == 0) {
          return FlarkV3PendingVisibleBlockSet(
            demand: demand,
            reason: FlarkV3DocumentPendingReason.structurePending,
            stableStructureRevision: 10,
          );
        }
        return _exact(demand, demandCovered: true);
      },
    );
    final scheduler = _ManualFrameScheduler();
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
    );
    addTearDown(() async {
      coordinator.dispose();
      await driver.close();
    });

    coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 20));
    scheduler.flushOne();
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.pending);
    expect(scheduler.pendingCount, 0);

    driver.emitChange();
    expect(scheduler.pendingCount, 1);
    scheduler.flushOne();
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
    expect(driver.advanceCount, 2);

    coordinator.requestVisibleSourceRange(const TextRange(start: 40, end: 60));
    scheduler.flushOne();
    expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.gap);
    expect(
      coordinator.value,
      isA<FlarkV3SourceGapVisibleBlockSet>().having(
        (value) => value.reason,
        'reason',
        FlarkV3DocumentQueryGapReason.treeNodeLimit,
      ),
    );
    expect(scheduler.pendingCount, 0);
  });

  test(
    'truncates at the sealed cap and exposes the next source window',
    () async {
      final driver = _FakeVisibleBlockDriver(
        sourceRevision: 2,
        sourceLengthUtf16: 100,
        advance: (demand, _) {
          if (demand.startUtf16 == 0) {
            return _exact(
              demand,
              blockCount: 2,
              coveredEndUtf16: 20,
              truncated: true,
            );
          }
          return _exact(demand, blockCount: 2, demandCovered: true);
        },
      );
      final scheduler = _ManualFrameScheduler();
      final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
        driver: driver,
        frameScheduler: scheduler,
      );
      addTearDown(() async {
        coordinator.dispose();
        await driver.close();
      });

      expect(
        () => coordinator.requestVisibleSourceRange(
          const TextRange(start: 0, end: 100),
          maximumBlocks:
              FlarkV3FlutterVisibleBlockCoordinator.maximumBlocksPerDemand + 1,
        ),
        throwsRangeError,
      );
      expect(scheduler.pendingCount, 0);

      coordinator.requestVisibleSourceRange(
        const TextRange(start: 0, end: 100),
        maximumBlocks: 2,
      );
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.truncated);
      expect(coordinator.nextWindowStartUtf16, 20);
      expect(scheduler.pendingCount, 0);

      coordinator.requestVisibleSourceRange(
        TextRange(start: coordinator.nextWindowStartUtf16!, end: 100),
        maximumBlocks: 2,
      );
      scheduler.flushOne();
      expect(driver.demands.last.startUtf16, 20);
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
    },
  );

  test(
    'source revision changes fail closed until layout reissues demand',
    () async {
      late final _FakeVisibleBlockDriver driver;
      driver = _FakeVisibleBlockDriver(
        sourceRevision: 4,
        sourceLengthUtf16: 40,
        advance: (demand, _) {
          if (demand.sourceRevision != driver.sourceRevision) {
            return FlarkV3PendingVisibleBlockSet(
              demand: demand,
              reason: FlarkV3DocumentPendingReason.sourceChanged,
              stableStructureRevision: demand.sourceRevision,
            );
          }
          return _exact(demand, demandCovered: true);
        },
      );
      final scheduler = _ManualFrameScheduler();
      final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
        driver: driver,
        frameScheduler: scheduler,
      );
      addTearDown(() async {
        coordinator.dispose();
        await driver.close();
      });

      coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 20));
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
      expect(coordinator.demand!.sourceRevision, 4);

      driver.sourceRevision = 5;
      driver.emitChange();
      expect(scheduler.pendingCount, 1);
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.pending);
      expect(coordinator.demand!.sourceRevision, 4);
      expect(
        coordinator.value,
        isA<FlarkV3PendingVisibleBlockSet>().having(
          (value) => value.reason,
          'reason',
          FlarkV3DocumentPendingReason.sourceChanged,
        ),
      );
      driver.emitChange();
      expect(
        scheduler.pendingCount,
        0,
        reason: 'an already rejected stale demand must not retry on progress',
      );

      coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 20));
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
      expect(coordinator.demand!.sourceRevision, 5);
    },
  );

  test(
    'same-source structural generation revokes and rematerializes exact range',
    () async {
      final driver = _FakeVisibleBlockDriver(
        sourceRevision: 9,
        structureGeneration: 3,
        sourceLengthUtf16: 40,
        advance: (demand, _) => _exact(demand, demandCovered: true),
      );
      final scheduler = _ManualFrameScheduler();
      final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
        driver: driver,
        frameScheduler: scheduler,
      );
      addTearDown(() async {
        coordinator.dispose();
        await driver.close();
      });

      coordinator.requestVisibleSourceRange(const TextRange(start: 0, end: 20));
      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
      expect(coordinator.demand!.structureGeneration, 3);
      expect(driver.advanceCount, 1);

      driver.structureGeneration = 4;
      driver.emitChange();
      expect(
        coordinator.phase,
        FlarkV3FlutterVisibleBlockPhase.scheduled,
        reason: 'old semantic authority is revoked synchronously',
      );
      expect(coordinator.exactValue, isNull);
      expect(coordinator.demand!.sourceRevision, 9);
      expect(coordinator.demand!.structureGeneration, 4);
      expect(scheduler.pendingCount, 1);

      scheduler.flushOne();
      expect(coordinator.phase, FlarkV3FlutterVisibleBlockPhase.exact);
      expect(driver.advanceCount, 2);
      expect(driver.demands.last.structureGeneration, 4);
    },
  );
}

typedef _FakeAdvance =
    FlarkV3VisibleBlockSet Function(
      FlarkV3VisibleBlockDemand demand,
      int zeroBasedCall,
    );

final class _FakeVisibleBlockDriver
    implements FlarkV3FlutterVisibleBlockDriver {
  _FakeVisibleBlockDriver({
    required this.sourceRevision,
    this.structureGeneration = 1,
    required this.sourceLengthUtf16,
    required _FakeAdvance advance,
  }) : _advance = advance;

  @override
  int sourceRevision;

  @override
  int structureGeneration;

  @override
  final int sourceLengthUtf16;

  @override
  bool isQueryable = true;

  final _FakeAdvance _advance;
  final StreamController<void> _changes = StreamController<void>.broadcast(
    sync: true,
  );
  final List<FlarkV3VisibleBlockDemand> demands = <FlarkV3VisibleBlockDemand>[];
  final List<FlarkV3DocumentBlockRangeBudget> budgets =
      <FlarkV3DocumentBlockRangeBudget>[];
  int resetCount = 0;

  int get advanceCount => demands.length;

  @override
  Stream<void> get changes => _changes.stream;

  @override
  FlarkV3VisibleBlockSet advance(
    FlarkV3VisibleBlockDemand demand, {
    required FlarkV3DocumentBlockRangeBudget budget,
  }) {
    final call = demands.length;
    demands.add(demand);
    budgets.add(budget);
    return _advance(demand, call);
  }

  @override
  void reset() {
    resetCount += 1;
  }

  void emitChange() => _changes.add(null);

  Future<void> close() => _changes.close();
}

final class _ManualFrameScheduler implements FlarkV3FrameScheduler {
  final List<VoidCallback> _callbacks = <VoidCallback>[];

  int get pendingCount => _callbacks.length;

  @override
  void schedule(VoidCallback callback) {
    _callbacks.add(callback);
  }

  void flushOne() {
    if (_callbacks.isEmpty) {
      throw StateError('No Flutter frame callback is pending.');
    }
    _callbacks.removeAt(0)();
  }
}

FlarkV3ExactVisibleBlockSet _exact(
  FlarkV3VisibleBlockDemand demand, {
  int blockCount = 1,
  int? coveredEndUtf16,
  bool demandCovered = false,
  bool truncated = false,
}) {
  final end = coveredEndUtf16 ?? demand.endUtf16;
  final width = blockCount == 0 ? 0 : (end - demand.startUtf16) ~/ blockCount;
  final blocks = List<FlarkV3DocumentStructuralBlock>.generate(blockCount, (
    index,
  ) {
    final start = demand.startUtf16 + (width * index);
    final blockEnd = index == blockCount - 1 ? end : start + width;
    final source = FlarkV3SourceSpan(
      startUtf8: start,
      endUtf8: blockEnd,
      startUtf16: start,
      endUtf16: blockEnd,
    );
    return FlarkV3DocumentStructuralBlock(
      ordinal: index,
      structure: FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: source,
        visibleSource: source,
        referenceDefinitionCount: 0,
      ),
      projection: FlarkV3DocumentProjection(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: source,
        projectedSource: source,
        runCount: 1,
      ),
    );
  }, growable: false);
  return FlarkV3ExactVisibleBlockSet(
    demand: demand,
    coveredSource: FlarkV3SourceSpan(
      startUtf8: demand.startUtf16,
      endUtf8: end,
      startUtf16: demand.startUtf16,
      endUtf16: end,
    ),
    blocks: blocks,
    demandCovered: demandCovered,
    truncated: truncated,
  );
}
