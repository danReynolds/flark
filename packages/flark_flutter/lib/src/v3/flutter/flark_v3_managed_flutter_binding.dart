import 'dart:async';

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flutter/services.dart';

import 'flark_v3_flutter_live_controller.dart';
import 'flark_v3_indented_code_editing.dart';
import 'flark_v3_inline_editing_presentation.dart';
import 'flark_v3_list_item_editing.dart';
import 'flark_v3_managed_viewport_presentation_source.dart';
import 'flark_v3_visible_block_coordinator.dart';

/// Presentation attachment for a Dart-owned managed v3 document runtime.
///
/// The binding borrows source and structural state, routes Flutter edits back
/// through the runtime-owned executor, and coalesces managed progress onto
/// Flutter frames. Disposing it never closes [runtime].
final class FlarkV3ManagedFlutterBinding {
  FlarkV3ManagedFlutterBinding._({
    required this.runtime,
    required this.controller,
    required FlarkV3DocumentRuntimeAdapterLease lease,
    required StreamSubscription<FlarkV3DocumentRuntimeStatus> subscription,
    required FlarkV3ManagedFlutterRefreshCoordinator refreshCoordinator,
    required this.visibleBlocks,
  }) : _lease = lease,
       _subscription = subscription,
       _refreshCoordinator = refreshCoordinator;

  factory FlarkV3ManagedFlutterBinding.attach({
    required FlarkV3DocumentRuntime runtime,
    required FlarkV3InputIslandSnapshot inputIsland,
    required FlarkV3HostQueryBudget queryBudget,
    FlarkV3FrameScheduler frameScheduler = const FlarkV3FlutterFrameScheduler(),
  }) {
    final lease = FlarkV3DocumentRuntimeAdapter.borrow(
      runtime,
      leafProjectionDemandOwner: true,
      viewportPresentationDemandOwner: true,
    );
    FlarkV3FlutterLiveController? controller;
    FlarkV3ManagedFlutterRefreshCoordinator? refreshCoordinator;
    FlarkV3FlutterVisibleBlockCoordinator? visibleBlocks;
    StreamSubscription<FlarkV3DocumentRuntimeStatus>? subscription;
    try {
      controller = FlarkV3FlutterLiveController.attach(
        documentSession: lease.document,
        inputIsland: inputIsland,
        queryBudget: queryBudget,
        frameScheduler: frameScheduler,
        sourceTransactionApplier: lease.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      refreshCoordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: lease.document,
        controller: controller,
        queryBudget: queryBudget,
        isExactCurrent: () =>
            runtime.status.state == FlarkV3DocumentRuntimeState.open &&
            lease.document.currentUiSourceCertified &&
            lease.document.presentationState
                is FlarkV3ExactStructuralPresentation,
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) =>
            lease.queryAtUtf16(
              positionUtf16,
              affinity: affinity,
              budget: budget,
            ),
        ensureActiveProjectionAtUtf16:
            (positionUtf16, {required affinity, required query}) =>
                lease.ensureActiveProjectionAtUtf16(
                  positionUtf16,
                  affinity: affinity,
                  query: query,
                ),
        inlinePresentationGeneration: () =>
            runtime.status.inlinePresentationGeneration,
        inlineAttemptOutcomeGeneration: () =>
            runtime.status.inlineAttemptOutcomeGeneration,
        inlineDemandReady: () =>
            runtime.status.state == FlarkV3DocumentRuntimeState.open &&
            runtime.status.sourceCurrent,
      );
      visibleBlocks = FlarkV3FlutterVisibleBlockCoordinator.attach(
        runtime: runtime,
        frameScheduler: frameScheduler,
      );
      final coordinator = refreshCoordinator;
      FlarkV3ManagedFlutterBinding? binding;
      subscription = lease.statuses.listen((_) {
        controller!.handleSessionExecutorProgress();
        coordinator.refresh();
        binding?._viewportPresentationSource?.handleRuntimeProgress();
      });
      binding = FlarkV3ManagedFlutterBinding._(
        runtime: runtime,
        controller: controller,
        lease: lease,
        subscription: subscription,
        refreshCoordinator: coordinator,
        visibleBlocks: visibleBlocks,
      );
      // Subscribe before the first refinement demand. A ready native endpoint
      // can complete that demand immediately; starting first would lose the
      // only progress edge and leave the controller in source-gap paint.
      coordinator.start();
      return binding;
    } catch (_) {
      unawaited(subscription?.cancel());
      visibleBlocks?.dispose();
      refreshCoordinator?.dispose();
      controller?.dispose();
      lease.release();
      rethrow;
    }
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3FlutterLiveController controller;

  /// Frame-bounded structural metadata for the Flutter-visible source window.
  ///
  /// The active [controller] remains the sole marker-free editing surface.
  /// This sibling owns no text input and performs no Markdown recognition.
  final FlarkV3FlutterVisibleBlockCoordinator visibleBlocks;

  final FlarkV3DocumentRuntimeAdapterLease _lease;
  final StreamSubscription<FlarkV3DocumentRuntimeStatus> _subscription;
  final FlarkV3ManagedFlutterRefreshCoordinator _refreshCoordinator;
  FlarkV3ManagedViewportPresentationSource? _viewportPresentationSource;
  bool _disposed = false;

  bool get isDisposed => _disposed;

  /// Runtime-backed passive presentation for the virtualized product surface.
  ///
  /// It is null until [attachCompleteDocumentViewportPresentation] is called.
  /// Both attachment methods use authenticated bounded ordinal windows; the
  /// complete-document spelling is a convenience for callers that begin at
  /// the current active source point.
  FlarkV3ManagedViewportPresentationSource? get viewportPresentationSource =>
      _viewportPresentationSource;

