import 'dart:async';
import 'dart:math';

import '../../host/host.dart';
import '../../session/session.dart';
import '../../source/source.dart';
import '../flark_v3_parser_transport.dart';
import '../flark_v3_session_driver.dart';
import '../flark_v3_session_executor.dart';
import '../flark_v3_wire_parser_transport.dart';
import 'flark_v3_current_revision_inline_cache.dart';
import 'flark_v3_platform_endpoint_factory.dart';
import 'flark_v3_platform_endpoint_handle.dart';
import 'flark_v3_platform_host_store_factory.dart';
import 'flark_v3_document_query.dart';
import 'flark_v3_inline_facts.dart';
import 'flark_v3_runtime_assets.dart';

/// Build-time support for the default parser endpoint on this Dart platform.
final class FlarkV3RuntimePlatformSupport {
  const FlarkV3RuntimePlatformSupport({
    required this.supported,
    required this.endpoint,
    this.unavailableReason,
  });

  final bool supported;
  final String endpoint;
  final String? unavailableReason;
}

/// Thrown when the selected Dart platform has no production v3 endpoint yet.
final class FlarkV3RuntimeUnavailable implements Exception {
  const FlarkV3RuntimeUnavailable({
    required this.endpoint,
    required this.reason,
  });

  final String endpoint;
  final String reason;

  @override
  String toString() => 'FlarkV3RuntimeUnavailable($endpoint: $reason)';
}

/// Reported by [FlarkV3DocumentRuntime.initialReady] when ownership ends before
/// exact structure for the opening source becomes queryable.
final class FlarkV3RuntimeClosedBeforeReady implements Exception {
  const FlarkV3RuntimeClosedBeforeReady();

  @override
  String toString() =>
      'FlarkV3RuntimeClosedBeforeReady(the runtime closed before its initial '
      'structure became ready)';
}

/// A recoverable in-protocol parser failure prevented initial structure.
final class FlarkV3RuntimeParserFailure implements Exception {
  const FlarkV3RuntimeParserFailure._();

  /// In-protocol parser failures retain exact source and can be recovered.
  bool get recoveryAvailable => true;

  @override
  String toString() => 'FlarkV3RuntimeParserFailure(recoveryAvailable: true)';
}

enum FlarkV3DocumentRuntimeState { opening, open, faulted, closing, closed }

/// Outcome of one bounded parser-authored leaf-projection demand.
///
/// A demand may target either whole-leaf inline facts or the exact physical
/// line recipe for an indented-code block. The parser remains authoritative
/// for choosing and producing the typed payload.
enum FlarkV3LeafProjectionDemandDisposition {
  scheduled,
  coalesced,
  notReady,
  stale,
  notApplicable,
  retryLimitReached,
}

/// Compatibility outcome for the original inline-only adapter method.
///
/// This unstable adapter receipt keeps scheduling state explicit without
/// exposing parser commands, generations, or publication protocol details.
enum FlarkV3InlineDemandDisposition {
  scheduled,
  coalesced,
  notReady,
  stale,
  notApplicable,
  retryLimitReached,
}

/// Outcome of one bounded passive viewport-presentation demand.
///
/// The adapter supplies exact source and structural identities. The runtime
/// owns parser generations, coalescing, and the focused-inline priority lane.
enum FlarkV3ViewportPresentationDemandDisposition {
  scheduled,
  coalesced,
  current,
  notReady,
  stale,
  unsupported,
  unavailable,
  retryLimitReached,
}

/// Exact passive source window requested by one presentation adapter.
///
/// [startBlockOrdinal] is the ordinal of the first top-level structural block
/// at [startUtf16]. The parser authenticates that cut against the exact
/// structural base; Dart never repairs or derives an ordinal mismatch.
final class FlarkV3ViewportPresentationDemand {
  FlarkV3ViewportPresentationDemand({
    required this.sourceRevision,
    required this.structureGeneration,
    required this.startUtf16,
    required this.endUtf16,
    required this.startBlockOrdinal,
  }) {
    if (sourceRevision < 0 ||
        structureGeneration < 0 ||
        startUtf16 < 0 ||
        endUtf16 <= startUtf16 ||
        startBlockOrdinal < 0 ||
        startBlockOrdinal > 0xffffffff) {
      throw RangeError(
        'A viewport demand requires one non-empty exact source range and a '
        '32-bit top-level block ordinal.',
      );
    }
  }

  final int sourceRevision;
  final int structureGeneration;
  final int startUtf16;
  final int endUtf16;
  final int startBlockOrdinal;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationDemand &&
      other.sourceRevision == sourceRevision &&
      other.structureGeneration == structureGeneration &&
      other.startUtf16 == startUtf16 &&
      other.endUtf16 == endUtf16 &&
      other.startBlockOrdinal == startBlockOrdinal;

  @override
  int get hashCode => Object.hash(
    sourceRevision,
    structureGeneration,
    startUtf16,
    endUtf16,
    startBlockOrdinal,
  );
}

/// Exact structural authority and top-level ordinal requested by an adapter.
///
/// The host returns parser-authenticated source cuts only. No Markdown text,
/// block payload, or Dart-side prefix scan participates in this lookup.
final class FlarkV3DocumentOrdinalWindowDemand {
  FlarkV3DocumentOrdinalWindowDemand({
    required this.sourceRevision,
    required this.structureGeneration,
    required this.startBlockOrdinal,
  }) {
    if (sourceRevision < 0 ||
        structureGeneration < 0 ||
        startBlockOrdinal < 0 ||
        startBlockOrdinal > 0xffffffff) {
      throw RangeError(
        'An ordinal-window demand requires non-negative exact authority and '
        'a 32-bit top-level block ordinal.',
      );
    }
  }

  final int sourceRevision;
  final int structureGeneration;
  final int startBlockOrdinal;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3DocumentOrdinalWindowDemand &&
      other.sourceRevision == sourceRevision &&
      other.structureGeneration == structureGeneration &&
      other.startBlockOrdinal == startBlockOrdinal;

  @override
  int get hashCode =>
      Object.hash(sourceRevision, structureGeneration, startBlockOrdinal);
}

/// Bounded foreground work for one ordinal-to-source window lookup.
///
/// The default admits a useful viewport quantum while remaining independent
/// of total document size. [maximumPackedEntriesInspected] covers the native
/// preflight needed to authenticate both boundary cuts.
final class FlarkV3DocumentOrdinalWindowBudget {
  const FlarkV3DocumentOrdinalWindowBudget({
    this.maximumEntries = 96,
    this.maximumStoragePagesVisited = 8,
    this.maximumTreeNodesVisited = 128,
    this.maximumPackedEntriesInspected = 1024,
  }) : assert(maximumEntries > 0),
       assert(
         maximumEntries <=
             FlarkV3HostStructuralOrdinalWindowBudget.maximumWindowEntries,
       ),
       assert(maximumStoragePagesVisited > 0),
       assert(maximumTreeNodesVisited > 0),
       assert(maximumPackedEntriesInspected > 0);

  final int maximumEntries;
  final int maximumStoragePagesVisited;
  final int maximumTreeNodesVisited;
  final int maximumPackedEntriesInspected;
}

/// Why an exact structural ordinal window could not be returned.
enum FlarkV3DocumentOrdinalWindowFailureReason {
  runtimeNotReady,
  sourceChanged,
  structureChanged,
  unavailable,
  entryLimit,
  storagePageLimit,
  treeNodeLimit,
  packedEntryLimit,
  ordinalOutOfRange,
  undecodable,
  hostRejected,
}

sealed class FlarkV3DocumentOrdinalWindowResult {
  const FlarkV3DocumentOrdinalWindowResult({required this.demand});

  final FlarkV3DocumentOrdinalWindowDemand demand;
}

/// Exact source cuts for consecutive top-level block ordinals.
final class FlarkV3ExactDocumentOrdinalWindow
    extends FlarkV3DocumentOrdinalWindowResult {
  const FlarkV3ExactDocumentOrdinalWindow({
    required super.demand,
    required this.structureRevision,
    required this.structureGeneration,
    required this.totalBlockCount,
    required this.startBlockOrdinal,
    required this.nextBlockOrdinal,
    required this.coveredSource,
    required this.complete,
    required this.storagePagesVisited,
    required this.treeNodesVisited,
    required this.packedEntriesInspected,
    required this.summaryNodesSkipped,
  });

  final int structureRevision;
  final int structureGeneration;
  final int totalBlockCount;
  final int startBlockOrdinal;
  final int nextBlockOrdinal;
  final FlarkV3SourceSpan coveredSource;
  final bool complete;
  final int storagePagesVisited;
  final int treeNodesVisited;
  final int packedEntriesInspected;
  final int summaryNodesSkipped;
}

/// Fail-closed locator outcome, including truthful bounded-work receipts.
final class FlarkV3UnavailableDocumentOrdinalWindow
    extends FlarkV3DocumentOrdinalWindowResult {
  const FlarkV3UnavailableDocumentOrdinalWindow({
    required super.demand,
    required this.reason,
    required this.totalBlockCount,
    required this.storagePagesVisited,
    required this.treeNodesVisited,
    required this.packedEntriesInspected,
    required this.summaryNodesSkipped,
  });

  final FlarkV3DocumentOrdinalWindowFailureReason reason;

  /// Authenticated total when the host could determine it.
  ///
  /// Null is deliberately distinct from the empty document.
  final int? totalBlockCount;
  final int storagePagesVisited;
  final int treeNodesVisited;
  final int packedEntriesInspected;
  final int summaryNodesSkipped;
}

/// Why an exact passive viewport page is not currently queryable.
enum FlarkV3ViewportPresentationUnavailableReason {
  runtimeNotReady,
  sourceChanged,
  structureChanged,
  unsupported,
  notInstalled,
  retryableBusy,
  budgetExceeded,
  derivationUnavailable,
  hostRejected,
  queryBoundExceeded,
  queryUnavailable,
}

/// Synchronous scheduling receipt for one passive viewport demand.
final class FlarkV3ViewportPresentationDemandReceipt {
  const FlarkV3ViewportPresentationDemandReceipt._({
    required this.disposition,
    required this.viewportGeneration,
    required this.attemptOutcomeGeneration,
    required this.unavailableReason,
  });

  final FlarkV3ViewportPresentationDemandDisposition disposition;

  /// Parser generation scheduled, coalesced, or already installed.
  ///
  /// Null means no parser attempt belongs to this receipt.
  final int? viewportGeneration;

  /// Monotonic completion edge observed while producing this receipt.
  ///
  /// Adapters observe the same value through
  /// [FlarkV3DocumentRuntimeStatus.viewportPresentationAttemptOutcomeGeneration]
  /// and therefore never poll for passive-page completion.
  final int attemptOutcomeGeneration;

  final FlarkV3ViewportPresentationUnavailableReason? unavailableReason;
}

sealed class FlarkV3ViewportPresentationPageResult {
  const FlarkV3ViewportPresentationPageResult({
    required this.demand,
    required this.attemptOutcomeGeneration,
  });

  final FlarkV3ViewportPresentationDemand demand;
  final int attemptOutcomeGeneration;
}

/// One exact installed schema-8 page and every authority needed to consume it.
///
/// This snapshot is deliberately the complete materializer join boundary:
/// consumers never reach into [FlarkDocumentSession] for source or structural
/// identities after querying the page.
final class FlarkV3ExactViewportPresentationPage
    extends FlarkV3ViewportPresentationPageResult {
  const FlarkV3ExactViewportPresentationPage({
    required super.demand,
    required super.attemptOutcomeGeneration,
    required this.page,
    required this.currentStructuralAck,
    required this.structureGeneration,
    required this.sourceDocument,
  });

  final FlarkV3ViewportPresentationAggregatePage page;
  final FlarkV3StructuralAck currentStructuralAck;
  final int structureGeneration;
  final FlarkV3SourceDocument sourceDocument;
}

/// Fail-closed passive-page query outcome.
final class FlarkV3UnavailableViewportPresentationPage
    extends FlarkV3ViewportPresentationPageResult {
  const FlarkV3UnavailableViewportPresentationPage({
    required super.demand,
    required super.attemptOutcomeGeneration,
    required this.reason,
  });

  final FlarkV3ViewportPresentationUnavailableReason reason;
}

/// Public result of one exact source edit or undo operation.
///
/// Parser batches, certification capabilities, and host adoption receipts are
/// intentionally kept behind the document facade.
final class FlarkV3DocumentEditResult {
  const FlarkV3DocumentEditResult._({
    required this.changed,
    required this.sourceRevision,
  });

  final bool changed;
  final int sourceRevision;
}

/// Small, read-consistent observation of one managed v3 document runtime.
///
/// This is deliberately semantic readiness state rather than a document-wide
/// AST or render plan. Call [FlarkV3DocumentRuntime.queryAtUtf16] for one
/// bounded, revision-stamped structural snapshot.
final class FlarkV3DocumentRuntimeStatus {
  const FlarkV3DocumentRuntimeStatus._({
    required this.state,
    required this.sourceRevision,
    required this.certifiedSourceRevision,
    required this.sourceCurrent,
    required this.structureRevision,
    required this.structureGeneration,
    required this.structureCurrent,
    required this.inlinePresentationGeneration,
    required this.inlineAttemptOutcomeGeneration,
    required this.viewportPresentationGeneration,
    required this.viewportPresentationAttemptOutcomeGeneration,
    required this.viewportPresentationUnavailableReason,
    required this.recoveryAvailable,
  });

  final FlarkV3DocumentRuntimeState state;

  /// Revision of the exact source currently owned by the caller-facing model.
  final int sourceRevision;

  /// Last source revision certified by the parser and adopted by the host.
  final int certifiedSourceRevision;

  /// Whether parser source and certification authority match [sourceRevision].
  final bool sourceCurrent;

