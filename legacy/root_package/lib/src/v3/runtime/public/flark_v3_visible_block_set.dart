import 'dart:math';

import '../flark_v3_parser_transport.dart';
import 'flark_v3_document_query.dart';
import 'flark_v3_document_runtime.dart';

/// One revision-bound source interval requested by a visible document view.
///
/// This is a source-coordinate demand, not a pixel-layout model. Flutter and
/// other adapters remain responsible for translating their layout viewport
/// into exact UTF-16 source bounds.
final class FlarkV3VisibleBlockDemand {
  FlarkV3VisibleBlockDemand({
    required this.sourceRevision,
    required this.structureGeneration,
    required this.startUtf16,
    required this.endUtf16,
    this.maximumBlocks = defaultMaximumBlocks,
  }) {
    if (sourceRevision < 0 ||
        structureGeneration < 0 ||
        startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        maximumBlocks <= 0 ||
        maximumBlocks > maximumMaterializedBlocks) {
      throw RangeError(
        'Visible-block demand is invalid or exceeds the '
        '$maximumMaterializedBlocks-block hard cap.',
      );
    }
  }

  /// Default bounded snapshot size for one visible source window.
  static const int defaultMaximumBlocks =
      flarkV3DefaultViewportPresentationEntryCapacity;

  /// Hard caller-isolate cap for one immutable visible-set snapshot.
  ///
  /// Callers with larger views must window them into multiple demands. This
  /// bounds both host output and the snapshot copy performed after each
  /// materialization quantum.
  static const int maximumMaterializedBlocks = 256;

  final int sourceRevision;
  final int structureGeneration;
  final int startUtf16;
  final int endUtf16;
  final int maximumBlocks;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3VisibleBlockDemand &&
      other.sourceRevision == sourceRevision &&
      other.structureGeneration == structureGeneration &&
      other.startUtf16 == startUtf16 &&
      other.endUtf16 == endUtf16 &&
      other.maximumBlocks == maximumBlocks;

  @override
  int get hashCode => Object.hash(
    sourceRevision,
    structureGeneration,
    startUtf16,
    endUtf16,
    maximumBlocks,
  );
}

sealed class FlarkV3VisibleBlockSet {
  const FlarkV3VisibleBlockSet({required this.demand});

  final FlarkV3VisibleBlockDemand demand;
}

/// Exact consecutive structural metadata accumulated for one visible demand.
final class FlarkV3ExactVisibleBlockSet extends FlarkV3VisibleBlockSet {
  FlarkV3ExactVisibleBlockSet({
    required super.demand,
    required this.coveredSource,
    required List<FlarkV3DocumentStructuralBlock> blocks,
    required this.demandCovered,
    required this.truncated,
  }) : blocks = List<FlarkV3DocumentStructuralBlock>.unmodifiable(blocks);

  final FlarkV3SourceSpan coveredSource;
  final List<FlarkV3DocumentStructuralBlock> blocks;

  /// True only when the exact host range closed the complete demand.
  final bool demandCovered;

  /// The caller's materialization cap was reached before host completion.
  final bool truncated;
}

final class FlarkV3PendingVisibleBlockSet extends FlarkV3VisibleBlockSet {
  const FlarkV3PendingVisibleBlockSet({
    required super.demand,
    required this.reason,
    required this.stableStructureRevision,
  });

  final FlarkV3DocumentPendingReason reason;
  final int? stableStructureRevision;
}

final class FlarkV3SourceGapVisibleBlockSet extends FlarkV3VisibleBlockSet {
  const FlarkV3SourceGapVisibleBlockSet({
    required super.demand,
    required this.reason,
  });

  final FlarkV3DocumentQueryGapReason reason;
}

/// Incrementally materializes one visible structural set in bounded quanta.
///
/// Every [advance] performs at most one host range call. The class never reads
/// block source text, runs Markdown grammar, or loops through continuation
/// pages synchronously.
final class FlarkV3VisibleBlockSetMaterializer {
  FlarkV3VisibleBlockSetMaterializer(this.runtime);

  final FlarkV3DocumentRuntime runtime;

  FlarkV3VisibleBlockDemand? _demand;
  final List<FlarkV3DocumentStructuralBlock> _blocks =
      <FlarkV3DocumentStructuralBlock>[];
  FlarkV3DocumentBlockRangeContinuation? _continuation;
  FlarkV3SourceSpan? _coveredSource;
  FlarkV3VisibleBlockSet? _value;
  bool _started = false;
  bool _complete = false;

  FlarkV3VisibleBlockSet? get value => _value;

  void reset() {
    _demand = null;
    _blocks.clear();
    _continuation = null;
    _coveredSource = null;
    _value = null;
    _started = false;
    _complete = false;
  }