  /// Attaches presentation around the current active source point.
  ///
  /// Small documents naturally return one complete cut. Structurally dense
  /// documents use the same authenticated ordinal-window path as large
  /// documents, so byte length never decides whether the surface can mount.
  /// The adapter never counts Markdown-looking blocks.
  FlarkV3ManagedViewportPresentationSource
  attachCompleteDocumentViewportPresentation({
    double estimatedBlockExtent = 44,
  }) {
    if (_disposed) {
      throw StateError('The managed Flutter binding is disposed.');
    }
    final current = _viewportPresentationSource;
    if (current != null) return current;
    return _viewportPresentationSource =
        FlarkV3ManagedViewportPresentationSource.attachCompleteDocument(
          runtime: runtime,
          lease: _lease,
          liveController: controller,
          visibleBlocks: visibleBlocks,
          estimatedBlockExtent: estimatedBlockExtent,
        );
  }

  /// Attaches the locator-backed production path around an exact source point.
  ///
  /// The source point is resolved to a canonical structural ordinal by a
  /// bounded parser range query. Subsequent windows use authenticated ordinal
  /// cuts and never infer ordinals from Markdown text.
  FlarkV3ManagedViewportPresentationSource
  attachViewportPresentationAroundSourcePoint({
    required int sourcePointUtf16,
    double estimatedBlockExtent = 44,
  }) {
    if (_disposed) {
      throw StateError('The managed Flutter binding is disposed.');
    }
    final current = _viewportPresentationSource;
    if (current != null) return current;
    return _viewportPresentationSource =
        FlarkV3ManagedViewportPresentationSource.attachAroundSourcePoint(
          runtime: runtime,
          lease: _lease,
          liveController: controller,
          visibleBlocks: visibleBlocks,
          estimatedBlockExtent: estimatedBlockExtent,
          sourcePointUtf16: sourcePointUtf16,
        );
  }

  /// Detaches Flutter resources while leaving the Dart runtime alive.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    unawaited(_subscription.cancel());
    _refreshCoordinator.dispose();
    _viewportPresentationSource?.dispose();
    visibleBlocks.dispose();
    controller.dispose();
    _lease.release();
  }
}

typedef FlarkV3ManagedDocumentPointQuery =
    FlarkV3DocumentQueryResult Function(
      int positionUtf16, {
      required FlarkV3DocumentQueryAffinity affinity,
      required FlarkV3DocumentQueryBudget budget,
    });

typedef FlarkV3ManagedActiveProjectionDemand =
    FlarkV3LeafProjectionDemandDisposition Function(
      int positionUtf16, {
      required FlarkV3DocumentQueryAffinity affinity,
      required FlarkV3DocumentQueryResult query,
    });

/// Package-internal selection-to-query coordinator shared with focused fakes.
///
/// The public binding remains the runtime owner. Keeping this seam separate
/// lets selection-driven behavior be tested without manufacturing parser
/// publications or adding a query override to the package API.
final class FlarkV3ManagedFlutterRefreshCoordinator {
  FlarkV3ManagedFlutterRefreshCoordinator({
    required this.document,
    required this.controller,
    required this.queryBudget,
    required this.isExactCurrent,
    required this.queryAtUtf16,
    required this.ensureActiveProjectionAtUtf16,
    required this.inlinePresentationGeneration,
    required this.inlineAttemptOutcomeGeneration,
    required this.inlineDemandReady,
  });

  final FlarkDocumentSession document;
  final FlarkV3FlutterLiveController controller;
  final FlarkV3HostQueryBudget queryBudget;
  final bool Function() isExactCurrent;
  final FlarkV3ManagedDocumentPointQuery queryAtUtf16;
  final FlarkV3ManagedActiveProjectionDemand ensureActiveProjectionAtUtf16;
  final int Function() inlinePresentationGeneration;
  final int Function() inlineAttemptOutcomeGeneration;
  final bool Function() inlineDemandReady;

  bool _started = false;
  bool _disposed = false;
  bool _refreshing = false;
  bool _refreshPending = false;
  _FlarkV3ManagedRefreshKey? _lastRefreshKey;
  _FlarkV3ManagedInlineQueryCache? _inlineQueryCache;
  bool _lastPointQueryReusedCache = false;

  void start() {
    if (_disposed) {
      throw StateError('The managed Flutter refresh coordinator is disposed.');
    }
    if (_started) return;
    if (controller.pointQueryOwnership !=
        FlarkV3FlutterPointQueryOwnership.managedCoordinator) {
      throw StateError(
        'A managed refresh coordinator requires managed point-query ownership.',
      );
    }
    _started = true;
    controller.addListener(_handleControllerChange);
    refresh();
  }

  void refresh() {
    if (_disposed) return;
    if (_refreshing) {
      _refreshPending = true;
      return;
    }
    _refreshing = true;
    try {
      do {
        _refreshPending = false;
        final key = _currentRefreshKey();
        if (key == _lastRefreshKey) continue;
        _lastRefreshKey = key;
        _refreshInlineIsland(
          document: document,
          controller: controller,
          queryBudget: queryBudget,
          isExactCurrent: isExactCurrent,
          queryAtUtf16: _queryAtUtf16WithCache,
          ensureActiveProjectionAtUtf16: ensureActiveProjectionAtUtf16,
          shouldRequestInline: () => !_lastPointQueryReusedCache,
        );
        _lastRefreshKey = _currentRefreshKey();
      } while (_refreshPending);
    } finally {
      _refreshing = false;
    }
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _inlineQueryCache = null;
    if (_started) controller.removeListener(_handleControllerChange);
  }

  void _handleControllerChange() => refresh();

  _FlarkV3ManagedRefreshKey _currentRefreshKey() => _FlarkV3ManagedRefreshKey(
    presentation: _FlarkV3ManagedPresentationKey.from(
      document.presentationState,
    ),
    exactCurrent: isExactCurrent(),
    uiRevision: document.uiRevision,
    selection: controller.globalEditingState.selection,
    composing: controller.globalEditingState.composing,
    islandStartUtf16: controller.inputIslandGlobalStartUtf16,
    islandEndUtf16: controller.inputIslandGlobalEndUtf16,
    inlinePresentationGeneration: inlinePresentationGeneration(),
    inlineAttemptOutcomeGeneration: inlineAttemptOutcomeGeneration(),
    inlineDemandReady: inlineDemandReady(),
  );

