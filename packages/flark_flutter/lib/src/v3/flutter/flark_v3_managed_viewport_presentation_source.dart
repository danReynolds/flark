import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'flark_v3_flutter_live_controller.dart';
import 'flark_v3_recursive_green_authority.dart';
import 'flark_v3_virtualized_live_surface.dart';
import 'flark_v3_visible_block_coordinator.dart';

/// Runtime-backed parser presentation for bounded ordinal windows.
///
/// The source borrows the managed binding's adapter lease. It owns no parser
/// session, source copy, text controller, or runtime progress subscription.
/// Small documents use this same path and naturally produce a complete cut
/// when all canonical structural entries fit.
final class FlarkV3ManagedViewportPresentationSource extends ChangeNotifier
    implements FlarkV3ViewportPresentationSource {
  @internal
  factory FlarkV3ManagedViewportPresentationSource.attachCompleteDocument({
    required FlarkV3DocumentRuntime runtime,
    required FlarkV3DocumentRuntimeAdapterLease lease,
    required FlarkV3FlutterLiveController liveController,
    required FlarkV3FlutterVisibleBlockCoordinator visibleBlocks,
    required double estimatedBlockExtent,
  }) => FlarkV3ManagedViewportPresentationSource._(
    runtime: runtime,
    lease: lease,
    liveController: liveController,
    visibleBlocks: visibleBlocks,
    estimatedBlockExtent: estimatedBlockExtent,
    initialSourcePointUtf16: null,
  );

  @internal
  factory FlarkV3ManagedViewportPresentationSource.attachAroundSourcePoint({
    required FlarkV3DocumentRuntime runtime,
    required FlarkV3DocumentRuntimeAdapterLease lease,
    required FlarkV3FlutterLiveController liveController,
    required FlarkV3FlutterVisibleBlockCoordinator visibleBlocks,
    required double estimatedBlockExtent,
    required int sourcePointUtf16,
  }) => FlarkV3ManagedViewportPresentationSource._(
    runtime: runtime,
    lease: lease,
    liveController: liveController,
    visibleBlocks: visibleBlocks,
    estimatedBlockExtent: estimatedBlockExtent,
    initialSourcePointUtf16: sourcePointUtf16,
  );

  FlarkV3ManagedViewportPresentationSource._({
    required FlarkV3DocumentRuntime runtime,
    required FlarkV3DocumentRuntimeAdapterLease lease,
    required FlarkV3FlutterLiveController liveController,
    required FlarkV3FlutterVisibleBlockCoordinator visibleBlocks,
    required double estimatedBlockExtent,
    required int? initialSourcePointUtf16,
  }) : _runtime = runtime,
       _lease = lease,
       _liveController = liveController,
       _visibleBlocks = visibleBlocks,
       _estimatedBlockExtent = estimatedBlockExtent,
       _relocationSourcePointUtf16 = initialSourcePointUtf16,
       _snapshot = const FlarkV3SourceGapViewportSurfaceSnapshot(
         totalBlockCount: 1,
         activeOrdinal: 0,
         estimatedBlockExtent: 44,
         reason: _FlarkV3ManagedViewportGap.awaitingCompleteStructure,
       ) {
    if (estimatedBlockExtent <= 0 || !estimatedBlockExtent.isFinite) {
      throw ArgumentError.value(
        estimatedBlockExtent,
        'estimatedBlockExtent',
        'The viewport extent estimate must be finite and positive.',
      );
    }
    if (initialSourcePointUtf16 != null &&
        (initialSourcePointUtf16 < 0 ||
            initialSourcePointUtf16 > runtime.sourceLengthUtf16)) {
      throw ArgumentError.value(
        initialSourcePointUtf16,
        'sourcePointUtf16',
        'An ordinal-window source needs one in-document source point.',
      );
    }
    _snapshot = FlarkV3SourceGapViewportSurfaceSnapshot(
      totalBlockCount: 1,
      activeOrdinal: 0,
      estimatedBlockExtent: estimatedBlockExtent,
      reason: _FlarkV3ManagedViewportGap.awaitingCompleteStructure,
    );
    _visibleBlocks.addListener(_handleVisibleBlocksProgress);
    _liveController.addListener(_handleActivePresentationProgress);
    final status = _runtime.status;
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.locating(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16:
          initialSourcePointUtf16 ??
          _liveController.globalEditingState.selection.extentOffset,
      maximumBlocks: FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
    );
    _requestAuthorityWindow();
    _drive();
  }

  final FlarkV3DocumentRuntime _runtime;
  final FlarkV3DocumentRuntimeAdapterLease _lease;
  final FlarkV3FlutterLiveController _liveController;
  final FlarkV3FlutterVisibleBlockCoordinator _visibleBlocks;
  final double _estimatedBlockExtent;
  final FlarkV3ViewportPageMaterializer _materializer =
      const FlarkV3ViewportPageMaterializer();
  final FlarkV3AdaptiveViewportWindowPolicy _adaptiveWindowPolicy =
      FlarkV3AdaptiveViewportWindowPolicy();

  FlarkV3ViewportSurfaceSnapshot _snapshot;
  late _FlarkV3ManagedWindowAcquisition _windowAcquisition;
  int? _relocationSourcePointUtf16;
  int? _authoritySourceRevision;
  int? _authorityStructureGeneration;
  int? _pendingActivationOrdinal;
  int? _pendingActivationSourcePointUtf16;
  int? _relocationMaximumBlocks;
  FlarkV3ViewportWindowDemand? _lastWindowDemand;
  Object? _lastDriveKey;
  bool _driving = false;
  bool _disposed = false;

  _FlarkV3ManagedLocatedWindow? get _window => _windowAcquisition.legacyWindow;

  FlarkV3RecursiveGreenRowRange? get _recursiveGreenWindow =>
      _windowAcquisition.recursiveGreenWindow;

  @override
  FlarkV3ViewportSurfaceSnapshot get snapshot => _snapshot;

  bool get isDisposed => _disposed;

  /// Called by the owning managed binding's single runtime subscription.
  @internal
  void handleRuntimeProgress() {
    if (_disposed) return;
    final status = _runtime.status;
    if (!status.structureCurrent ||
        status.sourceRevision != _authoritySourceRevision ||
        status.structureGeneration != _authorityStructureGeneration) {
      _retainAdaptiveRelocationLimit();
      _lastWindowDemand = null;
      _lastDriveKey = null;
      _adaptiveWindowPolicy.reset();
      _relocationSourcePointUtf16 =
          _liveController.globalEditingState.selection.extentOffset;
      _publishGap(_FlarkV3ManagedViewportGap.structureChanged);
      _requestAuthorityWindow();
    }
    _drive();
  }

  @override
  void requestWindow(FlarkV3ViewportWindowDemand demand) {
    _requireAttached();
    final total = _snapshot.totalBlockCount;
    if (demand.centerOrdinal >= total) {
      throw RangeError.index(demand.centerOrdinal, total, 'centerOrdinal');
    }
    final effectiveDemand = _adaptiveWindowPolicy.constrain(
      demand,
      sourceRevision: _runtime.status.sourceRevision,
      structureGeneration: _runtime.status.structureGeneration,
    );
    final green = _recursiveGreenWindow;
    if (green != null) {
      if (_recursiveGreenContainsOrdinal(
        green,
        effectiveDemand.centerOrdinal,
      )) {
        final start = _recursiveGreenOrdinalAsInt(green.startGlobalRowOrdinal);
        final next = start + green.rows.length;
        final margin = (effectiveDemand.maximumBlocks ~/ 4).clamp(4, 16);
        if (green.rows.length <= effectiveDemand.maximumBlocks &&
            (green.rows.length < margin * 2 ||
                effectiveDemand.centerOrdinal >= start + margin &&
                    effectiveDemand.centerOrdinal < next - margin)) {
          return;
        }
      }
      _requestRecursiveGreenOrdinalWindow(effectiveDemand);
      return;
    }
    final window = _window;
    if (window != null &&
        window.isCompleteDocument &&
        window.totalBlockCount <= effectiveDemand.maximumBlocks) {
      return;
    }
    if (window != null &&
        window.containsOrdinal(effectiveDemand.centerOrdinal)) {
      final margin = (effectiveDemand.maximumBlocks ~/ 4).clamp(4, 16);
      if (effectiveDemand.centerOrdinal >= window.startBlockOrdinal + margin &&
          effectiveDemand.centerOrdinal < window.nextBlockOrdinal - margin) {
        return;
      }
    }
    if (effectiveDemand == _lastWindowDemand &&
        window != null &&
        window.containsOrdinal(effectiveDemand.centerOrdinal)) {
      return;
    }
    _requestOrdinalWindow(effectiveDemand);
  }

  @override
  void activateOrdinal(int ordinal) {
    _requireAttached();
    if (ordinal < 0 || ordinal >= _snapshot.totalBlockCount) {
      throw RangeError.index(ordinal, _snapshot.totalBlockCount, 'ordinal');
    }
    final current = _snapshot;
    if (current is FlarkV3ExactViewportSurfaceSnapshot &&
        current.activeOrdinal == ordinal &&
        _pendingActivationOrdinal == null) {
      // Activating an already-active block is an idempotent reveal. Replaying
      // the handoff would replace its certified projected value with canonical
      // source even though neither block authority nor caret intent changed.
      return;
    }
    _pendingActivationOrdinal = ordinal;
    _pendingActivationSourcePointUtf16 = null;
    if (current is FlarkV3SourceGapViewportSurfaceSnapshot) {
      _publishGap(
        current.reason,
        totalBlockCount: current.totalBlockCount,
        activeOrdinal: ordinal,
      );
    }
    _activatePendingOrdinal();
  }

  /// Locates a canonical structural ordinal from exact range authority, then
  /// reveals and activates its bounded ordinal window.
  ///
  /// This performs one bounded source-range query and one bounded ordinal
  /// locator query. It never scans source or counts Markdown-looking blocks.
  void revealAndActivateSourcePoint(
    int positionUtf16, {
    int maximumBlocks = FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
  }) {
    _requireAttached();
    if (positionUtf16 < 0 || positionUtf16 > _runtime.sourceLengthUtf16) {
      throw RangeError.range(
        positionUtf16,
        0,
        _runtime.sourceLengthUtf16,
        'positionUtf16',
      );
    }
    if (maximumBlocks <= 0 ||
        maximumBlocks > flarkV3MaximumMountedViewportPresentations) {
      throw RangeError.range(
        maximumBlocks,
        1,
        flarkV3MaximumMountedViewportPresentations,
        'maximumBlocks',
      );
    }
    _pendingActivationSourcePointUtf16 = positionUtf16;
    _locateSourcePointWindow(
      positionUtf16,
      maximumBlocks: maximumBlocks,
      activate: true,
    );
  }

  /// Detaches presentation state without releasing the managed adapter lease.
  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _visibleBlocks.removeListener(_handleVisibleBlocksProgress);
    _liveController.removeListener(_handleActivePresentationProgress);
    super.dispose();
  }

  void _handleVisibleBlocksProgress() {
    if (_disposed) return;
    _drive();
  }

  void _handleActivePresentationProgress() {
    if (_disposed) return;
    _lastDriveKey = null;
    _drive();
  }

  void _requestAuthorityWindow() {
    final point =
        _relocationSourcePointUtf16 ??
        _liveController.globalEditingState.selection.extentOffset;
    _locateSourcePointWindow(
      point,
      maximumBlocks:
          _relocationMaximumBlocks ??
          FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
      activate: false,
    );
  }

  void _locateSourcePointWindow(
    int positionUtf16, {
    required int maximumBlocks,
    required bool activate,
  }) {
    final status = _runtime.status;
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.locating(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16: positionUtf16,
      maximumBlocks: maximumBlocks,
    );
    if (!status.structureCurrent || !status.sourceCurrent) return;
    final result = _queryStructuralWindowAtSourcePoint(positionUtf16);
    if (result is FlarkV3RecursiveGreenRowRange) {
      final row = _recursiveGreenRowAtSourcePoint(result, positionUtf16);
      if (row == null) {
        _windowAcquisition = _windowAcquisition.terminal(
          _FlarkV3ManagedViewportGap.sourcePointUnavailable,
        );
        _publishGap(_FlarkV3ManagedViewportGap.sourcePointUnavailable);
        return;
      }
      final ordinal = _recursiveGreenOrdinalAsInt(row.globalOrdinal);
      if (activate) {
        _pendingActivationOrdinal = ordinal;
        _pendingActivationSourcePointUtf16 = positionUtf16;
      }
      _requestRecursiveGreenOrdinalWindow(
        FlarkV3ViewportWindowDemand(
          centerOrdinal: ordinal,
          maximumBlocks: maximumBlocks,
        ),
      );
      return;
    }
    final block = _structuralBlockAtSourcePoint(result, positionUtf16);
    if (block == null) {
      _windowAcquisition = _windowAcquisition.terminal(
        _FlarkV3ManagedViewportGap.sourcePointUnavailable,
      );
      _publishGap(_FlarkV3ManagedViewportGap.sourcePointUnavailable);
      return;
    }
    if (activate) {
      _pendingActivationOrdinal = block.ordinal;
      _pendingActivationSourcePointUtf16 = positionUtf16;
    }
    _requestOrdinalWindow(
      FlarkV3ViewportWindowDemand(
        centerOrdinal: block.ordinal,
        maximumBlocks: maximumBlocks,
      ),
    );
  }

  FlarkV3DocumentBlockRangeResult _queryStructuralWindowAtSourcePoint(
    int positionUtf16,
  ) {
    final sourceLength = _runtime.sourceLengthUtf16;
    if (sourceLength == 0) {
      return _runtime.queryBlockRange(0, 0);
    }
    final (start, end) = _scalarSafeProbeRange(positionUtf16, sourceLength);
    return _runtime.queryBlockRange(
      start,
      end,
      budget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 64 * 1024,
        maximumBlockCount: 8,
        maximumStoragePagesVisited: 9,
        maximumOpenDepth: 64,
        maximumTreeNodesVisited: 512,
      ),
    );
  }

  FlarkV3DocumentStructuralBlock? _structuralBlockAtSourcePoint(
    FlarkV3DocumentBlockRangeResult result,
    int positionUtf16,
  ) {
    if (result is! FlarkV3DocumentStructuralBlockRange) return null;
    for (final block in result.blocks) {
      final source = block.structure.source;
      if (positionUtf16 >= source.startUtf16 &&
          positionUtf16 <= source.endUtf16) {
        return block;
      }
    }
    return null;
  }

  (int, int) _scalarSafeProbeRange(int positionUtf16, int sourceLength) {
    var start = positionUtf16 == sourceLength
        ? sourceLength - 1
        : positionUtf16;
    var end = start + 1;
    final unit = _runtime.readSourceRange(start, end).codeUnitAt(0);
    if (_isLowSurrogate(unit) && start > 0) {
      final previous = _runtime.readSourceRange(start - 1, start).codeUnitAt(0);
      if (_isHighSurrogate(previous)) start -= 1;
    } else if (_isHighSurrogate(unit) && end < sourceLength) {
      final next = _runtime.readSourceRange(end, end + 1).codeUnitAt(0);
      if (_isLowSurrogate(next)) end += 1;
    }
    return (start, end);
  }

  void _requestRecursiveGreenOrdinalWindow(FlarkV3ViewportWindowDemand demand) {
    final status = _runtime.status;
    final effectiveDemand = _adaptiveWindowPolicy.constrain(
      demand,
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
    );
    final sourcePointUtf16 =
        _windowAcquisition.sourcePointUtf16 ?? _relocationSourcePointUtf16;
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.locating(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16: sourcePointUtf16,
      demand: effectiveDemand,
      maximumBlocks: effectiveDemand.maximumBlocks,
    );
    if (!status.structureCurrent || !status.sourceCurrent) {
      _publishGap(_FlarkV3ManagedViewportGap.awaitingWindowLocation);
      return;
    }
    final halfWindow = effectiveDemand.maximumBlocks ~/ 2;
    final startOrdinal = (effectiveDemand.centerOrdinal - halfWindow).clamp(
      0,
      0xffffffff,
    );
    final ordinalResult = _lease.queryBlockOrdinalWindow(
      FlarkV3DocumentOrdinalWindowDemand(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startBlockOrdinal: startOrdinal,
      ),
      budget: FlarkV3DocumentOrdinalWindowBudget(
        maximumEntries: effectiveDemand.maximumBlocks,
      ),
    );
    if (ordinalResult is! FlarkV3ExactDocumentOrdinalWindow) {
      final unavailable =
          ordinalResult as FlarkV3UnavailableDocumentOrdinalWindow;
      _windowAcquisition = _windowAcquisition.terminal(unavailable.reason);
      _publishGap(
        unavailable.reason,
        totalBlockCount: unavailable.totalBlockCount,
        activeOrdinal: effectiveDemand.centerOrdinal,
      );
      return;
    }
    if (ordinalResult.startBlockOrdinal > effectiveDemand.centerOrdinal ||
        ordinalResult.nextBlockOrdinal <= effectiveDemand.centerOrdinal ||
        ordinalResult.nextBlockOrdinal - ordinalResult.startBlockOrdinal <= 0 ||
        ordinalResult.nextBlockOrdinal - ordinalResult.startBlockOrdinal >
            effectiveDemand.maximumBlocks ||
        ordinalResult.coveredSource.endUtf16 <=
            ordinalResult.coveredSource.startUtf16) {
      _windowAcquisition = _windowAcquisition.terminal(
        _FlarkV3ManagedViewportGap.invalidOrdinalWindow,
      );
      _publishGap(_FlarkV3ManagedViewportGap.invalidOrdinalWindow);
      return;
    }

    final rowRangeResult = _runtime.queryBlockRange(
      ordinalResult.coveredSource.startUtf16,
      ordinalResult.coveredSource.endUtf16,
      budget: FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 64 * 1024,
        maximumBlockCount: effectiveDemand.maximumBlocks,
        // Bisection reduces output cardinality, not the fixed cost of
        // authenticating and locating a point in a large persistent tree.
        maximumStoragePagesVisited: 25,
        maximumOpenDepth: 64,
        maximumTreeNodesVisited: 1024,
      ),
    );
    if (rowRangeResult is FlarkV3DocumentSourceGapBlockRange) {
      final adaptive = switch (rowRangeResult.reason) {
        FlarkV3DocumentQueryGapReason.encodedByteLimit ||
        FlarkV3DocumentQueryGapReason.leafLimit ||
        FlarkV3DocumentQueryGapReason.treeNodeLimit => true,
        FlarkV3DocumentQueryGapReason.openDepthLimit ||
        FlarkV3DocumentQueryGapReason.undecodableClosure ||
        FlarkV3DocumentQueryGapReason.unavailableFacts => false,
      };
      final failedWindow = _FlarkV3ManagedLocatedWindow(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        totalBlockCount: ordinalResult.totalBlockCount,
        startBlockOrdinal: ordinalResult.startBlockOrdinal,
        nextBlockOrdinal: ordinalResult.nextBlockOrdinal,
        coveredSource: ordinalResult.coveredSource,
        requestedMaximumBlocks: effectiveDemand.maximumBlocks,
      );
      if (adaptive &&
          _retrySmallerViewportWindow(
            failedWindow,
            effectiveDemand.centerOrdinal,
            recursiveGreen: true,
          )) {
        return;
      }
      _windowAcquisition = _windowAcquisition.terminal(rowRangeResult.reason);
      _publishGap(
        rowRangeResult.reason,
        totalBlockCount: ordinalResult.totalBlockCount,
        activeOrdinal: effectiveDemand.centerOrdinal,
      );
      return;
    }
    if (rowRangeResult is! FlarkV3RecursiveGreenRowRange ||
        rowRangeResult.startGlobalRowOrdinal !=
            BigInt.from(ordinalResult.startBlockOrdinal) ||
        rowRangeResult.totalGlobalRowCount !=
            BigInt.from(ordinalResult.totalBlockCount) ||
        rowRangeResult.rows.length !=
            ordinalResult.nextBlockOrdinal - ordinalResult.startBlockOrdinal ||
        !_sameSourceSpan(
          rowRangeResult.requestedSource,
          ordinalResult.coveredSource,
        ) ||
        !_sameSourceSpan(
          rowRangeResult.coveredSource,
          ordinalResult.coveredSource,
        )) {
      _windowAcquisition = _windowAcquisition.terminal(
        _FlarkV3ManagedViewportGap.invalidOrdinalWindow,
      );
      _publishGap(
        _FlarkV3ManagedViewportGap.invalidOrdinalWindow,
        totalBlockCount: ordinalResult.totalBlockCount,
        activeOrdinal: effectiveDemand.centerOrdinal,
      );
      return;
    }
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.readyGreen(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16: sourcePointUtf16,
      demand: effectiveDemand,
      window: rowRangeResult,
    );
    _relocationSourcePointUtf16 = null;
    if (_relocationMaximumBlocks != null) {
      final startOrdinal = _recursiveGreenOrdinalAsInt(
        rowRangeResult.startGlobalRowOrdinal,
      );
      _adaptiveWindowPolicy.rememberLimit(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startOrdinal: startOrdinal,
        nextOrdinal: startOrdinal + rowRangeResult.rows.length,
        maximumBlocks: effectiveDemand.maximumBlocks,
      );
      _relocationMaximumBlocks = null;
    }
    _lastWindowDemand = effectiveDemand;
    _authoritySourceRevision = status.sourceRevision;
    _authorityStructureGeneration = status.structureGeneration;
    _lastDriveKey = null;
    _publishGap(
      _FlarkV3ManagedViewportGap.awaitingParserPresentation,
      totalBlockCount: ordinalResult.totalBlockCount,
      activeOrdinal: _pendingActivationOrdinal ?? effectiveDemand.centerOrdinal,
    );
    _drive();
  }

  void _requestOrdinalWindow(FlarkV3ViewportWindowDemand demand) {
    final status = _runtime.status;
    if (!status.structureCurrent || !status.sourceCurrent) {
      _windowAcquisition = _FlarkV3ManagedWindowAcquisition.locating(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        sourcePointUtf16:
            _windowAcquisition.sourcePointUtf16 ?? _relocationSourcePointUtf16,
        demand: demand,
        maximumBlocks: demand.maximumBlocks,
      );
      _publishGap(_FlarkV3ManagedViewportGap.awaitingWindowLocation);
      return;
    }
    final effectiveDemand = _adaptiveWindowPolicy.constrain(
      demand,
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
    );
    final currentWindow = _window;
    if (effectiveDemand == _lastWindowDemand &&
        currentWindow != null &&
        currentWindow.containsOrdinal(effectiveDemand.centerOrdinal)) {
      _activatePendingOrdinal();
      return;
    }
    final sourcePointUtf16 =
        _windowAcquisition.sourcePointUtf16 ?? _relocationSourcePointUtf16;
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.locating(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16: sourcePointUtf16,
      demand: effectiveDemand,
      maximumBlocks: effectiveDemand.maximumBlocks,
    );
    final halfWindow = effectiveDemand.maximumBlocks ~/ 2;
    final startOrdinal = (effectiveDemand.centerOrdinal - halfWindow).clamp(
      0,
      0xffffffff,
    );
    final result = _lease.queryBlockOrdinalWindow(
      FlarkV3DocumentOrdinalWindowDemand(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startBlockOrdinal: startOrdinal,
      ),
      budget: FlarkV3DocumentOrdinalWindowBudget(
        maximumEntries: effectiveDemand.maximumBlocks,
      ),
    );
    if (result is! FlarkV3ExactDocumentOrdinalWindow) {
      final unavailable = result as FlarkV3UnavailableDocumentOrdinalWindow;
      _windowAcquisition = _windowAcquisition.terminal(unavailable.reason);
      _publishGap(
        unavailable.reason,
        totalBlockCount: unavailable.totalBlockCount,
        activeOrdinal: _pendingActivationOrdinal,
      );
      return;
    }
    if (result.startBlockOrdinal > effectiveDemand.centerOrdinal ||
        result.nextBlockOrdinal <= effectiveDemand.centerOrdinal ||
        result.nextBlockOrdinal - result.startBlockOrdinal <= 0 ||
        result.nextBlockOrdinal - result.startBlockOrdinal >
            effectiveDemand.maximumBlocks ||
        result.coveredSource.endUtf16 <= result.coveredSource.startUtf16) {
      _windowAcquisition = _windowAcquisition.terminal(
        _FlarkV3ManagedViewportGap.invalidOrdinalWindow,
      );
      _publishGap(_FlarkV3ManagedViewportGap.invalidOrdinalWindow);
      return;
    }
    _lastWindowDemand = effectiveDemand;
    final window = _FlarkV3ManagedLocatedWindow(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      totalBlockCount: result.totalBlockCount,
      startBlockOrdinal: result.startBlockOrdinal,
      nextBlockOrdinal: result.nextBlockOrdinal,
      coveredSource: result.coveredSource,
      requestedMaximumBlocks: effectiveDemand.maximumBlocks,
    );
    _windowAcquisition = _FlarkV3ManagedWindowAcquisition.readyLegacy(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      sourcePointUtf16: sourcePointUtf16,
      demand: effectiveDemand,
      window: window,
    );
    _relocationSourcePointUtf16 = null;
    if (_relocationMaximumBlocks != null) {
      _adaptiveWindowPolicy.rememberLimit(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startOrdinal: result.startBlockOrdinal,
        nextOrdinal: result.nextBlockOrdinal,
        maximumBlocks: effectiveDemand.maximumBlocks,
      );
      _relocationMaximumBlocks = null;
    }
    _authoritySourceRevision = status.sourceRevision;
    _authorityStructureGeneration = status.structureGeneration;
    _lastDriveKey = null;
    _publishGap(
      _FlarkV3ManagedViewportGap.awaitingWindowStructure,
      totalBlockCount: result.totalBlockCount,
      activeOrdinal: _pendingActivationOrdinal ?? _snapshot.activeOrdinal,
    );
    _visibleBlocks.requestVisibleSourceRange(
      TextRange(
        start: result.coveredSource.startUtf16,
        end: result.coveredSource.endUtf16,
      ),
      maximumBlocks: result.nextBlockOrdinal - result.startBlockOrdinal,
    );
    _drive();
  }

  void _drive() {
    if (_disposed || _driving) return;
    _driving = true;
    try {
      final status = _runtime.status;
      final visible = _visibleBlocks.exactValue;
      final phase = _visibleBlocks.phase;
      final key = (
        status.sourceRevision,
        status.structureGeneration,
        status.inlinePresentationGeneration,
        status.inlineAttemptOutcomeGeneration,
        status.viewportPresentationGeneration,
        status.viewportPresentationAttemptOutcomeGeneration,
        visible,
        phase,
        _windowAcquisition,
      );
      if (key == _lastDriveKey) return;
      _lastDriveKey = key;

      if (!status.sourceCurrent || !status.structureCurrent) {
        _publishGap(_FlarkV3ManagedViewportGap.awaitingWindowStructure);
        return;
      }
      if (_windowAcquisition.phase ==
          _FlarkV3ManagedWindowAcquisitionPhase.locating) {
        _publishGap(_FlarkV3ManagedViewportGap.awaitingWindowLocation);
        return;
      }
      if (_windowAcquisition.phase ==
          _FlarkV3ManagedWindowAcquisitionPhase.terminal) {
        return;
      }
      final recursiveGreen = _recursiveGreenWindow;
      if (recursiveGreen != null) {
        _driveRecursiveGreen(status, recursiveGreen);
        return;
      }
      final window = _window;
      if (window == null) {
        _publishGap(_FlarkV3ManagedViewportGap.invalidOrdinalWindow);
        return;
      }
      if (visible == null ||
          phase != FlarkV3FlutterVisibleBlockPhase.exact ||
          !_isExactLocatedWindow(visible, status, window)) {
        _publishGap(
          _FlarkV3ManagedViewportGap.awaitingWindowStructure,
          totalBlockCount: window.totalBlockCount,
          activeOrdinal: _pendingActivationOrdinal ?? _snapshot.activeOrdinal,
        );
        return;
      }
      final activeQuery = _liveController.paintState.documentQuery;
      final activeKind = activeQuery is FlarkV3DocumentStructuralQuery
          ? activeQuery.structure.kind
          : null;
      final inlineActive =
          activeKind == FlarkV3DocumentStructureKind.paragraph ||
          activeKind == FlarkV3DocumentStructureKind.heading;
      if (!_liveController.semanticActionsValid ||
          inlineActive && !_liveController.hasCertifiedInlinePresentation) {
        _publishGap(_FlarkV3ManagedViewportGap.awaitingActivePresentation);
        return;
      }

      final pendingOrdinal = _pendingActivationOrdinal;
      final activeOrdinal =
          pendingOrdinal != null && window.containsOrdinal(pendingOrdinal)
          ? pendingOrdinal
          : _ordinalContainingSourcePoint(
              visible.blocks,
              _liveController.globalEditingState.selection.extentOffset,
            );
      if (activeOrdinal == null) {
        _publishGap(_FlarkV3ManagedViewportGap.activeStructureUnavailable);
        return;
      }
      final totalBlockCount = window.totalBlockCount;
      _authoritySourceRevision = status.sourceRevision;
      _authorityStructureGeneration = status.structureGeneration;

      final demand = FlarkV3ViewportPresentationDemand(
        sourceRevision: status.sourceRevision,
        structureGeneration: status.structureGeneration,
        startUtf16: window.coveredSource.startUtf16,
        endUtf16: window.coveredSource.endUtf16,
        startBlockOrdinal: window.startBlockOrdinal,
      );
      final receipt = _lease.ensureViewportPresentation(demand);
      if (receipt.disposition !=
          FlarkV3ViewportPresentationDemandDisposition.current) {
        if (receipt.unavailableReason ==
                FlarkV3ViewportPresentationUnavailableReason.budgetExceeded &&
            _retrySmallerViewportWindow(window, activeOrdinal)) {
          return;
        }
        _publishGap(
          receipt.unavailableReason ??
              _FlarkV3ManagedViewportGap.awaitingParserPresentation,
          totalBlockCount: totalBlockCount,
          activeOrdinal: activeOrdinal,
        );
        return;
      }

      final pageResult = _lease.queryViewportPresentation(demand);
      if (pageResult case FlarkV3UnavailableViewportPresentationPage(
        :final reason,
      )) {
        if (reason ==
                FlarkV3ViewportPresentationUnavailableReason.budgetExceeded &&
            _retrySmallerViewportWindow(window, activeOrdinal)) {
          return;
        }
        _publishGap(
          reason,
          totalBlockCount: totalBlockCount,
          activeOrdinal: activeOrdinal,
        );
        return;
      }
      final page = pageResult as FlarkV3ExactViewportPresentationPage;
      final materialization = _materializer.materialize(
        sourceDocument: page.sourceDocument,
        currentStructuralAck: page.currentStructuralAck,
        currentStructureGeneration: page.structureGeneration,
        visibleBlocks: visible,
        page: page.page,
      );
      if (materialization is! FlarkV3ExactViewportPageMaterialization) {
        _publishGap(
          (materialization as FlarkV3SourceFallbackViewportPage).reason,
          totalBlockCount: totalBlockCount,
          activeOrdinal: activeOrdinal,
        );
        return;
      }
      _snapshot = FlarkV3ExactViewportSurfaceSnapshot.fromMaterialization(
        totalBlockCount: totalBlockCount,
        activeOrdinal: activeOrdinal,
        estimatedBlockExtent: _estimatedBlockExtent,
        materialization: materialization,
      );
      notifyListeners();
      _activatePendingOrdinal();
    } finally {
      _driving = false;
    }
  }

  void _driveRecursiveGreen(
    FlarkV3DocumentRuntimeStatus status,
    FlarkV3RecursiveGreenRowRange rowRange,
  ) {
    if (rowRange.sourceRevision != status.sourceRevision ||
        rowRange.structureGeneration != status.structureGeneration ||
        rowRange.rows.isEmpty) {
      _publishGap(_FlarkV3ManagedViewportGap.awaitingWindowStructure);
      return;
    }
    final activeQuery = _liveController.paintState.documentQuery;
    if (activeQuery is! FlarkV3RecursiveGreenPointQuery ||
        activeQuery.sourceRevision != status.sourceRevision ||
        activeQuery.structureRevision != status.sourceRevision ||
        (activeQuery.owner.kind?.isInlineBearing ?? false) &&
            !_liveController.hasCertifiedInlinePresentation) {
      _publishGap(_FlarkV3ManagedViewportGap.awaitingActivePresentation);
      return;
    }

    final pendingOrdinal = _pendingActivationOrdinal;
    final activeRow =
        pendingOrdinal != null &&
            _recursiveGreenContainsOrdinal(rowRange, pendingOrdinal)
        ? _recursiveGreenRowAtOrdinal(rowRange, pendingOrdinal)
        : _recursiveGreenRowAtSourcePoint(
            rowRange,
            _liveController.globalEditingState.selection.extentOffset,
          );
    if (activeRow == null) {
      _publishGap(_FlarkV3ManagedViewportGap.activeStructureUnavailable);
      return;
    }
    final activeOrdinal = _recursiveGreenOrdinalAsInt(activeRow.globalOrdinal);
    final totalBlockCount = _recursiveGreenOrdinalAsInt(
      rowRange.totalGlobalRowCount,
    );
    _authoritySourceRevision = status.sourceRevision;
    _authorityStructureGeneration = status.structureGeneration;
    final demand = FlarkV3ViewportPresentationDemand(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      startUtf16: rowRange.coveredSource.startUtf16,
      endUtf16: rowRange.coveredSource.endUtf16,
      startBlockOrdinal: _recursiveGreenOrdinalAsInt(
        rowRange.startGlobalRowOrdinal,
      ),
    );
    final failedWindow = _FlarkV3ManagedLocatedWindow(
      sourceRevision: status.sourceRevision,
      structureGeneration: status.structureGeneration,
      totalBlockCount: totalBlockCount,
      startBlockOrdinal: _recursiveGreenOrdinalAsInt(
        rowRange.startGlobalRowOrdinal,
      ),
      nextBlockOrdinal:
          _recursiveGreenOrdinalAsInt(rowRange.startGlobalRowOrdinal) +
          rowRange.rows.length,
      coveredSource: rowRange.coveredSource,
      requestedMaximumBlocks: _windowAcquisition.requestedMaximumBlocks,
    );
    final receipt = _lease.ensureViewportPresentation(demand);
    if (receipt.disposition !=
        FlarkV3ViewportPresentationDemandDisposition.current) {
      if (receipt.unavailableReason ==
              FlarkV3ViewportPresentationUnavailableReason.budgetExceeded &&
          _retrySmallerViewportWindow(
            failedWindow,
            activeOrdinal,
            recursiveGreen: true,
          )) {
        return;
      }
      _publishGap(
        receipt.unavailableReason ??
            _FlarkV3ManagedViewportGap.awaitingParserPresentation,
        totalBlockCount: totalBlockCount,
        activeOrdinal: activeOrdinal,
      );
      return;
    }
    final pageResult = _lease.queryViewportPresentation(demand);
    if (pageResult case FlarkV3UnavailableViewportPresentationPage(
      :final reason,
    )) {
      if (reason ==
              FlarkV3ViewportPresentationUnavailableReason.budgetExceeded &&
          _retrySmallerViewportWindow(
            failedWindow,
            activeOrdinal,
            recursiveGreen: true,
          )) {
        return;
      }
      _publishGap(
        reason,
        totalBlockCount: totalBlockCount,
        activeOrdinal: activeOrdinal,
      );
      return;
    }
    final page = pageResult as FlarkV3ExactViewportPresentationPage;
    final materialization = _materializer.materializeRecursiveGreenRows(
      sourceDocument: page.sourceDocument,
      currentStructuralAck: page.currentStructuralAck,
      currentStructureGeneration: page.structureGeneration,
      rowRange: rowRange,
      page: page.page,
    );
    if (materialization
        is! FlarkV3ExactRecursiveGreenViewportPageMaterialization) {
      _publishGap(
        (materialization as FlarkV3SourceFallbackViewportPage).reason,
        totalBlockCount: totalBlockCount,
        activeOrdinal: activeOrdinal,
      );
      return;
    }
    _snapshot =
        FlarkV3ExactViewportSurfaceSnapshot.fromRecursiveGreenMaterialization(
          activeOrdinal: activeOrdinal,
          estimatedBlockExtent: _estimatedBlockExtent,
          materialization: materialization,
        );
    notifyListeners();
    _activatePendingOrdinal();
    _adoptActiveRecursiveGreenAuthority();
  }

  bool _retrySmallerViewportWindow(
    _FlarkV3ManagedLocatedWindow failedWindow,
    int activeOrdinal, {
    bool recursiveGreen = false,
  }) {
    final retry = _adaptiveWindowPolicy.recordBudgetExceeded(
      sourceRevision: failedWindow.sourceRevision,
      structureGeneration: failedWindow.structureGeneration,
      failedStartOrdinal: failedWindow.startBlockOrdinal,
      failedNextOrdinal: failedWindow.nextBlockOrdinal,
      failedMaximumBlocks: failedWindow.requestedMaximumBlocks,
      activeOrdinal: activeOrdinal,
    );
    if (retry == null) {
      _lastWindowDemand = FlarkV3ViewportWindowDemand(
        centerOrdinal: activeOrdinal,
        maximumBlocks: 1,
      );
      return false;
    }
    _lastDriveKey = null;
    if (recursiveGreen) {
      _requestRecursiveGreenOrdinalWindow(retry);
    } else {
      _requestOrdinalWindow(retry);
    }
    return true;
  }

  void _retainAdaptiveRelocationLimit() {
    if (_relocationMaximumBlocks != null) return;
    final window = _window;
    final greenWindow = _recursiveGreenWindow;
    final retainedMaximumBlocks = switch ((window, greenWindow)) {
      (final legacy?, _) when legacy.containsOrdinal(_snapshot.activeOrdinal) =>
        legacy.requestedMaximumBlocks,
      (_, final green?)
          when _recursiveGreenContainsOrdinal(green, _snapshot.activeOrdinal) =>
        _windowAcquisition.demand?.maximumBlocks,
      _ => null,
    };
    if (retainedMaximumBlocks == null) return;
    final constrained = _adaptiveWindowPolicy.constrain(
      FlarkV3ViewportWindowDemand(
        centerOrdinal: _snapshot.activeOrdinal,
        maximumBlocks: FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
      ),
      sourceRevision: _windowAcquisition.sourceRevision,
      structureGeneration: _windowAcquisition.structureGeneration,
    );
    final maximumBlocks = constrained.maximumBlocks < retainedMaximumBlocks
        ? constrained.maximumBlocks
        : retainedMaximumBlocks;
    if (maximumBlocks < FlarkV3VisibleBlockDemand.defaultMaximumBlocks) {
      _relocationMaximumBlocks = maximumBlocks;
    }
  }

  bool _isExactLocatedWindow(
    FlarkV3ExactVisibleBlockSet visible,
    FlarkV3DocumentRuntimeStatus status,
    _FlarkV3ManagedLocatedWindow window,
  ) {
    final blocks = visible.blocks;
    return visible.demand.sourceRevision == status.sourceRevision &&
        visible.demand.structureGeneration == status.structureGeneration &&
        visible.demand.startUtf16 == window.coveredSource.startUtf16 &&
        visible.demand.endUtf16 == window.coveredSource.endUtf16 &&
        visible.demandCovered &&
        !visible.truncated &&
        blocks.isNotEmpty &&
        blocks.first.ordinal == window.startBlockOrdinal &&
        blocks.last.ordinal + 1 == window.nextBlockOrdinal &&
        visible.coveredSource.startUtf8 == window.coveredSource.startUtf8 &&
        visible.coveredSource.endUtf8 == window.coveredSource.endUtf8 &&
        visible.coveredSource.startUtf16 == window.coveredSource.startUtf16 &&
        visible.coveredSource.endUtf16 == window.coveredSource.endUtf16;
  }

  void _activatePendingOrdinal() {
    final ordinal = _pendingActivationOrdinal;
    final current = _snapshot;
    if (ordinal == null || current is! FlarkV3ExactViewportSurfaceSnapshot) {
      return;
    }
    FlarkV3ParserAuthoredBlockPresentation? target;
    for (final block in current.blocks) {
      if (block.ordinal == ordinal) {
        target = block;
        break;
      }
    }
    if (target == null) return;
    final requestedCaret = _pendingActivationSourcePointUtf16;
    final caret = requestedCaret == null
        ? target.visibleSource.startUtf16
        : requestedCaret.clamp(
            target.visibleSource.startUtf16,
            target.visibleSource.endUtf16,
          );
    final handoffSource = target.recursiveGreenRow == null
        ? target.physicalSource
        : target.visibleSource;
    final nextEditingState = FlarkV3GlobalEditingState(
      selection: TextSelection.collapsed(offset: caret),
      composing: TextRange.empty,
    );
    final greenInputLease = target.recursiveGreenInputLease;
    if (greenInputLease == null) {
      _liveController.handoffInputIslandWithinExactRange(
        startUtf16: handoffSource.startUtf16,
        endUtf16: handoffSource.endUtf16,
        nextGlobalEditingState: nextEditingState,
      );
    } else {
      final status = _runtime.status;
      final targetAck = target.recursiveGreenStructuralAck;
      final presentation = _liveController.documentSession.presentationState;
      if (targetAck == null ||
          presentation is! FlarkV3ExactStructuralPresentation ||
          presentation.ack != targetAck ||
          target.identity != current.identity ||
          target.identity.sourceVersion != targetAck.sourceVersion ||
          target.identity.sourceRoot != targetAck.sourceRoot ||
          target.identity.parseGeneration != targetAck.parseGeneration ||
          target.identity.sourceVersion !=
              greenInputLease.certifiedSourceVersion ||
          target.identity.structureGeneration != status.structureGeneration ||
          target.identity.viewportGeneration !=
              status.viewportPresentationGeneration ||
          !status.sourceCurrent ||
          !status.structureCurrent) {
        _lastWindowDemand = null;
        _lastDriveKey = null;
        _relocationSourcePointUtf16 = caret;
        _publishGap(
          _FlarkV3ManagedViewportGap.structureChanged,
          totalBlockCount: current.totalBlockCount,
          activeOrdinal: current.activeOrdinal,
        );
        _requestAuthorityWindow();
        return;
      }
      _liveController.handoffProjectedInputIslandToExactRange(
        inputLease: greenInputLease,
        nextGlobalEditingState: nextEditingState,
      );
    }
    _pendingActivationOrdinal = null;
    _pendingActivationSourcePointUtf16 = null;
    _snapshot = FlarkV3ExactViewportSurfaceSnapshot(
      totalBlockCount: current.totalBlockCount,
      activeOrdinal: ordinal,
      estimatedBlockExtent: current.estimatedBlockExtent,
      identity: current.identity,
      blocks: current.blocks,
    );
    notifyListeners();
    _adoptActiveRecursiveGreenAuthority();
  }

  void _adoptActiveRecursiveGreenAuthority() {
    final current = _snapshot;
    if (current is! FlarkV3ExactViewportSurfaceSnapshot) return;
    FlarkV3ParserAuthoredBlockPresentation? active;
    for (final block in current.blocks) {
      if (block.ordinal == current.activeOrdinal) {
        active = block;
        break;
      }
    }
    final row = active?.recursiveGreenRow;
    final ack = active?.recursiveGreenStructuralAck;
    final query = _liveController.paintState.documentQuery;
    if (row != null &&
        ack != null &&
        query is FlarkV3RecursiveGreenPointQuery &&
        flarkV3MatchesTopLevelThematicBreakAuthority(query, row)) {
      final atom = row.presentationPhysicalSource;
      final islandStart = _liveController.inputIslandGlobalStartUtf16;
      if (islandStart == _liveController.inputIslandGlobalEndUtf16 &&
          (islandStart == atom.startUtf16 || islandStart == atom.endUtf16)) {
        _liveController.adoptRecursiveGreenAtomicAuthority(
          structuralAck: ack,
          row: row,
        );
      }
      return;
    }
    final editableSource = row?.editableSource;
    if (row == null ||
        editableSource == null ||
        ack == null ||
        query is! FlarkV3RecursiveGreenPointQuery ||
        !_recursiveGreenQueryMatchesRow(query, row) ||
        _liveController.inputIslandGlobalStartUtf16 !=
            editableSource.startUtf16 ||
        _liveController.inputIslandGlobalEndUtf16 != editableSource.endUtf16) {
      return;
    }
    _liveController.adoptRecursiveGreenRowAuthority(
      structuralAck: ack,
      row: row,
    );
  }

  void _publishGap(Object reason, {int? totalBlockCount, int? activeOrdinal}) {
    final currentTotal = totalBlockCount ?? _snapshot.totalBlockCount;
    final safeTotal = currentTotal <= 0 ? 1 : currentTotal;
    final currentActive = activeOrdinal ?? _snapshot.activeOrdinal;
    final safeActive = currentActive.clamp(0, safeTotal - 1);
    final previous = _snapshot;
    if (previous is FlarkV3SourceGapViewportSurfaceSnapshot &&
        previous.totalBlockCount == safeTotal &&
        previous.activeOrdinal == safeActive &&
        previous.reason == reason) {
      return;
    }
    _snapshot = FlarkV3SourceGapViewportSurfaceSnapshot(
      totalBlockCount: safeTotal,
      activeOrdinal: safeActive,
      estimatedBlockExtent: _estimatedBlockExtent,
      reason: reason,
    );
    notifyListeners();
  }

  void _requireAttached() {
    if (_disposed) {
      throw StateError('The managed viewport presentation source is disposed.');
    }
  }
}