  /// Source revision of the last atomically installed structural root.
  ///
  /// This may trail [sourceRevision]. A trailing root is paint-only and must
  /// not authorize selection mapping, semantics, hit targets, or Markdown
  /// edits.
  final int? structureRevision;

  /// Monotonic runtime-local identity of the installed structural authority.
  ///
  /// Unlike [structureRevision], this advances when recovery or republication
  /// installs a different authenticated tree for unchanged source. Consumers
  /// must fence semantic caches and range materialization with both values.
  final int structureGeneration;

  /// Whether the installed structural root is authoritative for current UI
  /// source rather than merely a stable paint candidate.
  final bool structureCurrent;

  /// Monotonic caller-side generation of parser-certified inline facts that
  /// became atomically queryable.
  ///
  /// This changes at host commit, not at worker delivery acknowledgement.
  /// Consumers can therefore re-run one bounded point query as soon as the
  /// active leaf's marker-free presentation is available.
  final int inlinePresentationGeneration;

  /// Generic name for [inlinePresentationGeneration].
  ///
  /// The current transport commits inline facts and indented-code line recipes
  /// through the same selected-leaf refinement lane.
  int get leafProjectionPresentationGeneration => inlinePresentationGeneration;

  /// Monotonic completion edge for late inline attempts.
  ///
  /// Unlike [inlinePresentationGeneration], this also advances when an
  /// attempted sidecar publication aborts. Presentation adapters use the edge
  /// to permit one bounded retry without polling or spinning.
  final int inlineAttemptOutcomeGeneration;

  /// Generic name for [inlineAttemptOutcomeGeneration].
  int get leafProjectionAttemptOutcomeGeneration =>
      inlineAttemptOutcomeGeneration;

  /// Installed parser generation for the exact passive viewport page.
  ///
  /// Zero means no aggregate page is installed for the current structural
  /// authority.
  final int viewportPresentationGeneration;

  /// Monotonic completion edge for passive viewport attempts.
  ///
  /// It advances for commit, explicit unavailability, and completed abort.
  /// Combined with [viewportPresentationGeneration], this gives adapters a
  /// non-polling pending-to-exact transition.
  final int viewportPresentationAttemptOutcomeGeneration;

  /// Typed terminal reason for the most recently completed unavailable
  /// viewport attempt, if any.
  final FlarkV3ViewportPresentationUnavailableReason?
  viewportPresentationUnavailableReason;

  /// Whether [FlarkV3DocumentRuntime.recover] may replace the failed parser
  /// generation while preserving exact source truth.
  final bool recoveryAvailable;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3DocumentRuntimeStatus &&
      other.state == state &&
      other.sourceRevision == sourceRevision &&
      other.certifiedSourceRevision == certifiedSourceRevision &&
      other.sourceCurrent == sourceCurrent &&
      other.structureRevision == structureRevision &&
      other.structureGeneration == structureGeneration &&
      other.structureCurrent == structureCurrent &&
      other.inlinePresentationGeneration == inlinePresentationGeneration &&
      other.inlineAttemptOutcomeGeneration == inlineAttemptOutcomeGeneration &&
      other.viewportPresentationGeneration == viewportPresentationGeneration &&
      other.viewportPresentationAttemptOutcomeGeneration ==
          viewportPresentationAttemptOutcomeGeneration &&
      other.viewportPresentationUnavailableReason ==
          viewportPresentationUnavailableReason &&
      other.recoveryAvailable == recoveryAvailable;

  @override
  int get hashCode => Object.hash(
    state,
    sourceRevision,
    certifiedSourceRevision,
    sourceCurrent,
    structureRevision,
    structureGeneration,
    structureCurrent,
    inlinePresentationGeneration,
    inlineAttemptOutcomeGeneration,
    viewportPresentationGeneration,
    viewportPresentationAttemptOutcomeGeneration,
    viewportPresentationUnavailableReason,
    recoveryAvailable,
  );

  bool _canSupersedePendingSourceStatus(
    FlarkV3DocumentRuntimeStatus previous,
  ) =>
      sourceRevision > previous.sourceRevision &&
      (!sourceCurrent || !structureCurrent) &&
      (!previous.sourceCurrent || !previous.structureCurrent) &&
      state == previous.state &&
      certifiedSourceRevision == previous.certifiedSourceRevision &&
      sourceCurrent == previous.sourceCurrent &&
      structureRevision == previous.structureRevision &&
      structureGeneration == previous.structureGeneration &&
      structureCurrent == previous.structureCurrent &&
      inlinePresentationGeneration == previous.inlinePresentationGeneration &&
      inlineAttemptOutcomeGeneration ==
          previous.inlineAttemptOutcomeGeneration &&
      viewportPresentationGeneration ==
          previous.viewportPresentationGeneration &&
      viewportPresentationAttemptOutcomeGeneration ==
          previous.viewportPresentationAttemptOutcomeGeneration &&
      viewportPresentationUnavailableReason ==
          previous.viewportPresentationUnavailableReason &&
      recoveryAvailable == previous.recoveryAvailable;
}

final class _FlarkV3PendingStatusDelivery {
  const _FlarkV3PendingStatusDelivery(this.status, {required this.barrier});

  final FlarkV3DocumentRuntimeStatus status;
  final bool barrier;
}

/// Dart-first owner of the v3 parser endpoint and session execution loop.
///
/// Applications mutate exact source through [apply] or [undo], observe bounded
/// status snapshots, and [close] the runtime. They never construct an isolate,
/// transfer FLK3 frames, acknowledge source certification, or manually pump a
/// driver. Flutter adapters consume this same class; no Flutter type or import
/// participates in the runtime.
///
final class FlarkV3DocumentRuntime {
  FlarkV3DocumentRuntime._({
    required FlarkDocumentSession document,
    required FlarkV3SessionExecutor executor,
    required Future<void> endpointDone,
  }) : _document = document,
       _executor = executor,
       _endpointDone = endpointDone {
    // A parser can fail before an application chooses to await startup. Own an
    // error handler from construction so completing the public future never
    // creates an unhandled asynchronous error. Additional callers still see
    // the original success or failure when they await [initialReady].
    _initialReady.future.ignore();
  }

  /// Whether the default production endpoint can be created on this platform.
  ///
  /// Native Dart uses one long-lived isolate owning FFI. Web uses one external
  /// Worker parser plus an independent main-context WebAssembly host.
  static FlarkV3RuntimePlatformSupport get platformSupport =>
      FlarkV3RuntimePlatformSupport(
        supported: flarkV3DefaultPlatformEndpointSupported,
        endpoint: flarkV3DefaultPlatformEndpointName,
        unavailableReason: flarkV3DefaultPlatformEndpointUnavailableReason,
      );

