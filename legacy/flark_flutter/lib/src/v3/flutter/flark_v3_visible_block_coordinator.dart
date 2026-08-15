import 'dart:async';

import 'package:flark/flark_v3.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'flark_v3_flutter_live_controller.dart';

/// Current disposition of one Flutter-visible structural range.
///
/// [materializing] contains exact consecutive blocks, but another bounded
/// page is required before the complete source demand is covered. [truncated]
/// is terminal for this demand: the caller must issue a new source window
/// beginning at [FlarkV3FlutterVisibleBlockCoordinator.nextWindowStartUtf16].
enum FlarkV3FlutterVisibleBlockPhase {
  idle,
  scheduled,
  materializing,
  exact,
  pending,
  gap,
  truncated,
}

/// Flutter frame coordinator for Dart-owned visible structural blocks.
///
/// Layout remains the caller's responsibility. Once Flutter has translated a
/// viewport and cache extent into an exact UTF-16 source interval, it supplies
/// that interval to [requestVisibleSourceRange]. The coordinator performs at
/// most one [FlarkV3VisibleBlockSetMaterializer.advance] per scheduled frame.
///
/// This object borrows [runtime]. Dispose it before closing the runtime.
final class FlarkV3FlutterVisibleBlockCoordinator extends ChangeNotifier {
  factory FlarkV3FlutterVisibleBlockCoordinator.attach({
    required FlarkV3DocumentRuntime runtime,
    FlarkV3FrameScheduler frameScheduler = const FlarkV3FlutterFrameScheduler(),
    FlarkV3DocumentBlockRangeBudget pageBudget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) => FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
    driver: _FlarkV3RuntimeVisibleBlockDriver(runtime),
    frameScheduler: frameScheduler,
    pageBudget: pageBudget,
  );