int? _ordinalContainingSourcePoint(
  List<FlarkV3DocumentStructuralBlock> blocks,
  int positionUtf16,
) {
  FlarkV3DocumentStructuralBlock? downstreamBoundary;
  for (final block in blocks) {
    final source = block.structure.source;
    if (positionUtf16 > source.startUtf16 && positionUtf16 < source.endUtf16) {
      return block.ordinal;
    }
    if (positionUtf16 == source.startUtf16) return block.ordinal;
    if (positionUtf16 == source.endUtf16) downstreamBoundary = block;
  }
  return downstreamBoundary?.ordinal;
}

int _recursiveGreenOrdinalAsInt(BigInt ordinal) {
  if (ordinal < BigInt.zero || ordinal > BigInt.from(0xffffffff)) {
    throw RangeError('A Flutter row ordinal must fit the product u32 range.');
  }
  return ordinal.toInt();
}

bool _recursiveGreenContainsOrdinal(
  FlarkV3RecursiveGreenRowRange range,
  int ordinal,
) => range.rows.any((row) => row.globalOrdinal == BigInt.from(ordinal));

FlarkV3RecursiveGreenRenderableRow? _recursiveGreenRowAtOrdinal(
  FlarkV3RecursiveGreenRowRange range,
  int ordinal,
) {
  final expected = BigInt.from(ordinal);
  for (final row in range.rows) {
    if (row.globalOrdinal == expected) return row;
  }
  return null;
}