  /// Opens one managed exact-source Markdown document.
  ///
  /// Initial source indexing and grammar work are intentionally not performed
  /// synchronously here. The caller receives a provisional exact source root;
  /// the production parser endpoint certifies source facts and publishes the
  /// first structural root through bounded event-loop turns. Await
  /// [initialReady] when an exact-current structural query is required.
  static Future<FlarkV3DocumentRuntime> open(
    String markdown, {
    String? nativeLibraryPath,
    FlarkV3WebRuntimeAssets? webAssets,
  }) async {
    final support = platformSupport;
    if (!support.supported) {
      throw FlarkV3RuntimeUnavailable(
        endpoint: support.endpoint,
        reason: support.unavailableReason ?? 'No v3 endpoint is available.',
      );
    }

    final documentSession = _newDocumentSessionId();
    final sourceSession = FlarkV3SourceSession.fromProvisionalString(markdown);
    final hostStore = await createFlarkV3DefaultPlatformHostStore(
      documentSession: documentSession,
      nativeLibraryPath: nativeLibraryPath,
      webAssets: webAssets,
    );
    var ownershipTransferred = false;
    try {
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(documentSession),
      );
      final parserBinding = FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final platformEndpoint = await startFlarkV3DefaultPlatformEndpoint(
        nativeLibraryPath: nativeLibraryPath,
        webAssets: webAssets,
      );
      final runtime = await FlarkV3DocumentRuntimePlatformAttachment.attach(
        document: document,
        parserBinding: parserBinding,
        platformEndpoint: platformEndpoint,
      );
      ownershipTransferred = true;
      return runtime;
    } catch (error, stackTrace) {
      if (!ownershipTransferred) {
        _retireUnattachedHostStore(hostStore);
      }
      Error.throwWithStackTrace(error, stackTrace);
    }
  }

  final FlarkDocumentSession _document;
  final FlarkV3SessionExecutor _executor;
  final Future<void> _endpointDone;
  final StreamController<FlarkV3DocumentRuntimeStatus> _statuses =
      StreamController<FlarkV3DocumentRuntimeStatus>.broadcast();
  final List<_FlarkV3PendingStatusDelivery> _pendingStatusDeliveries = [];
  final Completer<void> _initialReady = Completer<void>.sync();
  final Object _blockRangeContinuationOwner = Object();
  FlarkV3StructuralAck? _observedStructuralAck;
  int _structureGeneration = 0;
  final FlarkV3CurrentRevisionInlineCache _inlineFactsCache =
      FlarkV3CurrentRevisionInlineCache(
        maximumEntries: _maximumCachedInlineLeaves,
        maximumFactRecords: _maximumCachedInlineFactRecords,
      );

  FlarkV3DocumentRuntimeStatus? _lastPublishedStatus;
  bool _statusDeliveryScheduled = false;
  Object? _lastLeafProjectionDemand;
  int _leafProjectionDemandAttempts = 0;
  int? _leafProjectionDemandLastRequestedOutcomeGeneration;
  Object? _leafProjectionDemandOwner;
  _FlarkV3ViewportPresentationDemandKey? _lastViewportPresentationDemand;
  int _viewportPresentationDemandAttempts = 0;
  int? _viewportPresentationDemandLastRequestedOutcomeGeneration;
  int? _viewportPresentationDemandGeneration;
  Object? _viewportPresentationDemandOwner;
  (Object, StackTrace)? _terminalFailure;
  bool _closeRequested = false;
  Future<void>? _closeFuture;

  /// Completes once exact structure for the opening source is queryable.
  ///
  /// This is a one-shot startup receipt. It stays complete across later edits;
  /// use [status]'s [FlarkV3DocumentRuntimeStatus.structureCurrent] value or
  /// [statuses] to observe later revisions. It completes with the causal parser
  /// failure, or [FlarkV3RuntimeClosedBeforeReady] if graceful close wins.
  Future<void> get initialReady => _initialReady.future;

  /// Bounded status changes. [status] is the synchronous current snapshot.
  ///
  /// Pending snapshots produced in one caller turn may coalesce when their
  /// only semantic difference is a newer source revision. Exact structure,
  /// inline, viewport, lifecycle, recovery, and terminal transitions retain
  /// their original order.
  Stream<FlarkV3DocumentRuntimeStatus> get statuses => _statuses.stream;

  FlarkV3DocumentRuntimeStatus get status => _status();

  int get sourceRevision => _document.uiRevision;
  int get sourceLengthUtf16 => _document.source.utf16Length;

  /// Explicit cold whole-document export.
  String exportMarkdown() => _document.source.toString();

  /// Copies only the requested exact UTF-16 source range.
  String readSourceRange(int startUtf16, int endUtf16) =>
      _document.source.readRange(startUtf16, endUtf16);

  /// Returns one exact-current bounded structural closure at [positionUtf16].
  ///
  /// This call performs no grammar work and never materializes a document-wide
  /// AST. The persistent host either copies a result within [budget] or
  /// returns a typed source gap. While a newer edit is still being certified
  /// or published, the result is [FlarkV3DocumentPendingQuery] and any prior
  /// installed revision is explicitly paint-only.
  FlarkV3DocumentQueryResult queryAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    FlarkV3DocumentQueryBudget budget = const FlarkV3DocumentQueryBudget(),
  }) {
    if (_closeRequested ||
        _terminalFailure != null ||
        _executor.state == FlarkV3SessionDriverState.closing ||
        _executor.state == FlarkV3SessionDriverState.closed) {
      throw _notQueryableStateError();
    }
    final source = _document.source;
    if (positionUtf16 < 0 || positionUtf16 > source.utf16Length) {
      throw RangeError.range(
        positionUtf16,
        0,
        source.utf16Length,
        'positionUtf16',
      );
    }
    if (budget.maximumEncodedBytes <= 0 ||
        budget.maximumOpenDepth <= 0 ||
        budget.maximumLeafCount <= 0 ||
        budget.maximumTreeNodesVisited <= 0) {
      throw ArgumentError.value(
        budget,
        'budget',
        'Every structural query budget must be greater than zero.',
      );
    }

    final presentation = _document.presentationState;
    if (presentation case FlarkV3StablePendingPresentation(
      :final stablePaintAck,
      :final reason,
    )) {
      return FlarkV3DocumentPendingQuery(
        sourceRevision: sourceRevision,
        reason: _pendingReason(reason),
        stableStructureRevision: stablePaintAck?.sourceVersion.revision,
      );
    }
    final exact = presentation as FlarkV3ExactStructuralPresentation;
    final queryBudget = FlarkV3HostQueryBudget(
      maxEncodedBytes: budget.maximumEncodedBytes,
      maxOpenDepth: budget.maximumOpenDepth,
      maxLeafCount: budget.maximumLeafCount,
      maxTreeNodesVisited: budget.maximumTreeNodesVisited,
    );
    final result = _document.query(
      FlarkV3HostPointQuery(
        sourceVersion: exact.sourceVersion,
        position: FlarkV3SourceMetric(
          bytes: source.utf16ToUtf8(positionUtf16),
          utf16: positionUtf16,
        ),
        budget: queryBudget,
        affinity: switch (affinity) {
          FlarkV3DocumentQueryAffinity.upstream =>
            FlarkV3MetricAffinity.upstream,
          FlarkV3DocumentQueryAffinity.downstream =>
            FlarkV3MetricAffinity.downstream,
        },
      ),
    );
    final decoded = switch (result) {
      FlarkV3HostRejected<FlarkV3HostPresentationQuery>(:final rejection) =>
        throw FlarkV3DocumentQueryException(rejection.message),
      FlarkV3HostAccepted<FlarkV3HostPresentationQuery>(
        value: FlarkV3StructuralPresentationQuery(:final viewport),
      ) =>
        FlarkV3DocumentQueryDecoder.decodePointViewport(
          sourceDocument: source,
          expectedSource: exact.sourceVersion,
          expectedProfilePartition: FlarkV3DocumentRuntimePlatformAttachment
              ._publicationAuthority
              .syntaxProfile
              .value,
          viewport: viewport,
        ),
      FlarkV3HostAccepted<FlarkV3HostPresentationQuery>(
        value: FlarkV3SourceGapPresentationQuery(:final gap),
      ) =>
        FlarkV3DocumentSourceGapQuery(
          sourceRevision: sourceRevision,
          structureRevision: gap.sourceVersion.revision,
          range: FlarkV3DocumentQueryDecoder.metricRange(gap.range),
          reason: _gapReason(gap.structuralReason),
        ),
    };
    return switch (decoded) {
      FlarkV3DocumentStructuralQuery() => _inlineFactsCache.resolve(
        authority: exact.ack,
        query: decoded,
      ),
      FlarkV3RecursiveGreenPointQuery() =>
        _inlineFactsCache.resolveRecursiveGreen(
          authority: exact.ack,
          query: _joinInstalledRecursiveGreenPresentation(
            query: decoded,
            presentation: exact,
          ),
        ),
      _ => decoded,
    };
  }

  /// Returns one bounded structural query after requesting its authoritative
  /// inline facts when the selected leaf supports them.
  ///
  /// This is the Dart-only convenience path for live Markdown presentation.
  /// Parser work remains on the native isolate or Web Worker, and each retry
  /// is driven by a credited parser outcome rather than polling. Unsupported
  /// and temporarily non-queryable leaves return their ordinary structural or
  /// pending result without inferring Markdown in Dart.
  Future<FlarkV3DocumentQueryResult> queryInlineAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    FlarkV3DocumentQueryBudget budget = const FlarkV3DocumentQueryBudget(),
  }) async {
    final owner = Object();
    _claimLeafProjectionDemandOwnership(owner);
    try {
      while (true) {
        final query = queryAtUtf16(
          positionUtf16,
          affinity: affinity,
          budget: budget,
        );
        final needsInline = switch (query) {
          FlarkV3DocumentStructuralQuery(:final inlineFacts) =>
            inlineFacts == null,
          FlarkV3RecursiveGreenPointQuery(:final inlineFacts) =>
            inlineFacts == null,
          _ => false,
        };
        if (!needsInline) {
          return query;
        }

        final baselineOutcome = status.inlineAttemptOutcomeGeneration;
        final outcome = Completer<void>.sync();
        late final StreamSubscription<FlarkV3DocumentRuntimeStatus>
        subscription;
        subscription = statuses.listen(
          (next) {
            if (next.inlineAttemptOutcomeGeneration != baselineOutcome ||
                next.state != FlarkV3DocumentRuntimeState.open) {
              if (!outcome.isCompleted) outcome.complete();
            }
          },
          onDone: () {
            if (!outcome.isCompleted) outcome.complete();
          },
        );
        final disposition = switch (query) {
          FlarkV3DocumentStructuralQuery() => _ensureInlineForQuery(
            positionUtf16,
            owner: owner,
            affinity: affinity,
            query: query,
          ),
          FlarkV3RecursiveGreenPointQuery() => _inlineDemandDisposition(
            _ensureRecursiveGreenPresentationForQuery(
              positionUtf16,
              owner: owner,
              affinity: affinity,
              query: query,
            ),
          ),
          _ => FlarkV3InlineDemandDisposition.notApplicable,
        };
        if (disposition != FlarkV3InlineDemandDisposition.scheduled &&
            disposition != FlarkV3InlineDemandDisposition.coalesced) {
          await subscription.cancel();
          return query;
        }
        try {
          await outcome.future;
        } finally {
          await subscription.cancel();
        }
      }
    } finally {
      _releaseLeafProjectionDemandOwnership(owner);
    }
  }

  /// Returns the first bounded page of top-level blocks intersecting an exact
  /// UTF-16 source interval.
  ///
  /// This is structure-only. Selected-leaf inline and noncontiguous projection
  /// payloads remain on [queryAtUtf16].
  FlarkV3DocumentBlockRangeResult queryBlockRange(
    int startUtf16,
    int endUtf16, {
    FlarkV3DocumentBlockRangeBudget budget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) {
    _requireRangeQueryable();
    final source = _document.source;
    if (startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > source.utf16Length ||
        (source.utf16Length != 0 && startUtf16 == endUtf16)) {
      throw RangeError.range(
        endUtf16,
        startUtf16,
        source.utf16Length,
        'endUtf16',
        source.utf16Length == 0
            ? 'Only the empty document admits an empty structural range.'
            : 'A structural range must contain source; use the point query '
                  'for a caret-only position.',
      );
    }
    _validateBlockRangeBudget(budget);
    final FlarkV3MetricRange requested;
    try {
      requested = FlarkV3MetricRange(
        start: FlarkV3SourceMetric(
          bytes: source.utf16ToUtf8(startUtf16),
          utf16: startUtf16,
        ),
        end: FlarkV3SourceMetric(
          bytes: source.utf16ToUtf8(endUtf16),
          utf16: endUtf16,
        ),
      );
    } on FlarkV3SourceFactsPending {
      return FlarkV3DocumentPendingBlockRange(
        sourceRevision: sourceRevision,
        reason: FlarkV3DocumentPendingReason.structurePending,
        stableStructureRevision: _stableStructureRevision,
      );
    }
    return _queryBlockRange(
      requested: requested,
      budget: budget,
      continuation: null,
      expectedSource: null,
    );
  }

  /// Locates one bounded consecutive top-level block window by ordinal.
  ///
  /// This is an O(window plus tree height) host query. It returns only exact
  /// source cuts and never traverses an earlier Dart block page to reach
  /// [FlarkV3DocumentOrdinalWindowDemand.startBlockOrdinal].
  FlarkV3DocumentOrdinalWindowResult queryBlockOrdinalWindow(
    FlarkV3DocumentOrdinalWindowDemand demand, {
    FlarkV3DocumentOrdinalWindowBudget budget =
        const FlarkV3DocumentOrdinalWindowBudget(),
  }) {
    _requireRangeQueryable();
    _validateOrdinalWindowBudget(budget);
    if (demand.sourceRevision != sourceRevision) {
      return _unavailableOrdinalWindow(
        demand,
        FlarkV3DocumentOrdinalWindowFailureReason.sourceChanged,
      );
    }
    final presentation = _document.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation) {
      return _unavailableOrdinalWindow(
        demand,
        FlarkV3DocumentOrdinalWindowFailureReason.runtimeNotReady,
      );
    }
    final structureGeneration = _structureGenerationFor(presentation.ack);
    if (demand.structureGeneration != structureGeneration) {
      return _unavailableOrdinalWindow(
        demand,
        FlarkV3DocumentOrdinalWindowFailureReason.structureChanged,
      );
    }

    final hostBudget = FlarkV3HostStructuralOrdinalWindowBudget(
      maximumEntries: budget.maximumEntries,
      maximumStoragePagesVisited: budget.maximumStoragePagesVisited,
      maximumTreeNodesVisited: budget.maximumTreeNodesVisited,
      maximumPackedEntriesInspected: budget.maximumPackedEntriesInspected,
    );
    final result = _document.queryStructuralOrdinalWindow(
      FlarkV3HostStructuralOrdinalWindowQuery(
        sourceVersion: presentation.sourceVersion,
        startBlockOrdinal: FlarkV3ProtocolU64.fromU32(demand.startBlockOrdinal),
        budget: hostBudget,
      ),
    );
    return switch (result) {
      FlarkV3HostRejected<FlarkV3HostStructuralOrdinalWindowOutcome>() =>
        _unavailableOrdinalWindow(
          demand,
          FlarkV3DocumentOrdinalWindowFailureReason.hostRejected,
        ),
      FlarkV3HostAccepted<FlarkV3HostStructuralOrdinalWindowOutcome>(
        value: FlarkV3HostStructuralOrdinalWindow(:final work) && final window,
      ) =>
        _decodeExactOrdinalWindow(
          demand: demand,
          presentation: presentation,
          structureGeneration: structureGeneration,
          window: window,
          work: work,
        ),
      FlarkV3HostAccepted<FlarkV3HostStructuralOrdinalWindowOutcome>(
        value: FlarkV3HostStructuralOrdinalWindowFailure(
          :final reason,
          :final totalBlockCount,
          :final work,
        ),
      ) =>
        _decodeFailedOrdinalWindow(
          demand: demand,
          reason: reason,
          totalBlockCount: totalBlockCount,
          work: work,
        ),
    };
  }

  FlarkV3DocumentOrdinalWindowResult _decodeExactOrdinalWindow({
    required FlarkV3DocumentOrdinalWindowDemand demand,
    required FlarkV3ExactStructuralPresentation presentation,
    required int structureGeneration,
    required FlarkV3HostStructuralOrdinalWindow window,
    required FlarkV3HostStructuralOrdinalWindowWorkReceipt work,
  }) {
    if (!window.totalBlockCount.fitsU32 ||
        !window.startBlockOrdinal.fitsU32 ||
        !window.nextBlockOrdinal.fitsU32) {
      return _unavailableOrdinalWindow(
        demand,
        FlarkV3DocumentOrdinalWindowFailureReason.undecodable,
        work: work,
      );
    }
    return FlarkV3ExactDocumentOrdinalWindow(
      demand: demand,
      structureRevision: presentation.sourceVersion.revision,
      structureGeneration: structureGeneration,
      totalBlockCount: window.totalBlockCount.lowWord,
      startBlockOrdinal: window.startBlockOrdinal.lowWord,
      nextBlockOrdinal: window.nextBlockOrdinal.lowWord,
      coveredSource: FlarkV3SourceSpan(
        startUtf8: window.startSource.bytes,
        endUtf8: window.nextSource.bytes,
        startUtf16: window.startSource.utf16,
        endUtf16: window.nextSource.utf16,
      ),
      complete: window.complete,
      storagePagesVisited: work.storagePagesVisited,
      treeNodesVisited: work.treeNodesVisited,
      packedEntriesInspected: work.packedEntriesInspected,
      summaryNodesSkipped: work.summaryNodesSkipped,
    );
  }

  FlarkV3UnavailableDocumentOrdinalWindow _decodeFailedOrdinalWindow({
    required FlarkV3DocumentOrdinalWindowDemand demand,
    required FlarkV3HostStructuralOrdinalWindowFailureReason reason,
    required FlarkV3ProtocolU64 totalBlockCount,
    required FlarkV3HostStructuralOrdinalWindowWorkReceipt work,
  }) {
    final totalIsAuthenticated =
        reason != FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable &&
        reason != FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable;
    if (totalIsAuthenticated && !totalBlockCount.fitsU32) {
      return _unavailableOrdinalWindow(
        demand,
        FlarkV3DocumentOrdinalWindowFailureReason.undecodable,
        work: work,
      );
    }
    return _unavailableOrdinalWindow(
      demand,
      switch (reason) {
        FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable =>
          FlarkV3DocumentOrdinalWindowFailureReason.unavailable,
        FlarkV3HostStructuralOrdinalWindowFailureReason.entryLimit =>
          FlarkV3DocumentOrdinalWindowFailureReason.entryLimit,
        FlarkV3HostStructuralOrdinalWindowFailureReason.storagePageLimit =>
          FlarkV3DocumentOrdinalWindowFailureReason.storagePageLimit,
        FlarkV3HostStructuralOrdinalWindowFailureReason.treeNodeLimit =>
          FlarkV3DocumentOrdinalWindowFailureReason.treeNodeLimit,
        FlarkV3HostStructuralOrdinalWindowFailureReason.packedEntryLimit =>
          FlarkV3DocumentOrdinalWindowFailureReason.packedEntryLimit,
        FlarkV3HostStructuralOrdinalWindowFailureReason.ordinalOutOfRange =>
          FlarkV3DocumentOrdinalWindowFailureReason.ordinalOutOfRange,
        FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable =>
          FlarkV3DocumentOrdinalWindowFailureReason.undecodable,
      },
      totalBlockCount: totalIsAuthenticated ? totalBlockCount.lowWord : null,
      work: work,
    );
  }

  FlarkV3UnavailableDocumentOrdinalWindow _unavailableOrdinalWindow(
    FlarkV3DocumentOrdinalWindowDemand demand,
    FlarkV3DocumentOrdinalWindowFailureReason reason, {
    int? totalBlockCount,
    FlarkV3HostStructuralOrdinalWindowWorkReceipt? work,
  }) {
    final receipt = work ?? FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero;
    return FlarkV3UnavailableDocumentOrdinalWindow(
      demand: demand,
      reason: reason,
      totalBlockCount: totalBlockCount,
      storagePagesVisited: receipt.storagePagesVisited,
      treeNodesVisited: receipt.treeNodesVisited,
      packedEntriesInspected: receipt.packedEntriesInspected,
      summaryNodesSkipped: receipt.summaryNodesSkipped,
    );
  }

  /// Resumes one exact range from its host-minted opaque continuation claim.
  ///
  /// A claim from another runtime is rejected. A claim from an older source
  /// revision fails closed as a typed pending result and is never rebased.
  FlarkV3DocumentBlockRangeResult continueBlockRange(
    FlarkV3DocumentBlockRangeContinuation continuation, {
    FlarkV3DocumentBlockRangeBudget budget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) {
    _requireRangeQueryable();
    _validateBlockRangeBudget(budget);
    if (continuation is! _FlarkV3DocumentBlockRangeContinuation ||
        !identical(continuation.owner, _blockRangeContinuationOwner)) {
      throw ArgumentError.value(
        continuation,
        'continuation',
        'The block-range continuation belongs to another runtime.',
      );
    }
    if (continuation.sourceVersion != _document.sourceVersion ||
        continuation.sourceRevision != sourceRevision) {
      return FlarkV3DocumentPendingBlockRange(
        sourceRevision: sourceRevision,
        reason: FlarkV3DocumentPendingReason.sourceChanged,
        stableStructureRevision: _stableStructureRevision,
      );
    }
    final presentation = _document.presentationState;
    if (presentation is FlarkV3ExactStructuralPresentation &&
        presentation.ack != continuation.structuralAck) {
      return FlarkV3DocumentPendingBlockRange(
        sourceRevision: sourceRevision,
        reason: FlarkV3DocumentPendingReason.structurePending,
        stableStructureRevision: presentation.sourceVersion.revision,
      );
    }
    return _queryBlockRange(
      requested: continuation.requestedRange,
      budget: budget,
      continuation: continuation.hostContinuation,
      expectedSource: continuation.sourceVersion,
    );
  }

  FlarkV3DocumentBlockRangeResult _queryBlockRange({
    required FlarkV3MetricRange requested,
    required FlarkV3DocumentBlockRangeBudget budget,
    required FlarkV3HostBlockRangeContinuation? continuation,
    required FlarkV3SourceVersion? expectedSource,
  }) {
    final presentation = _document.presentationState;
    if (presentation case FlarkV3StablePendingPresentation(
      :final stablePaintAck,
      :final reason,
    )) {
      return FlarkV3DocumentPendingBlockRange(
        sourceRevision: sourceRevision,
        reason: _pendingReason(reason),
        stableStructureRevision: stablePaintAck?.sourceVersion.revision,
      );
    }
    final exact = presentation as FlarkV3ExactStructuralPresentation;
    if (expectedSource != null && expectedSource != exact.sourceVersion) {
      return FlarkV3DocumentPendingBlockRange(
        sourceRevision: sourceRevision,
        reason: FlarkV3DocumentPendingReason.sourceChanged,
        stableStructureRevision: exact.sourceVersion.revision,
      );
    }
    final hostBudget = FlarkV3HostBlockRangeBudget(
      maxEncodedBytes: budget.maximumEncodedBytes,
      maxBlockCount: budget.maximumBlockCount,
      maxStoragePagesVisited: budget.maximumStoragePagesVisited,
      maxOpenDepth: budget.maximumOpenDepth,
      maxTreeNodesVisited: budget.maximumTreeNodesVisited,
    );
    final result = _document.queryBlockRange(
      FlarkV3HostBlockRangeQuery(
        sourceVersion: exact.sourceVersion,
        requestedRange: requested,
        budget: hostBudget,
        continuation: continuation,
      ),
    );
    return switch (result) {
      FlarkV3HostRejected<FlarkV3HostBlockRangePresentationQuery>(
        :final rejection,
      ) =>
        throw FlarkV3DocumentQueryException(rejection.message),
      FlarkV3HostAccepted<FlarkV3HostBlockRangePresentationQuery>(
        value: FlarkV3StructuralBlockRangePresentationQuery(:final range),
      ) =>
        _decodeDocumentBlockRange(exact.ack, range),
      FlarkV3HostAccepted<FlarkV3HostBlockRangePresentationQuery>(
        value: FlarkV3BlockRangeSourceGapPresentationQuery(:final gap),
      ) =>
        FlarkV3DocumentSourceGapBlockRange(
          sourceRevision: sourceRevision,
          structureRevision: gap.sourceVersion.revision,
          structureGeneration: _structureGenerationFor(exact.ack),
          requestedSource: FlarkV3DocumentQueryDecoder.metricRange(
            gap.requestedRange,
          ),
          reason: _gapReason(gap.reason),
        ),
    };
  }

  FlarkV3DocumentBlockRangeResult _decodeDocumentBlockRange(
    FlarkV3StructuralAck exactAck,
    FlarkV3HostStructuralBlockRange range,
  ) {
    final exactSource = exactAck.sourceVersion;
    final decoded = FlarkV3DocumentQueryDecoder.decodeBlockRange(
      sourceDocument: _document.source,
      expectedSource: exactSource,
      expectedStructuralAck: exactAck,
      range: range,
    );
    final next = range.continuation;
    final continuation = next == null
        ? null
        : _FlarkV3DocumentBlockRangeContinuation(
            owner: _blockRangeContinuationOwner,
            sourceVersion: exactSource,
            structuralAck: exactAck,
            structureGeneration: _structureGenerationFor(exactAck),
            requestedRange: range.requestedRange,
            hostContinuation: next,
          );
    final recursiveRows = decoded.recursiveGreenRows;
    if (recursiveRows != null) {
      return FlarkV3RecursiveGreenRowRange(
        sourceRevision: sourceRevision,
        structureRevision: exactSource.revision,
        structureGeneration: _structureGenerationFor(exactAck),
        structuralAck: exactAck,
        requestedSource: FlarkV3DocumentQueryDecoder.metricRange(
          range.requestedRange,
        ),
        coveredSource: decoded.coveredSource,
        startGlobalRowOrdinal: decoded.startGlobalRowOrdinal!,
        totalGlobalRowCount: decoded.totalGlobalRowCount!,
        selectedRowIndex: decoded.selectedRowIndex,
        rows: recursiveRows,
        continuation: continuation,
      );
    }
    return FlarkV3DocumentStructuralBlockRange(
      sourceRevision: sourceRevision,
      structureRevision: exactSource.revision,
      structureGeneration: _structureGenerationFor(exactAck),
      requestedSource: FlarkV3DocumentQueryDecoder.metricRange(
        range.requestedRange,
      ),
      coveredSource: decoded.coveredSource,
      blocks: decoded.blocks,
      continuation: continuation,
    );
  }

  int? get _stableStructureRevision => switch (_document.presentationState) {
    FlarkV3ExactStructuralPresentation(:final sourceVersion) =>
      sourceVersion.revision,
    FlarkV3StablePendingPresentation(:final stablePaintAck) =>
      stablePaintAck?.sourceVersion.revision,
  };

  void _requireRangeQueryable() {
    if (_closeRequested ||
        _terminalFailure != null ||
        _executor.state == FlarkV3SessionDriverState.closing ||
        _executor.state == FlarkV3SessionDriverState.closed) {
      throw _notQueryableStateError();
    }
  }

  StateError _notQueryableStateError() {
    final parserFailure = _executor.lastFailure;
    final hostRejection = _executor.lastHostRejection;
    return StateError(
      'The Flark v3 document runtime is not queryable '
      '(state: ${_executor.state.name}, '
      'terminalFailure: ${_terminalFailure?.$1}, '
      'parserFailure: ${parserFailure?.failureCode}, '
      'hostRejection: ${hostRejection?.reason.name}).',
    );
  }

  void _validateBlockRangeBudget(FlarkV3DocumentBlockRangeBudget budget) {
    if (budget.maximumEncodedBytes <= 0 ||
        budget.maximumBlockCount <= 0 ||
        budget.maximumStoragePagesVisited <= 0 ||
        budget.maximumOpenDepth <= 0 ||
        budget.maximumTreeNodesVisited <= 0) {
      throw ArgumentError.value(
        budget,
        'budget',
        'Every structural range query budget must be greater than zero.',
      );
    }
  }

  void _validateOrdinalWindowBudget(FlarkV3DocumentOrdinalWindowBudget budget) {
    if (budget.maximumEntries <= 0 ||
        budget.maximumEntries >
            FlarkV3HostStructuralOrdinalWindowBudget.maximumWindowEntries ||
        budget.maximumStoragePagesVisited <= 0 ||
        budget.maximumStoragePagesVisited >
            FlarkDocumentWorkProfile.prototype.maximumHostTransitions ||
        budget.maximumTreeNodesVisited <= 0 ||
        budget.maximumTreeNodesVisited >
            FlarkDocumentWorkProfile.prototype.maximumQueryTreeNodesVisited ||
        budget.maximumPackedEntriesInspected <= 0 ||
        budget.maximumPackedEntriesInspected >
            FlarkDocumentWorkProfile.prototype.maximumQueryTreeNodesVisited) {
      throw ArgumentError.value(
        budget,
        'budget',
        'The structural ordinal-window budget exceeds the managed runtime '
            'work profile.',
      );
    }
  }

  FlarkV3RecursiveGreenPointQuery _joinInstalledRecursiveGreenPresentation({
    required FlarkV3RecursiveGreenPointQuery query,
    required FlarkV3ExactStructuralPresentation presentation,
  }) {
    if (!(query.owner.kind?.isInlineBearing ?? false)) return query;
    final binding = _document.installedInlineSidecarBinding;
    final ack = _document.installedInlineSidecarAck;
    if (binding == null ||
        ack == null ||
        ack.baseAck != presentation.ack ||
        ack.refinementGeneration != binding.refinementGeneration ||
        ack.blockOrdinal != binding.blockOrdinal ||
        binding.parserProfile != presentation.ack.syntaxProfile ||
        query.source.startUtf8 < binding.physicalStartUtf8 ||
        query.source.endUtf8 > binding.physicalEndUtf8 ||
        query.source.startUtf16 < binding.physicalStartUtf16 ||
        query.source.endUtf16 > binding.physicalEndUtf16) {
      return query;
    }
    final ownsParagraph = _bindingMatchesRecursiveGreenFrame(
      binding.blockOrdinal,
      query.owner.frameId,
    );
    final blockQuoteAncestor = _nearestRecursiveGreenBlockQuoteAncestor(query);
    final ownsBlockQuote =
        blockQuoteAncestor != null &&
        _bindingMatchesRecursiveGreenFrame(
          binding.blockOrdinal,
          blockQuoteAncestor.frameId,
        );
    if (!ownsParagraph && !ownsBlockQuote) return query;
    final hostResult = _document.queryInlineSidecar(
      FlarkV3InlineSidecarQuery(
        binding: binding,
        maximumEncodedBytes: FlarkV3InlineSidecarQuery.maximumQueryBytes,
      ),
    );
    try {
      return switch (hostResult) {
        FlarkV3HostRejected<FlarkV3InlineSidecarQueryOutcome>() => query,
        FlarkV3HostAccepted<FlarkV3InlineSidecarQueryOutcome>(:final value) =>
          ownsParagraph
              ? FlarkV3DocumentQueryDecoder.joinRecursiveGreenInline(
                  sourceDocument: _document.source,
                  expectedSource: presentation.sourceVersion,
                  expectedProfilePartition:
                      FlarkV3DocumentRuntimePlatformAttachment
                          ._publicationAuthority
                          .syntaxProfile
                          .value,
                  query: query,
                  binding: binding,
                  outcome: value,
                )
              : FlarkV3DocumentQueryDecoder.joinRecursiveGreenBlockQuoteProjection(
                  sourceDocument: _document.source,
                  expectedSource: presentation.sourceVersion,
                  expectedProfilePartition:
                      FlarkV3DocumentRuntimePlatformAttachment
                          ._publicationAuthority
                          .syntaxProfile
                          .value,
                  query: query,
                  binding: binding,
                  outcome: value,
                ),
      };
    } on FlarkV3InlineFactsDecodeException catch (error) {
      throw FlarkV3DocumentQueryException(error.message);
    }
  }

  bool _bindingMatchesRecursiveGreenFrame(
    FlarkV3ProtocolU64 owner,
    BigInt frameId,
  ) {
    final tag = BigInt.one << 63;
    if (frameId <= BigInt.zero || frameId >= tag) return false;
    final encoded =
        (BigInt.from(owner.highWord) << 32) | BigInt.from(owner.lowWord);
    return encoded == (tag | frameId);
  }

  FlarkV3RecursiveGreenAncestor? _nearestRecursiveGreenBlockQuoteAncestor(
    FlarkV3RecursiveGreenPointQuery query,
  ) {
    for (var index = query.ownerIndex - 1; index >= 0; index -= 1) {
      final ancestor = query.ancestry[index];
      if (ancestor.kind == FlarkV3RecursiveGreenKind.blockQuote) {
        return ancestor;
      }
    }
    return null;
  }

  FlarkV3LeafProjectionDemandDisposition
  _ensureRecursiveGreenPresentationForQuery(
    int positionUtf16, {
    required Object owner,
    required FlarkV3DocumentQueryAffinity affinity,
    required FlarkV3RecursiveGreenPointQuery query,
  }) {
    if (!identical(_leafProjectionDemandOwner, owner) ||
        _closeRequested ||
        _terminalFailure != null ||
        _executor.state != FlarkV3SessionDriverState.open ||
        !_document.sourceWorkerSynchronized) {
      return FlarkV3LeafProjectionDemandDisposition.notReady;
    }
    final presentation = _document.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation ||
        query.sourceRevision != presentation.sourceVersion.revision ||
        query.structureRevision != presentation.sourceVersion.revision ||
        query.pointUtf16 != positionUtf16 ||
        query.affinity != affinity) {
      return FlarkV3LeafProjectionDemandDisposition.stale;
    }
    if (!(query.owner.kind?.isInlineBearing ?? false) ||
        query.source.endUtf8 > presentation.sourceVersion.metric.bytes ||
        query.source.endUtf16 > presentation.sourceVersion.metric.utf16) {
      return FlarkV3LeafProjectionDemandDisposition.notApplicable;
    }
    final blockQuoteAncestor = _nearestRecursiveGreenBlockQuoteAncestor(query);
    final canRequestBlockQuoteProjection =
        blockQuoteAncestor != null &&
        _recursiveGreenCanRequestBlockQuoteProjection(
          query,
          blockQuoteAncestor,
        );
    late final FlarkV3InlineRefinementTarget target;
    late final BigInt demandOwnerFrameId;
    if (query.owner.kind == FlarkV3RecursiveGreenKind.paragraph &&
        canRequestBlockQuoteProjection &&
        query.blockQuoteProjection == null) {
      target = FlarkV3InlineRefinementTarget.blockQuoteProjection;
      demandOwnerFrameId = blockQuoteAncestor.frameId;
    } else if (query.inlineFacts == null &&
        (!canRequestBlockQuoteProjection ||
            _recursiveGreenBlockQuoteHasContiguousInlineSource(query))) {
      target = FlarkV3InlineRefinementTarget.recursiveGreenParagraph;
      demandOwnerFrameId = query.owner.frameId;
    } else {
      return FlarkV3LeafProjectionDemandDisposition.notApplicable;
    }
    final requestAffinity = switch (affinity) {
      FlarkV3DocumentQueryAffinity.upstream =>
        FlarkV3InlinePointAffinity.before,
      FlarkV3DocumentQueryAffinity.downstream =>
        FlarkV3InlinePointAffinity.after,
    };
    final demand = _FlarkV3RecursiveGreenProjectionDemandKey(
      structuralAck: presentation.ack,
      ownerFrameId: demandOwnerFrameId,
      target: target,
    );
    if (demand != _lastLeafProjectionDemand) {
      _lastLeafProjectionDemand = demand;
      _leafProjectionDemandAttempts = 0;
      _leafProjectionDemandLastRequestedOutcomeGeneration = null;
    }
    final outcomeGeneration = _executor.inlineAttemptOutcomeGeneration;
    if (_leafProjectionDemandAttempts != 0 &&
        _leafProjectionDemandLastRequestedOutcomeGeneration ==
            outcomeGeneration) {
      return FlarkV3LeafProjectionDemandDisposition.coalesced;
    }
    if (_leafProjectionDemandAttempts >= _maximumLeafProjectionDemandAttempts) {
      return FlarkV3LeafProjectionDemandDisposition.retryLimitReached;
    }
    _executor.requestInlineRefinement(
      utf16Offset: positionUtf16,
      affinity: requestAffinity,
      target: target,
    );
    _resetViewportPresentationDemandTracking();
    _leafProjectionDemandAttempts += 1;
    _leafProjectionDemandLastRequestedOutcomeGeneration = outcomeGeneration;
    return FlarkV3LeafProjectionDemandDisposition.scheduled;
  }

  bool _recursiveGreenCanRequestBlockQuoteProjection(
    FlarkV3RecursiveGreenPointQuery query,
    FlarkV3RecursiveGreenAncestor blockQuote,
  ) =>
      query.ownerIndex == 2 &&
      query.ancestry.length == 3 &&
      query.ancestry[0].kind == FlarkV3RecursiveGreenKind.document &&
      query.ancestry[1].kind == FlarkV3RecursiveGreenKind.blockQuote &&
      query.ancestry[1].frameId == blockQuote.frameId &&
      query.ancestry[2].kind == FlarkV3RecursiveGreenKind.paragraph;

  bool _recursiveGreenBlockQuoteHasContiguousInlineSource(
    FlarkV3RecursiveGreenPointQuery query,
  ) {
    final projection = query.blockQuoteProjection;
    if (projection == null || projection.records.isEmpty) return false;
    // A leading quote marker precedes the Paragraph's contiguous inline
    // source. Any later hidden marker splits that source into physical
    // islands and therefore requires the projected-inline lane instead.
    return projection.records
        .skip(1)
        .every(
          (record) =>
              record.hiddenPrefix.startUtf8 == record.hiddenPrefix.endUtf8,
        );
  }

  FlarkV3LeafProjectionDemandDisposition _ensureLeafProjectionForQuery(
    int positionUtf16, {
    required Object owner,
    required FlarkV3DocumentQueryAffinity affinity,
    required FlarkV3DocumentStructuralQuery query,
    required bool inlineOnly,
  }) {
    if (!identical(_leafProjectionDemandOwner, owner) ||
        _closeRequested ||
        _terminalFailure != null ||
        _executor.state != FlarkV3SessionDriverState.open ||
        !_document.sourceWorkerSynchronized) {
      return FlarkV3LeafProjectionDemandDisposition.notReady;
    }
    final presentation = _document.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation ||
        query.sourceRevision != presentation.sourceVersion.revision ||
        query.structureRevision != presentation.sourceVersion.revision) {
      return FlarkV3LeafProjectionDemandDisposition.stale;
    }
    if (!_queryFitsSourceVersion(query, presentation.sourceVersion)) {
      return FlarkV3LeafProjectionDemandDisposition.notApplicable;
    }
    final target = _pendingLeafProjectionTarget(query, inlineOnly: inlineOnly);
    if (target == null) {
      return FlarkV3LeafProjectionDemandDisposition.notApplicable;
    }
    final requestPoint = _leafProjectionRequestPointForLeaf(
      positionUtf16,
      affinity: affinity,
      leaf: target.leaf,
    );
    final pointSelectsListItem =
        target.kind == _FlarkV3LeafProjectionKind.bulletListItemProjection ||
        target.kind == _FlarkV3LeafProjectionKind.orderedListItemProjection;
    final demand = _FlarkV3LeafProjectionDemandKey(
      structuralAck: presentation.ack,
      kind: target.kind,
      leaf: target.leaf,
      requestPositionUtf16: pointSelectsListItem
          ? requestPoint.positionUtf16
          : null,
      requestAffinity: pointSelectsListItem ? requestPoint.affinity : null,
    );
    if (demand != _lastLeafProjectionDemand) {
      _lastLeafProjectionDemand = demand;
      _leafProjectionDemandAttempts = 0;
      _leafProjectionDemandLastRequestedOutcomeGeneration = null;
    }
    final outcomeGeneration = _executor.inlineAttemptOutcomeGeneration;
    if (_leafProjectionDemandAttempts != 0 &&
        _leafProjectionDemandLastRequestedOutcomeGeneration ==
            outcomeGeneration) {
      return FlarkV3LeafProjectionDemandDisposition.coalesced;
    }
    if (_leafProjectionDemandAttempts >= _maximumLeafProjectionDemandAttempts) {
      return FlarkV3LeafProjectionDemandDisposition.retryLimitReached;
    }
    _executor.requestInlineRefinement(
      utf16Offset: requestPoint.positionUtf16,
      affinity: requestPoint.affinity,
      target: switch (target.kind) {
        _FlarkV3LeafProjectionKind.bulletListItemInline =>
          FlarkV3InlineRefinementTarget.bulletListItemInline,
        _FlarkV3LeafProjectionKind.bulletListItemProjection =>
          FlarkV3InlineRefinementTarget.bulletListItemProjection,
        _FlarkV3LeafProjectionKind.orderedListItemInline =>
          FlarkV3InlineRefinementTarget.orderedListItemInline,
        _FlarkV3LeafProjectionKind.orderedListItemProjection =>
          FlarkV3InlineRefinementTarget.orderedListItemProjection,
        _FlarkV3LeafProjectionKind.inline
            when query.structure.kind ==
                FlarkV3DocumentStructureKind.paragraph =>
          FlarkV3InlineRefinementTarget.recursiveGreenParagraph,
        _ => FlarkV3InlineRefinementTarget.automatic,
      },
    );
    // Focused demand intentionally clears an unsent passive request inside the
    // executor. Forget caller-side coalescing as well, otherwise that exact
    // passive window could remain falsely "coalesced" forever.
    _resetViewportPresentationDemandTracking();
    _leafProjectionDemandAttempts += 1;
    _leafProjectionDemandLastRequestedOutcomeGeneration = outcomeGeneration;
    return FlarkV3LeafProjectionDemandDisposition.scheduled;
  }

  FlarkV3InlineDemandDisposition _ensureInlineForQuery(
    int positionUtf16, {
    required Object owner,
    required FlarkV3DocumentQueryAffinity affinity,
    required FlarkV3DocumentStructuralQuery query,
  }) => _inlineDemandDisposition(
    _ensureLeafProjectionForQuery(
      positionUtf16,
      owner: owner,
      affinity: affinity,
      query: query,
      inlineOnly: true,
    ),
  );

  void _claimLeafProjectionDemandOwnership(Object owner) {
    if (_closeRequested || _terminalFailure != null) {
      throw StateError(
        'The Flark v3 document runtime cannot accept a leaf-projection '
        'demand owner.',
      );
    }
    final current = _leafProjectionDemandOwner;
    if (current != null && !identical(current, owner)) {
      throw StateError(
        'Only one active presentation adapter may drive leaf-projection '
        'refinement.',
      );
    }
    if (current == null) {
      _lastLeafProjectionDemand = null;
      _leafProjectionDemandAttempts = 0;
      _leafProjectionDemandLastRequestedOutcomeGeneration = null;
    }
    _leafProjectionDemandOwner = owner;
  }

  void _releaseLeafProjectionDemandOwnership(Object owner) {
    if (identical(_leafProjectionDemandOwner, owner)) {
      _leafProjectionDemandOwner = null;
      _lastLeafProjectionDemand = null;
      _leafProjectionDemandAttempts = 0;
      _leafProjectionDemandLastRequestedOutcomeGeneration = null;
    }
  }

  void _claimViewportPresentationDemandOwnership(Object owner) {
    if (_closeRequested || _terminalFailure != null) {
      throw StateError(
        'The Flark v3 document runtime cannot accept a viewport-presentation '
        'demand owner.',
      );
    }
    final current = _viewportPresentationDemandOwner;
    if (current != null && !identical(current, owner)) {
      throw StateError(
        'Only one active presentation adapter may drive passive viewport '
        'demand.',
      );
    }
    if (current == null) _resetViewportPresentationDemandTracking();
    _viewportPresentationDemandOwner = owner;
  }

  void _releaseViewportPresentationDemandOwnership(Object owner) {
    if (identical(_viewportPresentationDemandOwner, owner)) {
      _viewportPresentationDemandOwner = null;
      _resetViewportPresentationDemandTracking();
    }
  }

  FlarkV3ViewportPresentationDemandReceipt _ensureViewportPresentation(
    FlarkV3ViewportPresentationDemand demand, {
    required Object owner,
  }) {
    final outcomeGeneration =
        _executor.viewportPresentationAttemptOutcomeGeneration;
    FlarkV3ViewportPresentationDemandReceipt receipt(
      FlarkV3ViewportPresentationDemandDisposition disposition, {
      int? viewportGeneration,
      FlarkV3ViewportPresentationUnavailableReason? unavailableReason,
    }) => FlarkV3ViewportPresentationDemandReceipt._(
      disposition: disposition,
      viewportGeneration: viewportGeneration,
      attemptOutcomeGeneration: outcomeGeneration,
      unavailableReason: unavailableReason,
    );

    if (!identical(_viewportPresentationDemandOwner, owner) ||
        _closeRequested ||
        _terminalFailure != null ||
        _executor.state != FlarkV3SessionDriverState.open) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.notReady,
        unavailableReason:
            FlarkV3ViewportPresentationUnavailableReason.runtimeNotReady,
      );
    }
    if (demand.sourceRevision != sourceRevision) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.stale,
        unavailableReason:
            FlarkV3ViewportPresentationUnavailableReason.sourceChanged,
      );
    }
    if (!_document.supportsViewportPresentation) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.unsupported,
        unavailableReason:
            FlarkV3ViewportPresentationUnavailableReason.unsupported,
      );
    }

    final presentation = _document.presentationState;
    if (!_document.sourceWorkerSynchronized ||
        presentation is! FlarkV3ExactStructuralPresentation) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.notReady,
        unavailableReason:
            FlarkV3ViewportPresentationUnavailableReason.runtimeNotReady,
      );
    }
    final structureGeneration = _structureGenerationFor(presentation.ack);
    if (demand.structureGeneration != structureGeneration) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.stale,
        unavailableReason:
            FlarkV3ViewportPresentationUnavailableReason.structureChanged,
      );
    }

    final requestedRange = _viewportMetricRange(demand);
    final installed = _document.installedViewportPresentationAck;
    if (installed != null &&
        _viewportAckMatchesDemand(
          installed,
          baseAck: presentation.ack,
          demand: demand,
          requestedRange: requestedRange,
        )) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.current,
        viewportGeneration: installed.binding.viewportGeneration,
      );
    }

    final key = _FlarkV3ViewportPresentationDemandKey(
      structuralAck: presentation.ack,
      demand: demand,
    );
    if (key != _lastViewportPresentationDemand) {
      _resetViewportPresentationDemandTracking();
      _lastViewportPresentationDemand = key;
    }
    if (_viewportPresentationDemandAttempts != 0 &&
        _viewportPresentationDemandLastRequestedOutcomeGeneration ==
            outcomeGeneration) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.coalesced,
        viewportGeneration: _viewportPresentationDemandGeneration,
      );
    }

    final unavailable = _trackedViewportPresentationUnavailableReason();
    if (_viewportPresentationDemandAttempts != 0 &&
        unavailable != null &&
        unavailable !=
            FlarkV3ViewportPresentationUnavailableReason.retryableBusy) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.unavailable,
        viewportGeneration: _viewportPresentationDemandGeneration,
        unavailableReason: unavailable,
      );
    }
    if (_viewportPresentationDemandAttempts >=
        _maximumViewportPresentationDemandAttempts) {
      return receipt(
        FlarkV3ViewportPresentationDemandDisposition.retryLimitReached,
        viewportGeneration: _viewportPresentationDemandGeneration,
        unavailableReason: unavailable,
      );
    }

    final viewportGeneration = _executor.requestViewportPresentation(
      requestedStartUtf8: requestedRange.startUtf8,
      requestedStartUtf16: requestedRange.startUtf16,
      requestedEndUtf8: requestedRange.endUtf8,
      requestedEndUtf16: requestedRange.endUtf16,
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(demand.startBlockOrdinal),
    );
    _viewportPresentationDemandAttempts += 1;
    _viewportPresentationDemandLastRequestedOutcomeGeneration =
        outcomeGeneration;
    _viewportPresentationDemandGeneration = viewportGeneration;
    return receipt(
      FlarkV3ViewportPresentationDemandDisposition.scheduled,
      viewportGeneration: viewportGeneration,
    );
  }

  FlarkV3ViewportPresentationPageResult _queryViewportPresentation(
    FlarkV3ViewportPresentationDemand demand, {
    required int maximumEncodedBytes,
  }) {
    if (maximumEncodedBytes <= 0) {
      throw RangeError.value(
        maximumEncodedBytes,
        'maximumEncodedBytes',
        'A viewport page query bound must be greater than zero.',
      );
    }
    final outcomeGeneration =
        _executor.viewportPresentationAttemptOutcomeGeneration;
    FlarkV3UnavailableViewportPresentationPage unavailable(
      FlarkV3ViewportPresentationUnavailableReason reason,
    ) => FlarkV3UnavailableViewportPresentationPage(
      demand: demand,
      attemptOutcomeGeneration: outcomeGeneration,
      reason: reason,
    );

    if (_closeRequested ||
        _terminalFailure != null ||
        _executor.state != FlarkV3SessionDriverState.open) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.runtimeNotReady,
      );
    }
    if (demand.sourceRevision != sourceRevision) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.sourceChanged,
      );
    }
    if (!_document.sourceWorkerSynchronized) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.runtimeNotReady,
      );
    }
    if (!_document.supportsViewportPresentation) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.unsupported,
      );
    }
    final presentation = _document.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.runtimeNotReady,
      );
    }
    final structureGeneration = _structureGenerationFor(presentation.ack);
    if (demand.structureGeneration != structureGeneration) {
      return unavailable(
        FlarkV3ViewportPresentationUnavailableReason.structureChanged,
      );
    }
    final requestedRange = _viewportMetricRange(demand);
    final installed = _document.installedViewportPresentationAck;
    if (installed == null ||
        !_viewportAckMatchesDemand(
          installed,
          baseAck: presentation.ack,
          demand: demand,
          requestedRange: requestedRange,
        )) {
      return unavailable(
        _trackedViewportPresentationUnavailableReason(
              demand: _FlarkV3ViewportPresentationDemandKey(
                structuralAck: presentation.ack,
                demand: demand,
              ),
            ) ??
            FlarkV3ViewportPresentationUnavailableReason.notInstalled,
      );
    }

    final result = _document.queryViewportPresentation(
      FlarkV3ViewportPresentationQuery(
        ack: installed,
        maximumEncodedBytes: maximumEncodedBytes,
      ),
    );
    return switch (result) {
      FlarkV3HostRejected<FlarkV3ViewportPresentationQueryOutcome>(
        :final rejection,
      ) =>
        unavailable(
          rejection.reason == FlarkV3HostRejectReason.queryBoundExceeded
              ? FlarkV3ViewportPresentationUnavailableReason.queryBoundExceeded
              : FlarkV3ViewportPresentationUnavailableReason.hostRejected,
        ),
      FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>(
        value: FlarkV3ViewportPresentationQueryUnavailable(),
      ) =>
        unavailable(
          FlarkV3ViewportPresentationUnavailableReason.queryUnavailable,
        ),
      FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>(
        value: FlarkV3ViewportPresentationQueryAvailable(:final page),
      ) =>
        FlarkV3ExactViewportPresentationPage(
          demand: demand,
          attemptOutcomeGeneration: outcomeGeneration,
          page: page,
          currentStructuralAck: presentation.ack,
          structureGeneration: structureGeneration,
          sourceDocument: _document.source,
        ),
    };
  }

  FlarkV3ViewportPresentationMetricRange _viewportMetricRange(
    FlarkV3ViewportPresentationDemand demand,
  ) {
    final source = _document.source;
    if (demand.startUtf16 < 0 ||
        demand.endUtf16 <= demand.startUtf16 ||
        demand.endUtf16 > source.utf16Length) {
      throw RangeError.range(
        demand.endUtf16,
        demand.startUtf16 + 1,
        source.utf16Length,
        'endUtf16',
        'Viewport demand is outside exact current source.',
      );
    }
    return FlarkV3ViewportPresentationMetricRange(
      startUtf8: source.utf16ToUtf8(demand.startUtf16),
      startUtf16: demand.startUtf16,
      endUtf8: source.utf16ToUtf8(demand.endUtf16),
      endUtf16: demand.endUtf16,
    );
  }

  FlarkV3ViewportPresentationUnavailableReason?
  _trackedViewportPresentationUnavailableReason({
    _FlarkV3ViewportPresentationDemandKey? demand,
  }) {
    final demandGeneration = _viewportPresentationDemandGeneration;
    if (demandGeneration == null ||
        _executor.lastViewportPresentationUnavailableGeneration !=
            demandGeneration ||
        demand != null && demand != _lastViewportPresentationDemand) {
      return null;
    }
    return _viewportPresentationUnavailableReason(
      _executor.lastViewportPresentationUnavailableReason,
    );
  }

  void _resetViewportPresentationDemandTracking() {
    _lastViewportPresentationDemand = null;
    _viewportPresentationDemandAttempts = 0;
    _viewportPresentationDemandLastRequestedOutcomeGeneration = null;
    _viewportPresentationDemandGeneration = null;
  }

  FlarkV3DocumentEditResult apply(FlarkV3SourceTransaction transaction) {
    final receipt = _applyForAdapter(transaction);
    return FlarkV3DocumentEditResult._(
      changed: receipt.changed,
      sourceRevision: _document.uiRevision,
    );
  }

  FlarkDocumentEditReceipt _applyForAdapter(
    FlarkV3SourceTransaction transaction,
  ) {
    _requireWritable();
    final receipt = _document.apply(transaction);
    if (receipt.changed) {
      _inlineFactsCache.clear();
      _resetViewportPresentationDemandTracking();
      _executor.sourceChanged();
    }
    _publishStatus();
    return receipt;
  }

  FlarkV3DocumentEditResult? undo() {
    _requireWritable();
    final receipt = _document.undo();
    if (receipt?.changed ?? false) {
      _inlineFactsCache.clear();
      _resetViewportPresentationDemandTracking();
      _executor.sourceChanged();
    }
    _publishStatus();
    return receipt == null
        ? null
        : FlarkV3DocumentEditResult._(
            changed: receipt.changed,
            sourceRevision: _document.uiRevision,
          );
  }

  /// Recovers an in-protocol parser failure without changing exact source.
  void recover() {
    if (!status.recoveryAvailable) {
      throw StateError('The Flark v3 document runtime is not recoverable.');
    }
    _inlineFactsCache.clear();
    _resetViewportPresentationDemandTracking();
    _executor.restart();
    _publishStatus();
  }

  /// Gracefully drains both the parser endpoint and host-store owner.
  ///
  /// Repeated calls share one completion. The returned future does not finish
  /// until the platform endpoint confirms its slot has actually been released.
  Future<void> close() {
    final existing = _closeFuture;
    if (existing != null) return existing;

    final completion = Completer<void>();
    _closeFuture = completion.future;

    // Publish caller intent before beginning any potentially re-entrant close
    // work. apply()/undo() must not mutate exact source after this point even
    // if the executor has not transitioned to `closing` yet.
    _closeRequested = true;
    _inlineFactsCache.clear();
    _resetViewportPresentationDemandTracking();
    if (!_initialReady.isCompleted) {
      _initialReady.completeError(
        const FlarkV3RuntimeClosedBeforeReady(),
        StackTrace.current,
      );
    }
    _close().then(
      completion.complete,
      onError: (Object error, StackTrace stackTrace) =>
          completion.completeError(error, stackTrace),
    );
    return completion.future;
  }

  Future<void> _close() async {
    Object? closeError;
    StackTrace? closeStack;
    try {
      final executorClose = _executor.close();
      _publishStatus();
      await executorClose;
    } catch (error, stackTrace) {
      closeError = error;
      closeStack = stackTrace;
      _executor.emergencyDispose();
    }

    try {
      await _endpointDone;
    } catch (error, stackTrace) {
      closeError ??= error;
      closeStack ??= stackTrace;
    }
    _publishStatus(force: true);
    _flushPendingStatusDeliveries();
    await _statuses.close();

    final terminal = _terminalFailure;
    if (closeError != null) {
      Error.throwWithStackTrace(closeError, closeStack!);
    }
    if (terminal != null) {
      Error.throwWithStackTrace(terminal.$1, terminal.$2);
    }
  }

  void _recordFailure(Object error, StackTrace stackTrace) {
    if (_terminalFailure != null) return;
    _terminalFailure = (error, stackTrace);
    _inlineFactsCache.clear();
    _resetViewportPresentationDemandTracking();
    if (!_initialReady.isCompleted) {
      _initialReady.completeError(error, stackTrace);
    }
    _executor.emergencyDispose();
    _publishStatus(force: true);
  }

  void _publishStatus({bool force = false}) {
    if (_statuses.isClosed) return;
    final next = _status();
    if (!force && next == _lastPublishedStatus) return;
    _lastPublishedStatus = next;
    _enqueueStatusDelivery(next, barrier: force);
    if (!_initialReady.isCompleted) {
      final parserFailure = _executor.lastFailure;
      if (next.state == FlarkV3DocumentRuntimeState.faulted &&
          parserFailure != null) {
        _initialReady.completeError(
          const FlarkV3RuntimeParserFailure._(),
          StackTrace.current,
        );
      } else if (next.structureCurrent) {
        _initialReady.complete();
      }
    }
  }

  void _enqueueStatusDelivery(
    FlarkV3DocumentRuntimeStatus next, {
    required bool barrier,
  }) {
    // Preserve broadcast-stream no-replay semantics while buffering.
    if (!_statuses.hasListener) return;
    final last = _pendingStatusDeliveries.lastOrNull;
    if (!barrier &&
        last != null &&
        !last.barrier &&
        next._canSupersedePendingSourceStatus(last.status)) {
      _pendingStatusDeliveries[_pendingStatusDeliveries.length - 1] =
          _FlarkV3PendingStatusDelivery(next, barrier: false);
    } else {
      _pendingStatusDeliveries.add(
        _FlarkV3PendingStatusDelivery(next, barrier: barrier),
      );
    }
    if (_statusDeliveryScheduled) return;
    _statusDeliveryScheduled = true;
    scheduleMicrotask(_flushPendingStatusDeliveries);
  }

  void _flushPendingStatusDeliveries() {
    _statusDeliveryScheduled = false;
    if (_pendingStatusDeliveries.isEmpty) return;
    if (_statuses.isClosed) {
      _pendingStatusDeliveries.clear();
      return;
    }
    final pending = List<_FlarkV3PendingStatusDelivery>.of(
      _pendingStatusDeliveries,
      growable: false,
    );
    _pendingStatusDeliveries.clear();
    for (final delivery in pending) {
      _statuses.add(delivery.status);
    }
  }

  FlarkV3DocumentRuntimeStatus _status() {
    final presentation = _document.presentationState;
    final installed = switch (presentation) {
      FlarkV3ExactStructuralPresentation(:final ack) => ack,
      FlarkV3StablePendingPresentation(:final stablePaintAck) => stablePaintAck,
    };
    final structureGeneration = _observeStructuralAck(installed);
    return FlarkV3DocumentRuntimeStatus._(
      state: switch (_executor.state) {
        FlarkV3SessionDriverState.opening =>
          FlarkV3DocumentRuntimeState.opening,
        FlarkV3SessionDriverState.open => FlarkV3DocumentRuntimeState.open,
        FlarkV3SessionDriverState.faulted =>
          FlarkV3DocumentRuntimeState.faulted,
        FlarkV3SessionDriverState.closing =>
          FlarkV3DocumentRuntimeState.closing,
        FlarkV3SessionDriverState.closed => FlarkV3DocumentRuntimeState.closed,
      },
      sourceRevision: _document.uiRevision,
      certifiedSourceRevision: _document.sourceVersion.revision,
      sourceCurrent:
          _document.currentUiSourceCertified &&
          _document.sourceWorkerSynchronized,
      structureRevision: installed?.sourceVersion.revision,
      structureGeneration: structureGeneration,
      structureCurrent: presentation is FlarkV3ExactStructuralPresentation,
      inlinePresentationGeneration: _executor.inlinePresentationGeneration,
      inlineAttemptOutcomeGeneration: _executor.inlineAttemptOutcomeGeneration,
      viewportPresentationGeneration:
          _document
              .installedViewportPresentationAck
              ?.binding
              .viewportGeneration ??
          0,
      viewportPresentationAttemptOutcomeGeneration:
          _executor.viewportPresentationAttemptOutcomeGeneration,
      viewportPresentationUnavailableReason:
          _trackedViewportPresentationUnavailableReason(),
      recoveryAvailable:
          _terminalFailure == null &&
          _executor.state == FlarkV3SessionDriverState.faulted,
    );
  }

  int _observeStructuralAck(FlarkV3StructuralAck? ack) {
    if (ack == null) {
      _observedStructuralAck = null;
      return _structureGeneration;
    }
    if (ack != _observedStructuralAck) {
      _observedStructuralAck = ack;
      _structureGeneration += 1;
    }
    return _structureGeneration;
  }

  int _structureGenerationFor(FlarkV3StructuralAck ack) =>
      _observeStructuralAck(ack);

  void _requireWritable() {
    if (_closeRequested ||
        _terminalFailure != null ||
        (_executor.state != FlarkV3SessionDriverState.opening &&
            _executor.state != FlarkV3SessionDriverState.open)) {
      final parserFailure = _executor.lastFailure;
      final hostRejection = _executor.lastHostRejection;
      throw StateError(
        'The Flark v3 document runtime is not writable '
        '(state: ${_executor.state.name}, '
        'parserFailure: ${parserFailure?.failureCode}, '
        'hostRejection: ${hostRejection?.reason.name}).',
      );
    }
  }
}