  FlarkV3DocumentQueryResult _queryAtUtf16WithCache(
    int positionUtf16, {
    required FlarkV3DocumentQueryAffinity affinity,
    required FlarkV3DocumentQueryBudget budget,
  }) {
    final presentation = document.presentationState;
    final authority = _FlarkV3ManagedPresentationKey.from(presentation);
    final presentationGeneration = inlinePresentationGeneration();
    final outcomeGeneration = inlineAttemptOutcomeGeneration();
    final demandReady = inlineDemandReady();
    final cached = _inlineQueryCache;
    final exactCurrent = isExactCurrent();
    if (exactCurrent &&
        cached != null &&
        cached.authority == authority &&
        cached.uiRevision == document.uiRevision &&
        cached.inlinePresentationGeneration == presentationGeneration &&
        cached.inlineAttemptOutcomeGeneration == outcomeGeneration &&
        cached.inlineDemandReady == demandReady &&
        cached.containsStrictly(positionUtf16)) {
      _lastPointQueryReusedCache = true;
      return cached.query;
    }

    _lastPointQueryReusedCache = false;
    final result = queryAtUtf16(
      positionUtf16,
      affinity: affinity,
      budget: budget,
    );
    if (exactCurrent &&
        result is FlarkV3DocumentStructuralQuery &&
        (result.structure.canCarryInlineFacts ||
            result.structure.kind ==
                FlarkV3DocumentStructureKind.indentedCode ||
            result.structure.kind == FlarkV3DocumentStructureKind.blockQuote ||
            result.structure.kind == FlarkV3DocumentStructureKind.bulletList ||
            result.structure.kind ==
                FlarkV3DocumentStructureKind.orderedList) &&
        result.sourceRevision == presentation.sourceVersion.revision &&
        result.structureRevision == presentation.sourceVersion.revision) {
      _inlineQueryCache = _FlarkV3ManagedInlineQueryCache(
        authority: authority,
        uiRevision: document.uiRevision,
        inlinePresentationGeneration: presentationGeneration,
        inlineAttemptOutcomeGeneration: outcomeGeneration,
        inlineDemandReady: demandReady,
        query: result,
      );
    } else {
      _inlineQueryCache = null;
    }
    return result;
  }
}

final class _FlarkV3ManagedRefreshKey {
  const _FlarkV3ManagedRefreshKey({
    required this.presentation,
    required this.exactCurrent,
    required this.uiRevision,
    required this.selection,
    required this.composing,
    required this.islandStartUtf16,
    required this.islandEndUtf16,
    required this.inlinePresentationGeneration,
    required this.inlineAttemptOutcomeGeneration,
    required this.inlineDemandReady,
  });

  final _FlarkV3ManagedPresentationKey presentation;
  final bool exactCurrent;
  final int uiRevision;
  final TextSelection selection;
  final TextRange composing;
  final int islandStartUtf16;
  final int islandEndUtf16;
  final int inlinePresentationGeneration;
  final int inlineAttemptOutcomeGeneration;
  final bool inlineDemandReady;

  @override
  bool operator ==(Object other) =>
      other is _FlarkV3ManagedRefreshKey &&
      other.presentation == presentation &&
      other.exactCurrent == exactCurrent &&
      other.uiRevision == uiRevision &&
      other.selection == selection &&
      other.composing == composing &&
      other.islandStartUtf16 == islandStartUtf16 &&
      other.islandEndUtf16 == islandEndUtf16 &&
      other.inlinePresentationGeneration == inlinePresentationGeneration &&
      other.inlineAttemptOutcomeGeneration == inlineAttemptOutcomeGeneration &&
      other.inlineDemandReady == inlineDemandReady;

  @override
  int get hashCode => Object.hash(
    presentation,
    exactCurrent,
    uiRevision,
    selection,
    composing,
    islandStartUtf16,
    islandEndUtf16,
    inlinePresentationGeneration,
    inlineAttemptOutcomeGeneration,
    inlineDemandReady,
  );
}

final class _FlarkV3ManagedInlineQueryCache {
  const _FlarkV3ManagedInlineQueryCache({
    required this.authority,
    required this.uiRevision,
    required this.inlinePresentationGeneration,
    required this.inlineAttemptOutcomeGeneration,
    required this.inlineDemandReady,
    required this.query,
  });

  final _FlarkV3ManagedPresentationKey authority;
  final int uiRevision;
  final int inlinePresentationGeneration;
  final int inlineAttemptOutcomeGeneration;
  final bool inlineDemandReady;
  final FlarkV3DocumentStructuralQuery query;

  bool containsStrictly(int positionUtf16) {
    final leaf = switch (query.structure.kind) {
      FlarkV3DocumentStructureKind.bulletList =>
        query.bulletListProjection?.selectedItem.physicalSource,
      FlarkV3DocumentStructureKind.orderedList =>
        query.orderedListProjection?.selectedItem.physicalSource,
      _ => query.structure.source,
    };
    // A list payload selects one independently editable item. Until that
    // payload exists there is no safe leaf-local cache scope, and once it does
    // exist selection movement into another item must issue a fresh query.
    if (leaf == null) return false;
    return positionUtf16 > leaf.startUtf16 && positionUtf16 < leaf.endUtf16;
  }
}

/// Stable authority fingerprint for a presentation getter that intentionally
/// returns a fresh wrapper on every read.
///
/// Point queries only need to run again when exact authority changes, not when
/// Flutter applies a frame for an already-adopted inline presentation.
final class _FlarkV3ManagedPresentationKey {
  const _FlarkV3ManagedPresentationKey({
    required this.uiSource,
    required this.sourceVersion,
    required this.exactAck,
  });