FlarkV3RecursiveGreenRenderableRow? _recursiveGreenRowAtSourcePoint(
  FlarkV3RecursiveGreenRowRange range,
  int positionUtf16,
) {
  FlarkV3RecursiveGreenRenderableRow? downstreamBoundary;
  for (final row in range.rows) {
    final source = row.editableSource;
    if (source == null) continue;
    if (positionUtf16 >= source.startUtf16 && positionUtf16 < source.endUtf16) {
      return row;
    }
    if (positionUtf16 == source.endUtf16) downstreamBoundary = row;
  }
  if (downstreamBoundary != null) return downstreamBoundary;
  for (final row in range.rows) {
    final source = row.physicalSource;
    if (positionUtf16 >= source.startUtf16 && positionUtf16 < source.endUtf16) {
      return row;
    }
    if (positionUtf16 == source.endUtf16) downstreamBoundary = row;
  }
  if (downstreamBoundary != null) return downstreamBoundary;
  if (positionUtf16 >= range.coveredSource.startUtf16 &&
      positionUtf16 <= range.coveredSource.endUtf16) {
    // Separator bytes have no render row. The parser-selected row carries
    // the exact boundary affinity for this point query.
    return range.selectedRow;
  }
  return null;
}

bool _recursiveGreenQueryMatchesRow(
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3RecursiveGreenRenderableRow row,
) {
  final editableSource = row.editableSource;
  if (editableSource == null ||
      query.owner.frameId != row.frameId ||
      query.owner.kind != row.kind ||
      query.ancestry.length != row.path.length) {
    return false;
  }
  for (var index = 0; index < row.path.length; index += 1) {
    final queryFrame = query.ancestry[index];
    final rowFrame = row.path[index];
    if (queryFrame.frameId != rowFrame.frameId ||
        queryFrame.kind != rowFrame.kind) {
      return false;
    }
  }
  if (row.kind.isInlineBearing) {
    return _sameOptionalSpan(query.paragraphSource, row.physicalSource) &&
        _sameOptionalSpan(query.inlineSource, editableSource);
  }
  if (row.kind.isTerminalEmptyItem) {
    return row.presentationKind ==
            FlarkV3RecursiveGreenRowPresentationKind.inline &&
        row.editCapability ==
            FlarkV3RecursiveGreenRowEditCapability.contiguous &&
        !row.inlineCapable &&
        query.isIdentityEditableContent &&
        query.pointUtf8 == editableSource.startUtf8 &&
        query.pointUtf16 == editableSource.startUtf16 &&
        query.paragraphSource == null &&
        query.inlineSource == null &&
        query.inlineFacts == null &&
        _sourceSpanContainsSpan(row.presentationPhysicalSource, query.source);
  }
  return row.kind == FlarkV3RecursiveGreenKind.fencedCode &&
      row.presentationKind ==
          FlarkV3RecursiveGreenRowPresentationKind.fencedCode &&
      row.literal &&
      row.editCapability == FlarkV3RecursiveGreenRowEditCapability.contiguous &&
      query.isIdentityEditableContent &&
      _sourceSpanContainsSpan(editableSource, query.source);
}