  /// Dependency seam for focused frame-coordination tests.
  ///
  /// Product adapters should use [FlarkV3FlutterVisibleBlockCoordinator.attach].
  @visibleForTesting
  FlarkV3FlutterVisibleBlockCoordinator.fromDriver({
    required FlarkV3FlutterVisibleBlockDriver driver,
    required FlarkV3FrameScheduler frameScheduler,
    FlarkV3DocumentBlockRangeBudget pageBudget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) : _driver = driver,
       _frameScheduler = frameScheduler,
       _pageBudget = pageBudget {
    if (pageBudget.maximumEncodedBytes <= 0 ||
        pageBudget.maximumBlockCount <= 0 ||
        pageBudget.maximumStoragePagesVisited <= 0 ||
        pageBudget.maximumOpenDepth <= 0 ||
        pageBudget.maximumTreeNodesVisited <= 0) {
      throw ArgumentError.value(
        pageBudget,
        'pageBudget',
        'Every visible-block page bound must be greater than zero.',
      );
    }
    _subscription = _driver.changes.listen((_) => _handleRuntimeChange());
  }

  /// A visible-set demand is deliberately smaller than a document.
  ///
  /// If one unusually dense viewport contains more blocks, [phase] becomes
  /// [FlarkV3FlutterVisibleBlockPhase.truncated] and the viewport adapter
  /// continues from [nextWindowStartUtf16] as a new bounded demand.
  static const int maximumBlocksPerDemand =
      FlarkV3VisibleBlockDemand.maximumMaterializedBlocks;

  final FlarkV3FlutterVisibleBlockDriver _driver;
  final FlarkV3FrameScheduler _frameScheduler;
  final FlarkV3DocumentBlockRangeBudget _pageBudget;
  late final StreamSubscription<void> _subscription;

  FlarkV3VisibleBlockDemand? _demand;
  FlarkV3VisibleBlockSet? _value;
  FlarkV3FlutterVisibleBlockPhase _phase = FlarkV3FlutterVisibleBlockPhase.idle;
  bool _frameScheduled = false;
  bool _disposed = false;
  int _boundedAdvanceCount = 0;

  FlarkV3VisibleBlockDemand? get demand => _demand;
  FlarkV3VisibleBlockSet? get value => _value;
  FlarkV3FlutterVisibleBlockPhase get phase => _phase;
  bool get hasScheduledAdvance => _frameScheduled;

  /// Number of bounded materializer turns executed by this coordinator.
  int get boundedAdvanceCount => _boundedAdvanceCount;

  FlarkV3ExactVisibleBlockSet? get exactValue => switch (_value) {
    final FlarkV3ExactVisibleBlockSet exact => exact,
    _ => null,
  };

  /// Exact source boundary from which a truncated viewport can continue.
  int? get nextWindowStartUtf16 {
    final exact = exactValue;
    return exact != null && exact.truncated
        ? exact.coveredSource.endUtf16
        : null;
  }

  /// Requests one revision-bound source interval derived by Flutter layout.
  ///
  /// Repeating an already terminal demand is a no-op. Repeating a pending
  /// demand requests one bounded retry; runtime status changes also retry a
  /// pending demand without polling.
  void requestVisibleSourceRange(
    TextRange sourceRange, {
    int maximumBlocks = FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
  }) {
    _requireAttached();
    final sourceLength = _driver.sourceLengthUtf16;
    if (!sourceRange.isValid ||
        sourceRange.end > sourceLength ||
        (sourceLength != 0 && sourceRange.isCollapsed) ||
        maximumBlocks <= 0 ||
        maximumBlocks > maximumBlocksPerDemand) {
      throw RangeError(
        'Visible source range or block cap is outside the bounded document.',
      );
    }
    final next = FlarkV3VisibleBlockDemand(
      sourceRevision: _driver.sourceRevision,
      structureGeneration: _driver.structureGeneration,
      startUtf16: sourceRange.start,
      endUtf16: sourceRange.end,
      maximumBlocks: maximumBlocks,
    );
    if (next == _demand) {
      if (_phase == FlarkV3FlutterVisibleBlockPhase.pending) {
        _scheduleAdvance();
      }
      return;
    }

    _driver.reset();
    _demand = next;
    _value = null;
    _phase = FlarkV3FlutterVisibleBlockPhase.scheduled;
    notifyListeners();
    _scheduleAdvance();
  }

  /// Drops viewport demand without affecting the Dart document runtime.
  void clearVisibleSourceRange() {
    _requireAttached();
    _driver.reset();
    _demand = null;
    _value = null;
    _phase = FlarkV3FlutterVisibleBlockPhase.idle;
    notifyListeners();
  }

  void _handleRuntimeChange() {
    if (_disposed || _demand == null || !_driver.isQueryable) return;
    final demandIsStale = _demand!.sourceRevision != _driver.sourceRevision;
    if (demandIsStale) {
      final alreadyRejectedStaleDemand = switch (_value) {
        FlarkV3PendingVisibleBlockSet(
          reason: FlarkV3DocumentPendingReason.sourceChanged,
        ) =>
          true,
        _ => false,
      };
      if (!alreadyRejectedStaleDemand) _scheduleAdvance();
      return;
    }
    if (_demand!.structureGeneration != _driver.structureGeneration) {
      final previous = _demand!;
      _driver.reset();
      _demand = FlarkV3VisibleBlockDemand(
        sourceRevision: previous.sourceRevision,
        structureGeneration: _driver.structureGeneration,
        startUtf16: previous.startUtf16,
        endUtf16: previous.endUtf16,
        maximumBlocks: previous.maximumBlocks,
      );
      _value = null;
      _phase = FlarkV3FlutterVisibleBlockPhase.scheduled;
      notifyListeners();
      _scheduleAdvance();
      return;
    }
    if (_phase == FlarkV3FlutterVisibleBlockPhase.scheduled ||
        _phase == FlarkV3FlutterVisibleBlockPhase.materializing ||
        _phase == FlarkV3FlutterVisibleBlockPhase.pending) {
      _scheduleAdvance();
    }
  }

  void _scheduleAdvance() {
    if (_disposed ||
        _frameScheduled ||
        _demand == null ||
        !_driver.isQueryable) {
      return;
    }
    _frameScheduled = true;
    _frameScheduler.schedule(_runScheduledAdvance);
  }

  void _runScheduledAdvance() {
    _frameScheduled = false;
    if (_disposed || !_driver.isQueryable) return;
    final demand = _demand;
    if (demand == null) return;

    final next = _driver.advance(demand, budget: _pageBudget);
    _boundedAdvanceCount += 1;
    if (_disposed || _demand != demand) return;

    _value = next;
    _phase = _phaseFor(next);
    notifyListeners();
    if (!_disposed &&
        _demand == demand &&
        _phase == FlarkV3FlutterVisibleBlockPhase.materializing) {
      _scheduleAdvance();
    }
  }

  static FlarkV3FlutterVisibleBlockPhase _phaseFor(
    FlarkV3VisibleBlockSet value,
  ) => switch (value) {
    FlarkV3PendingVisibleBlockSet() => FlarkV3FlutterVisibleBlockPhase.pending,
    FlarkV3SourceGapVisibleBlockSet() => FlarkV3FlutterVisibleBlockPhase.gap,
    FlarkV3ExactVisibleBlockSet(:final truncated) when truncated =>
      FlarkV3FlutterVisibleBlockPhase.truncated,
    FlarkV3ExactVisibleBlockSet(:final demandCovered) when demandCovered =>
      FlarkV3FlutterVisibleBlockPhase.exact,
    FlarkV3ExactVisibleBlockSet() =>
      FlarkV3FlutterVisibleBlockPhase.materializing,
  };

  void _requireAttached() {
    if (_disposed) {
      throw StateError('The Flutter visible-block coordinator is disposed.');
    }
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    unawaited(_subscription.cancel());
    _driver.reset();
    super.dispose();
  }
}

/// Narrow materializer seam used only to prove Flutter frame scheduling.
///
/// The production implementation below always delegates to the Dart-first
/// [FlarkV3VisibleBlockSetMaterializer]. This interface is not exported by the
/// v3 Flutter barrel.
@visibleForTesting
abstract interface class FlarkV3FlutterVisibleBlockDriver {
  int get sourceRevision;
  int get structureGeneration;
  int get sourceLengthUtf16;
  bool get isQueryable;
  Stream<void> get changes;