  factory _FlarkV3ManagedPresentationKey.from(
    FlarkV3HostPresentationState presentation,
  ) => _FlarkV3ManagedPresentationKey(
    uiSource: presentation.uiSource,
    sourceVersion: presentation.sourceVersion,
    exactAck: switch (presentation) {
      FlarkV3ExactStructuralPresentation(:final ack) => ack,
      FlarkV3StablePendingPresentation() => null,
    },
  );

  final FlarkV3UiSourceIdentity uiSource;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck? exactAck;

  @override
  bool operator ==(Object other) =>
      other is _FlarkV3ManagedPresentationKey &&
      other.uiSource == uiSource &&
      other.sourceVersion == sourceVersion &&
      other.exactAck == exactAck;

  @override
  int get hashCode => Object.hash(uiSource, sourceVersion, exactAck);
}

void _refreshInlineIsland({
  required FlarkDocumentSession document,
  required FlarkV3FlutterLiveController controller,
  required FlarkV3HostQueryBudget queryBudget,
  required bool Function() isExactCurrent,
  required FlarkV3ManagedDocumentPointQuery queryAtUtf16,
  required FlarkV3ManagedActiveProjectionDemand ensureActiveProjectionAtUtf16,
  required bool Function() shouldRequestInline,
}) {
  if (!isExactCurrent()) {
    controller.clearManagedDocumentQuery();
    controller.markInlinePresentationProvisional();
    return;
  }

  final source = document.source;
  try {
    final selection = controller.globalEditingState.selection;
    final position = selection.extentOffset;
    final result = queryAtUtf16(
      position,
      affinity: switch (selection.affinity) {
        TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
        TextAffinity.downstream => FlarkV3DocumentQueryAffinity.downstream,
      },
      budget: FlarkV3DocumentQueryBudget(
        maximumEncodedBytes: queryBudget.maxEncodedBytes,
        maximumOpenDepth: queryBudget.maxOpenDepth,
        maximumLeafCount: queryBudget.maxLeafCount,
        maximumTreeNodesVisited: queryBudget.maxTreeNodesVisited,
      ),
    );
    controller.adoptManagedDocumentQuery(result);
    switch (result) {
      case final FlarkV3RecursiveGreenPointQuery recursiveQuery
          when recursiveQuery.owner.kind?.isInlineBearing ?? false:
        FlarkV3LeafProjectionDemandDisposition? demand;
        if (recursiveQuery.inlineFacts == null && shouldRequestInline()) {
          demand = ensureActiveProjectionAtUtf16(
            position,
            affinity: switch (selection.affinity) {
              TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
              TextAffinity.downstream =>
                FlarkV3DocumentQueryAffinity.downstream,
            },
            query: recursiveQuery,
          );
        }
        final terminalWithoutFacts = switch (demand) {
          FlarkV3LeafProjectionDemandDisposition.notApplicable ||
          FlarkV3LeafProjectionDemandDisposition.retryLimitReached => true,
          _ => false,
        };
        final paragraphSource = recursiveQuery.paragraphSource;
        final activeIsland = recursiveQuery.inlineSource;
        if (paragraphSource == null || activeIsland == null) {
          if (recursiveQuery.inlineFacts != null) {
            throw const FormatException(
              'Recursive Paragraph facts lack parser-authored geometry.',
            );
          }
          if (terminalWithoutFacts) {
            controller.adoptLiteralSourcePaint();
          } else {
            controller.markInlinePresentationProvisional();
          }
          return;
        }
        if (activeIsland.startUtf8 < paragraphSource.startUtf8 ||
            activeIsland.endUtf8 > paragraphSource.endUtf8 ||
            activeIsland.startUtf16 < paragraphSource.startUtf16 ||
            activeIsland.endUtf16 > paragraphSource.endUtf16 ||
            !_sourceSpanContainsEditingState(
              activeIsland,
              controller.globalEditingState,
            )) {
          throw const FormatException(
            'Recursive Paragraph geometry does not authorize the active edit.',
          );
        }
        _handoffWithinParserAuthorizedRangeIfNeeded(
          controller,
          activeIsland,
          controller.globalEditingState,
        );
        if (activeIsland.endUtf16 - activeIsland.startUtf16 >
            controller.maximumInputIslandUtf16) {
          controller.adoptLiteralSourcePaint();
          return;
        }
        if (recursiveQuery.inlineFacts == null) {
          if (terminalWithoutFacts) {
            controller.adoptLiteralSourcePaint();
          } else {
            controller.markInlinePresentationProvisional();
          }
          return;
        }
        controller.adoptInlineIslandPresentation(
          FlarkV3InlineIslandPresentation.resolveRecursiveGreenInlineLeaf(
            sourceDocument: source,
            expectedSource: document.sourceVersion,
            recursiveQuery: recursiveQuery,
          ),
        );
      case FlarkV3RecursiveGreenPointQuery(:final source):
        _handoffToExactRangeIfNeeded(
          controller,
          source,
          controller.globalEditingState,
        );
        controller.adoptLiteralSourcePaint();
      case FlarkV3DocumentStructuralQuery(:final structure, :final projection)
          when structure.canCarryInlineFacts:
        final activeIsland = structure.inlineContentSource!;
        if (!_sameSourceSpan(activeIsland, projection.projectedSource)) {
          throw const FormatException(
            'Inline-bearing structure and projection authority disagree.',
          );
        }
        final activeEditingState = _editingStateWithExtentInside(
          activeIsland,
          controller.globalEditingState,
        );
        _handoffWithinParserAuthorizedRangeIfNeeded(
          controller,
          activeIsland,
          activeEditingState,
        );
        if (activeIsland.endUtf16 - activeIsland.startUtf16 >
            controller.maximumInputIslandUtf16) {
          // Whole-leaf inline facts cannot safely authorize a projected shard.
          // Keep the selected neighborhood bounded and source-visible until a
          // parser-authored windowed inline-facts contract exists.
          controller.adoptLiteralSourcePaint();
          return;
        }
        FlarkV3LeafProjectionDemandDisposition? demand;
        if (result.inlineFacts == null && shouldRequestInline()) {
          demand = ensureActiveProjectionAtUtf16(
            position,
            affinity: switch (selection.affinity) {
              TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
              TextAffinity.downstream =>
                FlarkV3DocumentQueryAffinity.downstream,
            },
            query: result,
          );
        }
        if (result.inlineFacts == null) {
          final terminalWithoutFacts = switch (demand) {
            FlarkV3LeafProjectionDemandDisposition.notApplicable ||
            FlarkV3LeafProjectionDemandDisposition.retryLimitReached => true,
            _ => false,
          };
          if (terminalWithoutFacts) {
            controller.adoptLiteralSourcePaint();
          } else {
            // An absent sidecar is normally an in-flight authority state, not
            // a parser decision to expose source markers. Preserve an existing
            // mechanically exact projection while demand settles.
            controller.markInlinePresentationProvisional();
          }
          return;
        }
        controller.adoptInlineIslandPresentation(
          FlarkV3InlineIslandPresentation.resolve(
            sourceDocument: source,
            expectedSource: document.sourceVersion,
            structuralQuery: result,
            activeIsland: activeIsland,
          ),
        );
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.fencedCode,
          source: final fenceSource,
          :final visibleSource,
          :final fencedCode,
        ),
        :final projection,
      ):
        if (fencedCode == null ||
            !_sameSourceSpan(fencedCode.bodySource, visibleSource) ||
            !_sameSourceSpan(
              fencedCode.bodySource,
              projection.projectedSource,
            )) {
          throw const FormatException(
            'Fenced-code body and projection authority disagree.',
          );
        }
        final globalEditingState = controller.globalEditingState;
        final parserAuthorizedIsland =
            _sourceSpanContainsEditingState(
              fencedCode.bodySource,
              globalEditingState,
            )
            ? fencedCode.bodySource
            : fenceSource;
        _handoffWithinParserAuthorizedRangeIfNeeded(
          controller,
          parserAuthorizedIsland,
          globalEditingState,
        );
        // The parser projection excludes the opener, info string, and closer.
        // Code contents intentionally remain literal, so Markdown-looking
        // bytes in the body never enter the inline projection path.
        controller.adoptLiteralSourcePaint();
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.indentedCode,
          source: final blockSource,
          :final visibleSource,
          :final indentedCode,
        ),
        :final projection,
        :final indentedCodeProjection,
      ):
        if (indentedCode == null ||
            !_sameSourceSpan(blockSource, projection.source) ||
            !_sameSourceSpan(visibleSource, projection.projectedSource) ||
            projection.runCount != indentedCode.lineCount) {
          throw const FormatException(
            'Indented-code structure and projection authority disagree.',
          );
        }
        final activeEditingState = _editingStateWithExtentInside(
          blockSource,
          controller.globalEditingState,
        );
        _handoffWithinParserAuthorizedRangeIfNeeded(
          controller,
          blockSource,
          activeEditingState,
        );
        if (blockSource.endUtf16 - blockSource.startUtf16 >
            controller.maximumInputIslandUtf16) {
          controller.adoptLiteralSourcePaint();
          return;
        }

        FlarkV3LeafProjectionDemandDisposition? demand;
        if (indentedCodeProjection == null && shouldRequestInline()) {
          demand = ensureActiveProjectionAtUtf16(
            position,
            affinity: switch (selection.affinity) {
              TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
              TextAffinity.downstream =>
                FlarkV3DocumentQueryAffinity.downstream,
            },
            query: result,
          );
        }
        if (indentedCodeProjection == null) {
          final terminalWithoutProjection = switch (demand) {
            FlarkV3LeafProjectionDemandDisposition.notApplicable ||
            FlarkV3LeafProjectionDemandDisposition.retryLimitReached => true,
            _ => false,
          };
          if (terminalWithoutProjection) {
            controller.adoptLiteralSourcePaint();
          } else {
            controller.markProjectedInputLeaseProvisional();
          }
          return;
        }

        final sourceProjection = indentedCodeProjection.toSourceProjection(
          maximumSourceUtf16: controller.maximumInputIslandUtf16,
          maximumDisplayUtf16: controller.maximumInputIslandUtf16,
        );
        if (indentedCodeProjection.sourceVersion != document.sourceVersion ||
            sourceProjection.sourceStartUtf16 != blockSource.startUtf16 ||
            sourceProjection.sourceEndUtf16 != blockSource.endUtf16 ||
            sourceProjection.displayLengthUtf16 !=
                indentedCode.projectedUtf16Length) {
          throw const FormatException(
            'Indented-code payload does not cover its exact structural block.',
          );
        }
        controller.adoptProjectedInputLease(
          FlarkV3ProjectedInputLease.fromSourceProjection(
            sourceProjection,
            editPolicy: FlarkV3IndentedCodeEditPolicy(),
          ),
        );
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.blockQuote,
          source: final blockSource,
          :final visibleSource,
          :final blockQuote,
        ),
        :final projection,
        :final pointPath,
        :final blockQuoteProjection,
      ):
        if (blockQuote == null ||
            !_sameSourceSpan(blockSource, projection.source) ||
            !_sameSourceSpan(visibleSource, projection.projectedSource) ||
            projection.runCount != blockQuote.lineCount) {
          throw const FormatException(
            'Block-quote structure and projection authority disagree.',
          );
        }
        final activeEditingState = _editingStateWithExtentInside(
          blockSource,
          controller.globalEditingState,
        );
        _handoffWithinParserAuthorizedRangeIfNeeded(
          controller,
          blockSource,
          activeEditingState,
        );
        if (blockSource.endUtf16 - blockSource.startUtf16 >
            controller.maximumInputIslandUtf16) {
          controller.adoptLiteralSourcePaint();
          return;
        }

        FlarkV3LeafProjectionDemandDisposition? demand;
        if (blockQuoteProjection == null && shouldRequestInline()) {
          demand = ensureActiveProjectionAtUtf16(
            position,
            affinity: switch (selection.affinity) {
              TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
              TextAffinity.downstream =>
                FlarkV3DocumentQueryAffinity.downstream,
            },
            query: result,
          );
        }
        if (blockQuoteProjection == null) {
          if (pointPath != null) {
            throw const FormatException(
              'Block-quote path exists without its projection payload.',
            );
          }
          final terminalWithoutProjection = switch (demand) {
            FlarkV3LeafProjectionDemandDisposition.notApplicable ||
            FlarkV3LeafProjectionDemandDisposition.retryLimitReached => true,
            _ => false,
          };
          if (terminalWithoutProjection) {
            controller.adoptLiteralSourcePaint();
          } else {
            controller.markProjectedInputLeaseProvisional();
          }
          return;
        }

        final sourceProjection = blockQuoteProjection.toSourceProjection(
          maximumSourceUtf16: controller.maximumInputIslandUtf16,
          maximumDisplayUtf16: controller.maximumInputIslandUtf16,
        );
        if (pointPath == null ||
            blockQuoteProjection.sourceVersion != document.sourceVersion ||
            !_sameSourceSpan(blockQuoteProjection.source, blockSource) ||
            sourceProjection.sourceStartUtf16 != blockSource.startUtf16 ||
            sourceProjection.sourceEndUtf16 != blockSource.endUtf16 ||
            sourceProjection.displayLengthUtf16 !=
                blockQuote.projectedUtf16Length ||
            blockQuoteProjection.records.length != blockQuote.lineCount) {
          throw const FormatException(
            'Block-quote payload does not cover its exact structural path.',
          );
        }
        // The quote projection supplies exact marker hiding and block-level
        // presentation. Inline styles inside its Paragraph remain a separate
        // parser-certified layer and are intentionally not inferred here.
        controller.adoptProjectedInputLease(
          FlarkV3ProjectedInputLease.fromSourceProjection(
            sourceProjection,
            editPolicy: FlarkV3BlockQuoteEditPolicy(),
          ),
        );
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.bulletList,
          source: final listSource,
          :final visibleSource,
          :final bulletList,
        ),
        :final projection,
        :final inlineFacts,
        :final pointPath,
        :final bulletListProjection,
      ):
        if (bulletList == null ||
            !bulletList.tight ||
            !_sameSourceSpan(listSource, projection.source) ||
            !_sameSourceSpan(visibleSource, projection.projectedSource) ||
            projection.runCount != bulletList.itemCount) {
          throw const FormatException(
            'Bullet-list structure and projection authority disagree.',
          );
        }
        _adoptTightListItem(
          document: document,
          controller: controller,
          result: result,
          position: position,
          selection: selection,
          listSource: listSource,
          itemCount: bulletList.itemCount,
          pointPath: pointPath,
          payload: bulletListProjection,
          inlineFacts: inlineFacts,
          markerPresentation: (_) =>
              const FlarkV3ListItemMarkerPresentation.bullet(),
          ensureActiveProjectionAtUtf16: ensureActiveProjectionAtUtf16,
          shouldRequestInline: shouldRequestInline,
          label: 'Bullet list',
        );
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.orderedList,
          source: final listSource,
          :final visibleSource,
          :final orderedList,
        ),
        :final projection,
        :final inlineFacts,
        :final pointPath,
        :final orderedListProjection,
      ):
        if (orderedList == null ||
            !orderedList.tight ||
            !_sameSourceSpan(listSource, projection.source) ||
            !_sameSourceSpan(visibleSource, projection.projectedSource) ||
            projection.runCount != orderedList.itemCount) {
          throw const FormatException(
            'Ordered-list structure and projection authority disagree.',
          );
        }
        _adoptTightListItem(
          document: document,
          controller: controller,
          result: result,
          position: position,
          selection: selection,
          listSource: listSource,
          itemCount: orderedList.itemCount,
          pointPath: pointPath,
          payload: orderedListProjection,
          inlineFacts: inlineFacts,
          markerPresentation: (payload) =>
              FlarkV3ListItemMarkerPresentation.parserText(
                parserText: (payload as FlarkV3OrderedListProjectionPayload)
                    .selectedMarkerText,
              ),
          ensureActiveProjectionAtUtf16: ensureActiveProjectionAtUtf16,
          shouldRequestInline: shouldRequestInline,
          label: 'Ordered list',
        );
      case FlarkV3DocumentStructuralQuery(
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.thematicBreak,
          source: final atomicSource,
          :final visibleSource,
          thematicBreak: final thematicBreak,
        ),
        :final projection,
      ):
        if (thematicBreak == null ||
            visibleSource.startUtf16 != visibleSource.endUtf16 ||
            visibleSource.startUtf16 != atomicSource.startUtf16 ||
            !_sameSourceSpan(visibleSource, projection.projectedSource) ||
            projection.runCount != 0) {
          throw const FormatException(
            'Thematic-break structure and atomic projection disagree.',
          );
        }
        final activeEditingState = _editingStateAtThematicBreakBoundary(
          atomicSource,
          controller.globalEditingState,
        );
        final activeIsland = _collapsedSourceSpanAt(
          source,
          activeEditingState.selection.extentOffset,
        );
        _handoffToExactRangeIfNeeded(
          controller,
          activeIsland,
          activeEditingState,
        );
        // The canonical marker line remains outside EditableText. Flutter
        // paints the parser-certified atom from the adopted document query.
        controller.adoptLiteralSourcePaint();
      case FlarkV3DocumentStructuralQuery(:final structure, :final projection):
        final inlineContent = structure.inlineContentSource;
        final activeIsland =
            inlineContent ??
            (projection.projectedSource.startUtf16 ==
                    projection.projectedSource.endUtf16
                ? _collapsedSourceSpanAt(source, position)
                : structure.source);
        if (inlineContent != null &&
            !_sameSourceSpan(inlineContent, projection.projectedSource)) {
          throw const FormatException(
            'Inline-bearing structure and projection authority disagree.',
          );
        }
        _handoffToExactRangeIfNeeded(
          controller,
          activeIsland,
          controller.globalEditingState,
        );
        controller.adoptLiteralSourcePaint();
      case FlarkV3DocumentSourceGapQuery(:final range):
        _handoffToExactRangeIfNeeded(
          controller,
          range,
          controller.globalEditingState,
        );
        controller.adoptLiteralSourcePaint();
      case FlarkV3DocumentPendingQuery():
        controller.markInlinePresentationProvisional();
    }
  } on FlarkV3DocumentQueryException {
    controller.clearManagedDocumentQuery();
    controller.adoptLiteralSourcePaint();
  } on RangeError {
    controller.clearManagedDocumentQuery();
    controller.adoptLiteralSourcePaint();
  } on StateError {
    controller.clearManagedDocumentQuery();
    controller.adoptLiteralSourcePaint();
  } on FormatException {
    controller.clearManagedDocumentQuery();
    controller.adoptLiteralSourcePaint();
  }
}