const _maximumLeafProjectionDemandAttempts = 2;
const _maximumViewportPresentationDemandAttempts = 2;
const _maximumWholeLeafProjectionSourceBytes = 8 * 1024;
const _maximumCachedInlineLeaves = 128;
const _maximumCachedInlineFactRecords = 2048;

({int positionUtf16, FlarkV3InlinePointAffinity affinity})
_leafProjectionRequestPointForLeaf(
  int positionUtf16, {
  required FlarkV3DocumentQueryAffinity affinity,
  required FlarkV3SourceSpan leaf,
}) {
  if (positionUtf16 <= leaf.startUtf16) {
    return (
      positionUtf16: leaf.startUtf16,
      affinity: FlarkV3InlinePointAffinity.after,
    );
  }
  if (positionUtf16 >= leaf.endUtf16) {
    return (
      positionUtf16: leaf.endUtf16,
      affinity: FlarkV3InlinePointAffinity.before,
    );
  }
  return (
    positionUtf16: positionUtf16,
    affinity: switch (affinity) {
      FlarkV3DocumentQueryAffinity.upstream =>
        FlarkV3InlinePointAffinity.before,
      FlarkV3DocumentQueryAffinity.downstream =>
        FlarkV3InlinePointAffinity.after,
    },
  );
}

