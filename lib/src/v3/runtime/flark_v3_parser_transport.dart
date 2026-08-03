import '../host/host.dart';
import '../source/source.dart';

/// Exact endpoint identity carried by parser-session control traffic.
///
/// Keeping the full binding on typed lifecycle commands prevents a platform
/// adapter from accidentally applying a command or acknowledgement to a
/// different document, source owner, or worker epoch.
final class FlarkV3ParserSessionBinding {
  FlarkV3ParserSessionBinding({
    required this.documentSession,
    required this.sourceSessionIdentity,
    required this.workerGeneration,
  }) {
    _checkPositiveV1Identity(sourceSessionIdentity, 'sourceSessionIdentity');
    _checkPositiveV1Identity(workerGeneration, 'workerGeneration');
  }

  final FlarkV3DocumentSessionId documentSession;
  final int sourceSessionIdentity;
  final int workerGeneration;

  FlarkV3ParserSessionBinding nextGeneration(int generation) =>
      FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSessionIdentity,
        workerGeneration: generation,
      );

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ParserSessionBinding &&
      other.documentSession == documentSession &&
      other.sourceSessionIdentity == sourceSessionIdentity &&
      other.workerGeneration == workerGeneration;

  @override
  int get hashCode =>
      Object.hash(documentSession, sourceSessionIdentity, workerGeneration);
}

enum FlarkV3ParserOpenMode { fresh, recovery }

const int flarkV3ParserMaximumDrainTransitions = 256;
const int flarkV3ParserMaximumViewportStructuralEntries = 256;
const int flarkV3ParserMaximumViewportStoragePages = 257;
const int flarkV3ParserMaximumViewportInlineLeaves = 128;
const int flarkV3ParserMaximumViewportInlineLeafSourceBytes = 8 * 1024;
const int flarkV3ParserMaximumViewportInlineSourceBytes = 1024 * 1024;
const int flarkV3ParserMaximumViewportFactRecords = 2048;
const int flarkV3ParserMaximumViewportEncodedFrameBytes = 4 * 1024 * 1024;
const int flarkV3ParserMaximumViewportTransitions = 1000000;

/// Grammar identity compiled into the production v3 parser and host pair.
///
/// This must advance whenever parser-authored restart or presentation
/// authority changes. Platform host defaults and the managed publication
/// authority derive from this value so one process cannot silently assemble
/// mismatched parser and host generations.
const int flarkV3CurrentGrammarRevision = 9;

/// Product default for one bounded viewport-presentation window.
///
/// Structural and inline-leaf admission intentionally share this capacity:
/// every structural entry in a requested window may be an inline-bearing
/// leaf. View adapters use the same value for their default window demand so
/// an ordinary page never relies on budget-exhaustion retries to become
/// admissible.
const int flarkV3DefaultViewportPresentationEntryCapacity = 64;

/// Parser build/profile authority admitted by one document driver.
///
/// A null authority on the driver means source synchronization only. This
/// value never guesses a profile from worker output: all three fields must
/// match exactly before a structural begin reaches the host store.
final class FlarkV3ParserPublicationAuthority {
  FlarkV3ParserPublicationAuthority({
    required this.grammarRevision,
    required this.syntaxProfile,
    required this.authorityMask,
  }) {
    if (grammarRevision <= 0 || grammarRevision > flarkV3TransportV1Maximum) {
      throw RangeError.range(
        grammarRevision,
        1,
        flarkV3TransportV1Maximum,
        'grammarRevision',
      );
    }
  }

  final int grammarRevision;
  final FlarkV3SyntaxProfileId syntaxProfile;
  final FlarkV3StructuralAuthorityMask authorityMask;

  bool admits(FlarkV3HostOfferBegin begin) =>
      begin.schema == FlarkV3HostOfferBegin.supportedManifestSchema &&
      begin.grammarRevision == grammarRevision &&
      begin.syntaxProfile == syntaxProfile &&
      begin.authorityMask == authorityMask;
}

/// Receives one credited parser event.
///
/// The callback is intentionally synchronous. Implementations must not emit a
/// second event until the driver returns an exact [FlarkV3ParserEventReceipt].
typedef FlarkV3ParserEventCallback = void Function(FlarkV3ParserEvent event);