void _adoptTightListItem({
  required FlarkDocumentSession document,
  required FlarkV3FlutterLiveController controller,
  required FlarkV3DocumentStructuralQuery result,
  required int position,
  required TextSelection selection,
  required FlarkV3SourceSpan listSource,
  required int itemCount,
  required FlarkV3DocumentPointPath? pointPath,
  required FlarkV3TightListItemProjectionPayload? payload,
  required FlarkV3InlineFacts? inlineFacts,
  required FlarkV3ListItemMarkerPresentation Function(
    FlarkV3TightListItemProjectionPayload payload,
  )
  markerPresentation,
  required FlarkV3ManagedActiveProjectionDemand ensureActiveProjectionAtUtf16,
  required bool Function() shouldRequestInline,
  required String label,
}) {
  final affinity = switch (selection.affinity) {
    TextAffinity.upstream => FlarkV3DocumentQueryAffinity.upstream,
    TextAffinity.downstream => FlarkV3DocumentQueryAffinity.downstream,
  };
  FlarkV3LeafProjectionDemandDisposition? demand;
  if (payload == null && shouldRequestInline()) {
    demand = ensureActiveProjectionAtUtf16(
      position,
      affinity: affinity,
      query: result,
    );
  }
  if (payload == null) {
    if (pointPath != null) {
      throw FormatException(
        '$label path exists without its projection payload.',
      );
    }
    final activeEditingState = _editingStateWithExtentInside(
      listSource,
      controller.globalEditingState,
    );
    _handoffWithinParserAuthorizedRangeIfNeeded(
      controller,
      listSource,
      activeEditingState,
    );
    final terminalWithoutProjection = switch (demand) {
      FlarkV3LeafProjectionDemandDisposition.notApplicable ||
      FlarkV3LeafProjectionDemandDisposition.retryLimitReached => true,
      _ => false,
    };
    if (terminalWithoutProjection || inlineFacts != null) {
      controller.adoptLiteralSourcePaint();
    } else {
      controller.markProjectedInputLeaseProvisional();
    }
    return;
  }

  final selectedItem = payload.selectedItem;
  final selectedSource = selectedItem.physicalSource;
  final activeEditingState = _editingStateWithExtentInside(
    selectedSource,
    controller.globalEditingState,
  );
  _handoffToExactRangeIfNeeded(controller, selectedSource, activeEditingState);
  final sourceProjection = payload.toSelectedItemSourceProjection(
    maximumSourceUtf16: controller.maximumInputIslandUtf16,
    maximumDisplayUtf16: controller.maximumInputIslandUtf16,
  );
  if (pointPath == null ||
      payload.sourceVersion != document.sourceVersion ||
      !_sameSourceSpan(payload.source, listSource) ||
      !_sameSourceSpan(payload.pointPath.root.source, listSource) ||
      payload.pointPath != pointPath ||
      (payload.coversWholeList
          ? payload.records.length != itemCount
          : payload.records.length != 1) ||
      sourceProjection.sourceStartUtf16 != selectedSource.startUtf16 ||
      sourceProjection.sourceEndUtf16 != selectedSource.endUtf16 ||
      sourceProjection.displayLengthUtf16 !=
          payload.selectedItemDisplayUtf16Length) {
    throw FormatException(
      '$label payload does not cover its exact selected item.',
    );
  }
  final configuration = _tightListItemConfiguration(
    payload.editingInputs,
    markerPresentation: markerPresentation(payload),
  );
  if (inlineFacts == null && !selectedItem.isEmpty && shouldRequestInline()) {
    ensureActiveProjectionAtUtf16(position, affinity: affinity, query: result);
  }
  var inputLease = FlarkV3ProjectedInputLease.fromSourceProjection(
    sourceProjection,
    editPolicy: FlarkV3TightListItemEditPolicy(configuration: configuration),
  );
  if (inlineFacts != null && !selectedItem.isEmpty) {
    final inlinePresentation =
        FlarkV3InlineIslandPresentation.resolveTightListItem(
          sourceDocument: document.source,
          expectedSource: document.sourceVersion,
          structuralQuery: result,
        );
    if (inlinePresentation
        case final FlarkV3AuthoritativeInlineIslandPresentation authoritative) {
      inputLease =
          FlarkV3ProjectedInputLease.fromSourceProjectionWithAuthoritativeInline(
            sourceProjection,
            authoritative,
            editPolicy: FlarkV3TightListItemEditPolicy(
              configuration: configuration,
            ),
          );
    }
  }
  controller.adoptTightListItemInputLease(
    inputLease,
    configuration: configuration,
  );
}