  FlarkV3VisibleBlockSet advance(
    FlarkV3VisibleBlockDemand demand, {
    FlarkV3DocumentBlockRangeBudget budget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) {
    if (demand.startUtf16 < 0 ||
        demand.endUtf16 < demand.startUtf16 ||
        demand.maximumBlocks <= 0 ||
        demand.maximumBlocks >
            FlarkV3VisibleBlockDemand.maximumMaterializedBlocks) {
      throw RangeError('Visible-block demand is outside exact source.');
    }
    final runtimeStatus = runtime.status;
    if (demand.sourceRevision != runtime.sourceRevision) {
      reset();
      return _value = FlarkV3PendingVisibleBlockSet(
        demand: demand,
        reason: FlarkV3DocumentPendingReason.sourceChanged,
        stableStructureRevision: runtime.status.structureRevision,
      );
    }
    if (demand.endUtf16 > runtime.sourceLengthUtf16 ||
        (runtime.sourceLengthUtf16 != 0 &&
            demand.startUtf16 == demand.endUtf16)) {
      throw RangeError('Visible-block demand is outside exact source.');
    }
    if (!runtimeStatus.structureCurrent ||
        demand.structureGeneration != runtimeStatus.structureGeneration) {
      reset();
      return _value = FlarkV3PendingVisibleBlockSet(
        demand: demand,
        reason: FlarkV3DocumentPendingReason.structurePending,
        stableStructureRevision: runtimeStatus.structureRevision,
      );
    }
    if (_demand != demand) {
      reset();
      _demand = demand;
    }
    if (_complete || _blocks.length >= demand.maximumBlocks) {
      return _value = _exactValue(demand, truncated: !_complete);
    }

    final remaining = demand.maximumBlocks - _blocks.length;
    final boundedBudget = FlarkV3DocumentBlockRangeBudget(
      maximumEncodedBytes: budget.maximumEncodedBytes,
      maximumBlockCount: min(budget.maximumBlockCount, remaining),
      maximumStoragePagesVisited: budget.maximumStoragePagesVisited,
      maximumOpenDepth: budget.maximumOpenDepth,
      maximumTreeNodesVisited: budget.maximumTreeNodesVisited,
    );
    final FlarkV3DocumentBlockRangeResult result;
    if (!_started) {
      _started = true;
      result = runtime.queryBlockRange(
        demand.startUtf16,
        demand.endUtf16,
        budget: boundedBudget,
      );
    } else {
      final continuation = _continuation;
      if (continuation == null) {
        _complete = true;
        return _value = _exactValue(demand, truncated: false);
      }
      result = runtime.continueBlockRange(continuation, budget: boundedBudget);
    }

    switch (result) {
      case FlarkV3DocumentPendingBlockRange(
        :final reason,
        :final stableStructureRevision,
      ):
        reset();
        _demand = demand;
        return _value = FlarkV3PendingVisibleBlockSet(
          demand: demand,
          reason: reason,
          stableStructureRevision: stableStructureRevision,
        );
      case FlarkV3DocumentSourceGapBlockRange(:final reason):
        reset();
        _demand = demand;
        return _value = FlarkV3SourceGapVisibleBlockSet(
          demand: demand,
          reason: reason,
        );
      case FlarkV3DocumentStructuralBlockRange():
        _adoptExactPage(demand, result);
        return _value = _exactValue(
          demand,
          truncated: !_complete && _blocks.length >= demand.maximumBlocks,
        );
      case FlarkV3RecursiveGreenRowRange():
        // Recursive-Green rows are materialized by the row/aggregate join,
        // never coerced into this legacy flat top-level block coordinator.
        reset();
        _demand = demand;
        return _value = FlarkV3SourceGapVisibleBlockSet(
          demand: demand,
          reason: FlarkV3DocumentQueryGapReason.unavailableFacts,
        );
    }
  }

  void _adoptExactPage(
    FlarkV3VisibleBlockDemand demand,
    FlarkV3DocumentStructuralBlockRange page,
  ) {
    if (page.sourceRevision != demand.sourceRevision ||
        page.structureGeneration != demand.structureGeneration ||
        page.requestedSource.startUtf16 != demand.startUtf16 ||
        page.requestedSource.endUtf16 != demand.endUtf16 ||
        page.blocks.length > demand.maximumBlocks - _blocks.length) {
      throw const FlarkV3DocumentQueryException(
        'A visible block page escaped its exact demand.',
      );
    }
    if (_blocks.isNotEmpty && page.blocks.isNotEmpty) {
      final previous = _blocks.last;
      final next = page.blocks.first;
      if (next.ordinal != previous.ordinal + 1 ||
          next.structure.source.startUtf8 !=
              previous.structure.source.endUtf8 ||
          next.structure.source.startUtf16 !=
              previous.structure.source.endUtf16) {
        throw const FlarkV3DocumentQueryException(
          'Visible block pages are not one consecutive sequence.',
        );
      }
    }
    final priorCoverage = _coveredSource;
    if (priorCoverage != null &&
        (priorCoverage.endUtf8 != page.coveredSource.startUtf8 ||
            priorCoverage.endUtf16 != page.coveredSource.startUtf16)) {
      throw const FlarkV3DocumentQueryException(
        'Visible block page coverage is discontinuous.',
      );
    }
    _blocks.addAll(page.blocks);
    _coveredSource = priorCoverage == null
        ? page.coveredSource
        : FlarkV3SourceSpan(
            startUtf8: priorCoverage.startUtf8,
            endUtf8: page.coveredSource.endUtf8,
            startUtf16: priorCoverage.startUtf16,
            endUtf16: page.coveredSource.endUtf16,
          );
    _continuation = page.continuation;
    _complete = page.complete;
  }

  FlarkV3ExactVisibleBlockSet _exactValue(
    FlarkV3VisibleBlockDemand demand, {
    required bool truncated,
  }) {
    final coverage =
        _coveredSource ??
        FlarkV3SourceSpan(
          startUtf8: 0,
          endUtf8: 0,
          startUtf16: demand.startUtf16,
          endUtf16: demand.startUtf16,
        );
    return FlarkV3ExactVisibleBlockSet(
      demand: demand,
      coveredSource: coverage,
      blocks: _blocks,
      demandCovered: _complete,
      truncated: truncated,
    );
  }
}