bool _sameOptionalSpan(FlarkV3SourceSpan? left, FlarkV3SourceSpan right) =>
    left != null && _sameSourceSpan(left, right);

bool _sameSourceSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _sourceSpanContainsSpan(
  FlarkV3SourceSpan outer,
  FlarkV3SourceSpan inner,
) =>
    inner.startUtf8 >= outer.startUtf8 &&
    inner.endUtf8 <= outer.endUtf8 &&
    inner.startUtf16 >= outer.startUtf16 &&
    inner.endUtf16 <= outer.endUtf16;

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xD800 && codeUnit <= 0xDBFF;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xDC00 && codeUnit <= 0xDFFF;

final class _FlarkV3ManagedLocatedWindow {
  const _FlarkV3ManagedLocatedWindow({
    required this.sourceRevision,
    required this.structureGeneration,
    required this.totalBlockCount,
    required this.startBlockOrdinal,
    required this.nextBlockOrdinal,
    required this.coveredSource,
    required this.requestedMaximumBlocks,
  });

  final int sourceRevision;
  final int structureGeneration;
  final int totalBlockCount;
  final int startBlockOrdinal;
  final int nextBlockOrdinal;
  final FlarkV3SourceSpan coveredSource;
  final int requestedMaximumBlocks;

  bool containsOrdinal(int ordinal) =>
      ordinal >= startBlockOrdinal && ordinal < nextBlockOrdinal;