FlarkV3TightListItemConfiguration _tightListItemConfiguration(
  FlarkV3TightListItemEditingInputs editingInputs, {
  required FlarkV3ListItemMarkerPresentation markerPresentation,
}) {
  return FlarkV3TightListItemConfiguration(
    activeHiddenSourcePrefix: editingInputs.activeHiddenSourcePrefix,
    activeRemovableSourcePrefix: editingInputs.activeRemovableSourcePrefix,
    activeRemovableSourcePrefixOffsetUtf16:
        editingInputs.activeRemovableSourcePrefixOffsetUtf16,
    continuationSourcePrefix: editingInputs.continuationSourcePrefix,
    canonicalLineEnding: editingInputs.canonicalLineEnding,
    emptyEnterExits: editingInputs.emptyEnterExits,
    backspaceAtStartRemovesPrefix: editingInputs.backspaceAtStartRemovesPrefix,
    markerPresentation: markerPresentation,
  );
}

bool _sourceSpanContainsEditingState(
  FlarkV3SourceSpan span,
  FlarkV3GlobalEditingState state,
) => _utf16RangeContainsEditingState(span.startUtf16, span.endUtf16, state);

FlarkV3GlobalEditingState _editingStateWithExtentInside(
  FlarkV3SourceSpan span,
  FlarkV3GlobalEditingState state,
) {
  final extent = state.selection.extentOffset;
  if (extent >= span.startUtf16 && extent <= span.endUtf16) return state;
  final composing = state.composing;
  if (composing.isValid && !composing.isCollapsed) return state;

  final clampedExtent = extent < span.startUtf16
      ? span.startUtf16
      : span.endUtf16;
  final selection = state.selection;
  return FlarkV3GlobalEditingState(
    selection: selection.isCollapsed
        ? TextSelection.collapsed(
            offset: clampedExtent,
            affinity: selection.affinity,
          )
        : TextSelection(
            baseOffset: selection.baseOffset,
            extentOffset: clampedExtent,
            affinity: selection.affinity,
            isDirectional: selection.isDirectional,
          ),
    composing: composing.isValid
        ? TextRange.collapsed(clampedExtent)
        : TextRange.empty,
  );
}