enum _FlarkV3LeafProjectionKind {
  inline,
  indentedCode,
  blockQuote,
  bulletListItemProjection,
  bulletListItemInline,
  orderedListItemProjection,
  orderedListItemInline,
}

typedef _FlarkV3PendingLeafProjectionTarget = ({
  _FlarkV3LeafProjectionKind kind,
  FlarkV3SourceSpan leaf,
});

bool _queryFitsSourceVersion(
  FlarkV3DocumentStructuralQuery query,
  FlarkV3SourceVersion sourceVersion,
) {
  bool fits(FlarkV3SourceSpan span) =>
      span.endUtf8 <= sourceVersion.metric.bytes &&
      span.endUtf16 <= sourceVersion.metric.utf16;
  return fits(query.structure.source) &&
      fits(query.structure.visibleSource) &&
      fits(query.projection.source) &&
      fits(query.projection.projectedSource);
}

_FlarkV3PendingLeafProjectionTarget? _pendingLeafProjectionTarget(
  FlarkV3DocumentStructuralQuery query, {
  required bool inlineOnly,
}) {
  final structure = query.structure;
  final projection = query.projection;
  if (projection.kind != structure.kind ||
      !_sameSourceSpan(projection.source, structure.source)) {
    return null;
  }

  final inlineContentSource = structure.inlineContentSource;
  if (structure.canCarryInlineFacts &&
      inlineContentSource != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.bulletListProjection == null &&
      query.orderedListProjection == null &&
      _sameSourceSpan(inlineContentSource, projection.projectedSource) &&
      inlineContentSource.endUtf8 - inlineContentSource.startUtf8 <=
          FlarkV3InlineFacts.maximumWholeLeafSourceBytes) {
    return (kind: _FlarkV3LeafProjectionKind.inline, leaf: structure.source);
  }

  final indentedCode = structure.indentedCode;
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.indentedCode &&
      indentedCode != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.bulletListProjection == null &&
      query.orderedListProjection == null &&
      _sameSourceSpan(structure.visibleSource, projection.projectedSource) &&
      projection.runCount == indentedCode.lineCount &&
      structure.source.endUtf8 - structure.source.startUtf8 <=
          _maximumWholeLeafProjectionSourceBytes) {
    return (
      kind: _FlarkV3LeafProjectionKind.indentedCode,
      leaf: structure.source,
    );
  }

  final blockQuote = structure.blockQuote;
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.blockQuote &&
      blockQuote != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.pointPath == null &&
      query.blockQuoteProjection == null &&
      query.bulletListProjection == null &&
      query.orderedListProjection == null &&
      _sameSourceSpan(structure.visibleSource, projection.projectedSource) &&
      projection.runCount == blockQuote.lineCount &&
      structure.source.endUtf8 - structure.source.startUtf8 <=
          _maximumWholeLeafProjectionSourceBytes) {
    return (
      kind: _FlarkV3LeafProjectionKind.blockQuote,
      leaf: structure.source,
    );
  }

  final bulletList = structure.bulletList;
  final bulletListProjection = query.bulletListProjection;
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.bulletList &&
      bulletList != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.blockQuoteProjection == null &&
      query.orderedListProjection == null &&
      bulletListProjection != null &&
      _sameSourceSpan(bulletListProjection.source, structure.source) &&
      bulletListProjection.sourceRevision == query.sourceRevision &&
      !bulletListProjection.selectedItem.isEmpty &&
      bulletListProjection.selectedItem.content.endUtf8 -
              bulletListProjection.selectedItem.content.startUtf8 <=
          FlarkV3InlineFacts.maximumWholeLeafSourceBytes) {
    return (
      kind: _FlarkV3LeafProjectionKind.bulletListItemInline,
      leaf: bulletListProjection.selectedItem.content,
    );
  }
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.bulletList &&
      bulletList != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.pointPath == null &&
      query.blockQuoteProjection == null &&
      query.bulletListProjection == null &&
      query.orderedListProjection == null &&
      _sameSourceSpan(structure.visibleSource, projection.projectedSource) &&
      projection.runCount == bulletList.itemCount) {
    return (
      kind: _FlarkV3LeafProjectionKind.bulletListItemProjection,
      leaf: structure.source,
    );
  }

  final orderedList = structure.orderedList;
  final orderedListProjection = query.orderedListProjection;
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.orderedList &&
      orderedList != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.blockQuoteProjection == null &&
      query.bulletListProjection == null &&
      orderedListProjection != null &&
      _sameSourceSpan(orderedListProjection.source, structure.source) &&
      orderedListProjection.sourceRevision == query.sourceRevision &&
      !orderedListProjection.selectedItem.isEmpty &&
      orderedListProjection.selectedItem.content.endUtf8 -
              orderedListProjection.selectedItem.content.startUtf8 <=
          FlarkV3InlineFacts.maximumWholeLeafSourceBytes) {
    return (
      kind: _FlarkV3LeafProjectionKind.orderedListItemInline,
      leaf: orderedListProjection.selectedItem.content,
    );
  }
  if (!inlineOnly &&
      structure.kind == FlarkV3DocumentStructureKind.orderedList &&
      orderedList != null &&
      query.inlineFacts == null &&
      query.indentedCodeProjection == null &&
      query.pointPath == null &&
      query.blockQuoteProjection == null &&
      query.bulletListProjection == null &&
      query.orderedListProjection == null &&
      _sameSourceSpan(structure.visibleSource, projection.projectedSource) &&
      projection.runCount == orderedList.itemCount) {
    return (
      kind: _FlarkV3LeafProjectionKind.orderedListItemProjection,
      leaf: structure.source,
    );
  }
  return null;
}

