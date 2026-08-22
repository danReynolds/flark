import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import '../host/host.dart';
import '../source/source.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_wire_protocol.dart';

export 'flark_v3_parser_transport.dart'
    show FlarkV3ParserOpenMode, FlarkV3ParserSessionBinding;

sealed class FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionWireCommand({required this.binding});

  final FlarkV3ParserSessionBinding binding;
}

final class FlarkV3ParserSessionOpenCommand
    extends FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionOpenCommand({
    required super.binding,
    required this.mode,
  });

  final FlarkV3ParserOpenMode mode;
}

sealed class FlarkV3ParserSessionSourceCommand
    extends FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionSourceCommand({
    required super.binding,
    required this.leaseId,
  });

  final int leaseId;
  FlarkV3SourceWorkerSyncKind get kind;
  FlarkV3SourceWorkerSyncAcknowledgement acknowledgement({
    required FlarkV3ObservedSourceReplicaVersion? observedReplica,
  });
}

final class FlarkV3ParserSessionSnapshotCommand
    extends FlarkV3ParserSessionSourceCommand {
  FlarkV3ParserSessionSnapshotCommand({
    required super.binding,
    required super.leaseId,
    required this.baseUiRevision,
    required this.startUtf16,
    required this.endUtf16,
    required this.totalUtf16Length,
    required this.throughIntentSequence,
    required this.targetStamp,
    required this.source,
  });

  factory FlarkV3ParserSessionSnapshotCommand.fromLease({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3SourceSnapshotSyncLease lease,
  }) => FlarkV3ParserSessionSnapshotCommand(
    binding: binding,
    leaseId: lease.leaseId,
    baseUiRevision: lease.baseUiRevision,
    startUtf16: lease.startUtf16,
    endUtf16: lease.endUtf16,
    totalUtf16Length: lease.totalUtf16Length,
    throughIntentSequence: lease.throughIntentSequence,
    targetStamp: lease.targetStamp,
    source: lease.source,
  );

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.snapshot;

  final int baseUiRevision;
  final int startUtf16;
  final int endUtf16;
  final int totalUtf16Length;
  final int throughIntentSequence;
  final FlarkV3SourceStamp targetStamp;
  final String source;

  bool get isSeed => startUtf16 == 0;

  @override
  FlarkV3SourceSnapshotSyncAcknowledgement acknowledgement({
    required FlarkV3ObservedSourceReplicaVersion? observedReplica,
  }) => FlarkV3SourceSnapshotSyncAcknowledgement(
    sourceSessionIdentity: binding.sourceSessionIdentity,
    leaseId: leaseId,
    workerGeneration: binding.workerGeneration,
    baseUiRevision: baseUiRevision,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    throughIntentSequence: throughIntentSequence,
    observedReplica: observedReplica,
  );
}

final class FlarkV3ParserSessionEditCommand
    extends FlarkV3ParserSessionSourceCommand {
  FlarkV3ParserSessionEditCommand({
    required super.binding,
    required super.leaseId,
    required List<FlarkV3SourceIntent> intents,
    required this.payloadUtf16,
  }) : intents = List.unmodifiable(intents);

  factory FlarkV3ParserSessionEditCommand.fromLease({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3SourceIntentSyncLease lease,
  }) => FlarkV3ParserSessionEditCommand(
    binding: binding,
    leaseId: lease.leaseId,
    intents: lease.intents,
    payloadUtf16: lease.payloadUtf16,
  );

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.intents;

  final List<FlarkV3SourceIntent> intents;
  final int payloadUtf16;

  int get firstSequence => intents.first.sequence;
  int get lastSequence => intents.last.sequence;
  FlarkV3SourceStamp get baseStamp => intents.first.baseStamp;
  FlarkV3SourceStamp get targetStamp => intents.last.targetStamp;

  @override
  FlarkV3SourceIntentSyncAcknowledgement acknowledgement({
    required FlarkV3ObservedSourceReplicaVersion? observedReplica,
  }) {
    if (observedReplica == null) {
      throw ArgumentError.notNull('observedReplica');
    }
    return FlarkV3SourceIntentSyncAcknowledgement(
      sourceSessionIdentity: binding.sourceSessionIdentity,
      leaseId: leaseId,
      workerGeneration: binding.workerGeneration,
      firstSequence: firstSequence,
      lastSequence: lastSequence,
      entryCount: intents.length,
      payloadUtf16: payloadUtf16,
      observedReplica: observedReplica,
    );
  }
}

final class FlarkV3ParserSessionBeginCloseCommand
    extends FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionBeginCloseCommand({
    required super.binding,
    required this.activeGeneration,
  });

  /// Zero when the driver closed before sending its first source lease.
  final int activeGeneration;
}

final class FlarkV3ParserSessionSupersedeCommand
    extends FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionSupersedeCommand({
    required super.binding,
    required this.targetUiRevision,
  });

  final int targetUiRevision;
}

final class FlarkV3ParserSessionInlineRefinementCommand
    extends FlarkV3ParserSessionWireCommand {
  FlarkV3ParserSessionInlineRefinementCommand({
    required super.binding,
    required this.refinementGeneration,
    required this.sourceVersion,
    required this.baseAck,
    required this.byteOffset,
    required this.utf16Offset,
    required this.affinity,
    this.target = FlarkV3InlineRefinementTarget.automatic,
  }) {
    _positiveU32(refinementGeneration, 'refinementGeneration');
    _u32(byteOffset, 'byteOffset');
    _u32(utf16Offset, 'utf16Offset');
    if (sourceVersion.documentSession != binding.documentSession ||
        baseAck.sourceVersion != sourceVersion) {
      throw ArgumentError(
        'Inline refinement must bind one exact source and structural ACK.',
      );
    }
    if (byteOffset > sourceVersion.metric.bytes ||
        utf16Offset > sourceVersion.metric.utf16) {
      throw RangeError('Inline refinement point exceeds its source version.');
    }
  }

  final int refinementGeneration;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck baseAck;
  final int byteOffset;
  final int utf16Offset;
  final FlarkV3InlinePointAffinity affinity;
  final FlarkV3InlineRefinementTarget target;
}

final class FlarkV3ParserSessionViewportPresentationCommand
    extends FlarkV3ParserSessionWireCommand {
  FlarkV3ParserSessionViewportPresentationCommand({
    required super.binding,
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
    // Reuse the public transport value's validation so the session codec and
    // driver cannot drift into accepting different authority or budget shapes.
    FlarkV3ParserPresentViewport(
      binding: binding,
      viewportGeneration: viewportGeneration,
      sourceVersion: sourceVersion,
      baseAck: baseAck,
      requestedStartUtf8: requestedStartUtf8,
      requestedStartUtf16: requestedStartUtf16,
      requestedEndUtf8: requestedEndUtf8,
      requestedEndUtf16: requestedEndUtf16,
      startBlockOrdinal: startBlockOrdinal,
      startUtf8: startUtf8,
      startUtf16: startUtf16,
      limits: limits,
    );
  }

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

final class FlarkV3ParserSessionDrainGrant
    extends FlarkV3ParserSessionWireCommand {
  FlarkV3ParserSessionDrainGrant({
    required super.binding,
    required this.drainId,
    required this.maximumTransitions,
  }) {
    _positiveU32(drainId, 'drainId');
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

  final int drainId;
  final int maximumTransitions;
}

final class FlarkV3ParserSessionEventReceiptCommand
    extends FlarkV3ParserSessionWireCommand {
  const FlarkV3ParserSessionEventReceiptCommand({
    required super.binding,
    required this.eventId,
    required this.disposition,
    required this.sourceSync,
    this.sourceCertification,
  });

  factory FlarkV3ParserSessionEventReceiptCommand.fromParserCommand({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3ParserEventReceipt receipt,
  }) => FlarkV3ParserSessionEventReceiptCommand(
    binding: binding,
    eventId: receipt.eventId,
    disposition: receipt.disposition,
    sourceSync: receipt.sourceSync,
    sourceCertification: receipt.sourceCertification,
  );

  final int eventId;
  final FlarkV3ParserEventDisposition disposition;
  final FlarkV3SourceWorkerSyncAckReceipt? sourceSync;
  final FlarkV3CanonicalSourcePromotionProof? sourceCertification;

  FlarkV3ParserEventReceipt toParserCommand() => FlarkV3ParserEventReceipt(
    eventId: eventId,
    workerGeneration: binding.workerGeneration,
    disposition: disposition,
    sourceSync: sourceSync,
    sourceCertification: sourceCertification,
  );
}

sealed class FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionWireEvent({
    required this.binding,
    required this.eventId,
  });

  final FlarkV3ParserSessionBinding binding;
  final int eventId;
}

final class FlarkV3ParserSessionOpenedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionOpenedEvent({
    required super.binding,
    required super.eventId,
    required this.mode,
  });

  final FlarkV3ParserOpenMode mode;

  FlarkV3ParserOpened toParserEvent() =>
      FlarkV3ParserOpened(eventId: eventId, binding: binding, mode: mode);
}

final class FlarkV3ParserSessionSourceSynchronizedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceSynchronizedEvent({
    required super.binding,
    required super.eventId,
    required this.acknowledgement,
  });

  factory FlarkV3ParserSessionSourceSynchronizedEvent.fromParserEvent({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3ParserSourceSynchronized event,
  }) => FlarkV3ParserSessionSourceSynchronizedEvent(
    binding: binding,
    eventId: event.eventId,
    acknowledgement: event.acknowledgement,
  );

  final FlarkV3SourceWorkerSyncAcknowledgement acknowledgement;

  FlarkV3ParserSourceSynchronized toParserEvent() =>
      FlarkV3ParserSourceSynchronized(
        eventId: eventId,
        workerGeneration: binding.workerGeneration,
        acknowledgement: acknowledgement,
      );
}

final class FlarkV3ParserSessionSourceFactsPageEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceFactsPageEvent({
    required super.binding,
    required super.eventId,
    required this.page,
  });

  factory FlarkV3ParserSessionSourceFactsPageEvent.fromParserEvent(
    FlarkV3ParserSourceFactsPage event,
  ) => FlarkV3ParserSessionSourceFactsPageEvent(
    binding: event.binding,
    eventId: event.eventId,
    page: event.page,
  );

  final FlarkV3CanonicalSourceFactCheckpointPage page;

  FlarkV3ParserSourceFactsPage toParserEvent() => FlarkV3ParserSourceFactsPage(
    eventId: eventId,
    binding: binding,
    page: page,
  );
}

final class FlarkV3ParserSessionSourceFactsCompletedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceFactsCompletedEvent({
    required super.binding,
    required super.eventId,
    required this.completion,
  });

  factory FlarkV3ParserSessionSourceFactsCompletedEvent.fromParserEvent(
    FlarkV3ParserSourceFactsCompleted event,
  ) => FlarkV3ParserSessionSourceFactsCompletedEvent(
    binding: event.binding,
    eventId: event.eventId,
    completion: event.completion,
  );

  final FlarkV3CanonicalSourceFactCompletion completion;

  FlarkV3ParserSourceFactsCompleted toParserEvent() =>
      FlarkV3ParserSourceFactsCompleted(
        eventId: eventId,
        binding: binding,
        completion: completion,
      );
}

final class FlarkV3ParserSessionSourceFactsDeltaBeginEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceFactsDeltaBeginEvent({
    required super.binding,
    required super.eventId,
    required this.header,
  });

  factory FlarkV3ParserSessionSourceFactsDeltaBeginEvent.fromParserEvent(
    FlarkV3ParserSourceFactsDeltaBegin event,
  ) => FlarkV3ParserSessionSourceFactsDeltaBeginEvent(
    binding: event.binding,
    eventId: event.eventId,
    header: event.header,
  );

  final FlarkV3ParserSourceFactsDeltaHeader header;

  FlarkV3ParserSourceFactsDeltaBegin toParserEvent() =>
      FlarkV3ParserSourceFactsDeltaBegin(
        eventId: eventId,
        binding: binding,
        header: header,
      );
}

final class FlarkV3ParserSessionSourceFactsDeltaPageEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceFactsDeltaPageEvent({
    required super.binding,
    required super.eventId,
    required this.page,
  });

  factory FlarkV3ParserSessionSourceFactsDeltaPageEvent.fromParserEvent(
    FlarkV3ParserSourceFactsDeltaPage event,
  ) => FlarkV3ParserSessionSourceFactsDeltaPageEvent(
    binding: event.binding,
    eventId: event.eventId,
    page: event.page,
  );

  final FlarkV3CanonicalSourceFactDeltaCheckpointPage page;

  FlarkV3ParserSourceFactsDeltaPage toParserEvent() =>
      FlarkV3ParserSourceFactsDeltaPage(
        eventId: eventId,
        binding: binding,
        page: page,
      );
}

final class FlarkV3ParserSessionSourceFactsDeltaCompletedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionSourceFactsDeltaCompletedEvent({
    required super.binding,
    required super.eventId,
    required this.completion,
  });

  factory FlarkV3ParserSessionSourceFactsDeltaCompletedEvent.fromParserEvent(
    FlarkV3ParserSourceFactsDeltaCompleted event,
  ) => FlarkV3ParserSessionSourceFactsDeltaCompletedEvent(
    binding: event.binding,
    eventId: event.eventId,
    completion: event.completion,
  );

  final FlarkV3CanonicalSourceFactDeltaCompletion completion;

  FlarkV3ParserSourceFactsDeltaCompleted toParserEvent() =>
      FlarkV3ParserSourceFactsDeltaCompleted(
        eventId: eventId,
        binding: binding,
        completion: completion,
      );
}

final class FlarkV3ParserSessionInlineRefinementUnavailableEvent
    extends FlarkV3ParserSessionWireEvent {
  FlarkV3ParserSessionInlineRefinementUnavailableEvent({
    required super.binding,
    required super.eventId,
    required this.refinementGeneration,
    required this.reasonCode,
  }) {
    _positiveU32(refinementGeneration, 'refinementGeneration');
    _positiveU32(reasonCode, 'reasonCode');
  }

  factory FlarkV3ParserSessionInlineRefinementUnavailableEvent.fromParserEvent(
    FlarkV3ParserInlineRefinementUnavailable event,
  ) => FlarkV3ParserSessionInlineRefinementUnavailableEvent(
    binding: event.binding,
    eventId: event.eventId,
    refinementGeneration: event.refinementGeneration,
    reasonCode: event.reasonCode,
  );

  final int refinementGeneration;
  final int reasonCode;

  FlarkV3ParserInlineRefinementUnavailable toParserEvent() =>
      FlarkV3ParserInlineRefinementUnavailable(
        eventId: eventId,
        binding: binding,
        refinementGeneration: refinementGeneration,
        reasonCode: reasonCode,
      );
}