bool _utf16RangeContainsEditingState(
  int startUtf16,
  int endUtf16,
  FlarkV3GlobalEditingState state,
) {
  final extent = state.selection.extentOffset;
  if (extent < startUtf16 || extent > endUtf16) return false;
  final composing = state.composing;
  return !composing.isValid ||
      (composing.start >= startUtf16 && composing.end <= endUtf16);
}

void _handoffWithinParserAuthorizedRangeIfNeeded(
  FlarkV3FlutterLiveController controller,
  FlarkV3SourceSpan range,
  FlarkV3GlobalEditingState globalEditingState,
) {
  final currentStart = controller.inputIslandGlobalStartUtf16;
  final currentEnd = controller.inputIslandGlobalEndUtf16;
  if (currentStart == range.startUtf16 && currentEnd == range.endUtf16) {
    return;
  }
  final rangeExceedsBound =
      range.endUtf16 - range.startUtf16 > controller.maximumInputIslandUtf16;
  final currentIslandRemainsAuthorized =
      rangeExceedsBound &&
      currentStart >= range.startUtf16 &&
      currentEnd <= range.endUtf16 &&
      _utf16RangeContainsEditingState(
        currentStart,
        currentEnd,
        globalEditingState,
      );
  if (currentIslandRemainsAuthorized) return;

  controller.handoffInputIslandWithinExactRange(
    startUtf16: range.startUtf16,
    endUtf16: range.endUtf16,
    nextGlobalEditingState: globalEditingState,
  );
}