  bool get isCompleteDocument =>
      startBlockOrdinal == 0 && nextBlockOrdinal == totalBlockCount;

  _FlarkV3ManagedLocatedWindow withRequestedMaximumBlocks(int value) =>
      _FlarkV3ManagedLocatedWindow(
        sourceRevision: sourceRevision,
        structureGeneration: structureGeneration,
        totalBlockCount: totalBlockCount,
        startBlockOrdinal: startBlockOrdinal,
        nextBlockOrdinal: nextBlockOrdinal,
        coveredSource: coveredSource,
        requestedMaximumBlocks: value,
      );
}

enum _FlarkV3ManagedWindowAcquisitionPhase {
  locating,
  readyLegacy,
  readyGreen,
  terminal,
}

/// The authority-scoped state of the single viewport-acquisition lane.
///
/// A missing installed window is not ambiguous: it is either still locating
/// or it ended in a typed terminal result. The retained intent also lets an
/// authority refresh restart at the learned bounded output size.
final class _FlarkV3ManagedWindowAcquisition {
  const _FlarkV3ManagedWindowAcquisition._({
    required this.phase,
    required this.sourceRevision,
    required this.structureGeneration,
    required this.sourcePointUtf16,
    required this.demand,
    required this.requestedMaximumBlocks,
    required this.legacyWindow,
    required this.recursiveGreenWindow,
    required this.terminalReason,
  }) : assert(requestedMaximumBlocks > 0),
       assert(
         phase != _FlarkV3ManagedWindowAcquisitionPhase.readyLegacy ||
             legacyWindow != null &&
                 recursiveGreenWindow == null &&
                 terminalReason == null,
       ),
       assert(
         phase != _FlarkV3ManagedWindowAcquisitionPhase.readyGreen ||
             legacyWindow == null &&
                 recursiveGreenWindow != null &&
                 terminalReason == null,
       ),
       assert(
         phase != _FlarkV3ManagedWindowAcquisitionPhase.locating ||
             legacyWindow == null &&
                 recursiveGreenWindow == null &&
                 terminalReason == null,
       ),
       assert(
         phase != _FlarkV3ManagedWindowAcquisitionPhase.terminal ||
             legacyWindow == null &&
                 recursiveGreenWindow == null &&
                 terminalReason != null,
       );