final class FlarkV3ParserSessionViewportPresentationUnavailableEvent
    extends FlarkV3ParserSessionWireEvent {
  FlarkV3ParserSessionViewportPresentationUnavailableEvent({
    required super.binding,
    required super.eventId,
    required this.viewportGeneration,
    required this.reasonCode,
  }) {
    _positiveU32(viewportGeneration, 'viewportGeneration');
    _positiveU32(reasonCode, 'reasonCode');
  }

  factory FlarkV3ParserSessionViewportPresentationUnavailableEvent.fromParserEvent(
    FlarkV3ParserViewportPresentationUnavailable event,
  ) => FlarkV3ParserSessionViewportPresentationUnavailableEvent(
    binding: event.binding,
    eventId: event.eventId,
    viewportGeneration: event.viewportGeneration,
    reasonCode: event.reasonCode,
  );

  final int viewportGeneration;
  final int reasonCode;

  FlarkV3ParserViewportPresentationUnavailable toParserEvent() =>
      FlarkV3ParserViewportPresentationUnavailable(
        eventId: eventId,
        binding: binding,
        viewportGeneration: viewportGeneration,
        reasonCode: reasonCode,
      );
}

final class FlarkV3ParserSessionDrainProgressEvent
    extends FlarkV3ParserSessionWireEvent {
  FlarkV3ParserSessionDrainProgressEvent({
    required super.binding,
    required super.eventId,
    required this.drainId,
    required this.releasedSourceLeases,
    required this.releasedSourceBytes,
    required this.arenaTransitions,
    required this.arenaNodesReclaimed,
    required this.complete,
  }) {
    _positiveU32(drainId, 'drainId');
    _u32(releasedSourceLeases, 'releasedSourceLeases');
    _u32(releasedSourceBytes, 'releasedSourceBytes');
    _u32(arenaTransitions, 'arenaTransitions');
    _u32(arenaNodesReclaimed, 'arenaNodesReclaimed');
  }

  final int drainId;
  final int releasedSourceLeases;
  final int releasedSourceBytes;
  final int arenaTransitions;
  final int arenaNodesReclaimed;
  final bool complete;

  bool bindsGrant(FlarkV3ParserSessionDrainGrant grant) =>
      binding == grant.binding &&
      drainId == grant.drainId &&
      releasedSourceLeases + arenaTransitions <= grant.maximumTransitions;

  FlarkV3ParserDrainProgress toParserEvent() => FlarkV3ParserDrainProgress(
    eventId: eventId,
    binding: binding,
    drainId: drainId,
    releasedSourceLeases: releasedSourceLeases,
    releasedSourceBytes: releasedSourceBytes,
    arenaTransitions: arenaTransitions,
    arenaNodesReclaimed: arenaNodesReclaimed,
    complete: complete,
  );
}

final class FlarkV3ParserSessionFailedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionFailedEvent({
    required super.binding,
    required super.eventId,
    required this.failureCode,
  });

  factory FlarkV3ParserSessionFailedEvent.fromParserEvent({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3ParserFailed event,
  }) => FlarkV3ParserSessionFailedEvent(
    binding: binding,
    eventId: event.eventId,
    failureCode: event.failureCode,
  );

  final int failureCode;

  FlarkV3ParserFailed toParserEvent() => FlarkV3ParserFailed(
    eventId: eventId,
    workerGeneration: binding.workerGeneration,
    failureCode: failureCode,
  );
}

final class FlarkV3ParserSessionClosedEvent
    extends FlarkV3ParserSessionWireEvent {
  const FlarkV3ParserSessionClosedEvent({
    required super.binding,
    required super.eventId,
  });

  factory FlarkV3ParserSessionClosedEvent.fromParserEvent({
    required FlarkV3ParserSessionBinding binding,
    required FlarkV3ParserClosed event,
  }) =>
      FlarkV3ParserSessionClosedEvent(binding: binding, eventId: event.eventId);

  FlarkV3ParserClosed toParserEvent() => FlarkV3ParserClosed(
    eventId: eventId,
    workerGeneration: binding.workerGeneration,
  );
}

/// Version-3 binary session codec shared by native-isolate and Web Worker
/// endpoints.
///
/// Publication events remain exclusively owned by
/// `FlarkV3ParserPublicationWireCodec`; this codec rejects those opcodes.
final class FlarkV3ParserSessionWireCodec {
  const FlarkV3ParserSessionWireCodec._();

  static const int payloadSchema = 3;
  static const int maximumSnapshotUtf16 = 8192;
  static const int maximumIntentCount = 64;
  static const int maximumOperationCount = 1024;
  static const int maximumIntentPayloadUtf16 = 8192;
  static const int maximumDrainTransitions =
      flarkV3ParserMaximumDrainTransitions;

  static Uint8List encodeParserCommand(
    FlarkV3ParserCommand command, {
    required FlarkV3ParserSessionBinding binding,
    required int correlationId,
  }) {
    final FlarkV3ParserSessionWireCommand value;
    switch (command) {
      case FlarkV3ParserOpen(:final mode):
        _requireExactBinding(command.binding, binding);
        value = FlarkV3ParserSessionOpenCommand(binding: binding, mode: mode);
      case FlarkV3ParserSynchronizeSource(:final lease):
        _requireLeaseBinding(binding, lease);
        value = switch (lease) {
          FlarkV3SourceSnapshotSyncLease() =>
            FlarkV3ParserSessionSnapshotCommand.fromLease(
              binding: binding,
              lease: lease,
            ),
          FlarkV3SourceIntentSyncLease() =>
            FlarkV3ParserSessionEditCommand.fromLease(
              binding: binding,
              lease: lease,
            ),
        };
      case FlarkV3ParserRestart(:final workerGeneration):
        if (workerGeneration != binding.workerGeneration) {
          throw ArgumentError('Restart generation does not match its binding.');
        }
        if (workerGeneration <= 1) {
          throw ArgumentError(
            'Restart generation must follow an earlier epoch.',
          );
        }
        value = FlarkV3ParserSessionOpenCommand(
          binding: binding,
          mode: FlarkV3ParserOpenMode.recovery,
        );
      case FlarkV3ParserBeginClose(:final workerGeneration):
        if (workerGeneration != null &&
            workerGeneration != binding.workerGeneration) {
          throw ArgumentError('Close generation does not match its binding.');
        }
        value = FlarkV3ParserSessionBeginCloseCommand(
          binding: binding,
          activeGeneration: workerGeneration ?? 0,
        );
      case FlarkV3ParserSupersede(:final targetUiRevision):
        _requireExactBinding(command.binding, binding);
        value = FlarkV3ParserSessionSupersedeCommand(
          binding: binding,
          targetUiRevision: targetUiRevision,
        );
      case FlarkV3ParserRefineInline():
        _requireExactBinding(command.binding, binding);
        value = FlarkV3ParserSessionInlineRefinementCommand(
          binding: binding,
          refinementGeneration: command.refinementGeneration,
          sourceVersion: command.sourceVersion,
          baseAck: command.baseAck,
          byteOffset: command.byteOffset,
          utf16Offset: command.utf16Offset,
          affinity: command.affinity,
          target: command.target,
        );
      case FlarkV3ParserPresentViewport():
        _requireExactBinding(command.binding, binding);
        value = FlarkV3ParserSessionViewportPresentationCommand(
          binding: binding,
          viewportGeneration: command.viewportGeneration,
          sourceVersion: command.sourceVersion,
          baseAck: command.baseAck,
          requestedStartUtf8: command.requestedStartUtf8,
          requestedStartUtf16: command.requestedStartUtf16,
          requestedEndUtf8: command.requestedEndUtf8,
          requestedEndUtf16: command.requestedEndUtf16,
          startBlockOrdinal: command.startBlockOrdinal,
          startUtf8: command.startUtf8,
          startUtf16: command.startUtf16,
          limits: command.limits,
        );
      case FlarkV3ParserDrainGrant():
        _requireExactBinding(command.binding, binding);
        value = FlarkV3ParserSessionDrainGrant(
          binding: binding,
          drainId: command.drainId,
          maximumTransitions: command.maximumTransitions,
        );
      case FlarkV3ParserEventReceipt():
        if (command.workerGeneration != binding.workerGeneration) {
          throw ArgumentError('Receipt generation does not match its binding.');
        }
        value = FlarkV3ParserSessionEventReceiptCommand.fromParserCommand(
          binding: binding,
          receipt: command,
        );
      default:
        throw ArgumentError.value(
          command,
          'command',
          'Publication commands use the publication wire codec.',
        );
    }
    final priorBinding = binding.workerGeneration > 1
        ? FlarkV3ParserSessionBinding(
            documentSession: binding.documentSession,
            sourceSessionIdentity: binding.sourceSessionIdentity,
            workerGeneration: binding.workerGeneration - 1,
          )
        : null;
    final establishedBinding = switch (command) {
      FlarkV3ParserOpen(mode: FlarkV3ParserOpenMode.fresh) => null,
      FlarkV3ParserOpen(mode: FlarkV3ParserOpenMode.recovery) => priorBinding,
      FlarkV3ParserRestart() => priorBinding,
      _ => binding,
    };
    return encodeCommand(
      value,
      correlationId: correlationId,
      establishedBinding: establishedBinding,
    );
  }

  static Uint8List encodeCommand(
    FlarkV3ParserSessionWireCommand command, {
    required int correlationId,
    FlarkV3ParserSessionBinding? establishedBinding,
  }) {
    _positiveU32(correlationId, 'correlationId');
    _validateCommandTransition(command, establishedBinding);

    final _EncodedCommand encoded = switch (command) {
      FlarkV3ParserSessionOpenCommand() => _encodeOpen(command),
      FlarkV3ParserSessionSnapshotCommand() => _encodeSnapshot(command),
      FlarkV3ParserSessionEditCommand() => _encodeEdits(command),
      FlarkV3ParserSessionInlineRefinementCommand() => _encodeInlineRefinement(
        command,
      ),
      FlarkV3ParserSessionViewportPresentationCommand() =>
        _encodeViewportPresentation(command),
      FlarkV3ParserSessionSupersedeCommand() => _encodeSupersede(command),
      FlarkV3ParserSessionBeginCloseCommand() => _encodeClose(command),
      FlarkV3ParserSessionDrainGrant() => _encodeDrainGrant(command),
      FlarkV3ParserSessionEventReceiptCommand() => _encodeReceipt(command),
    };
    if (command case FlarkV3ParserSessionSourceCommand(:final leaseId)) {
      if (correlationId != leaseId) {
        throw ArgumentError('Source command correlation must equal lease ID.');
      }
    }
    if (command case FlarkV3ParserSessionEventReceiptCommand(:final eventId)) {
      if (correlationId != eventId) {
        throw ArgumentError('Receipt correlation must equal event ID.');
      }
    }
    if (command case FlarkV3ParserSessionDrainGrant(:final drainId)) {
      if (correlationId != drainId) {
        throw ArgumentError('Drain correlation must equal drain ID.');
      }
    }
    return _frame(
      opcode: encoded.opcode,
      correlationId: correlationId,
      payload: encoded.payload,
    );
  }