  FlarkV3VisibleBlockSet advance(
    FlarkV3VisibleBlockDemand demand, {
    required FlarkV3DocumentBlockRangeBudget budget,
  });

  void reset();
}

final class _FlarkV3RuntimeVisibleBlockDriver
    implements FlarkV3FlutterVisibleBlockDriver {
  _FlarkV3RuntimeVisibleBlockDriver(this.runtime)
    : _materializer = FlarkV3VisibleBlockSetMaterializer(runtime);

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3VisibleBlockSetMaterializer _materializer;

  @override
  int get sourceRevision => runtime.sourceRevision;

  @override
  int get structureGeneration => runtime.status.structureGeneration;

  @override
  int get sourceLengthUtf16 => runtime.sourceLengthUtf16;

  @override
  bool get isQueryable => switch (runtime.status.state) {
    FlarkV3DocumentRuntimeState.opening ||
    FlarkV3DocumentRuntimeState.open => true,
    FlarkV3DocumentRuntimeState.faulted ||
    FlarkV3DocumentRuntimeState.closing ||
    FlarkV3DocumentRuntimeState.closed => false,
  };

  @override
  Stream<void> get changes => runtime.statuses.map((_) {});

  @override
  FlarkV3VisibleBlockSet advance(
    FlarkV3VisibleBlockDemand demand, {
    required FlarkV3DocumentBlockRangeBudget budget,
  }) => _materializer.advance(demand, budget: budget);

  @override
  void reset() => _materializer.reset();
}