  factory _FlarkV3ManagedWindowAcquisition.locating({
    required int sourceRevision,
    required int structureGeneration,
    required int? sourcePointUtf16,
    FlarkV3ViewportWindowDemand? demand,
    required int maximumBlocks,
  }) => _FlarkV3ManagedWindowAcquisition._(
    phase: _FlarkV3ManagedWindowAcquisitionPhase.locating,
    sourceRevision: sourceRevision,
    structureGeneration: structureGeneration,
    sourcePointUtf16: sourcePointUtf16,
    demand: demand,
    requestedMaximumBlocks: maximumBlocks,
    legacyWindow: null,
    recursiveGreenWindow: null,
    terminalReason: null,
  );

  factory _FlarkV3ManagedWindowAcquisition.readyLegacy({
    required int sourceRevision,
    required int structureGeneration,
    required int? sourcePointUtf16,
    required FlarkV3ViewportWindowDemand demand,
    required _FlarkV3ManagedLocatedWindow window,
  }) => _FlarkV3ManagedWindowAcquisition._(
    phase: _FlarkV3ManagedWindowAcquisitionPhase.readyLegacy,
    sourceRevision: sourceRevision,
    structureGeneration: structureGeneration,
    sourcePointUtf16: sourcePointUtf16,
    demand: demand,
    requestedMaximumBlocks: demand.maximumBlocks,
    legacyWindow: window,
    recursiveGreenWindow: null,
    terminalReason: null,
  );