  static FlarkV3DecodedParserSessionCommand decodeCommand(
    Uint8List bytes, {
    FlarkV3ParserSessionBinding? establishedBinding,
  }) {
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.request,
    );
    _requirePositiveCorrelation(frame);
    _requireCommandOpcode(frame.opcode);
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    try {
      final FlarkV3ParserSessionWireCommand command;
      switch (frame.opcode) {
        case FlarkV3WireOpcode.parserOpen:
          command = switch (header.variant) {
            0 => FlarkV3ParserSessionOpenCommand(
              binding: header.binding,
              mode: FlarkV3ParserOpenMode.fresh,
            ),
            1 => FlarkV3ParserSessionOpenCommand(
              binding: header.binding,
              mode: FlarkV3ParserOpenMode.recovery,
            ),
            _ => throw _variant(reader, header.variant),
          };
        case FlarkV3WireOpcode.snapshotPage:
          if (header.variant != 0 && header.variant != 1) {
            throw _variant(reader, header.variant);
          }
          command = _readSnapshot(reader, header, seed: header.variant == 0);
        case FlarkV3WireOpcode.edit:
          if (header.variant != 0) throw _variant(reader, header.variant);
          command = _readEdits(reader, header);
        case FlarkV3WireOpcode.parserRefineInline:
          if (header.variant != 0) throw _variant(reader, header.variant);
          command = _readInlineRefinement(reader, header);
        case FlarkV3WireOpcode.parserPresentViewport:
          if (header.variant != 0) throw _variant(reader, header.variant);
          command = _readViewportPresentation(reader, header);
        case FlarkV3WireOpcode.supersede:
          if (header.variant != 0) throw _variant(reader, header.variant);
          command = FlarkV3ParserSessionSupersedeCommand(
            binding: header.binding,
            targetUiRevision: reader.u32(),
          );
        case FlarkV3WireOpcode.parserAcknowledge:
          command = _readReceipt(reader, header, frame.correlationId);
        case FlarkV3WireOpcode.close:
          if (header.variant != 0) throw _variant(reader, header.variant);
          final activeGeneration = reader.u32();
          if (activeGeneration != 0 &&
              activeGeneration != header.binding.workerGeneration) {
            throw _identity(
              reader,
              activeGeneration,
              header.binding.workerGeneration,
            );
          }
          command = FlarkV3ParserSessionBeginCloseCommand(
            binding: header.binding,
            activeGeneration: activeGeneration,
          );
        case FlarkV3WireOpcode.drain:
          if (header.variant != 0) throw _variant(reader, header.variant);
          command = FlarkV3ParserSessionDrainGrant(
            binding: header.binding,
            drainId: reader.u32(),
            maximumTransitions: reader.u32(),
          );
        default:
          throw FlarkV3ParserSessionWireFormatException(
            FlarkV3ParserSessionWireFailure.unexpectedOpcode,
            byteOffset: 8,
            actual: frame.opcode.code,
          );
      }
      reader.finish();
      _validateDecodedCommandTransition(command, establishedBinding, reader);
      if (command case FlarkV3ParserSessionSourceCommand(:final leaseId)) {
        if (frame.correlationId != leaseId) {
          throw _identity(reader, frame.correlationId, leaseId);
        }
      }
      if (command case FlarkV3ParserSessionEventReceiptCommand(
        :final eventId,
      )) {
        if (frame.correlationId != eventId) {
          throw _identity(reader, frame.correlationId, eventId);
        }
      }
      if (command case FlarkV3ParserSessionDrainGrant(:final drainId)) {
        if (frame.correlationId != drainId) {
          throw _identity(reader, frame.correlationId, drainId);
        }
      }
      return FlarkV3DecodedParserSessionCommand(
        correlationId: frame.correlationId,
        command: command,
      );
    } on FlarkV3ParserSessionWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }

  static Uint8List encodeParserEvent(
    FlarkV3ParserEvent event, {
    required FlarkV3ParserSessionBinding binding,
    FlarkV3ParserDrainGrant? expectedDrainGrant,
  }) {
    if (event.workerGeneration != binding.workerGeneration) {
      throw ArgumentError(
        'Parser event generation does not match its binding.',
      );
    }
    final FlarkV3ParserSessionWireEvent value = switch (event) {
      FlarkV3ParserOpened() => FlarkV3ParserSessionOpenedEvent(
        binding: event.binding,
        eventId: event.eventId,
        mode: event.mode,
      ),
      FlarkV3ParserSourceSynchronized() =>
        FlarkV3ParserSessionSourceSynchronizedEvent.fromParserEvent(
          binding: binding,
          event: event,
        ),
      FlarkV3ParserSourceFactsPage() =>
        FlarkV3ParserSessionSourceFactsPageEvent.fromParserEvent(event),
      FlarkV3ParserSourceFactsCompleted() =>
        FlarkV3ParserSessionSourceFactsCompletedEvent.fromParserEvent(event),
      FlarkV3ParserSourceFactsDeltaBegin() =>
        FlarkV3ParserSessionSourceFactsDeltaBeginEvent.fromParserEvent(event),
      FlarkV3ParserSourceFactsDeltaPage() =>
        FlarkV3ParserSessionSourceFactsDeltaPageEvent.fromParserEvent(event),
      FlarkV3ParserSourceFactsDeltaCompleted() =>
        FlarkV3ParserSessionSourceFactsDeltaCompletedEvent.fromParserEvent(
          event,
        ),
      FlarkV3ParserInlineRefinementUnavailable() =>
        FlarkV3ParserSessionInlineRefinementUnavailableEvent.fromParserEvent(
          event,
        ),
      FlarkV3ParserViewportPresentationUnavailable() =>
        FlarkV3ParserSessionViewportPresentationUnavailableEvent.fromParserEvent(
          event,
        ),
      FlarkV3ParserFailed() => FlarkV3ParserSessionFailedEvent.fromParserEvent(
        binding: binding,
        event: event,
      ),
      FlarkV3ParserClosed() => FlarkV3ParserSessionClosedEvent.fromParserEvent(
        binding: binding,
        event: event,
      ),
      FlarkV3ParserDrainProgress() => FlarkV3ParserSessionDrainProgressEvent(
        binding: event.binding,
        eventId: event.eventId,
        drainId: event.drainId,
        releasedSourceLeases: event.releasedSourceLeases,
        releasedSourceBytes: event.releasedSourceBytes,
        arenaTransitions: event.arenaTransitions,
        arenaNodesReclaimed: event.arenaNodesReclaimed,
        complete: event.complete,
      ),
      _ => throw ArgumentError.value(
        event,
        'event',
        'Publication events use the publication wire codec.',
      ),
    };
    final wireDrainGrant = expectedDrainGrant == null
        ? null
        : FlarkV3ParserSessionDrainGrant(
            binding: expectedDrainGrant.binding,
            drainId: expectedDrainGrant.drainId,
            maximumTransitions: expectedDrainGrant.maximumTransitions,
          );
    return encodeEvent(
      value,
      expectedBinding: binding,
      expectedDrainGrant: wireDrainGrant,
    );
  }

  static Uint8List encodeEvent(
    FlarkV3ParserSessionWireEvent event, {
    required FlarkV3ParserSessionBinding expectedBinding,
    FlarkV3ParserSessionDrainGrant? expectedDrainGrant,
  }) {
    _positiveU32(event.eventId, 'eventId');
    _requireExactBinding(event.binding, expectedBinding);
    final _EncodedCommand encoded = switch (event) {
      FlarkV3ParserSessionOpenedEvent() => _encodeOpened(event),
      FlarkV3ParserSessionSourceSynchronizedEvent() =>
        _encodeSourceAcknowledgement(event),
      FlarkV3ParserSessionSourceFactsPageEvent() => _encodeSourceFactsPage(
        event,
      ),
      FlarkV3ParserSessionSourceFactsCompletedEvent() =>
        _encodeSourceFactsCompletion(event),
      FlarkV3ParserSessionSourceFactsDeltaBeginEvent() =>
        _encodeSourceFactsDeltaBegin(event),
      FlarkV3ParserSessionSourceFactsDeltaPageEvent() =>
        _encodeSourceFactsDeltaPage(event),
      FlarkV3ParserSessionSourceFactsDeltaCompletedEvent() =>
        _encodeSourceFactsDeltaCompletion(event),
      FlarkV3ParserSessionInlineRefinementUnavailableEvent() =>
        _encodeInlineRefinementUnavailable(event),
      FlarkV3ParserSessionViewportPresentationUnavailableEvent() =>
        _encodeViewportPresentationUnavailable(event),
      FlarkV3ParserSessionDrainProgressEvent() => _encodeDrainProgress(event),
      FlarkV3ParserSessionFailedEvent() => _encodeFailure(event),
      FlarkV3ParserSessionClosedEvent() => _encodeClosed(event),
    };
    if (event case final FlarkV3ParserSessionDrainProgressEvent progress) {
      final grant = expectedDrainGrant;
      if (grant == null || !progress.bindsGrant(grant)) {
        throw ArgumentError('Drain progress exceeds or crosses its grant.');
      }
    }
    return _frame(
      opcode: encoded.opcode,
      correlationId: event.eventId,
      payload: encoded.payload,
    );
  }

  static FlarkV3ParserSessionWireEvent decodeEvent(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
    FlarkV3ParserSessionDrainGrant? expectedDrainGrant,
    bool requireDrainGrant = true,
  }) {
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.request,
    );
    _requirePositiveCorrelation(frame);
    _requireEventOpcode(frame.opcode);
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    try {
      final FlarkV3ParserSessionWireEvent event;
      switch (frame.opcode) {
        case FlarkV3WireOpcode.parserOpen:
          event = switch (header.variant) {
            2 => FlarkV3ParserSessionOpenedEvent(
              binding: header.binding,
              eventId: frame.correlationId,
              mode: FlarkV3ParserOpenMode.fresh,
            ),
            3 => FlarkV3ParserSessionOpenedEvent(
              binding: header.binding,
              eventId: frame.correlationId,
              mode: FlarkV3ParserOpenMode.recovery,
            ),
            _ => throw _variant(reader, header.variant),
          };
        case FlarkV3WireOpcode.snapshotPage:
          if (header.variant != 2 && header.variant != 3) {
            throw _variant(reader, header.variant);
          }
          event = _readSnapshotAcknowledgement(
            reader,
            header,
            eventId: frame.correlationId,
            seed: header.variant == 2,
          );
        case FlarkV3WireOpcode.edit:
          if (header.variant != 1) throw _variant(reader, header.variant);
          event = _readEditAcknowledgement(
            reader,
            header,
            eventId: frame.correlationId,
          );
        case FlarkV3WireOpcode.drain:
          if (header.variant != 1) throw _variant(reader, header.variant);
          final drainId = reader.u32();
          final releasedSourceLeases = reader.u32();
          final releasedSourceBytes = reader.u32();
          final arenaTransitions = reader.u32();
          final arenaNodesReclaimed = reader.u32();
          final complete = reader.u32();
          if (complete > 1) {
            throw FlarkV3ParserSessionWireFormatException(
              FlarkV3ParserSessionWireFailure.invalidValue,
              byteOffset: reader.offset - 4,
              expected: 1,
              actual: complete,
            );
          }
          event = FlarkV3ParserSessionDrainProgressEvent(
            binding: header.binding,
            eventId: frame.correlationId,
            drainId: drainId,
            releasedSourceLeases: releasedSourceLeases,
            releasedSourceBytes: releasedSourceBytes,
            arenaTransitions: arenaTransitions,
            arenaNodesReclaimed: arenaNodesReclaimed,
            complete: complete == 1,
          );
        case FlarkV3WireOpcode.parserPoll:
          event = switch (header.variant) {
            1 => FlarkV3ParserSessionFailedEvent(
              binding: header.binding,
              eventId: frame.correlationId,
              failureCode: reader.u32(),
            ),
            2 => _readSourceFactsPage(
              reader,
              header,
              eventId: frame.correlationId,
            ),
            3 => _readSourceFactsCompletion(
              reader,
              header,
              eventId: frame.correlationId,
            ),
            4 => _readSourceFactsDeltaBegin(
              reader,
              header,
              eventId: frame.correlationId,
            ),
            5 => _readSourceFactsDeltaPage(
              reader,
              header,
              eventId: frame.correlationId,
            ),
            6 => _readSourceFactsDeltaCompletion(
              reader,
              header,
              eventId: frame.correlationId,
            ),
            7 => FlarkV3ParserSessionInlineRefinementUnavailableEvent(
              binding: header.binding,
              eventId: frame.correlationId,
              refinementGeneration: reader.u32(),
              reasonCode: reader.u32(),
            ),
            8 => FlarkV3ParserSessionViewportPresentationUnavailableEvent(
              binding: header.binding,
              eventId: frame.correlationId,
              viewportGeneration: reader.u32(),
              reasonCode: reader.u32(),
            ),
            _ => throw _variant(reader, header.variant),
          };
        case FlarkV3WireOpcode.close:
          if (header.variant != 1) throw _variant(reader, header.variant);
          event = FlarkV3ParserSessionClosedEvent(
            binding: header.binding,
            eventId: frame.correlationId,
          );
        default:
          throw FlarkV3ParserSessionWireFormatException(
            FlarkV3ParserSessionWireFailure.unexpectedOpcode,
            byteOffset: 8,
            actual: frame.opcode.code,
          );
      }
      reader.finish();
      _requireDecodedBinding(event.binding, expectedBinding, reader);
      if (event case final FlarkV3ParserSessionDrainProgressEvent progress) {
        final grant = expectedDrainGrant;
        if (requireDrainGrant &&
            (grant == null || !progress.bindsGrant(grant))) {
          throw FlarkV3ParserSessionWireFormatException(
            FlarkV3ParserSessionWireFailure.identityMismatch,
            byteOffset: reader.offset,
          );
        }
      }
      return event;
    } on FlarkV3ParserSessionWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }
}

final class FlarkV3DecodedParserSessionCommand {
  const FlarkV3DecodedParserSessionCommand({
    required this.correlationId,
    required this.command,
  });

  final int correlationId;
  final FlarkV3ParserSessionWireCommand command;
}

enum FlarkV3ParserSessionWireFailure {
  unsupportedSchema,
  unexpectedOpcode,
  unknownVariant,
  truncatedPayload,
  trailingPayload,
  invalidValue,
  oversizedValue,
  identityMismatch,
  invalidUtf8,
}

final class FlarkV3ParserSessionWireFormatException implements FormatException {
  const FlarkV3ParserSessionWireFormatException(
    this.failure, {
    required this.byteOffset,
    this.expected,
    this.actual,
  });

  final FlarkV3ParserSessionWireFailure failure;
  final int byteOffset;
  final int? expected;
  final int? actual;

  @override
  String get message => 'Invalid Flark v3 session payload: ${failure.name}';

  @override
  int get offset => byteOffset;

  @override
  Object? get source => null;
}

final class _Header {
  const _Header(this.variant, this.binding);

  final int variant;
  final FlarkV3ParserSessionBinding binding;
}

final class _EncodedCommand {
  const _EncodedCommand(this.opcode, this.payload);

  final FlarkV3WireOpcode opcode;
  final Uint8List payload;
}

final class _EncodedEdit {
  const _EncodedEdit({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacementUtf16,
    required this.replacementUtf8,
  });

  final int startUtf16;
  final int endUtf16;
  final int replacementUtf16;
  final Uint8List replacementUtf8;
}

final class _EncodedIntent {
  const _EncodedIntent({required this.intent, required this.operations});

  final FlarkV3SourceIntent intent;
  final List<_EncodedEdit> operations;
}

const int _commonBytes = 28;
const int _sourceStampBytes = 32;
const int _observedReplicaBytes = 20;
const int _sourceFactCheckpointBytes = 28;
const int _canonicalCompletionBytes = 84;
const int _canonicalDeltaBeginBytes = 124;
const int _canonicalDeltaCompletionBytes = 104;
const int _maximumCanonicalSourceFactCheckpoints = 64;

_EncodedCommand _encodeOpen(FlarkV3ParserSessionOpenCommand command) {
  final writer = _writer(_commonBytes);
  _writeHeader(
    writer,
    command.mode == FlarkV3ParserOpenMode.fresh ? 0 : 1,
    command.binding,
  );
  return _EncodedCommand(FlarkV3WireOpcode.parserOpen, writer.finish());
}

_EncodedCommand _encodeOpened(FlarkV3ParserSessionOpenedEvent event) {
  final writer = _writer(_commonBytes);
  _writeHeader(
    writer,
    event.mode == FlarkV3ParserOpenMode.fresh ? 2 : 3,
    event.binding,
  );
  return _EncodedCommand(FlarkV3WireOpcode.parserOpen, writer.finish());
}

_EncodedCommand _encodeSnapshot(FlarkV3ParserSessionSnapshotCommand command) {
  final source = _strictUtf8(command.source, 'source');
  _validateSnapshot(command, encodedBytes: source.length);
  final writer = _writer(_commonBytes + 32 + _sourceStampBytes + source.length);
  _writeHeader(writer, command.isSeed ? 0 : 1, command.binding);
  writer
    ..u32(command.leaseId)
    ..u32(command.baseUiRevision)
    ..u32(command.startUtf16)
    ..u32(command.endUtf16)
    ..u32(command.totalUtf16Length)
    ..u32(command.throughIntentSequence);
  _writeSourceStamp(writer, command.targetStamp);
  writer
    ..u32(command.source.length)
    ..u32(source.length)
    ..raw(source);
  return _EncodedCommand(FlarkV3WireOpcode.snapshotPage, writer.finish());
}

_EncodedCommand _encodeEdits(FlarkV3ParserSessionEditCommand command) {
  final encoded = _prepareIntents(command);
  final operationCount = encoded.fold<int>(
    0,
    (total, intent) => total + intent.operations.length,
  );
  final payloadBytes = encoded.fold<int>(
    0,
    (total, intent) =>
        total +
        intent.operations.fold<int>(
          0,
          (sum, edit) => sum + edit.replacementUtf8.length,
        ),
  );
  final bodyBytes =
      28 +
      encoded.length * (16 + _sourceStampBytes * 2) +
      operationCount * 16 +
      payloadBytes;
  final writer = _writer(_commonBytes + bodyBytes);
  _writeHeader(writer, 0, command.binding);
  writer
    ..u32(command.leaseId)
    ..u32(command.firstSequence)
    ..u32(command.lastSequence)
    ..u32(encoded.length)
    ..u32(operationCount)
    ..u32(command.payloadUtf16)
    ..u32(payloadBytes);
  for (final intent in encoded) {
    writer
      ..u32(intent.intent.sequence)
      ..u32(intent.intent.baseUiRevision)
      ..u32(intent.intent.uiRevision)
      ..u32(intent.operations.length);
    _writeSourceStamp(writer, intent.intent.baseStamp);
    _writeSourceStamp(writer, intent.intent.targetStamp);
    for (final operation in intent.operations) {
      writer
        ..u32(operation.startUtf16)
        ..u32(operation.endUtf16)
        ..u32(operation.replacementUtf16)
        ..u32(operation.replacementUtf8.length)
        ..raw(operation.replacementUtf8);
    }
  }
  return _EncodedCommand(FlarkV3WireOpcode.edit, writer.finish());
}