FlarkV3InlineDemandDisposition _inlineDemandDisposition(
  FlarkV3LeafProjectionDemandDisposition disposition,
) => switch (disposition) {
  FlarkV3LeafProjectionDemandDisposition.scheduled =>
    FlarkV3InlineDemandDisposition.scheduled,
  FlarkV3LeafProjectionDemandDisposition.coalesced =>
    FlarkV3InlineDemandDisposition.coalesced,
  FlarkV3LeafProjectionDemandDisposition.notReady =>
    FlarkV3InlineDemandDisposition.notReady,
  FlarkV3LeafProjectionDemandDisposition.stale =>
    FlarkV3InlineDemandDisposition.stale,
  FlarkV3LeafProjectionDemandDisposition.notApplicable =>
    FlarkV3InlineDemandDisposition.notApplicable,
  FlarkV3LeafProjectionDemandDisposition.retryLimitReached =>
    FlarkV3InlineDemandDisposition.retryLimitReached,
};

final class _FlarkV3RecursiveGreenProjectionDemandKey {
  const _FlarkV3RecursiveGreenProjectionDemandKey({
    required this.structuralAck,
    required this.ownerFrameId,
    required this.target,
  });

  final FlarkV3StructuralAck structuralAck;
  final BigInt ownerFrameId;
  final FlarkV3InlineRefinementTarget target;