  factory _FlarkV3ManagedWindowAcquisition.readyGreen({
    required int sourceRevision,
    required int structureGeneration,
    required int? sourcePointUtf16,
    required FlarkV3ViewportWindowDemand demand,
    required FlarkV3RecursiveGreenRowRange window,
  }) => _FlarkV3ManagedWindowAcquisition._(
    phase: _FlarkV3ManagedWindowAcquisitionPhase.readyGreen,
    sourceRevision: sourceRevision,
    structureGeneration: structureGeneration,
    sourcePointUtf16: sourcePointUtf16,
    demand: demand,
    requestedMaximumBlocks: demand.maximumBlocks,
    legacyWindow: null,
    recursiveGreenWindow: window,
    terminalReason: null,
  );

  final _FlarkV3ManagedWindowAcquisitionPhase phase;
  final int sourceRevision;
  final int structureGeneration;
  final int? sourcePointUtf16;
  final FlarkV3ViewportWindowDemand? demand;
  final int requestedMaximumBlocks;
  final _FlarkV3ManagedLocatedWindow? legacyWindow;
  final FlarkV3RecursiveGreenRowRange? recursiveGreenWindow;
  final Object? terminalReason;

  _FlarkV3ManagedWindowAcquisition terminal(Object reason) =>
      _FlarkV3ManagedWindowAcquisition._(
        phase: _FlarkV3ManagedWindowAcquisitionPhase.terminal,
        sourceRevision: sourceRevision,
        structureGeneration: structureGeneration,
        sourcePointUtf16: sourcePointUtf16,
        demand: demand,
        requestedMaximumBlocks: requestedMaximumBlocks,
        legacyWindow: null,
        recursiveGreenWindow: null,
        terminalReason: reason,
      );
}