_EncodedCommand _encodeSupersede(FlarkV3ParserSessionSupersedeCommand command) {
  _u32(command.targetUiRevision, 'targetUiRevision');
  final writer = _writer(_commonBytes + 4);
  _writeHeader(writer, 0, command.binding);
  writer.u32(command.targetUiRevision);
  return _EncodedCommand(FlarkV3WireOpcode.supersede, writer.finish());
}

_EncodedCommand _encodeInlineRefinement(
  FlarkV3ParserSessionInlineRefinementCommand command,
) {
  const sourceVersionBytes = 44;
  const structuralAckBytes = 124;
  final writer = _writer(
    _commonBytes + 4 + sourceVersionBytes + structuralAckBytes + 16,
  );
  _writeHeader(writer, 0, command.binding);
  writer.u32(command.refinementGeneration);
  _writePublicationSourceVersion(writer, command.sourceVersion);
  _writeStructuralAck(writer, command.baseAck);
  writer
    ..u32(command.byteOffset)
    ..u32(command.utf16Offset)
    ..u32(command.affinity == FlarkV3InlinePointAffinity.before ? 0 : 1)
    ..u32(command.target.index);
  return _EncodedCommand(FlarkV3WireOpcode.parserRefineInline, writer.finish());
}

_EncodedCommand _encodeViewportPresentation(
  FlarkV3ParserSessionViewportPresentationCommand command,
) {
  const sourceVersionBytes = 44;
  const structuralAckBytes = 124;
  const viewportBodyBytes = 64;
  final writer = _writer(
    _commonBytes +
        4 +
        sourceVersionBytes +
        structuralAckBytes +
        viewportBodyBytes,
  );
  _writeHeader(writer, 0, command.binding);
  writer.u32(command.viewportGeneration);
  _writePublicationSourceVersion(writer, command.sourceVersion);
  _writeStructuralAck(writer, command.baseAck);
  writer
    ..u32(command.requestedStartUtf8)
    ..u32(command.requestedStartUtf16)
    ..u32(command.requestedEndUtf8)
    ..u32(command.requestedEndUtf16)
    ..u32(command.startBlockOrdinal.lowWord)
    ..u32(command.startBlockOrdinal.highWord)
    ..u32(command.startUtf8)
    ..u32(command.startUtf16)
    ..u32(command.limits.maximumStructuralEntries)
    ..u32(command.limits.maximumStoragePages)
    ..u32(command.limits.maximumInlineLeaves)
    ..u32(command.limits.maximumInlineLeafSourceBytes)
    ..u32(command.limits.maximumInlineSourceBytes)
    ..u32(command.limits.maximumFactRecords)
    ..u32(command.limits.maximumEncodedFrameBytes)
    ..u32(command.limits.maximumParserTransitions);
  return _EncodedCommand(
    FlarkV3WireOpcode.parserPresentViewport,
    writer.finish(),
  );
}

FlarkV3ParserSessionInlineRefinementCommand _readInlineRefinement(
  _PayloadReader reader,
  _Header header,
) {
  final refinementGeneration = reader.u32();
  final sourceVersion = _readPublicationSourceVersion(reader);
  final baseAck = _readStructuralAck(reader);
  final byteOffset = reader.u32();
  final utf16Offset = reader.u32();
  final affinityTag = reader.u32();
  if (affinityTag > 1) {
    throw _variant(reader, affinityTag);
  }
  final targetTag = reader.u32();
  if (targetTag >= FlarkV3InlineRefinementTarget.values.length) {
    throw _variant(reader, targetTag);
  }
  return FlarkV3ParserSessionInlineRefinementCommand(
    binding: header.binding,
    refinementGeneration: refinementGeneration,
    sourceVersion: sourceVersion,
    baseAck: baseAck,
    byteOffset: byteOffset,
    utf16Offset: utf16Offset,
    affinity: FlarkV3InlinePointAffinity.values[affinityTag],
    target: FlarkV3InlineRefinementTarget.values[targetTag],
  );
}

FlarkV3ParserSessionViewportPresentationCommand _readViewportPresentation(
  _PayloadReader reader,
  _Header header,
) {
  final viewportGeneration = reader.u32();
  final sourceVersion = _readPublicationSourceVersion(reader);
  final baseAck = _readStructuralAck(reader);
  final requestedStartUtf8 = reader.u32();
  final requestedStartUtf16 = reader.u32();
  final requestedEndUtf8 = reader.u32();
  final requestedEndUtf16 = reader.u32();
  final startBlockOrdinal = FlarkV3ProtocolU64(
    lowWord: reader.u32(),
    highWord: reader.u32(),
  );
  final startUtf8 = reader.u32();
  final startUtf16 = reader.u32();
  final limits = FlarkV3ParserViewportPresentationLimits(
    maximumStructuralEntries: reader.u32(),
    maximumStoragePages: reader.u32(),
    maximumInlineLeaves: reader.u32(),
    maximumInlineLeafSourceBytes: reader.u32(),
    maximumInlineSourceBytes: reader.u32(),
    maximumFactRecords: reader.u32(),
    maximumEncodedFrameBytes: reader.u32(),
    maximumParserTransitions: reader.u32(),
  );
  return FlarkV3ParserSessionViewportPresentationCommand(
    binding: header.binding,
    viewportGeneration: viewportGeneration,
    sourceVersion: sourceVersion,
    baseAck: baseAck,
    requestedStartUtf8: requestedStartUtf8,
    requestedStartUtf16: requestedStartUtf16,
    requestedEndUtf8: requestedEndUtf8,
    requestedEndUtf16: requestedEndUtf16,
    startBlockOrdinal: startBlockOrdinal,
    startUtf8: startUtf8,
    startUtf16: startUtf16,
    limits: limits,
  );
}

_EncodedCommand _encodeClose(FlarkV3ParserSessionBeginCloseCommand command) {
  _u32(command.activeGeneration, 'activeGeneration');
  if (command.activeGeneration != 0 &&
      command.activeGeneration != command.binding.workerGeneration) {
    throw ArgumentError('Active close generation does not match binding.');
  }
  final writer = _writer(_commonBytes + 4);
  _writeHeader(writer, 0, command.binding);
  writer.u32(command.activeGeneration);
  return _EncodedCommand(FlarkV3WireOpcode.close, writer.finish());
}

_EncodedCommand _encodeClosed(FlarkV3ParserSessionClosedEvent event) {
  final writer = _writer(_commonBytes);
  _writeHeader(writer, 1, event.binding);
  return _EncodedCommand(FlarkV3WireOpcode.close, writer.finish());
}

_EncodedCommand _encodeDrainGrant(FlarkV3ParserSessionDrainGrant command) {
  final writer = _writer(_commonBytes + 8);
  _writeHeader(writer, 0, command.binding);
  writer
    ..u32(command.drainId)
    ..u32(command.maximumTransitions);
  return _EncodedCommand(FlarkV3WireOpcode.drain, writer.finish());
}

_EncodedCommand _encodeDrainProgress(
  FlarkV3ParserSessionDrainProgressEvent event,
) {
  final writer = _writer(_commonBytes + 24);
  _writeHeader(writer, 1, event.binding);
  writer
    ..u32(event.drainId)
    ..u32(event.releasedSourceLeases)
    ..u32(event.releasedSourceBytes)
    ..u32(event.arenaTransitions)
    ..u32(event.arenaNodesReclaimed)
    ..u32(event.complete ? 1 : 0);
  return _EncodedCommand(FlarkV3WireOpcode.drain, writer.finish());
}

_EncodedCommand _encodeReceipt(
  FlarkV3ParserSessionEventReceiptCommand command,
) {
  _positiveU32(command.eventId, 'eventId');
  final source = command.sourceSync;
  final certification = command.sourceCertification;
  if (source != null) _validateSourceReceipt(source);
  if (source != null && certification != null) {
    throw ArgumentError(
      'One event receipt cannot contain source sync and certification.',
    );
  }
  if (certification != null &&
      command.disposition != FlarkV3ParserEventDisposition.accepted) {
    throw ArgumentError('Canonical promotion proof requires acceptance.');
  }
  final writer = _writer(
    _commonBytes +
        8 +
        (source == null ? 0 : 24) +
        (certification == null ? 0 : _canonicalCompletionBytes),
  );
  _writeHeader(writer, command.disposition.index, command.binding);
  writer
    ..u32(source == null ? 0 : 1)
    ..u32(certification == null ? 0 : 1);
  if (source != null) {
    writer
      ..u32(source.disposition.index)
      ..u32(source.droppedIntentEntries)
      ..u32(source.droppedPayloadUtf16)
      ..u32(source.droppedDeletedUtf16)
      ..u32(source.droppedOperationCount)
      ..u32(source.workerRevision);
  }
  if (certification != null) {
    _writeCanonicalPromotionProof(writer, command.binding, certification);
  }
  return _EncodedCommand(FlarkV3WireOpcode.parserAcknowledge, writer.finish());
}

_EncodedCommand _encodeSourceAcknowledgement(
  FlarkV3ParserSessionSourceSynchronizedEvent event,
) {
  final acknowledgement = event.acknowledgement;
  _validateAcknowledgementBinding(event.binding, acknowledgement);
  _validateAcknowledgementShape(acknowledgement);
  final writer = _writer(_commonBytes + 20 + _observedReplicaBytes);
  switch (acknowledgement) {
    case FlarkV3SourceSnapshotSyncAcknowledgement():
      final seed = acknowledgement.startUtf16 == 0;
      _writeHeader(writer, seed ? 2 : 3, event.binding);
      writer
        ..u32(acknowledgement.leaseId)
        ..u32(acknowledgement.baseUiRevision)
        ..u32(acknowledgement.startUtf16)
        ..u32(acknowledgement.endUtf16)
        ..u32(acknowledgement.throughIntentSequence);
      _writeObservedReplica(writer, acknowledgement.observedReplica);
      return _EncodedCommand(FlarkV3WireOpcode.snapshotPage, writer.finish());
    case FlarkV3SourceIntentSyncAcknowledgement():
      _writeHeader(writer, 1, event.binding);
      writer
        ..u32(acknowledgement.leaseId)
        ..u32(acknowledgement.firstSequence)
        ..u32(acknowledgement.lastSequence)
        ..u32(acknowledgement.entryCount)
        ..u32(acknowledgement.payloadUtf16);
      _writeObservedReplica(writer, acknowledgement.observedReplica);
      return _EncodedCommand(FlarkV3WireOpcode.edit, writer.finish());
  }
}