  @override
  bool operator ==(Object other) =>
      other is _FlarkV3RecursiveGreenProjectionDemandKey &&
      other.structuralAck == structuralAck &&
      other.ownerFrameId == ownerFrameId &&
      other.target == target;

  @override
  int get hashCode => Object.hash(structuralAck, ownerFrameId, target);
}

final class _FlarkV3LeafProjectionDemandKey {
  const _FlarkV3LeafProjectionDemandKey({
    required this.structuralAck,
    required this.kind,
    required this.leaf,
    required this.requestPositionUtf16,
    required this.requestAffinity,
  });

  final FlarkV3StructuralAck structuralAck;
  final _FlarkV3LeafProjectionKind kind;
  final FlarkV3SourceSpan leaf;
  final int? requestPositionUtf16;
  final FlarkV3InlinePointAffinity? requestAffinity;

  @override
  bool operator ==(Object other) =>
      other is _FlarkV3LeafProjectionDemandKey &&
      other.structuralAck == structuralAck &&
      other.kind == kind &&
      _sameSourceSpan(other.leaf, leaf) &&
      other.requestPositionUtf16 == requestPositionUtf16 &&
      other.requestAffinity == requestAffinity;

  @override
  int get hashCode => Object.hash(
    structuralAck,
    kind,
    leaf.startUtf8,
    leaf.endUtf8,
    leaf.startUtf16,
    leaf.endUtf16,
    requestPositionUtf16,
    requestAffinity,
  );
}

final class _FlarkV3ViewportPresentationDemandKey {
  const _FlarkV3ViewportPresentationDemandKey({
    required this.structuralAck,
    required this.demand,
  });

  final FlarkV3StructuralAck structuralAck;
  final FlarkV3ViewportPresentationDemand demand;

  @override
  bool operator ==(Object other) =>
      other is _FlarkV3ViewportPresentationDemandKey &&
      other.structuralAck == structuralAck &&
      other.demand == demand;

  @override
  int get hashCode => Object.hash(structuralAck, demand);
}

bool _viewportAckMatchesDemand(
  FlarkV3ViewportPresentationAck ack, {
  required FlarkV3StructuralAck baseAck,
  required FlarkV3ViewportPresentationDemand demand,
  required FlarkV3ViewportPresentationMetricRange requestedRange,
}) =>
    ack.baseAck == baseAck &&
    ack.binding.requestedRange == requestedRange &&
    ack.binding.start.blockOrdinal ==
        FlarkV3ProtocolU64.fromU32(demand.startBlockOrdinal) &&
    ack.binding.start.utf8Offset == requestedRange.startUtf8 &&
    ack.binding.start.utf16Offset == requestedRange.startUtf16;

FlarkV3ViewportPresentationUnavailableReason?
_viewportPresentationUnavailableReason(int? reason) => switch (reason) {
  null => null,
  FlarkV3ParserViewportPresentationUnavailable.retryableBusyReason =>
    FlarkV3ViewportPresentationUnavailableReason.retryableBusy,
  FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason =>
    FlarkV3ViewportPresentationUnavailableReason.budgetExceeded,
  FlarkV3ParserViewportPresentationUnavailable.derivationUnavailableReason =>
    FlarkV3ViewportPresentationUnavailableReason.derivationUnavailable,
  FlarkV3ParserViewportPresentationUnavailable.hostRejectedReason =>
    FlarkV3ViewportPresentationUnavailableReason.hostRejected,
  _ => FlarkV3ViewportPresentationUnavailableReason.hostRejected,
};

bool _sameSourceSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

FlarkV3DocumentPendingReason _pendingReason(
  FlarkV3StablePendingReason reason,
) => switch (reason) {
  FlarkV3StablePendingReason.initialSnapshot =>
    FlarkV3DocumentPendingReason.initializing,
  FlarkV3StablePendingReason.sourceUncertified =>
    FlarkV3DocumentPendingReason.sourceChanged,
  FlarkV3StablePendingReason.sourceAdvanced =>
    FlarkV3DocumentPendingReason.sourceChanged,
  FlarkV3StablePendingReason.storeUnsynchronized =>
    FlarkV3DocumentPendingReason.structurePending,
  FlarkV3StablePendingReason.publicationPending =>
    FlarkV3DocumentPendingReason.structurePending,
};

FlarkV3DocumentQueryGapReason _gapReason(FlarkV3HostSourceGapReason? reason) =>
    switch (reason) {
      FlarkV3HostSourceGapReason.openDepthLimit =>
        FlarkV3DocumentQueryGapReason.openDepthLimit,
      FlarkV3HostSourceGapReason.encodedByteLimit =>
        FlarkV3DocumentQueryGapReason.encodedByteLimit,
      FlarkV3HostSourceGapReason.leafLimit =>
        FlarkV3DocumentQueryGapReason.leafLimit,
      FlarkV3HostSourceGapReason.treeNodeLimit =>
        FlarkV3DocumentQueryGapReason.treeNodeLimit,
      FlarkV3HostSourceGapReason.undecodableClosure =>
        FlarkV3DocumentQueryGapReason.undecodableClosure,
      FlarkV3HostSourceGapReason.unavailableFacts ||
      null => FlarkV3DocumentQueryGapReason.unavailableFacts,
    };

final Random _documentSessionRandom = Random.secure();

FlarkV3DocumentSessionId _newDocumentSessionId() => FlarkV3DocumentSessionId(
  0x464C4B33, // `FLK3`; the native host rejects an all-zero identity.
  _documentSessionRandom.nextInt(0x100000000),
  _documentSessionRandom.nextInt(0x100000000),
  _documentSessionRandom.nextInt(0x100000000),
);