/// Platform-neutral boundary to one long-lived parser worker.
///
/// Native isolate ports and Web workers implement this same contract. Values
/// are typed here; platform adapters own serialization and transfer details.
abstract interface class FlarkV3ParserTransport {
  /// Installs the only parser-event callback for this transport.
  void bind(FlarkV3ParserEventCallback onEvent);

  /// Sends one bounded command without waiting for parser work to complete.
  void send(FlarkV3ParserCommand command);

  /// Releases transport resources without waiting for document-sized work.
  void close();
}

sealed class FlarkV3ParserCommand {
  const FlarkV3ParserCommand();
}

/// Establishes the exact endpoint binding before any source or publication
/// authority is transferred.
final class FlarkV3ParserOpen extends FlarkV3ParserCommand {
  const FlarkV3ParserOpen({required this.binding, required this.mode});

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserOpenMode mode;
}

/// Transfers the only live source-replica credit to the parser worker.
final class FlarkV3ParserSynchronizeSource extends FlarkV3ParserCommand {
  FlarkV3ParserSynchronizeSource(this.lease) {
    _checkSourceLeaseV1(lease);
  }

  final FlarkV3SourceWorkerSyncLease lease;
}

enum FlarkV3ParserEventDisposition { accepted, stale, rejected }

/// Returns the exact event credit to the parser worker.
final class FlarkV3ParserEventReceipt extends FlarkV3ParserCommand {
  FlarkV3ParserEventReceipt({
    required this.eventId,
    this.binding,
    int? workerGeneration,
    required this.disposition,
    this.sourceSync,
    this.sourceCertification,
  }) : workerGeneration = binding?.workerGeneration ?? workerGeneration ?? 0 {
    _checkPositiveV1Identity(eventId, 'eventId');
    _checkPositiveV1Identity(this.workerGeneration, 'workerGeneration');
    if (binding != null &&
        workerGeneration != null &&
        binding!.workerGeneration != workerGeneration) {
      throw ArgumentError(
        'Receipt generation and exact parser binding must agree.',
      );
    }
    if (sourceSync != null && sourceCertification != null) {
      throw ArgumentError(
        'One event receipt cannot acknowledge source sync and certification.',
      );
    }
    if (sourceCertification != null &&
        disposition != FlarkV3ParserEventDisposition.accepted) {
      throw ArgumentError(
        'A canonical promotion proof requires an accepted event receipt.',
      );
    }
  }

  final int eventId;

  /// Exact endpoint identity for credited wire events.
  ///
  /// The field remains nullable for direct in-process transports. The wire
  /// transport requires it and encodes every receipt through the one canonical
  /// parser-session receipt schema.
  final FlarkV3ParserSessionBinding? binding;
  final int workerGeneration;
  final FlarkV3ParserEventDisposition disposition;
  final FlarkV3SourceWorkerSyncAckReceipt? sourceSync;
  final FlarkV3CanonicalSourcePromotionProof? sourceCertification;
}

/// Starts a clean parser replica generation after the prior worker failed.
final class FlarkV3ParserRestart extends FlarkV3ParserCommand {
  FlarkV3ParserRestart(this.workerGeneration) {
    _checkPositiveV1Identity(workerGeneration, 'workerGeneration');
  }

  final int workerGeneration;
}

/// Revokes parser work derived from an older UI revision.
///
/// This is deliberately distinct from a host-poll rejection: supersession is
/// caller-side source authority, not a fabricated host-store outcome.
final class FlarkV3ParserSupersede extends FlarkV3ParserCommand {
  FlarkV3ParserSupersede({
    required this.binding,
    required this.targetUiRevision,
  }) {
    _checkV1Lane(targetUiRevision, 'targetUiRevision');
  }

  final FlarkV3ParserSessionBinding binding;
  final int targetUiRevision;
}

enum FlarkV3InlinePointAffinity { before, after }

/// Parser-certified projection target selected by the presentation adapter.
///
/// [automatic] retains the existing structural-leaf dispatch. The list-item
/// target is admitted only after an exact BulletList projection identifies the
/// selected content range; it never asks either Dart or Rust host code to
/// recognize list syntax.
enum FlarkV3InlineRefinementTarget {
  automatic,
  bulletListItemInline,
  bulletListItemProjection,
  orderedListItemInline,
  orderedListItemProjection,

  /// Experimental production-path checkpoint backed by the recursive-Green
  /// Paragraph owner fence. It deliberately reuses the normal sidecar output.
  recursiveGreenParagraph,