bool _sameSourceSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

FlarkV3SourceSpan _collapsedSourceSpanAt(
  FlarkV3SourceDocument source,
  int positionUtf16,
) {
  final positionUtf8 = source.utf16ToUtf8(positionUtf16);
  return FlarkV3SourceSpan(
    startUtf8: positionUtf8,
    endUtf8: positionUtf8,
    startUtf16: positionUtf16,
    endUtf16: positionUtf16,
  );
}

FlarkV3GlobalEditingState _editingStateAtThematicBreakBoundary(
  FlarkV3SourceSpan atom,
  FlarkV3GlobalEditingState state,
) {
  final selection = state.selection;
  final positionUtf16 = selection.extentOffset;
  final (boundary, affinity) = positionUtf16 <= atom.startUtf16
      ? (atom.startUtf16, TextAffinity.downstream)
      : positionUtf16 >= atom.endUtf16
      ? (atom.endUtf16, TextAffinity.upstream)
      : selection.affinity == TextAffinity.upstream
      ? (atom.startUtf16, TextAffinity.downstream)
      : (atom.endUtf16, TextAffinity.upstream);
  final composing = state.composing;
  if (composing.isValid && !composing.isCollapsed) return state;
  return FlarkV3GlobalEditingState(
    selection: selection.isCollapsed
        ? TextSelection.collapsed(offset: boundary, affinity: affinity)
        : TextSelection(
            baseOffset: selection.baseOffset,
            extentOffset: boundary,
            affinity: affinity,
            isDirectional: selection.isDirectional,
          ),
    composing: composing.isValid
        ? TextRange.collapsed(boundary)
        : TextRange.empty,
  );
}

void _handoffToExactRangeIfNeeded(
  FlarkV3FlutterLiveController controller,
  FlarkV3SourceSpan range,
  FlarkV3GlobalEditingState globalEditingState,
) {
  if (range.startUtf16 == controller.inputIslandGlobalStartUtf16 &&
      range.endUtf16 == controller.inputIslandGlobalEndUtf16) {
    return;
  }
  controller.handoffInputIslandToExactRange(
    startUtf16: range.startUtf16,
    endUtf16: range.endUtf16,
    nextGlobalEditingState: globalEditingState,
  );
}