_EncodedCommand _encodeFailure(FlarkV3ParserSessionFailedEvent event) {
  _u32(event.failureCode, 'failureCode');
  final writer = _writer(_commonBytes + 4);
  _writeHeader(writer, 1, event.binding);
  writer.u32(event.failureCode);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeSourceFactsPage(
  FlarkV3ParserSessionSourceFactsPageEvent event,
) {
  final page = event.page;
  _validateCanonicalPage(event.binding, page);
  final writer = _writer(
    _commonBytes + 40 + page.pageCheckpointCount * _sourceFactCheckpointBytes,
  );
  _writeHeader(writer, 2, event.binding);
  _writeCanonicalLineage(writer, page.lineage);
  writer
    ..u32(page.checkpointSpacingUtf16)
    ..u32(page.pageOrdinal)
    ..u32(page.pageCount)
    ..u32(page.checkpointCount)
    ..u32(page.pageCheckpointCount);
  for (final fact in page.checkpoints) {
    writer
      ..u32(fact.utf8Offset)
      ..u32(fact.utf16Offset)
      ..u32(fact.newlines);
    _writeContentHash(writer, fact.hash);
  }
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeSourceFactsCompletion(
  FlarkV3ParserSessionSourceFactsCompletedEvent event,
) {
  final completion = event.completion;
  _validateCanonicalCompletion(event.binding, completion);
  final writer = _writer(_commonBytes + _canonicalCompletionBytes);
  _writeHeader(writer, 3, event.binding);
  _writeCanonicalCompletion(writer, completion);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeSourceFactsDeltaBegin(
  FlarkV3ParserSessionSourceFactsDeltaBeginEvent event,
) {
  final header = event.header;
  _validateCanonicalDeltaHeader(event.binding, header);
  final writer = _writer(_commonBytes + _canonicalDeltaBeginBytes);
  _writeHeader(writer, 4, event.binding);
  _writeCanonicalLineage(writer, header.lineage);
  writer
    ..u32(header.baseFingerprint.revision)
    ..u32(header.baseFingerprint.utf16Length)
    ..u32(header.baseFingerprint.utf8Length);
  _writeContentHash(writer, header.baseFingerprint.contentHash128);
  _writeContentHash(writer, header.baseCheckpointRootGuard128);
  writer
    ..u32(header.baseCheckpointCount)
    ..u32(header.basePageCount)
    ..u32(header.baseCheckpointSpacingUtf16)
    ..u32(header.basePageStart)
    ..u32(header.basePageEnd)
    ..u32(header.targetPageStart)
    ..u32(header.targetPageEnd)
    ..u32(header.targetCheckpointCount)
    ..u32(header.targetPageCount)
    ..u32(header.targetCheckpointRootGuardAlgorithm);
  _writeContentHash(writer, header.targetCheckpointRootGuard128);
  writer.u32(header.replacementCheckpointCount);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeSourceFactsDeltaPage(
  FlarkV3ParserSessionSourceFactsDeltaPageEvent event,
) {
  final page = event.page;
  _validateCanonicalDeltaPage(event.binding, page);
  final writer = _writer(
    _commonBytes + 28 + page.checkpointCount * _sourceFactCheckpointBytes,
  );
  _writeHeader(writer, 5, event.binding);
  _writeCanonicalLineage(writer, page.lineage);
  writer
    ..u32(page.pageOrdinal)
    ..u32(page.checkpointCount);
  for (final fact in page.checkpoints) {
    writer
      ..u32(fact.utf8Offset)
      ..u32(fact.utf16Offset)
      ..u32(fact.newlines);
    _writeContentHash(writer, fact.hash);
  }
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeSourceFactsDeltaCompletion(
  FlarkV3ParserSessionSourceFactsDeltaCompletedEvent event,
) {
  final completion = event.completion;
  _validateCanonicalDeltaCompletion(event.binding, completion);
  final writer = _writer(_commonBytes + _canonicalDeltaCompletionBytes);
  _writeHeader(writer, 6, event.binding);
  _writeCanonicalDeltaCompletion(writer, completion);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeInlineRefinementUnavailable(
  FlarkV3ParserSessionInlineRefinementUnavailableEvent event,
) {
  _positiveU32(event.refinementGeneration, 'refinementGeneration');
  _positiveU32(event.reasonCode, 'reasonCode');
  final writer = _writer(_commonBytes + 8);
  _writeHeader(writer, 7, event.binding);
  writer
    ..u32(event.refinementGeneration)
    ..u32(event.reasonCode);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

_EncodedCommand _encodeViewportPresentationUnavailable(
  FlarkV3ParserSessionViewportPresentationUnavailableEvent event,
) {
  _positiveU32(event.viewportGeneration, 'viewportGeneration');
  _positiveU32(event.reasonCode, 'reasonCode');
  final writer = _writer(_commonBytes + 8);
  _writeHeader(writer, 8, event.binding);
  writer
    ..u32(event.viewportGeneration)
    ..u32(event.reasonCode);
  return _EncodedCommand(FlarkV3WireOpcode.parserPoll, writer.finish());
}

FlarkV3ParserSessionSnapshotCommand _readSnapshot(
  _PayloadReader reader,
  _Header header, {
  required bool seed,
}) {
  final leaseId = reader.u32();
  final baseUiRevision = reader.u32();
  final startUtf16 = reader.u32();
  final endUtf16 = reader.u32();
  final totalUtf16Length = reader.u32();
  final throughIntentSequence = reader.u32();
  final targetStamp = _readSourceStamp(reader);
  final sourceUtf16Length = reader.u32();
  final sourceUtf8Bytes = reader.u32();
  if (sourceUtf16Length > FlarkV3ParserSessionWireCodec.maximumSnapshotUtf16) {
    throw _oversized(
      reader,
      sourceUtf16Length,
      FlarkV3ParserSessionWireCodec.maximumSnapshotUtf16,
    );
  }
  final source = reader.strictString(sourceUtf8Bytes);
  if (source.length != sourceUtf16Length) {
    throw _invalid(reader, source.length, sourceUtf16Length);
  }
  final command = FlarkV3ParserSessionSnapshotCommand(
    binding: header.binding,
    leaseId: leaseId,
    baseUiRevision: baseUiRevision,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    totalUtf16Length: totalUtf16Length,
    throughIntentSequence: throughIntentSequence,
    targetStamp: targetStamp,
    source: source,
  );
  _validateSnapshot(command, encodedBytes: sourceUtf8Bytes);
  if (command.isSeed != seed) {
    throw _invalid(reader, command.isSeed ? 1 : 0, seed ? 1 : 0);
  }
  return command;
}

FlarkV3ParserSessionEditCommand _readEdits(
  _PayloadReader reader,
  _Header header,
) {
  final leaseId = reader.u32();
  final firstSequence = reader.u32();
  final lastSequence = reader.u32();
  final intentCount = reader.u32();
  final operationCount = reader.u32();
  final payloadUtf16 = reader.u32();
  final payloadUtf8Bytes = reader.u32();
  if (intentCount == 0 ||
      intentCount > FlarkV3ParserSessionWireCodec.maximumIntentCount) {
    throw _oversized(
      reader,
      intentCount,
      FlarkV3ParserSessionWireCodec.maximumIntentCount,
    );
  }
  if (operationCount == 0 ||
      operationCount > FlarkV3ParserSessionWireCodec.maximumOperationCount) {
    throw _oversized(
      reader,
      operationCount,
      FlarkV3ParserSessionWireCodec.maximumOperationCount,
    );
  }
  if (payloadUtf16 > FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16) {
    throw _oversized(
      reader,
      payloadUtf16,
      FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16,
    );
  }
  if (payloadUtf8Bytes > reader.remaining) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.truncatedPayload,
      byteOffset: reader.offset,
      expected: payloadUtf8Bytes,
      actual: reader.remaining,
    );
  }

  final intents = <FlarkV3SourceIntent>[];
  var decodedOperations = 0;
  var decodedUtf16 = 0;
  var decodedUtf8 = 0;
  for (var intentIndex = 0; intentIndex < intentCount; intentIndex += 1) {
    final sequence = reader.u32();
    final baseUiRevision = reader.u32();
    final uiRevision = reader.u32();
    final intentOperationCount = reader.u32();
    final baseStamp = _readSourceStamp(reader);
    final targetStamp = _readSourceStamp(reader);
    if (intentOperationCount == 0 ||
        decodedOperations + intentOperationCount > operationCount) {
      throw _invalid(reader, intentOperationCount, operationCount);
    }
    final operations = <FlarkV3SourceIntentEdit>[];
    for (
      var operationIndex = 0;
      operationIndex < intentOperationCount;
      operationIndex += 1
    ) {
      final startUtf16 = reader.u32();
      final endUtf16 = reader.u32();
      final replacementUtf16 = reader.u32();
      final replacementUtf8 = reader.u32();
      if (replacementUtf16 >
          FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16) {
        throw _oversized(
          reader,
          replacementUtf16,
          FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16,
        );
      }
      final replacement = reader.strictString(replacementUtf8);
      if (replacement.length != replacementUtf16 || endUtf16 < startUtf16) {
        throw _invalid(reader, replacement.length, replacementUtf16);
      }
      decodedUtf16 += replacementUtf16;
      decodedUtf8 += replacementUtf8;
      operations.add(
        FlarkV3SourceIntentEdit(
          startUtf16: startUtf16,
          endUtf16: endUtf16,
          replacement: FlarkV3StringSourcePayload(replacement),
        ),
      );
    }
    decodedOperations += intentOperationCount;
    intents.add(
      FlarkV3SourceIntent(
        workerGeneration: header.binding.workerGeneration,
        sequence: sequence,
        baseUiRevision: baseUiRevision,
        uiRevision: uiRevision,
        baseStamp: baseStamp,
        targetStamp: targetStamp,
        operations: operations,
      ),
    );
  }
  final command = FlarkV3ParserSessionEditCommand(
    binding: header.binding,
    leaseId: leaseId,
    intents: intents,
    payloadUtf16: payloadUtf16,
  );
  _validateIntentShape(command);
  if (command.firstSequence != firstSequence ||
      command.lastSequence != lastSequence ||
      decodedOperations != operationCount ||
      decodedUtf16 != payloadUtf16 ||
      decodedUtf8 != payloadUtf8Bytes) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return command;
}

FlarkV3ParserSessionEventReceiptCommand _readReceipt(
  _PayloadReader reader,
  _Header header,
  int eventId,
) {
  if (header.variant >= FlarkV3ParserEventDisposition.values.length) {
    throw _variant(reader, header.variant);
  }
  final hasSource = reader.u32();
  if (hasSource > 1) throw _invalid(reader, hasSource, 1);
  final hasCertification = reader.u32();
  if (hasCertification > 1) throw _invalid(reader, hasCertification, 1);
  if (hasSource == 1 && hasCertification == 1) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  FlarkV3SourceWorkerSyncAckReceipt? source;
  if (hasSource == 1) {
    final disposition = reader.u32();
    if (disposition >= FlarkV3SourceWorkerSyncAckDisposition.values.length) {
      throw _variant(reader, disposition);
    }
    final droppedIntentEntries = reader.u32();
    final droppedPayloadUtf16 = reader.u32();
    final droppedDeletedUtf16 = reader.u32();
    final droppedOperationCount = reader.u32();
    final workerRevision = reader.u32();
    source = disposition == FlarkV3SourceWorkerSyncAckDisposition.stale.index
        ? FlarkV3SourceWorkerSyncAckReceipt.stale(
            workerRevision: workerRevision,
          )
        : FlarkV3SourceWorkerSyncAckReceipt.acknowledged(
            droppedIntentEntries: droppedIntentEntries,
            droppedPayloadUtf16: droppedPayloadUtf16,
            droppedDeletedUtf16: droppedDeletedUtf16,
            droppedOperationCount: droppedOperationCount,
            workerRevision: workerRevision,
          );
    _validateSourceReceipt(source);
    if (source.disposition == FlarkV3SourceWorkerSyncAckDisposition.stale &&
        (droppedIntentEntries != 0 ||
            droppedPayloadUtf16 != 0 ||
            droppedDeletedUtf16 != 0 ||
            droppedOperationCount != 0)) {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }
  final sourceCertification = hasCertification == 1
      ? _readCanonicalPromotionProof(reader, header.binding)
      : null;
  if (sourceCertification != null &&
      header.variant != FlarkV3ParserEventDisposition.accepted.index) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionEventReceiptCommand(
    binding: header.binding,
    eventId: eventId,
    disposition: FlarkV3ParserEventDisposition.values[header.variant],
    sourceSync: source,
    sourceCertification: sourceCertification,
  );
}

FlarkV3ParserSessionSourceFactsPageEvent _readSourceFactsPage(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final lineage = _readCanonicalLineage(reader, header.binding);
  final spacing = reader.u32();
  final pageOrdinal = reader.u32();
  final pageCount = reader.u32();
  final checkpointCount = reader.u32();
  final pageCheckpointCount = reader.u32();
  if (pageCheckpointCount == 0 ||
      pageCheckpointCount > _maximumCanonicalSourceFactCheckpoints) {
    throw _oversized(
      reader,
      pageCheckpointCount,
      _maximumCanonicalSourceFactCheckpoints,
    );
  }
  final checkpoints = <FlarkV3SourcePrefixFacts>[];
  for (var index = 0; index < pageCheckpointCount; index += 1) {
    checkpoints.add(
      FlarkV3SourcePrefixFacts(
        utf8Offset: reader.u32(),
        utf16Offset: reader.u32(),
        newlines: reader.u32(),
        hash: _readContentHash(reader),
      ),
    );
  }
  final page = FlarkV3CanonicalSourceFactCheckpointPage(
    lineage: lineage,
    pageOrdinal: pageOrdinal,
    pageCount: pageCount,
    checkpointCount: checkpointCount,
    checkpointSpacingUtf16: spacing,
    checkpoints: checkpoints,
  );
  try {
    _validateCanonicalPage(header.binding, page);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionSourceFactsPageEvent(
    binding: header.binding,
    eventId: eventId,
    page: page,
  );
}

FlarkV3ParserSessionSourceFactsCompletedEvent _readSourceFactsCompletion(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final proof = _readCanonicalPromotionProof(reader, header.binding);
  return FlarkV3ParserSessionSourceFactsCompletedEvent(
    binding: header.binding,
    eventId: eventId,
    completion: FlarkV3CanonicalSourceFactCompletion(
      lineage: proof.lineage,
      fingerprintAlgorithm: proof.fingerprintAlgorithm,
      fingerprint: proof.fingerprint,
      logicalLineBreaks: proof.logicalLineBreaks,
      checkpointSpacingUtf16: proof.checkpointSpacingUtf16,
      checkpointCount: proof.checkpointCount,
      pageCount: proof.pageCount,
      checkpointHash128: proof.checkpointHash128,
    ),
  );
}

FlarkV3ParserSessionSourceFactsDeltaBeginEvent _readSourceFactsDeltaBegin(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final lineage = _readCanonicalLineage(reader, header.binding);
  final baseRevision = reader.u32();
  final baseUtf16Length = reader.u32();
  final baseUtf8Length = reader.u32();
  final baseContentHash = _readContentHash(reader);
  final value = FlarkV3ParserSourceFactsDeltaHeader(
    lineage: lineage,
    baseFingerprint: FlarkV3SourceFingerprint(
      revision: baseRevision,
      utf16Length: baseUtf16Length,
      utf8Length: baseUtf8Length,
      contentHash128: baseContentHash,
    ),
    baseCheckpointRootGuard128: _readContentHash(reader),
    baseCheckpointCount: reader.u32(),
    basePageCount: reader.u32(),
    baseCheckpointSpacingUtf16: reader.u32(),
    basePageStart: reader.u32(),
    basePageEnd: reader.u32(),
    targetPageStart: reader.u32(),
    targetPageEnd: reader.u32(),
    targetCheckpointCount: reader.u32(),
    targetPageCount: reader.u32(),
    targetCheckpointRootGuardAlgorithm: reader.u32(),
    targetCheckpointRootGuard128: _readContentHash(reader),
    replacementCheckpointCount: reader.u32(),
  );
  try {
    _validateCanonicalDeltaHeader(header.binding, value);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionSourceFactsDeltaBeginEvent(
    binding: header.binding,
    eventId: eventId,
    header: value,
  );
}

FlarkV3ParserSessionSourceFactsDeltaPageEvent _readSourceFactsDeltaPage(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final lineage = _readCanonicalLineage(reader, header.binding);
  final pageOrdinal = reader.u32();
  final checkpointCount = reader.u32();
  if (checkpointCount == 0 ||
      checkpointCount > _maximumCanonicalSourceFactCheckpoints) {
    throw _oversized(
      reader,
      checkpointCount,
      _maximumCanonicalSourceFactCheckpoints,
    );
  }
  final checkpoints = <FlarkV3SourcePrefixFacts>[];
  for (var index = 0; index < checkpointCount; index += 1) {
    checkpoints.add(
      FlarkV3SourcePrefixFacts(
        utf8Offset: reader.u32(),
        utf16Offset: reader.u32(),
        newlines: reader.u32(),
        hash: _readContentHash(reader),
      ),
    );
  }
  final page = FlarkV3CanonicalSourceFactDeltaCheckpointPage(
    lineage: lineage,
    pageOrdinal: pageOrdinal,
    checkpoints: checkpoints,
  );
  try {
    _validateCanonicalDeltaPage(header.binding, page);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionSourceFactsDeltaPageEvent(
    binding: header.binding,
    eventId: eventId,
    page: page,
  );
}

FlarkV3ParserSessionSourceFactsDeltaCompletedEvent
_readSourceFactsDeltaCompletion(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final proof = _readCanonicalPromotionProof(reader, header.binding);
  final completion = FlarkV3CanonicalSourceFactDeltaCompletion(
    lineage: proof.lineage,
    fingerprintAlgorithm: proof.fingerprintAlgorithm,
    fingerprint: proof.fingerprint,
    logicalLineBreaks: proof.logicalLineBreaks,
    checkpointSpacingUtf16: proof.checkpointSpacingUtf16,
    checkpointCount: proof.checkpointCount,
    pageCount: proof.pageCount,
    checkpointRootGuardAlgorithm: reader.u32(),
    checkpointRootGuard128: proof.checkpointHash128,
    replacementCheckpointHash128: _readContentHash(reader),
  );
  try {
    _validateCanonicalDeltaCompletion(header.binding, completion);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionSourceFactsDeltaCompletedEvent(
    binding: header.binding,
    eventId: eventId,
    completion: completion,
  );
}

FlarkV3CanonicalSourcePromotionProof _readCanonicalPromotionProof(
  _PayloadReader reader,
  FlarkV3ParserSessionBinding binding,
) {
  final lineage = _readCanonicalLineage(reader, binding);
  final fingerprintAlgorithm = reader.u32();
  final fingerprint = FlarkV3SourceFingerprint(
    revision: reader.u32(),
    utf16Length: reader.u32(),
    utf8Length: reader.u32(),
    contentHash128: FlarkV3ContentHash128.zero,
  );
  final logicalLineBreaks = reader.u32();
  final checkpointSpacingUtf16 = reader.u32();
  final checkpointCount = reader.u32();
  final pageCount = reader.u32();
  final contentHash = _readContentHash(reader);
  final checkpointHash = _readContentHash(reader);
  final proof = FlarkV3CanonicalSourcePromotionProof(
    lineage: lineage,
    fingerprintAlgorithm: fingerprintAlgorithm,
    fingerprint: FlarkV3SourceFingerprint(
      revision: fingerprint.revision,
      utf16Length: fingerprint.utf16Length,
      utf8Length: fingerprint.utf8Length,
      contentHash128: contentHash,
    ),
    logicalLineBreaks: logicalLineBreaks,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
    checkpointCount: checkpointCount,
    pageCount: pageCount,
    checkpointHash128: checkpointHash,
  );
  try {
    _validateCanonicalProof(binding, proof);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return proof;
}

FlarkV3SourceCertificationLineage _readCanonicalLineage(
  _PayloadReader reader,
  FlarkV3ParserSessionBinding binding,
) => FlarkV3SourceCertificationLineage(
  sourceSessionIdentity: binding.sourceSessionIdentity,
  requestId: reader.u32(),
  workerGeneration: binding.workerGeneration,
  workerReplicaRevision: reader.u32(),
  uiRevision: reader.u32(),
  utf16Length: reader.u32(),
  intentHighWater: reader.u32(),
);

FlarkV3ContentHash128 _readContentHash(_PayloadReader reader) =>
    FlarkV3ContentHash128(
      reader.u32(),
      reader.u32(),
      reader.u32(),
      reader.u32(),
    );

FlarkV3ParserSessionSourceSynchronizedEvent _readSnapshotAcknowledgement(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
  required bool seed,
}) {
  final acknowledgement = FlarkV3SourceSnapshotSyncAcknowledgement(
    sourceSessionIdentity: header.binding.sourceSessionIdentity,
    leaseId: reader.u32(),
    workerGeneration: header.binding.workerGeneration,
    baseUiRevision: reader.u32(),
    startUtf16: reader.u32(),
    endUtf16: reader.u32(),
    throughIntentSequence: reader.u32(),
    observedReplica: _readObservedReplica(reader),
  );
  _validateAcknowledgementBinding(header.binding, acknowledgement);
  _validateDecodedAcknowledgementShape(acknowledgement, reader);
  if ((acknowledgement.startUtf16 == 0) != seed) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3ParserSessionSourceSynchronizedEvent(
    binding: header.binding,
    eventId: eventId,
    acknowledgement: acknowledgement,
  );
}

FlarkV3ParserSessionSourceSynchronizedEvent _readEditAcknowledgement(
  _PayloadReader reader,
  _Header header, {
  required int eventId,
}) {
  final acknowledgement = FlarkV3SourceIntentSyncAcknowledgement(
    sourceSessionIdentity: header.binding.sourceSessionIdentity,
    leaseId: reader.u32(),
    workerGeneration: header.binding.workerGeneration,
    firstSequence: reader.u32(),
    lastSequence: reader.u32(),
    entryCount: reader.u32(),
    payloadUtf16: reader.u32(),
    observedReplica: _readRequiredObservedReplica(reader),
  );
  _validateAcknowledgementBinding(header.binding, acknowledgement);
  _validateDecodedAcknowledgementShape(acknowledgement, reader);
  return FlarkV3ParserSessionSourceSynchronizedEvent(
    binding: header.binding,
    eventId: eventId,
    acknowledgement: acknowledgement,
  );
}

List<_EncodedIntent> _prepareIntents(FlarkV3ParserSessionEditCommand command) {
  _validateIntentShape(command);
  final encoded = <_EncodedIntent>[];
  for (final intent in command.intents) {
    final operations = <_EncodedEdit>[];
    for (final operation in intent.operations) {
      final replacement = operation.replacement.readRange(
        0,
        operation.replacement.utf16Length,
      );
      if (replacement.length != operation.replacement.utf16Length) {
        throw ArgumentError('Source payload materialized a different length.');
      }
      operations.add(
        _EncodedEdit(
          startUtf16: operation.startUtf16,
          endUtf16: operation.endUtf16,
          replacementUtf16: replacement.length,
          replacementUtf8: _strictUtf8(replacement, 'replacement'),
        ),
      );
    }
    encoded.add(_EncodedIntent(intent: intent, operations: operations));
  }
  return encoded;
}

void _writeSourceStamp(_PayloadWriter writer, FlarkV3SourceStamp stamp) {
  _validateSourceStamp(stamp);
  switch (stamp) {
    case FlarkV3ProvisionalSourceStamp():
      writer
        ..u32(0)
        ..u32(stamp.revision)
        ..u32(stamp.utf16Length)
        ..u32(0)
        ..u32(0)
        ..u32(0)
        ..u32(0)
        ..u32(0);
    case FlarkV3KnownSourceStamp():
      writer
        ..u32(1)
        ..u32(stamp.revision)
        ..u32(stamp.utf16Length)
        ..u32(stamp.utf8Length)
        ..u32(stamp.contentHash128.word0)
        ..u32(stamp.contentHash128.word1)
        ..u32(stamp.contentHash128.word2)
        ..u32(stamp.contentHash128.word3);
  }
}

FlarkV3SourceStamp _readSourceStamp(_PayloadReader reader) {
  final tag = reader.u32();
  final revision = reader.u32();
  final utf16Length = reader.u32();
  final utf8Length = reader.u32();
  final word0 = reader.u32();
  final word1 = reader.u32();
  final word2 = reader.u32();
  final word3 = reader.u32();
  switch (tag) {
    case 0:
      if (utf8Length != 0 ||
          word0 != 0 ||
          word1 != 0 ||
          word2 != 0 ||
          word3 != 0) {
        throw _invalid(reader, 1, 0);
      }
      return FlarkV3ProvisionalSourceStamp(
        revision: revision,
        utf16Length: utf16Length,
      );
    case 1:
      return FlarkV3KnownSourceStamp(
        revision: revision,
        utf16Length: utf16Length,
        utf8Length: utf8Length,
        contentHash128: FlarkV3ContentHash128(word0, word1, word2, word3),
      );
    default:
      throw _invalid(reader, tag, 1);
  }
}

void _writeObservedReplica(
  _PayloadWriter writer,
  FlarkV3ObservedSourceReplicaVersion? observed,
) {
  if (observed == null) {
    writer
      ..u32(0)
      ..u32(0)
      ..u32(0)
      ..u32(0)
      ..u32(0);
    return;
  }
  _validateObservedReplica(observed);
  writer
    ..u32(1)
    ..u32(observed.revision)
    ..u32(observed.utf16Length)
    ..u32(observed.utf8Length)
    ..u32(observed.intentHighWater);
}

FlarkV3ObservedSourceReplicaVersion? _readObservedReplica(
  _PayloadReader reader,
) {
  final tag = reader.u32();
  final revision = reader.u32();
  final utf16Length = reader.u32();
  final utf8Length = reader.u32();
  final intentHighWater = reader.u32();
  if (tag == 0) {
    if (revision != 0 ||
        utf16Length != 0 ||
        utf8Length != 0 ||
        intentHighWater != 0) {
      throw _invalid(reader, 1, 0);
    }
    return null;
  }
  if (tag != 1) throw _invalid(reader, tag, 1);
  return FlarkV3ObservedSourceReplicaVersion(
    revision: revision,
    utf16Length: utf16Length,
    utf8Length: utf8Length,
    intentHighWater: intentHighWater,
  );
}

FlarkV3ObservedSourceReplicaVersion _readRequiredObservedReplica(
  _PayloadReader reader,
) {
  final observed = _readObservedReplica(reader);
  if (observed == null) throw _invalid(reader, 0, 1);
  return observed;
}

void _validateSourceStamp(FlarkV3SourceStamp stamp) {
  _u32(stamp.revision, 'stamp.revision');
  _u32(stamp.utf16Length, 'stamp.utf16Length');
  if (stamp case final FlarkV3KnownSourceStamp known) {
    _u32(known.utf8Length, 'stamp.utf8Length');
    _u32(known.contentHash128.word0, 'stamp.contentHash128.word0');
    _u32(known.contentHash128.word1, 'stamp.contentHash128.word1');
    _u32(known.contentHash128.word2, 'stamp.contentHash128.word2');
    _u32(known.contentHash128.word3, 'stamp.contentHash128.word3');
  }
}

void _validateObservedReplica(FlarkV3ObservedSourceReplicaVersion observed) {
  _u32(observed.revision, 'observedReplica.revision');
  _u32(observed.utf16Length, 'observedReplica.utf16Length');
  _u32(observed.utf8Length, 'observedReplica.utf8Length');
  _u32(observed.intentHighWater, 'observedReplica.intentHighWater');
}

void _validateIntentShape(FlarkV3ParserSessionEditCommand command) {
  _positiveU32(command.leaseId, 'leaseId');
  if (command.intents.isEmpty ||
      command.intents.length >
          FlarkV3ParserSessionWireCodec.maximumIntentCount) {
    throw RangeError.range(
      command.intents.length,
      1,
      FlarkV3ParserSessionWireCodec.maximumIntentCount,
      'intents.length',
    );
  }
  var operationCount = 0;
  var payloadUtf16 = 0;
  int? priorSequence;
  int? expectedBaseRevision;
  FlarkV3SourceStamp? expectedBaseStamp;
  for (final intent in command.intents) {
    if (intent.workerGeneration != command.binding.workerGeneration) {
      throw ArgumentError('Intent generation crosses its session binding.');
    }
    _positiveU32(intent.sequence, 'intent.sequence');
    _u32(intent.baseUiRevision, 'intent.baseUiRevision');
    _positiveU32(intent.uiRevision, 'intent.uiRevision');
    _validateSourceStamp(intent.baseStamp);
    _validateSourceStamp(intent.targetStamp);
    if (intent.uiRevision != intent.baseUiRevision + 1 ||
        intent.baseStamp.revision != intent.baseUiRevision ||
        intent.targetStamp.revision != intent.uiRevision ||
        (priorSequence != null && intent.sequence <= priorSequence) ||
        (expectedBaseRevision != null &&
            intent.baseUiRevision != expectedBaseRevision) ||
        (expectedBaseStamp != null && intent.baseStamp != expectedBaseStamp) ||
        intent.operations.isEmpty) {
      throw ArgumentError(
        'Intent page does not form one exact revision chain.',
      );
    }
    priorSequence = intent.sequence;
    expectedBaseRevision = intent.uiRevision;
    expectedBaseStamp = intent.targetStamp;
    operationCount += intent.operations.length;
    int? priorOperationStart;
    int? priorOperationEnd;
    for (final operation in intent.operations) {
      _u32(operation.startUtf16, 'operation.startUtf16');
      _u32(operation.endUtf16, 'operation.endUtf16');
      if (operation.endUtf16 < operation.startUtf16 ||
          (priorOperationStart != null &&
              (operation.startUtf16 < priorOperationStart ||
                  (operation.startUtf16 == priorOperationStart &&
                      operation.endUtf16 < priorOperationEnd!))) ||
          (priorOperationEnd != null &&
              operation.startUtf16 < priorOperationEnd)) {
        throw ArgumentError(
          'Intent operations must be source ordered and non-overlapping.',
        );
      }
      priorOperationStart = operation.startUtf16;
      priorOperationEnd = operation.endUtf16;
      payloadUtf16 += operation.replacement.utf16Length;
    }
    if (intent.targetStamp.utf16Length !=
        intent.baseStamp.utf16Length -
            intent.deletedUtf16 +
            intent.payloadUtf16) {
      throw ArgumentError('Intent stamps disagree with its UTF-16 edits.');
    }
  }
  if (operationCount > FlarkV3ParserSessionWireCodec.maximumOperationCount ||
      payloadUtf16 != command.payloadUtf16 ||
      payloadUtf16 > FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16) {
    throw ArgumentError('Edit page totals exceed or disagree with its bounds.');
  }
}

void _validateSnapshot(
  FlarkV3ParserSessionSnapshotCommand command, {
  required int encodedBytes,
}) {
  _positiveU32(command.leaseId, 'leaseId');
  _u32(command.baseUiRevision, 'baseUiRevision');
  _u32(command.startUtf16, 'startUtf16');
  _u32(command.endUtf16, 'endUtf16');
  _u32(command.totalUtf16Length, 'totalUtf16Length');
  _u32(command.throughIntentSequence, 'throughIntentSequence');
  _validateSourceStamp(command.targetStamp);
  if (command.endUtf16 < command.startUtf16 ||
      command.endUtf16 > command.totalUtf16Length ||
      command.endUtf16 - command.startUtf16 != command.source.length ||
      command.source.length >
          FlarkV3ParserSessionWireCodec.maximumSnapshotUtf16 ||
      (command.startUtf16 > 0 && command.source.isEmpty) ||
      encodedBytes > FlarkV3WireProtocol.maximumPayloadBytes) {
    throw ArgumentError('Snapshot page coordinates or size are invalid.');
  }
  if (command.targetStamp.revision != command.baseUiRevision ||
      command.targetStamp.utf16Length != command.totalUtf16Length) {
    throw ArgumentError('Snapshot target stamp disagrees with its root.');
  }
}

void _validateSourceReceipt(FlarkV3SourceWorkerSyncAckReceipt receipt) {
  _u32(receipt.droppedIntentEntries, 'droppedIntentEntries');
  _u32(receipt.droppedPayloadUtf16, 'droppedPayloadUtf16');
  _u32(receipt.droppedDeletedUtf16, 'droppedDeletedUtf16');
  _u32(receipt.droppedOperationCount, 'droppedOperationCount');
  _u32(receipt.workerRevision, 'workerRevision');
  if (receipt.droppedIntentEntries >
          FlarkV3ParserSessionWireCodec.maximumIntentCount ||
      receipt.droppedPayloadUtf16 >
          FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16 ||
      receipt.droppedOperationCount >
          FlarkV3ParserSessionWireCodec.maximumOperationCount) {
    throw RangeError('Source receipt exceeds a source lease bound.');
  }
}

void _validateCanonicalLineage(
  FlarkV3ParserSessionBinding binding,
  FlarkV3SourceCertificationLineage lineage,
) {
  _positiveU32(lineage.requestId, 'certificationId');
  _u32(lineage.workerReplicaRevision, 'workerReplicaRevision');
  _u32(lineage.uiRevision, 'uiRevision');
  _u32(lineage.utf16Length, 'utf16Length');
  _u32(lineage.intentHighWater, 'intentHighWater');
  if (lineage.sourceSessionIdentity != binding.sourceSessionIdentity ||
      lineage.workerGeneration != binding.workerGeneration ||
      lineage.workerReplicaRevision != lineage.uiRevision) {
    throw ArgumentError('Canonical SourceFacts crosses its exact lineage.');
  }
}

void _validateCanonicalPage(
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourceFactCheckpointPage page,
) {
  _validateCanonicalLineage(binding, page.lineage);
  _u32(page.pageOrdinal, 'pageOrdinal');
  _positiveU32(page.pageCount, 'pageCount');
  _positiveU32(page.checkpointCount, 'checkpointCount');
  if (page.checkpointSpacingUtf16 < 2 ||
      page.checkpointSpacingUtf16 > 8192 ||
      page.pageOrdinal >= page.pageCount ||
      page.pageCount !=
          (page.checkpointCount + _maximumCanonicalSourceFactCheckpoints - 1) ~/
              _maximumCanonicalSourceFactCheckpoints) {
    throw ArgumentError('Canonical SourceFacts page shape is invalid.');
  }
  final expectedPageCheckpoints = math.min(
    _maximumCanonicalSourceFactCheckpoints,
    page.checkpointCount -
        page.pageOrdinal * _maximumCanonicalSourceFactCheckpoints,
  );
  if (page.isConsumed ||
      page.pageCheckpointCount != expectedPageCheckpoints ||
      expectedPageCheckpoints <= 0) {
    throw ArgumentError('Canonical SourceFacts page count is invalid.');
  }
  FlarkV3SourcePrefixFacts? prior;
  for (final fact in page.checkpoints) {
    _u32(fact.utf8Offset, 'checkpoint.utf8Offset');
    _u32(fact.utf16Offset, 'checkpoint.utf16Offset');
    _u32(fact.newlines, 'checkpoint.newlines');
    _validateContentHash(fact.hash);
    if (fact.utf8Offset == 0 ||
        fact.utf16Offset == 0 ||
        fact.utf16Offset > page.lineage.utf16Length ||
        fact.newlines > fact.utf16Offset ||
        (prior != null &&
            (fact.utf8Offset <= prior.utf8Offset ||
                fact.utf16Offset <= prior.utf16Offset ||
                fact.utf16Offset - prior.utf16Offset >
                    page.checkpointSpacingUtf16 + 1 ||
                fact.newlines < prior.newlines))) {
      throw ArgumentError('Canonical SourceFacts checkpoints regress.');
    }
    prior = fact;
  }
  final isFinal = page.pageOrdinal + 1 == page.pageCount;
  if (isFinal != (prior!.utf16Offset == page.lineage.utf16Length)) {
    throw ArgumentError('Canonical SourceFacts terminal coverage is invalid.');
  }
}

void _validateCanonicalCompletion(
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourceFactCompletion completion,
) => _validateCanonicalProof(
  binding,
  FlarkV3CanonicalSourcePromotionProof(
    lineage: completion.lineage,
    fingerprintAlgorithm: completion.fingerprintAlgorithm,
    fingerprint: completion.fingerprint,
    logicalLineBreaks: completion.logicalLineBreaks,
    checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
    checkpointCount: completion.checkpointCount,
    pageCount: completion.pageCount,
    checkpointHash128: completion.checkpointHash128,
  ),
);

void _validateCanonicalDeltaHeader(
  FlarkV3ParserSessionBinding binding,
  FlarkV3ParserSourceFactsDeltaHeader header,
) {
  _validateCanonicalLineage(binding, header.lineage);
  _u32(header.baseFingerprint.revision, 'baseFingerprint.revision');
  _u32(header.baseFingerprint.utf16Length, 'baseFingerprint.utf16Length');
  _u32(header.baseFingerprint.utf8Length, 'baseFingerprint.utf8Length');
  _validateContentHash(header.baseFingerprint.contentHash128);
  _validateContentHash(header.baseCheckpointRootGuard128);
  _u32(header.baseCheckpointCount, 'baseCheckpointCount');
  _u32(header.basePageCount, 'basePageCount');
  _u32(header.basePageStart, 'basePageStart');
  _u32(header.basePageEnd, 'basePageEnd');
  _u32(header.targetPageStart, 'targetPageStart');
  _u32(header.targetPageEnd, 'targetPageEnd');
  _u32(header.targetCheckpointCount, 'targetCheckpointCount');
  _u32(header.targetPageCount, 'targetPageCount');
  _u32(
    header.targetCheckpointRootGuardAlgorithm,
    'targetCheckpointRootGuardAlgorithm',
  );
  _validateContentHash(header.targetCheckpointRootGuard128);
  _u32(header.replacementCheckpointCount, 'replacementCheckpointCount');

  final expectedBasePages = header.baseCheckpointCount == 0
      ? 0
      : (header.baseCheckpointCount +
                _maximumCanonicalSourceFactCheckpoints -
                1) ~/
            _maximumCanonicalSourceFactCheckpoints;
  final expectedTargetPages = header.targetCheckpointCount == 0
      ? 0
      : (header.targetCheckpointCount +
                _maximumCanonicalSourceFactCheckpoints -
                1) ~/
            _maximumCanonicalSourceFactCheckpoints;
  final removedPages = header.basePageEnd - header.basePageStart;
  final replacementPages = header.targetPageEnd - header.targetPageStart;
  final baseIsEmpty = header.baseFingerprint.utf16Length == 0;
  final targetIsEmpty = header.lineage.utf16Length == 0;
  if (header.baseFingerprint.revision >= header.lineage.uiRevision ||
      header.baseCheckpointSpacingUtf16 < 2 ||
      header.baseCheckpointSpacingUtf16 > 8192 ||
      header.basePageCount != expectedBasePages ||
      baseIsEmpty != (header.baseCheckpointCount == 0) ||
      (baseIsEmpty &&
          (header.baseFingerprint.utf8Length != 0 ||
              header.baseFingerprint.contentHash128 !=
                  FlarkV3ContentHash128.zero ||
              header.baseCheckpointRootGuard128 !=
                  FlarkV3ContentHash128.zero)) ||
      header.basePageEnd < header.basePageStart ||
      header.basePageEnd > header.basePageCount ||
      header.targetPageStart != header.basePageStart ||
      header.targetPageEnd < header.targetPageStart ||
      header.targetPageEnd > header.targetPageCount ||
      header.targetPageCount != expectedTargetPages ||
      targetIsEmpty != (header.targetCheckpointCount == 0) ||
      header.targetCheckpointRootGuardAlgorithm !=
          flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm ||
      (targetIsEmpty &&
          header.targetCheckpointRootGuard128 != FlarkV3ContentHash128.zero) ||
      header.targetPageCount !=
          header.basePageCount - removedPages + replacementPages ||
      header.replacementCheckpointCount >
          replacementPages * _maximumCanonicalSourceFactCheckpoints ||
      (replacementPages == 0) != (header.replacementCheckpointCount == 0)) {
    throw ArgumentError('Canonical SourceFacts delta header is invalid.');
  }
}

void _validateCanonicalDeltaPage(
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourceFactDeltaCheckpointPage page,
) {
  _validateCanonicalLineage(binding, page.lineage);
  _u32(page.pageOrdinal, 'pageOrdinal');
  if (page.isConsumed ||
      page.checkpointCount <= 0 ||
      page.checkpointCount > _maximumCanonicalSourceFactCheckpoints) {
    throw ArgumentError('Canonical SourceFacts delta page is invalid.');
  }
  FlarkV3SourcePrefixFacts? prior;
  for (final fact in page.checkpoints) {
    _u32(fact.utf8Offset, 'checkpoint.utf8Offset');
    _u32(fact.utf16Offset, 'checkpoint.utf16Offset');
    _u32(fact.newlines, 'checkpoint.newlines');
    _validateContentHash(fact.hash);
    if (fact.utf8Offset == 0 ||
        fact.utf16Offset == 0 ||
        fact.utf16Offset > page.lineage.utf16Length ||
        fact.newlines > fact.utf16Offset ||
        (prior != null &&
            (fact.utf8Offset <= prior.utf8Offset ||
                fact.utf16Offset <= prior.utf16Offset ||
                fact.newlines < prior.newlines))) {
      throw ArgumentError('Canonical SourceFacts delta checkpoints regress.');
    }
    prior = fact;
  }
}

void _validateCanonicalDeltaCompletion(
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourceFactDeltaCompletion completion,
) {
  _validateCanonicalProof(
    binding,
    FlarkV3CanonicalSourcePromotionProof(
      lineage: completion.lineage,
      fingerprintAlgorithm: completion.fingerprintAlgorithm,
      fingerprint: completion.fingerprint,
      logicalLineBreaks: completion.logicalLineBreaks,
      checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      checkpointHash128: completion.checkpointRootGuard128,
    ),
  );
  if (completion.checkpointRootGuardAlgorithm !=
      flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm) {
    throw ArgumentError('Canonical SourceFacts delta guard is unsupported.');
  }
  _validateContentHash(completion.checkpointRootGuard128);
  _validateContentHash(completion.replacementCheckpointHash128);
}

void _validateCanonicalProof(
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourcePromotionProof proof,
) {
  _validateCanonicalLineage(binding, proof.lineage);
  _u32(proof.fingerprint.revision, 'fingerprint.revision');
  _u32(proof.fingerprint.utf16Length, 'fingerprint.utf16Length');
  _u32(proof.fingerprint.utf8Length, 'fingerprint.utf8Length');
  _u32(proof.logicalLineBreaks, 'logicalLineBreaks');
  _u32(proof.checkpointCount, 'checkpointCount');
  _u32(proof.pageCount, 'pageCount');
  _validateContentHash(proof.fingerprint.contentHash128);
  _validateContentHash(proof.checkpointHash128);
  final expectedPageCount = proof.checkpointCount == 0
      ? 0
      : (proof.checkpointCount + _maximumCanonicalSourceFactCheckpoints - 1) ~/
            _maximumCanonicalSourceFactCheckpoints;
  if (proof.fingerprintAlgorithm != 1 ||
      proof.fingerprint.revision != proof.lineage.uiRevision ||
      proof.fingerprint.utf16Length != proof.lineage.utf16Length ||
      proof.logicalLineBreaks > proof.fingerprint.utf16Length ||
      proof.checkpointSpacingUtf16 < 2 ||
      proof.checkpointSpacingUtf16 > 8192 ||
      proof.pageCount != expectedPageCount ||
      (proof.fingerprint.utf16Length == 0) != (proof.checkpointCount == 0) ||
      (proof.fingerprint.utf16Length == 0 &&
          (proof.fingerprint.utf8Length != 0 ||
              proof.logicalLineBreaks != 0 ||
              proof.fingerprint.contentHash128 != FlarkV3ContentHash128.zero ||
              proof.checkpointHash128 != FlarkV3ContentHash128.zero))) {
    throw ArgumentError('Canonical SourceFacts completion is invalid.');
  }
}

void _validateContentHash(FlarkV3ContentHash128 hash) {
  _u32(hash.word0, 'hash.word0');
  _u32(hash.word1, 'hash.word1');
  _u32(hash.word2, 'hash.word2');
  _u32(hash.word3, 'hash.word3');
}

void _validateAcknowledgementBinding(
  FlarkV3ParserSessionBinding binding,
  FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
) {
  _positiveU32(acknowledgement.sourceSessionIdentity, 'sourceSessionIdentity');
  _positiveU32(acknowledgement.leaseId, 'leaseId');
  _positiveU32(acknowledgement.workerGeneration, 'workerGeneration');
  if (acknowledgement.sourceSessionIdentity != binding.sourceSessionIdentity ||
      acknowledgement.workerGeneration != binding.workerGeneration) {
    throw ArgumentError('Source acknowledgement crosses its session binding.');
  }
}

void _validateAcknowledgementShape(
  FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
) {
  switch (acknowledgement) {
    case FlarkV3SourceSnapshotSyncAcknowledgement():
      _u32(acknowledgement.baseUiRevision, 'baseUiRevision');
      _u32(acknowledgement.startUtf16, 'startUtf16');
      _u32(acknowledgement.endUtf16, 'endUtf16');
      _u32(acknowledgement.throughIntentSequence, 'throughIntentSequence');
      final observed = acknowledgement.observedReplica;
      if (observed != null) _validateObservedReplica(observed);
      if (acknowledgement.endUtf16 < acknowledgement.startUtf16 ||
          acknowledgement.endUtf16 - acknowledgement.startUtf16 >
              FlarkV3ParserSessionWireCodec.maximumSnapshotUtf16) {
        throw ArgumentError(
          'Snapshot acknowledgement coordinates are invalid.',
        );
      }
    case FlarkV3SourceIntentSyncAcknowledgement():
      _positiveU32(acknowledgement.firstSequence, 'firstSequence');
      _positiveU32(acknowledgement.lastSequence, 'lastSequence');
      _positiveU32(acknowledgement.entryCount, 'entryCount');
      _u32(acknowledgement.payloadUtf16, 'payloadUtf16');
      _validateObservedReplica(acknowledgement.observedReplica);
      if (acknowledgement.lastSequence < acknowledgement.firstSequence ||
          acknowledgement.entryCount >
              FlarkV3ParserSessionWireCodec.maximumIntentCount ||
          acknowledgement.payloadUtf16 >
              FlarkV3ParserSessionWireCodec.maximumIntentPayloadUtf16) {
        throw ArgumentError('Edit acknowledgement bounds are invalid.');
      }
  }
}

void _validateDecodedAcknowledgementShape(
  FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
  _PayloadReader reader,
) {
  try {
    _validateAcknowledgementShape(acknowledgement);
  } on ArgumentError {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
}

void _requireLeaseBinding(
  FlarkV3ParserSessionBinding binding,
  FlarkV3SourceWorkerSyncLease lease,
) {
  if (lease.sourceSessionIdentity != binding.sourceSessionIdentity ||
      lease.workerGeneration != binding.workerGeneration) {
    throw ArgumentError('Source lease crosses its parser-session binding.');
  }
}

void _validateCommandTransition(
  FlarkV3ParserSessionWireCommand command,
  FlarkV3ParserSessionBinding? established,
) {
  if (command case FlarkV3ParserSessionOpenCommand(:final mode)) {
    switch (mode) {
      case FlarkV3ParserOpenMode.fresh:
        if (established != null) {
          throw ArgumentError(
            'Fresh open cannot replace an established binding.',
          );
        }
      case FlarkV3ParserOpenMode.recovery:
        if (established == null ||
            command.binding.documentSession != established.documentSession ||
            command.binding.sourceSessionIdentity !=
                established.sourceSessionIdentity ||
            established.workerGeneration == flarkV3TransportV1Maximum ||
            command.binding.workerGeneration !=
                established.workerGeneration + 1) {
          throw ArgumentError(
            'Recovery open does not advance the exact binding.',
          );
        }
    }
    return;
  }
  if (established == null) {
    throw ArgumentError('Session command requires an established binding.');
  }
  _requireExactBinding(command.binding, established);
}

void _validateDecodedCommandTransition(
  FlarkV3ParserSessionWireCommand command,
  FlarkV3ParserSessionBinding? established,
  _PayloadReader reader,
) {
  if (command case FlarkV3ParserSessionOpenCommand(:final mode)) {
    switch (mode) {
      case FlarkV3ParserOpenMode.fresh:
        if (established != null) {
          throw _identity(
            reader,
            command.binding.workerGeneration,
            established.workerGeneration,
          );
        }
      case FlarkV3ParserOpenMode.recovery:
        if (established == null) {
          throw FlarkV3ParserSessionWireFormatException(
            FlarkV3ParserSessionWireFailure.identityMismatch,
            byteOffset: reader.offset,
          );
        }
        if (command.binding.documentSession != established.documentSession ||
            command.binding.sourceSessionIdentity !=
                established.sourceSessionIdentity ||
            established.workerGeneration == flarkV3TransportV1Maximum ||
            command.binding.workerGeneration !=
                established.workerGeneration + 1) {
          throw _identity(
            reader,
            command.binding.workerGeneration,
            established.workerGeneration == flarkV3TransportV1Maximum
                ? established.workerGeneration
                : established.workerGeneration + 1,
          );
        }
    }
    return;
  }
  if (established == null) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.identityMismatch,
      byteOffset: reader.offset,
    );
  }
  _requireDecodedBinding(command.binding, established, reader);
}

void _requireExactBinding(
  FlarkV3ParserSessionBinding actual,
  FlarkV3ParserSessionBinding expected,
) {
  if (actual != expected) {
    throw ArgumentError('Parser-session binding mismatch.');
  }
}

void _requireDecodedBinding(
  FlarkV3ParserSessionBinding actual,
  FlarkV3ParserSessionBinding expected,
  _PayloadReader reader,
) {
  if (actual != expected) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.identityMismatch,
      byteOffset: reader.offset,
    );
  }
}

_Header _readHeader(_PayloadReader reader) {
  final schema = reader.u16();
  if (schema != FlarkV3ParserSessionWireCodec.payloadSchema) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.unsupportedSchema,
      byteOffset: 0,
      expected: FlarkV3ParserSessionWireCodec.payloadSchema,
      actual: schema,
    );
  }
  final variant = reader.u16();
  final generation = reader.u32();
  if (generation == 0) {
    throw const FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: 4,
      expected: 1,
      actual: 0,
    );
  }
  final documentSession = FlarkV3DocumentSessionId(
    reader.u32(),
    reader.u32(),
    reader.u32(),
    reader.u32(),
  );
  final sourceSessionIdentity = reader.u32();
  if (sourceSessionIdentity == 0) {
    throw const FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: 24,
      expected: 1,
      actual: 0,
    );
  }
  return _Header(
    variant,
    FlarkV3ParserSessionBinding(
      documentSession: documentSession,
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: generation,
    ),
  );
}

void _writeHeader(
  _PayloadWriter writer,
  int variant,
  FlarkV3ParserSessionBinding binding,
) {
  writer
    ..u16(FlarkV3ParserSessionWireCodec.payloadSchema)
    ..u16(variant)
    ..u32(binding.workerGeneration)
    ..u32(binding.documentSession.word0)
    ..u32(binding.documentSession.word1)
    ..u32(binding.documentSession.word2)
    ..u32(binding.documentSession.word3)
    ..u32(binding.sourceSessionIdentity);
}

void _writeCanonicalLineage(
  _PayloadWriter writer,
  FlarkV3SourceCertificationLineage lineage,
) {
  writer
    ..u32(lineage.requestId)
    ..u32(lineage.workerReplicaRevision)
    ..u32(lineage.uiRevision)
    ..u32(lineage.utf16Length)
    ..u32(lineage.intentHighWater);
}

void _writeContentHash(_PayloadWriter writer, FlarkV3ContentHash128 hash) {
  writer
    ..u32(hash.word0)
    ..u32(hash.word1)
    ..u32(hash.word2)
    ..u32(hash.word3);
}

void _writePublicationSourceVersion(
  _PayloadWriter writer,
  FlarkV3SourceVersion source,
) {
  writer
    ..u32(source.documentSession.word0)
    ..u32(source.documentSession.word1)
    ..u32(source.documentSession.word2)
    ..u32(source.documentSession.word3)
    ..u32(source.revision)
    ..u32(source.metric.bytes)
    ..u32(source.metric.utf16)
    ..u32(source.contentHash.word0)
    ..u32(source.contentHash.word1)
    ..u32(source.contentHash.word2)
    ..u32(source.contentHash.word3);
}

FlarkV3SourceVersion _readPublicationSourceVersion(_PayloadReader reader) =>
    FlarkV3SourceVersion(
      documentSession: FlarkV3DocumentSessionId(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
      revision: reader.u32(),
      metric: FlarkV3SourceMetric(bytes: reader.u32(), utf16: reader.u32()),
      contentHash: FlarkV3ContentHash128(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
    );

void _writeStructuralAck(_PayloadWriter writer, FlarkV3StructuralAck ack) {
  writer
    ..u32(ack.publicationSession.word0)
    ..u32(ack.publicationSession.word1)
    ..u32(ack.publicationSession.word2)
    ..u32(ack.publicationSession.word3)
    ..u32(ack.hostRevision.value);
  _writePublicationSourceVersion(writer, ack.sourceVersion);
  writer
    ..u32(ack.sourceRoot.highWord)
    ..u32(ack.sourceRoot.lowWord)
    ..u32(ack.parseGeneration)
    ..u32(ack.grammarRevision)
    ..u32(ack.syntaxProfile.value)
    ..u32(ack.authorityMask.bits)
    ..u32(ack.recordCount)
    ..u32(ack.sequenceDigest.word0)
    ..u32(ack.sequenceDigest.word1)
    ..u32(ack.sequenceDigest.word2)
    ..u32(ack.sequenceDigest.word3)
    ..u32(ack.manifestDigest.word0)
    ..u32(ack.manifestDigest.word1)
    ..u32(ack.manifestDigest.word2)
    ..u32(ack.manifestDigest.word3);
}

FlarkV3StructuralAck _readStructuralAck(_PayloadReader reader) =>
    FlarkV3StructuralAck(
      publicationSession: FlarkV3PublicationSessionId(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
      hostRevision: FlarkV3HostRevisionId(reader.u32()),
      sourceVersion: _readPublicationSourceVersion(reader),
      sourceRoot: FlarkV3SourceRootId(reader.u32(), reader.u32()),
      parseGeneration: reader.u32(),
      grammarRevision: reader.u32(),
      syntaxProfile: FlarkV3SyntaxProfileId(reader.u32()),
      authorityMask: FlarkV3StructuralAuthorityMask(reader.u32()),
      recordCount: reader.u32(),
      sequenceDigest: FlarkV3ProtocolDigest128(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
      manifestDigest: FlarkV3ProtocolDigest128(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
    );

void _writeCanonicalCompletion(
  _PayloadWriter writer,
  FlarkV3CanonicalSourceFactCompletion completion,
) {
  _writeCanonicalLineage(writer, completion.lineage);
  writer
    ..u32(completion.fingerprintAlgorithm)
    ..u32(completion.fingerprint.revision)
    ..u32(completion.fingerprint.utf16Length)
    ..u32(completion.fingerprint.utf8Length)
    ..u32(completion.logicalLineBreaks)
    ..u32(completion.checkpointSpacingUtf16)
    ..u32(completion.checkpointCount)
    ..u32(completion.pageCount);
  _writeContentHash(writer, completion.fingerprint.contentHash128);
  _writeContentHash(writer, completion.checkpointHash128);
}

void _writeCanonicalDeltaCompletion(
  _PayloadWriter writer,
  FlarkV3CanonicalSourceFactDeltaCompletion completion,
) {
  _writeCanonicalLineage(writer, completion.lineage);
  writer
    ..u32(completion.fingerprintAlgorithm)
    ..u32(completion.fingerprint.revision)
    ..u32(completion.fingerprint.utf16Length)
    ..u32(completion.fingerprint.utf8Length)
    ..u32(completion.logicalLineBreaks)
    ..u32(completion.checkpointSpacingUtf16)
    ..u32(completion.checkpointCount)
    ..u32(completion.pageCount);
  _writeContentHash(writer, completion.fingerprint.contentHash128);
  _writeContentHash(writer, completion.checkpointRootGuard128);
  writer.u32(completion.checkpointRootGuardAlgorithm);
  _writeContentHash(writer, completion.replacementCheckpointHash128);
}

void _writeCanonicalPromotionProof(
  _PayloadWriter writer,
  FlarkV3ParserSessionBinding binding,
  FlarkV3CanonicalSourcePromotionProof proof,
) {
  _validateCanonicalProof(binding, proof);
  _writeCanonicalLineage(writer, proof.lineage);
  writer
    ..u32(proof.fingerprintAlgorithm)
    ..u32(proof.fingerprint.revision)
    ..u32(proof.fingerprint.utf16Length)
    ..u32(proof.fingerprint.utf8Length)
    ..u32(proof.logicalLineBreaks)
    ..u32(proof.checkpointSpacingUtf16)
    ..u32(proof.checkpointCount)
    ..u32(proof.pageCount);
  _writeContentHash(writer, proof.fingerprint.contentHash128);
  _writeContentHash(writer, proof.checkpointHash128);
}

Uint8List _frame({
  required FlarkV3WireOpcode opcode,
  required int correlationId,
  required Uint8List payload,
}) => FlarkV3WireProtocol.encode(
  FlarkV3WireFrame.owned(
    kind: FlarkV3WireFrameKind.request,
    opcode: opcode,
    correlationId: correlationId,
    payload: payload,
  ),
);

void _requirePositiveCorrelation(FlarkV3WireFrame frame) {
  if (frame.correlationId == 0) {
    throw const FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.invalidValue,
      byteOffset: 16,
      expected: 1,
      actual: 0,
    );
  }
}

void _requireCommandOpcode(FlarkV3WireOpcode opcode) {
  if (opcode != FlarkV3WireOpcode.parserOpen &&
      opcode != FlarkV3WireOpcode.snapshotPage &&
      opcode != FlarkV3WireOpcode.edit &&
      opcode != FlarkV3WireOpcode.parserRefineInline &&
      opcode != FlarkV3WireOpcode.parserPresentViewport &&
      opcode != FlarkV3WireOpcode.supersede &&
      opcode != FlarkV3WireOpcode.parserAcknowledge &&
      opcode != FlarkV3WireOpcode.close &&
      opcode != FlarkV3WireOpcode.drain) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.unexpectedOpcode,
      byteOffset: 8,
      actual: opcode.code,
    );
  }
}

void _requireEventOpcode(FlarkV3WireOpcode opcode) {
  if (opcode != FlarkV3WireOpcode.parserOpen &&
      opcode != FlarkV3WireOpcode.snapshotPage &&
      opcode != FlarkV3WireOpcode.edit &&
      opcode != FlarkV3WireOpcode.drain &&
      opcode != FlarkV3WireOpcode.parserPoll &&
      opcode != FlarkV3WireOpcode.close) {
    throw FlarkV3ParserSessionWireFormatException(
      FlarkV3ParserSessionWireFailure.unexpectedOpcode,
      byteOffset: 8,
      actual: opcode.code,
    );
  }
}

_PayloadWriter _writer(int length) {
  if (length > FlarkV3WireProtocol.maximumPayloadBytes) {
    throw RangeError.range(
      length,
      0,
      FlarkV3WireProtocol.maximumPayloadBytes,
      'payloadBytes',
    );
  }
  return _PayloadWriter(length);
}

Uint8List _strictUtf8(String value, String name) {
  _validateScalarString(value, name);
  return Uint8List.fromList(utf8.encode(value));
}

void _validateScalarString(String value, String name) {
  for (var index = 0; index < value.length; index += 1) {
    final unit = value.codeUnitAt(index);
    if (unit >= 0xD800 && unit <= 0xDBFF) {
      if (index + 1 >= value.length) {
        throw FormatException('$name ends in an unpaired high surrogate.');
      }
      final next = value.codeUnitAt(index + 1);
      if (next < 0xDC00 || next > 0xDFFF) {
        throw FormatException('$name contains an unpaired high surrogate.');
      }
      index += 1;
    } else if (unit >= 0xDC00 && unit <= 0xDFFF) {
      throw FormatException('$name contains an unpaired low surrogate.');
    }
  }
}

void _u32(int value, String name) {
  if (value < 0 || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, 0, flarkV3TransportV1Maximum, name);
  }
}

void _positiveU32(int value, String name) {
  if (value <= 0 || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, 1, flarkV3TransportV1Maximum, name);
  }
}

FlarkV3ParserSessionWireFormatException _variant(
  _PayloadReader reader,
  int actual,
) => FlarkV3ParserSessionWireFormatException(
  FlarkV3ParserSessionWireFailure.unknownVariant,
  byteOffset: 2,
  actual: actual,
);

FlarkV3ParserSessionWireFormatException _invalid(
  _PayloadReader reader,
  int actual,
  int expected,
) => FlarkV3ParserSessionWireFormatException(
  FlarkV3ParserSessionWireFailure.invalidValue,
  byteOffset: reader.offset,
  actual: actual,
  expected: expected,
);

FlarkV3ParserSessionWireFormatException _oversized(
  _PayloadReader reader,
  int actual,
  int expected,
) => FlarkV3ParserSessionWireFormatException(
  FlarkV3ParserSessionWireFailure.oversizedValue,
  byteOffset: reader.offset,
  actual: actual,
  expected: expected,
);

FlarkV3ParserSessionWireFormatException _identity(
  _PayloadReader reader,
  int actual,
  int expected,
) => FlarkV3ParserSessionWireFormatException(
  FlarkV3ParserSessionWireFailure.identityMismatch,
  byteOffset: reader.offset,
  actual: actual,
  expected: expected,
);

final class _PayloadWriter {
  _PayloadWriter(int length) : bytes = Uint8List(length) {
    _data = ByteData.sublistView(bytes);
  }

  final Uint8List bytes;
  late final ByteData _data;
  int _offset = 0;

  void u16(int value) {
    if (value < 0 || value > 0xffff) {
      throw RangeError.range(value, 0, 0xffff, 'u16');
    }
    _require(2);
    _data.setUint16(_offset, value, Endian.little);
    _offset += 2;
  }

  void u32(int value) {
    _u32(value, 'u32');
    _require(4);
    _data.setUint32(_offset, value, Endian.little);
    _offset += 4;
  }

  void raw(Uint8List value) {
    _require(value.length);
    bytes.setRange(_offset, _offset + value.length, value);
    _offset += value.length;
  }

  Uint8List finish() {
    if (_offset != bytes.length) {
      throw StateError('Session payload size calculation diverged.');
    }
    return bytes;
  }

  void _require(int count) {
    if (_offset + count > bytes.length) {
      throw StateError('Session payload writer overflowed.');
    }
  }
}

final class _PayloadReader {
  _PayloadReader(this.bytes) : _data = ByteData.sublistView(bytes);

  final Uint8List bytes;
  final ByteData _data;
  int offset = 0;

  int get remaining => bytes.length - offset;

  int u16() {
    _require(2);
    final value = _data.getUint16(offset, Endian.little);
    offset += 2;
    return value;
  }

  int u32() {
    _require(4);
    final value = _data.getUint32(offset, Endian.little);
    offset += 4;
    return value;
  }

  String strictString(int byteLength) {
    if (byteLength > remaining) {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.truncatedPayload,
        byteOffset: offset,
        expected: offset + byteLength,
        actual: bytes.length,
      );
    }
    final view = Uint8List.sublistView(bytes, offset, offset + byteLength);
    offset += byteLength;
    try {
      final result = utf8.decode(view, allowMalformed: false);
      _validateScalarString(result, 'decoded UTF-8');
      return result;
    } on FormatException {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.invalidUtf8,
        byteOffset: offset - byteLength,
      );
    }
  }

  void finish() {
    if (offset != bytes.length) {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.trailingPayload,
        byteOffset: offset,
        expected: offset,
        actual: bytes.length,
      );
    }
  }

  void _require(int count) {
    if (offset + count > bytes.length) {
      throw FlarkV3ParserSessionWireFormatException(
        FlarkV3ParserSessionWireFailure.truncatedPayload,
        byteOffset: offset,
        expected: offset + count,
        actual: bytes.length,
      );
    }
  }
}