void _retireUnattachedHostStore(FlarkV3HostStore hostStore) {
  final close = hostStore.close();
  if (close is FlarkV3HostRejected<FlarkV3HostUnit>) return;

  // No parser was attached successfully, so this host cannot own a
  // document-sized publication. One maximally bounded poll is sufficient to
  // observe its closed state and lets native adapters detach/remove their
  // generation-safe registry handle.
  hostStore.poll(
    FlarkV3HostWorkGrant(
      inspectBytes: FlarkDocumentWorkProfile.prototype.maximumHostInspectBytes,
      copyBytes: FlarkDocumentWorkProfile.prototype.maximumHostCopyBytes,
      transitions: FlarkDocumentWorkProfile.prototype.maximumHostTransitions,
    ),
  );
}

final class _FlarkV3DocumentBlockRangeContinuation
    implements FlarkV3DocumentBlockRangeContinuation {
  const _FlarkV3DocumentBlockRangeContinuation({
    required this.owner,
    required this.sourceVersion,
    required this.structuralAck,
    required this.structureGeneration,
    required this.requestedRange,
    required this.hostContinuation,
  });

  final Object owner;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck structuralAck;
  @override
  final int structureGeneration;
  final FlarkV3MetricRange requestedRange;
  final FlarkV3HostBlockRangeContinuation hostContinuation;

  @override
  int get sourceRevision => sourceVersion.revision;

  @override
  FlarkV3SourceSpan get requestedSource =>
      FlarkV3DocumentQueryDecoder.metricRange(requestedRange);
}

/// Unstable adapter assembly seam below the ordinary document facade.
///
/// Official adapters and integration tests may attach custom source/session
/// ownership here. Applications should use [FlarkV3DocumentRuntime.open].
/// This type is exported only by `package:flark/flark_adapter.dart`.
final class FlarkV3DocumentRuntimeAdapter {
  const FlarkV3DocumentRuntimeAdapter._();

  /// Borrows the managed runtime for one official presentation adapter.
  ///
  /// The lease cannot close the runtime. It keeps source mutation on the
  /// runtime-owned executor path while allowing the adapter to query the
  /// already-owned document session and observe deduplicated progress.
  static FlarkV3DocumentRuntimeAdapterLease borrow(
    FlarkV3DocumentRuntime runtime, {
    bool leafProjectionDemandOwner = false,
    bool inlineDemandOwner = false,
    bool viewportPresentationDemandOwner = false,
  }) {
    final leafOwner = leafProjectionDemandOwner || inlineDemandOwner
        ? Object()
        : null;
    final viewportOwner = viewportPresentationDemandOwner ? Object() : null;
    if (leafOwner != null) {
      runtime._claimLeafProjectionDemandOwnership(leafOwner);
    }
    try {
      if (viewportOwner != null) {
        runtime._claimViewportPresentationDemandOwnership(viewportOwner);
      }
    } catch (_) {
      if (leafOwner != null) {
        runtime._releaseLeafProjectionDemandOwnership(leafOwner);
      }
      rethrow;
    }
    return FlarkV3DocumentRuntimeAdapterLease._(
      runtime,
      leafProjectionDemandOwner: leafOwner,
      viewportPresentationDemandOwner: viewportOwner,
    );
  }

  static Future<FlarkV3DocumentRuntime> attach({
    required FlarkDocumentSession document,
    required FlarkV3ParserSessionBinding parserBinding,
    String? nativeLibraryPath,
    FlarkV3WebRuntimeAssets? webAssets,
  }) async {
    final support = FlarkV3DocumentRuntime.platformSupport;
    if (!support.supported) {
      throw FlarkV3RuntimeUnavailable(
        endpoint: support.endpoint,
        reason: support.unavailableReason ?? 'No v3 endpoint is available.',
      );
    }

    final platformEndpoint = await startFlarkV3DefaultPlatformEndpoint(
      nativeLibraryPath: nativeLibraryPath,
      webAssets: webAssets,
    );
    return FlarkV3DocumentRuntimePlatformAttachment.attach(
      document: document,
      parserBinding: parserBinding,
      platformEndpoint: platformEndpoint,
    );
  }
}

/// Narrow, releasable view used by official adapters.
///
/// Releasing this lease detaches only the adapter. The Dart runtime remains
/// the sole owner of parser execution and must be closed by its caller.
final class FlarkV3DocumentRuntimeAdapterLease {
  FlarkV3DocumentRuntimeAdapterLease._(
    this._runtime, {
    required Object? leafProjectionDemandOwner,
    required Object? viewportPresentationDemandOwner,
  }) : _leafProjectionDemandOwner = leafProjectionDemandOwner,
       _viewportPresentationDemandOwner = viewportPresentationDemandOwner;

  final FlarkV3DocumentRuntime _runtime;
  final Object? _leafProjectionDemandOwner;
  final Object? _viewportPresentationDemandOwner;
  bool _released = false;

  FlarkDocumentSession get document {
    _requireAttached();
    return _runtime._document;
  }

  Stream<FlarkV3DocumentRuntimeStatus> get statuses {
    _requireAttached();
    return _runtime.statuses;
  }

  FlarkDocumentEditReceipt apply(FlarkV3SourceTransaction transaction) {
    _requireAttached();
    return _runtime._applyForAdapter(transaction);
  }

  FlarkV3DocumentQueryResult queryAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    FlarkV3DocumentQueryBudget budget = const FlarkV3DocumentQueryBudget(),
  }) {
    _requireAttached();
    return _runtime.queryAtUtf16(
      positionUtf16,
      affinity: affinity,
      budget: budget,
    );
  }

  FlarkV3DocumentBlockRangeResult queryBlockRange(
    int startUtf16,
    int endUtf16, {
    FlarkV3DocumentBlockRangeBudget budget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) {
    _requireAttached();
    return _runtime.queryBlockRange(startUtf16, endUtf16, budget: budget);
  }

  FlarkV3DocumentBlockRangeResult continueBlockRange(
    FlarkV3DocumentBlockRangeContinuation continuation, {
    FlarkV3DocumentBlockRangeBudget budget =
        const FlarkV3DocumentBlockRangeBudget(),
  }) {
    _requireAttached();
    return _runtime.continueBlockRange(continuation, budget: budget);
  }

  FlarkV3DocumentOrdinalWindowResult queryBlockOrdinalWindow(
    FlarkV3DocumentOrdinalWindowDemand demand, {
    FlarkV3DocumentOrdinalWindowBudget budget =
        const FlarkV3DocumentOrdinalWindowBudget(),
  }) {
    _requireAttached();
    return _runtime.queryBlockOrdinalWindow(demand, budget: budget);
  }

  FlarkV3InlineDemandDisposition ensureInlineAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    required FlarkV3DocumentStructuralQuery structuralQuery,
  }) {
    _requireAttached();
    final owner =
        _leafProjectionDemandOwner ??
        (throw StateError(
          'This adapter lease does not own inline refinement demand.',
        ));
    return _runtime._ensureInlineForQuery(
      positionUtf16,
      owner: owner,
      affinity: affinity,
      query: structuralQuery,
    );
  }

  /// Ensures the selected exact leaf's parser-authored display payload.
  ///
  /// The current grammar supports whole-leaf inline facts, indented-code
  /// physical-line recipes, and bounded block-quote path projections. This
  /// method only schedules existing parser work; it never recognizes Markdown
  /// in Dart.
  FlarkV3LeafProjectionDemandDisposition ensureActiveProjectionAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    required FlarkV3DocumentQueryResult query,
  }) {
    _requireAttached();
    final owner =
        _leafProjectionDemandOwner ??
        (throw StateError(
          'This adapter lease does not own leaf-projection refinement demand.',
        ));
    return switch (query) {
      FlarkV3DocumentStructuralQuery() =>
        _runtime._ensureLeafProjectionForQuery(
          positionUtf16,
          owner: owner,
          affinity: affinity,
          query: query,
          inlineOnly: false,
        ),
      FlarkV3RecursiveGreenPointQuery() =>
        _runtime._ensureRecursiveGreenPresentationForQuery(
          positionUtf16,
          owner: owner,
          affinity: affinity,
          query: query,
        ),
      _ => FlarkV3LeafProjectionDemandDisposition.notApplicable,
    };
  }

  /// Source-compatible structural-query entry point.
  FlarkV3LeafProjectionDemandDisposition ensureLeafProjectionAtUtf16(
    int positionUtf16, {
    FlarkV3DocumentQueryAffinity affinity =
        FlarkV3DocumentQueryAffinity.downstream,
    required FlarkV3DocumentStructuralQuery structuralQuery,
  }) {
    return ensureActiveProjectionAtUtf16(
      positionUtf16,
      affinity: affinity,
      query: structuralQuery,
    );
  }

  /// Coalesces one exact passive viewport request into the parser lane.
  ///
  /// The request is bounded by the production viewport profile. Focused leaf
  /// refinement remains higher priority and may preempt an unsent or active
  /// passive attempt.
  FlarkV3ViewportPresentationDemandReceipt ensureViewportPresentation(
    FlarkV3ViewportPresentationDemand demand,
  ) {
    _requireAttached();
    final owner =
        _viewportPresentationDemandOwner ??
        (throw StateError(
          'This adapter lease does not own passive viewport demand.',
        ));
    return _runtime._ensureViewportPresentation(demand, owner: owner);
  }

  /// Queries only the exact installed aggregate page for [demand].
  ///
  /// The result carries the current structural ACK, structure generation, and
  /// certified source document needed by the viewport page materializer.
  /// Stale or unavailable authority returns a typed fail-closed result.
  FlarkV3ViewportPresentationPageResult queryViewportPresentation(
    FlarkV3ViewportPresentationDemand demand, {
    int maximumEncodedBytes = 256 * 1024,
  }) {
    _requireAttached();
    return _runtime._queryViewportPresentation(
      demand,
      maximumEncodedBytes: maximumEncodedBytes,
    );
  }

  void release() {
    if (_released) return;
    _released = true;
    final owner = _leafProjectionDemandOwner;
    if (owner != null) {
      _runtime._releaseLeafProjectionDemandOwnership(owner);
    }
    final viewportOwner = _viewportPresentationDemandOwner;
    if (viewportOwner != null) {
      _runtime._releaseViewportPresentationDemandOwnership(viewportOwner);
    }
  }

  void _requireAttached() {
    if (_released) {
      throw StateError('The Flark v3 runtime adapter lease was released.');
    }
  }
}

/// Package-internal assembly boundary between the Dart runtime and one
/// platform endpoint.
///
/// Native and Web factories share this ownership path. Keeping endpoint
/// construction outside the managed runtime also lets protocol integration
/// tests exercise the full wire/driver/facade chain without a test-only mode
/// in a production endpoint.
final class FlarkV3DocumentRuntimePlatformAttachment {
  const FlarkV3DocumentRuntimePlatformAttachment._();

  /// Exact parser authority implemented by the current M1.1 production
  /// endpoint. Keeping this below the public facade prevents applications from
  /// accepting a worker-selected grammar/profile while still making real
  /// structural publication available to every managed runtime.
  static final FlarkV3ParserPublicationAuthority _publicationAuthority =
      FlarkV3ParserPublicationAuthority(
        grammarRevision: flarkV3CurrentGrammarRevision,
        syntaxProfile: FlarkV3SyntaxProfileId(1),
        authorityMask: FlarkV3StructuralAuthorityMask.complete,
      );

  static Future<FlarkV3DocumentRuntime> attach({
    required FlarkDocumentSession document,
    required FlarkV3ParserSessionBinding parserBinding,
    required FlarkV3PlatformEndpointHandle platformEndpoint,
  }) async {
    final signals = _FlarkV3RuntimeSignals();
    try {
      final transport = FlarkV3WireParserTransport(
        endpoint: platformEndpoint.endpoint,
        onFailure: signals.failure,
      );
      final executor = FlarkV3SessionExecutor.attach(
        session: document,
        transport: transport,
        parserBinding: parserBinding,
        publicationAuthority: _publicationAuthority,
        onProgress: signals.progress,
        onFailure: signals.failure,
      );
      final runtime = FlarkV3DocumentRuntime._(
        document: document,
        executor: executor,
        endpointDone: signals.observeEndpoint(platformEndpoint.done),
      );
      signals.attach(runtime);
      runtime._publishStatus(force: true);
      return runtime;
    } catch (error, stackTrace) {
      platformEndpoint.endpoint.close();
      try {
        await platformEndpoint.done;
      } on Object {
        // Preserve the attachment failure. Endpoint disposal is best effort
        // because no managed runtime exists yet to surface a second failure.
      }
      Error.throwWithStackTrace(error, stackTrace);
    }
  }
}

final class _FlarkV3RuntimeSignals {
  FlarkV3DocumentRuntime? _runtime;
  (Object, StackTrace)? _pendingFailure;
  final Completer<void> _endpointDone = Completer<void>.sync();

  void attach(FlarkV3DocumentRuntime runtime) {
    _runtime = runtime;
    final pending = _pendingFailure;
    _pendingFailure = null;
    if (pending != null) runtime._recordFailure(pending.$1, pending.$2);
  }

  void progress() => _runtime?._publishStatus();

  void failure(Object error, StackTrace stackTrace) {
    final runtime = _runtime;
    if (runtime == null) {
      _pendingFailure ??= (error, stackTrace);
      return;
    }
    runtime._recordFailure(error, stackTrace);
  }

  Future<void> observeEndpoint(Future<void> done) {
    done.then(
      (_) {
        if (!_endpointDone.isCompleted) _endpointDone.complete();
      },
      onError: (Object error, StackTrace stackTrace) {
        failure(error, stackTrace);
        // The runtime records and rethrows the causal terminal failure from
        // close(). Keep this ownership future error-free so an endpoint crash
        // cannot become an unhandled asynchronous error before close begins.
        if (!_endpointDone.isCompleted) _endpointDone.complete();
      },
    );
    return _endpointDone.future;
  }
}