  /// Requests the parser-certified block-quote projection at the source point.
  blockQuoteProjection,
}

/// Requests parser-certified inline facts for the structural leaf at one
/// exact source point.
///
/// The demand is revision-bound and monotonic. It never changes canonical
/// structure; a later generation supersedes an older hot-inline request.
final class FlarkV3ParserRefineInline extends FlarkV3ParserCommand {
  FlarkV3ParserRefineInline({
    required this.binding,
    required this.refinementGeneration,
    required this.sourceVersion,
    required this.baseAck,
    required this.byteOffset,
    required this.utf16Offset,
    required this.affinity,
    this.target = FlarkV3InlineRefinementTarget.automatic,
  }) {
    _checkPositiveV1Identity(refinementGeneration, 'refinementGeneration');
    _checkV1Lane(byteOffset, 'byteOffset');
    _checkV1Lane(utf16Offset, 'utf16Offset');
    if (sourceVersion.documentSession != binding.documentSession ||
        baseAck.sourceVersion != sourceVersion) {
      throw ArgumentError(
        'Inline refinement must bind one exact session source and base ACK.',
      );
    }
    if (byteOffset > sourceVersion.metric.bytes ||
        utf16Offset > sourceVersion.metric.utf16) {
      throw RangeError('Inline refinement point exceeds its source version.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final int refinementGeneration;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck baseAck;
  final int byteOffset;
  final int utf16Offset;
  final FlarkV3InlinePointAffinity affinity;
  final FlarkV3InlineRefinementTarget target;
}

/// Whole-batch bounds for one passive viewport-presentation page.
final class FlarkV3ParserViewportPresentationLimits {
  FlarkV3ParserViewportPresentationLimits({
    this.maximumStructuralEntries =
        flarkV3DefaultViewportPresentationEntryCapacity,
    this.maximumStoragePages = 25,
    this.maximumInlineLeaves = flarkV3DefaultViewportPresentationEntryCapacity,
    this.maximumInlineLeafSourceBytes = 8 * 1024,
    this.maximumInlineSourceBytes = 64 * 1024,
    this.maximumFactRecords = 2048,
    // Admission covers the authenticated worst-case wrapper, directory,
    // child-node, and terminal frames for the selected projections, not only
    // the compact bytes eventually transferred. The default 64-leaf profile
    // exceeds the former 512 KiB proof after the authenticated inline bundle
    // gained a distinct value root. Two MiB covers the conservative
    // 64-leaf/2,048-fact transfer closure while remaining below the 4 MiB
    // product maximum. Keep this private stream ceiling distinct from the
    // 256 KiB default public-page query bound.
    this.maximumEncodedFrameBytes = 2 * 1024 * 1024,
    this.maximumParserTransitions = 250000,
  }) {
    for (final (name, value, maximum) in [
      (
        'maximumStructuralEntries',
        maximumStructuralEntries,
        flarkV3ParserMaximumViewportStructuralEntries,
      ),
      (
        'maximumStoragePages',
        maximumStoragePages,
        flarkV3ParserMaximumViewportStoragePages,
      ),
      (
        'maximumInlineLeaves',
        maximumInlineLeaves,
        flarkV3ParserMaximumViewportInlineLeaves,
      ),
      (
        'maximumInlineLeafSourceBytes',
        maximumInlineLeafSourceBytes,
        flarkV3ParserMaximumViewportInlineLeafSourceBytes,
      ),
      (
        'maximumInlineSourceBytes',
        maximumInlineSourceBytes,
        flarkV3ParserMaximumViewportInlineSourceBytes,
      ),
      (
        'maximumFactRecords',
        maximumFactRecords,
        flarkV3ParserMaximumViewportFactRecords,
      ),
      (
        'maximumEncodedFrameBytes',
        maximumEncodedFrameBytes,
        flarkV3ParserMaximumViewportEncodedFrameBytes,
      ),
      (
        'maximumParserTransitions',
        maximumParserTransitions,
        flarkV3ParserMaximumViewportTransitions,
      ),
    ]) {
      if (value <= 0 || value > maximum) {
        throw RangeError.range(value, 1, maximum, name);
      }
    }
    if (maximumInlineLeaves > maximumStructuralEntries) {
      throw RangeError(
        'Viewport inline-leaf capacity cannot exceed structural capacity.',
      );
    }
    if (maximumInlineLeafSourceBytes > maximumInlineSourceBytes) {
      throw RangeError(
        'Per-leaf source capacity cannot exceed aggregate source capacity.',
      );
    }
  }

  final int maximumStructuralEntries;
  final int maximumStoragePages;
  final int maximumInlineLeaves;
  final int maximumInlineLeafSourceBytes;
  final int maximumInlineSourceBytes;
  final int maximumFactRecords;
  final int maximumEncodedFrameBytes;
  final int maximumParserTransitions;
}

/// Requests one atomically installable passive presentation page from a
/// parser-authenticated structural range walk.
final class FlarkV3ParserPresentViewport extends FlarkV3ParserCommand {
  FlarkV3ParserPresentViewport({
    required this.binding,
    required this.viewportGeneration,
    required this.sourceVersion,
    required this.baseAck,
    required this.requestedStartUtf8,
    required this.requestedStartUtf16,
    required this.requestedEndUtf8,
    required this.requestedEndUtf16,
    required this.startBlockOrdinal,
    required this.startUtf8,
    required this.startUtf16,
    required this.limits,
  }) {
    _checkPositiveV1Identity(viewportGeneration, 'viewportGeneration');
    for (final (name, value) in [
      ('requestedStartUtf8', requestedStartUtf8),
      ('requestedStartUtf16', requestedStartUtf16),
      ('requestedEndUtf8', requestedEndUtf8),
      ('requestedEndUtf16', requestedEndUtf16),
      ('startUtf8', startUtf8),
      ('startUtf16', startUtf16),
    ]) {
      _checkV1Lane(value, name);
    }
    if (sourceVersion.documentSession != binding.documentSession ||
        baseAck.sourceVersion != sourceVersion) {
      throw ArgumentError(
        'Viewport presentation must bind one exact session source and base ACK.',
      );
    }
    if (requestedStartUtf8 >= requestedEndUtf8 ||
        requestedStartUtf16 >= requestedEndUtf16 ||
        requestedEndUtf8 > sourceVersion.metric.bytes ||
        requestedEndUtf16 > sourceVersion.metric.utf16 ||
        startUtf8 != requestedStartUtf8 ||
        startUtf16 != requestedStartUtf16) {
      throw RangeError('Viewport presentation source range is invalid.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final int viewportGeneration;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck baseAck;
  final int requestedStartUtf8;
  final int requestedStartUtf16;
  final int requestedEndUtf8;
  final int requestedEndUtf16;
  final FlarkV3ProtocolU64 startBlockOrdinal;
  final int startUtf8;
  final int startUtf16;
  final FlarkV3ParserViewportPresentationLimits limits;
}

/// Begins worker shutdown. A null generation means no source lease was sent.
final class FlarkV3ParserBeginClose extends FlarkV3ParserCommand {
  FlarkV3ParserBeginClose(this.workerGeneration) {
    final generation = workerGeneration;
    if (generation != null) {
      _checkPositiveV1Identity(generation, 'workerGeneration');
    }
  }

  final int? workerGeneration;
}

/// Grants bounded parser-side retirement work during close.
final class FlarkV3ParserDrainGrant extends FlarkV3ParserCommand {
  FlarkV3ParserDrainGrant({
    required this.binding,
    required this.drainId,
    required this.maximumTransitions,
  }) {
    _checkPositiveV1Identity(drainId, 'drainId');
    if (maximumTransitions <= 0 ||
        maximumTransitions > flarkV3ParserMaximumDrainTransitions) {
      throw RangeError.range(
        maximumTransitions,
        1,
        flarkV3ParserMaximumDrainTransitions,
        'maximumTransitions',
      );
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final int drainId;
  final int maximumTransitions;
}

enum FlarkV3ParserHostPollPhase { packetCredit, commit, abort }

/// Exact cause of one host-poll sequence.
///
/// [pollTicket] is the credited publication event ID that initiated the host
/// work. Pending host polls retain the same ticket; only the terminal outcome
/// is returned to the worker.
final class FlarkV3ParserHostPollTicket {
  FlarkV3ParserHostPollTicket({
    required this.binding,
    required this.pollTicket,
    required this.offerId,
    required this.phase,
  }) {
    _checkPositiveV1Identity(pollTicket, 'pollTicket');
  }

  final FlarkV3ParserSessionBinding binding;
  final int pollTicket;
  final FlarkV3OfferId offerId;
  final FlarkV3ParserHostPollPhase phase;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ParserHostPollTicket &&
      other.binding == binding &&
      other.pollTicket == pollTicket &&
      other.offerId == offerId &&
      other.phase == phase;

  @override
  int get hashCode => Object.hash(binding, pollTicket, offerId, phase);
}

/// Returns a terminal host-poll result to the parser worker.
///
/// A committed result carries the exact structural ACK, but sending it does
/// not prove worker receipt. The worker must return a credited
/// [FlarkV3ParserPublicationDeliveryAcknowledged] event before the host base is
/// acknowledged.
final class FlarkV3ParserHostPollCompleted extends FlarkV3ParserCommand {
  FlarkV3ParserHostPollCompleted({
    required this.ticket,
    required this.outcome,
  }) {
    final valid = switch ((ticket.phase, outcome)) {
      (
        FlarkV3ParserHostPollPhase.packetCredit,
        FlarkV3HostPacketCredit(:final offerId),
      ) =>
        offerId == ticket.offerId,
      (FlarkV3ParserHostPollPhase.commit, FlarkV3HostCommitted()) => true,
      (
        FlarkV3ParserHostPollPhase.abort,
        FlarkV3HostAbortComplete(:final offerId),
      ) =>
        offerId == ticket.offerId,
      _ => false,
    };
    if (!valid) {
      throw ArgumentError(
        'Host-poll outcome does not match its causal publication ticket.',
      );
    }
  }

  final FlarkV3ParserHostPollTicket ticket;
  final FlarkV3HostPollOutcome outcome;

  FlarkV3ParserSessionBinding get binding => ticket.binding;
  int get pollTicket => ticket.pollTicket;
}

/// Reports that a host poll failed without manufacturing a completion.
final class FlarkV3ParserHostPollRejected extends FlarkV3ParserCommand {
  const FlarkV3ParserHostPollRejected({
    required this.ticket,
    required this.reason,
  });

  final FlarkV3ParserHostPollTicket ticket;
  final FlarkV3HostRejectReason reason;

  FlarkV3ParserSessionBinding get binding => ticket.binding;
  int get pollTicket => ticket.pollTicket;
}

sealed class FlarkV3ParserEvent {
  FlarkV3ParserEvent({required this.eventId, required this.workerGeneration}) {
    if (eventId <= 0 ||
        eventId > flarkV3TransportV1Maximum ||
        workerGeneration <= 0 ||
        workerGeneration > flarkV3TransportV1Maximum) {
      throw RangeError('Parser event identity must fit a positive v1 lane.');
    }
  }

  final int eventId;
  final int workerGeneration;
}

/// A publication event tied to one exact document/source/worker endpoint.
sealed class FlarkV3ParserPublicationEvent extends FlarkV3ParserEvent {
  FlarkV3ParserPublicationEvent({required super.eventId, required this.binding})
    : super(workerGeneration: binding.workerGeneration);

  final FlarkV3ParserSessionBinding binding;
}

/// Exact acknowledgement that one fresh or recovery binding is established.
final class FlarkV3ParserOpened extends FlarkV3ParserEvent {
  FlarkV3ParserOpened({
    required super.eventId,
    required this.binding,
    required this.mode,
  }) : super(workerGeneration: binding.workerGeneration);

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserOpenMode mode;
}

/// The worker applied one exact source-sync lease.
final class FlarkV3ParserSourceSynchronized extends FlarkV3ParserEvent {
  FlarkV3ParserSourceSynchronized({
    required super.eventId,
    required super.workerGeneration,
    required this.acknowledgement,
  }) {
    if (acknowledgement.workerGeneration != workerGeneration) {
      throw ArgumentError(
        'Source acknowledgement and event generations must agree.',
      );
    }
  }

  final FlarkV3SourceWorkerSyncAcknowledgement acknowledgement;
}

/// Transfers one bounded canonical global SourceFacts page.
final class FlarkV3ParserSourceFactsPage extends FlarkV3ParserEvent {
  FlarkV3ParserSourceFactsPage({
    required super.eventId,
    required this.binding,
    required this.page,
  }) : super(workerGeneration: binding.workerGeneration) {
    final lineage = page.lineage;
    if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
        lineage.workerGeneration != binding.workerGeneration) {
      throw ArgumentError('SourceFacts page crosses its parser binding.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3CanonicalSourceFactCheckpointPage page;
}

/// Completes one canonical global SourceFacts stream.
final class FlarkV3ParserSourceFactsCompleted extends FlarkV3ParserEvent {
  FlarkV3ParserSourceFactsCompleted({
    required super.eventId,
    required this.binding,
    required this.completion,
  }) : super(workerGeneration: binding.workerGeneration) {
    final lineage = completion.lineage;
    if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
        lineage.workerGeneration != binding.workerGeneration) {
      throw ArgumentError('SourceFacts completion crosses its parser binding.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3CanonicalSourceFactCompletion completion;
}

/// Wire-safe header for one exact-base canonical SourceFacts splice.
///
/// The unforgeable [FlarkV3CanonicalSourceFactAuthority] remains owned by the
/// Dart source session. The worker transfers only the authenticated scalar
/// description; the session driver resolves the retained authority by object
/// identity immediately before opening the splice.
final class FlarkV3ParserSourceFactsDeltaHeader {
  const FlarkV3ParserSourceFactsDeltaHeader({
    required this.lineage,
    required this.baseFingerprint,
    required this.baseCheckpointRootGuard128,
    required this.baseCheckpointCount,
    required this.basePageCount,
    required this.baseCheckpointSpacingUtf16,
    required this.basePageStart,
    required this.basePageEnd,
    required this.targetPageStart,
    required this.targetPageEnd,
    required this.targetCheckpointCount,
    required this.targetPageCount,
    required this.targetCheckpointRootGuardAlgorithm,
    required this.targetCheckpointRootGuard128,
    required this.replacementCheckpointCount,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3SourceFingerprint baseFingerprint;
  final FlarkV3ContentHash128 baseCheckpointRootGuard128;
  final int baseCheckpointCount;
  final int basePageCount;
  final int baseCheckpointSpacingUtf16;
  final int basePageStart;
  final int basePageEnd;
  final int targetPageStart;
  final int targetPageEnd;
  final int targetCheckpointCount;
  final int targetPageCount;
  final int targetCheckpointRootGuardAlgorithm;
  final FlarkV3ContentHash128 targetCheckpointRootGuard128;
  final int replacementCheckpointCount;

  FlarkV3CanonicalSourceFactDelta bindBase(
    FlarkV3CanonicalSourceFactAuthority baseAuthority,
  ) => FlarkV3CanonicalSourceFactDelta(
    lineage: lineage,
    baseAuthority: baseAuthority,
    baseFingerprint: baseFingerprint,
    baseCheckpointRootGuard128: baseCheckpointRootGuard128,
    baseCheckpointCount: baseCheckpointCount,
    basePageCount: basePageCount,
    baseCheckpointSpacingUtf16: baseCheckpointSpacingUtf16,
    basePageStart: basePageStart,
    basePageEnd: basePageEnd,
    targetPageStart: targetPageStart,
    targetPageEnd: targetPageEnd,
    targetCheckpointCount: targetCheckpointCount,
    targetPageCount: targetPageCount,
    targetCheckpointRootGuardAlgorithm: targetCheckpointRootGuardAlgorithm,
    targetCheckpointRootGuard128: targetCheckpointRootGuard128,
    replacementCheckpointCount: replacementCheckpointCount,
  );
}

/// Opens one exact-base canonical SourceFacts splice.
final class FlarkV3ParserSourceFactsDeltaBegin extends FlarkV3ParserEvent {
  FlarkV3ParserSourceFactsDeltaBegin({
    required super.eventId,
    required this.binding,
    required this.header,
  }) : super(workerGeneration: binding.workerGeneration) {
    final lineage = header.lineage;
    if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
        lineage.workerGeneration != binding.workerGeneration) {
      throw ArgumentError('SourceFacts delta crosses its parser binding.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserSourceFactsDeltaHeader header;
}

/// Transfers one bounded replacement page for an open SourceFacts splice.
final class FlarkV3ParserSourceFactsDeltaPage extends FlarkV3ParserEvent {
  FlarkV3ParserSourceFactsDeltaPage({
    required super.eventId,
    required this.binding,
    required this.page,
  }) : super(workerGeneration: binding.workerGeneration) {
    final lineage = page.lineage;
    if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
        lineage.workerGeneration != binding.workerGeneration) {
      throw ArgumentError('SourceFacts delta page crosses its parser binding.');
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3CanonicalSourceFactDeltaCheckpointPage page;
}

/// Completes one exact-base canonical SourceFacts splice.
final class FlarkV3ParserSourceFactsDeltaCompleted extends FlarkV3ParserEvent {
  FlarkV3ParserSourceFactsDeltaCompleted({
    required super.eventId,
    required this.binding,
    required this.completion,
  }) : super(workerGeneration: binding.workerGeneration) {
    final lineage = completion.lineage;
    if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
        lineage.workerGeneration != binding.workerGeneration) {
      throw ArgumentError(
        'SourceFacts delta completion crosses its parser binding.',
      );
    }
  }

  final FlarkV3ParserSessionBinding binding;
  final FlarkV3CanonicalSourceFactDeltaCompletion completion;
}

/// Terminal, generation-bound response when an accepted late-inline request
/// cannot mint exact block authority from the retained structural candidate.
final class FlarkV3ParserInlineRefinementUnavailable
    extends FlarkV3ParserEvent {
  FlarkV3ParserInlineRefinementUnavailable({
    required super.eventId,
    required this.binding,
    required this.refinementGeneration,
    required this.reasonCode,
  }) : super(workerGeneration: binding.workerGeneration) {
    _checkPositiveV1Identity(refinementGeneration, 'refinementGeneration');
    _checkPositiveV1Identity(reasonCode, 'reasonCode');
  }

  static const int lateQueryUnavailableReason = 1;
  static const int retryableBusyReason = 2;

  final FlarkV3ParserSessionBinding binding;
  final int refinementGeneration;
  final int reasonCode;
}

/// Terminal, generation-bound response when one passive viewport attempt
/// cannot produce an atomic page under its exact base and caller-owned bounds.
final class FlarkV3ParserViewportPresentationUnavailable
    extends FlarkV3ParserEvent {
  FlarkV3ParserViewportPresentationUnavailable({
    required super.eventId,
    required this.binding,
    required this.viewportGeneration,
    required this.reasonCode,
  }) : super(workerGeneration: binding.workerGeneration) {
    _checkPositiveV1Identity(viewportGeneration, 'viewportGeneration');
    if (reasonCode < retryableBusyReason || reasonCode > hostRejectedReason) {
      throw RangeError.range(
        reasonCode,
        retryableBusyReason,
        hostRejectedReason,
        'reasonCode',
      );
    }
  }

  static const int retryableBusyReason = 1;
  static const int budgetExceededReason = 2;
  static const int derivationUnavailableReason = 3;
  static const int hostRejectedReason = 4;

  final FlarkV3ParserSessionBinding binding;
  final int viewportGeneration;
  final int reasonCode;
}

/// Begins one exact structural publication offer.
final class FlarkV3ParserPublicationBegin
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationBegin({
    required super.eventId,
    required super.binding,
    required this.begin,
  });

  final FlarkV3HostOfferBegin begin;
}

/// Transfers one bounded FPK3 packet without materializing its frames.
final class FlarkV3ParserPublicationPacket
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationPacket({
    required super.eventId,
    required super.binding,
    required this.packet,
  });

  final FlarkV3HostPublicationPacket packet;
}

/// Requests commit after the worker's one-pass encoder has closed.
final class FlarkV3ParserPublicationCommitRequested
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationCommitRequested({
    required super.eventId,
    required super.binding,
    required this.request,
  });

  final FlarkV3HostCommitRequest request;
}

/// Explicit proof that the worker received the exact committed ACK.
final class FlarkV3ParserPublicationDeliveryAcknowledged
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationDeliveryAcknowledged({
    required super.eventId,
    required super.binding,
    required this.ack,
  });

  final FlarkV3StructuralAck ack;
}

/// Requests cancellation of the active pre-commit offer.
final class FlarkV3ParserPublicationAbortRequested
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationAbortRequested({
    required super.eventId,
    required super.binding,
    required this.offerId,
  });

  final FlarkV3OfferId offerId;
}

/// Stable publication-local failure; the parser worker itself remains alive.
final class FlarkV3ParserPublicationFailed
    extends FlarkV3ParserPublicationEvent {
  FlarkV3ParserPublicationFailed({
    required super.eventId,
    required super.binding,
    required this.offerId,
    required this.failureCode,
  }) {
    if (failureCode < 0 || failureCode > flarkV3TransportV1Maximum) {
      throw RangeError.range(
        failureCode,
        0,
        flarkV3TransportV1Maximum,
        'failureCode',
      );
    }
  }

  final FlarkV3OfferId offerId;
  final int failureCode;
}

/// A typed terminal failure from the current parser generation.
final class FlarkV3ParserFailed extends FlarkV3ParserEvent {
  FlarkV3ParserFailed({
    required super.eventId,
    required super.workerGeneration,
    required this.failureCode,
  }) {
    if (failureCode < 0 || failureCode > flarkV3TransportV1Maximum) {
      throw RangeError.range(
        failureCode,
        0,
        flarkV3TransportV1Maximum,
        'failureCode',
      );
    }
  }

  /// Stable transport-level failure code. Human diagnostics are mapped above
  /// this boundary so wire values never depend on localized strings.
  final int failureCode;
}

/// Reports work performed under one exact parser-drain grant.
final class FlarkV3ParserDrainProgress extends FlarkV3ParserEvent {
  FlarkV3ParserDrainProgress({
    required super.eventId,
    required this.binding,
    required this.drainId,
    required this.releasedSourceLeases,
    required this.releasedSourceBytes,
    required this.arenaTransitions,
    required this.arenaNodesReclaimed,
    required this.complete,
  }) : super(workerGeneration: binding.workerGeneration) {
    _checkPositiveV1Identity(drainId, 'drainId');
    _checkV1Lane(releasedSourceLeases, 'releasedSourceLeases');
    _checkV1Lane(releasedSourceBytes, 'releasedSourceBytes');
    _checkV1Lane(arenaTransitions, 'arenaTransitions');
    _checkV1Lane(arenaNodesReclaimed, 'arenaNodesReclaimed');
  }

  final FlarkV3ParserSessionBinding binding;
  final int drainId;
  final int releasedSourceLeases;
  final int releasedSourceBytes;
  final int arenaTransitions;
  final int arenaNodesReclaimed;
  final bool complete;

  bool bindsGrant(FlarkV3ParserDrainGrant grant) =>
      binding == grant.binding &&
      drainId == grant.drainId &&
      releasedSourceLeases + arenaTransitions <= grant.maximumTransitions;
}

/// Confirms that the worker has completed bounded shutdown.
final class FlarkV3ParserClosed extends FlarkV3ParserEvent {
  FlarkV3ParserClosed({
    required super.eventId,
    required super.workerGeneration,
  });
}

void _checkPositiveV1Identity(int value, String name) {
  if (value <= 0 || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, 1, flarkV3TransportV1Maximum, name);
  }
}

void _checkV1Lane(int value, String name) {
  if (value < 0 || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, 0, flarkV3TransportV1Maximum, name);
  }
}

void _checkSourceLeaseV1(FlarkV3SourceWorkerSyncLease lease) {
  _checkPositiveV1Identity(
    lease.sourceSessionIdentity,
    'sourceSessionIdentity',
  );
  _checkPositiveV1Identity(lease.leaseId, 'leaseId');
  _checkPositiveV1Identity(lease.workerGeneration, 'workerGeneration');
  switch (lease) {
    case FlarkV3SourceSnapshotSyncLease():
      _checkV1Lane(lease.baseUiRevision, 'baseUiRevision');
      _checkV1Lane(lease.startUtf16, 'startUtf16');
      _checkV1Lane(lease.endUtf16, 'endUtf16');
      _checkV1Lane(lease.totalUtf16Length, 'totalUtf16Length');
      _checkV1Lane(lease.throughIntentSequence, 'throughIntentSequence');
      _checkV1Lane(lease.source.length, 'source.length');
    case FlarkV3SourceIntentSyncLease():
      _checkV1Lane(lease.intents.length, 'intentCount');
      _checkV1Lane(lease.payloadUtf16, 'payloadUtf16');
      for (final intent in lease.intents) {
        _checkPositiveV1Identity(intent.sequence, 'intent.sequence');
        _checkV1Lane(intent.baseUiRevision, 'intent.baseUiRevision');
        _checkV1Lane(intent.uiRevision, 'intent.uiRevision');
        _checkV1Lane(intent.operations.length, 'intent.operationCount');
        for (final operation in intent.operations) {
          _checkV1Lane(operation.startUtf16, 'operation.startUtf16');
          _checkV1Lane(operation.endUtf16, 'operation.endUtf16');
          _checkV1Lane(
            operation.replacement.utf16Length,
            'operation.replacementUtf16',
          );
        }
      }
  }
}