/// Bounded, authority-scoped policy for reducing unusually dense viewports.
///
/// The parser remains definitive. This policy only chooses a smaller exact
/// ordinal cut after the parser truthfully reports that a larger cut exceeded
/// its production work profile. Learned limits are retained around the failed
/// region so ordinary scroll notifications cannot immediately rebound to the
/// failed size.
@visibleForTesting
final class FlarkV3AdaptiveViewportWindowPolicy {
  static const int _maximumRememberedRegions = 8;

  final List<_FlarkV3AdaptiveViewportRegion> _regions =
      <_FlarkV3AdaptiveViewportRegion>[];
  int? _sourceRevision;
  int? _structureGeneration;

  FlarkV3ViewportWindowDemand constrain(
    FlarkV3ViewportWindowDemand demand, {
    required int sourceRevision,
    required int structureGeneration,
  }) {
    _bindAuthority(sourceRevision, structureGeneration);
    var maximumBlocks = demand.maximumBlocks;
    for (final region in _regions) {
      if (region.contains(demand.centerOrdinal) &&
          region.maximumBlocks < maximumBlocks) {
        maximumBlocks = region.maximumBlocks;
      }
    }
    return maximumBlocks == demand.maximumBlocks
        ? demand
        : FlarkV3ViewportWindowDemand(
            centerOrdinal: demand.centerOrdinal,
            maximumBlocks: maximumBlocks,
          );
  }

  FlarkV3ViewportWindowDemand? recordBudgetExceeded({
    required int sourceRevision,
    required int structureGeneration,
    required int failedStartOrdinal,
    required int failedNextOrdinal,
    required int failedMaximumBlocks,
    required int activeOrdinal,
  }) {
    _bindAuthority(sourceRevision, structureGeneration);
    if (failedStartOrdinal < 0 ||
        failedNextOrdinal <= failedStartOrdinal ||
        activeOrdinal < failedStartOrdinal ||
        activeOrdinal >= failedNextOrdinal ||
        failedMaximumBlocks <= 0) {
      throw ArgumentError('A failed viewport window must contain its anchor.');
    }
    final failedBlockCount = failedNextOrdinal - failedStartOrdinal;
    final halfRequested = failedMaximumBlocks ~/ 2;
    final halfActual = failedBlockCount ~/ 2;
    var nextMaximumBlocks = halfRequested < halfActual
        ? halfRequested
        : halfActual;
    if (nextMaximumBlocks < 1) nextMaximumBlocks = 1;
    _remember(
      _FlarkV3AdaptiveViewportRegion(
        startOrdinal: failedStartOrdinal,
        nextOrdinal: failedNextOrdinal,
        maximumBlocks: nextMaximumBlocks,
      ),
    );
    if (failedMaximumBlocks <= 1 || failedBlockCount <= 1) return null;
    return FlarkV3ViewportWindowDemand(
      centerOrdinal: activeOrdinal,
      maximumBlocks: nextMaximumBlocks,
    );
  }

  void rememberLimit({
    required int sourceRevision,
    required int structureGeneration,
    required int startOrdinal,
    required int nextOrdinal,
    required int maximumBlocks,
  }) {
    _bindAuthority(sourceRevision, structureGeneration);
    if (startOrdinal < 0 ||
        nextOrdinal <= startOrdinal ||
        maximumBlocks <= 0 ||
        maximumBlocks > flarkV3MaximumMountedViewportPresentations) {
      throw ArgumentError('An adaptive viewport limit must be bounded.');
    }
    _remember(
      _FlarkV3AdaptiveViewportRegion(
        startOrdinal: startOrdinal,
        nextOrdinal: nextOrdinal,
        maximumBlocks: maximumBlocks,
      ),
    );
  }

  void reset() {
    _sourceRevision = null;
    _structureGeneration = null;
    _regions.clear();
  }

  void _bindAuthority(int sourceRevision, int structureGeneration) {
    if (_sourceRevision == sourceRevision &&
        _structureGeneration == structureGeneration) {
      return;
    }
    _sourceRevision = sourceRevision;
    _structureGeneration = structureGeneration;
    _regions.clear();
  }

  void _remember(_FlarkV3AdaptiveViewportRegion next) {
    var start = next.startOrdinal;
    var end = next.nextOrdinal;
    var maximumBlocks = next.maximumBlocks;
    final retained = <_FlarkV3AdaptiveViewportRegion>[];
    for (final region in _regions) {
      if (region.nextOrdinal < start || region.startOrdinal > end) {
        retained.add(region);
        continue;
      }
      if (region.startOrdinal < start) start = region.startOrdinal;
      if (region.nextOrdinal > end) end = region.nextOrdinal;
      if (region.maximumBlocks < maximumBlocks) {
        maximumBlocks = region.maximumBlocks;
      }
    }
    retained.add(
      _FlarkV3AdaptiveViewportRegion(
        startOrdinal: start,
        nextOrdinal: end,
        maximumBlocks: maximumBlocks,
      ),
    );
    if (retained.length > _maximumRememberedRegions) {
      retained.removeRange(0, retained.length - _maximumRememberedRegions);
    }
    _regions
      ..clear()
      ..addAll(retained);
  }
}

final class _FlarkV3AdaptiveViewportRegion {
  const _FlarkV3AdaptiveViewportRegion({
    required this.startOrdinal,
    required this.nextOrdinal,
    required this.maximumBlocks,
  });

  final int startOrdinal;
  final int nextOrdinal;
  final int maximumBlocks;

  bool contains(int ordinal) =>
      ordinal >= startOrdinal && ordinal < nextOrdinal;
}

enum _FlarkV3ManagedViewportGap {
  awaitingCompleteStructure,
  structureChanged,
  awaitingParserPresentation,
  awaitingActivePresentation,
  awaitingWindowLocation,
  awaitingWindowStructure,
  sourcePointUnavailable,
  invalidOrdinalWindow,
  activeStructureUnavailable,
}
