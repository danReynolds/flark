import 'dart:async';

import 'package:flark/flark_adapter.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_driver.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3SessionExecutor', () {
    test('opens and synchronizes without exposing a manual pump', () {
      final harness = _ExecutorHarness();
      addTearDown(harness.dispose);

      expect(harness.executor.state, FlarkV3SessionDriverState.opening);
      expect(harness.executor.inlineAttemptOutcomeGeneration, 0);
      final open = harness.transport.last<FlarkV3ParserOpen>();
      harness.transport.emit(
        FlarkV3ParserOpened(eventId: 1, binding: open.binding, mode: open.mode),
      );
      expect(harness.scheduler.pendingTasks, 1);

      harness.scheduler.runAll();

      expect(harness.executor.state, FlarkV3SessionDriverState.open);
      expect(
        harness.transport.commands,
        contains(isA<FlarkV3ParserEventReceipt>()),
      );
      expect(
        harness.transport.commands,
        contains(isA<FlarkV3ParserSynchronizeSource>()),
      );

      final lease = harness.transport
          .last<FlarkV3ParserSynchronizeSource>()
          .lease;
      harness.transport.emit(_ackEvent(2, lease));
      harness.scheduler.runAll();

      expect(harness.session.sourceWorkerSynchronized, isTrue);
      expect(harness.progressCount, greaterThanOrEqualTo(2));
    });

    test('count budget yields before the next bounded action', () {
      final harness = _ExecutorHarness(maximumActionsPerTurn: 1);
      addTearDown(harness.dispose);
      final open = harness.transport.last<FlarkV3ParserOpen>();
      harness.transport.emit(
        FlarkV3ParserOpened(eventId: 1, binding: open.binding, mode: open.mode),
      );

      harness.scheduler.runNext();
      expect(harness.executor.state, FlarkV3SessionDriverState.open);
      expect(
        harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
        isEmpty,
      );
      expect(harness.scheduler.pendingTasks, 1);

      harness.scheduler.runNext();
      expect(
        harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
        hasLength(1),
      );
    });

    test('rapid edit wakeups coalesce before the event-queue turn', () {
      final harness = _ExecutorHarness();
      addTearDown(harness.dispose);
      harness.openAndSynchronize();

      for (final text in <String>['a', 'b', 'c']) {
        final end = harness.session.source.utf16Length;
        harness.session.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: harness.session.uiRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: end,
              endUtf16: end,
              replacement: text,
            ),
          ),
        );
        harness.executor.sourceChanged();
      }

      expect(harness.scheduler.pendingTasks, 1);
      final before = harness.transport.commands
          .whereType<FlarkV3ParserSynchronizeSource>()
          .length;
      harness.scheduler.runAll();
      final after = harness.transport.commands
          .whereType<FlarkV3ParserSynchronizeSource>()
          .length;
      expect(after, before + 1);
      expect(harness.session.source.toString(), 'liveabc');
    });

    test(
      'adapter callbacks cannot move scheduling out of the attachment zone',
      () {
        late _ExecutorHarness harness;
        late Zone attachmentZone;
        Zone.current.fork().run(() {
          attachmentZone = Zone.current;
          harness = _ExecutorHarness();
        });
        addTearDown(harness.dispose);
        final open = harness.transport.last<FlarkV3ParserOpen>();

        Zone.current.fork().run(() {
          harness.transport.emit(
            FlarkV3ParserOpened(
              eventId: 1,
              binding: open.binding,
              mode: open.mode,
            ),
          );
        });

        expect(harness.scheduler.schedulingZones, hasLength(1));
        expect(harness.scheduler.schedulingZones.single, same(attachmentZone));
        harness.scheduler.runAll();
        expect(harness.executor.state, FlarkV3SessionDriverState.open);
      },
    );

    test('publication edit emits one command in an unseen-credit window', () {
      final harness = _ExecutorHarness();
      addTearDown(harness.dispose);
      harness.openAndSynchronize();

      final begin = _publicationOffer(harness.session.sourceVersion);
      harness.transport.emit(
        FlarkV3ParserPublicationBegin(
          eventId: 3,
          binding: harness.executor.parserBinding,
          begin: begin,
        ),
      );
      harness.scheduler.runAll();
      expect(
        harness.executor.publicationState,
        FlarkV3PublicationDriverState.acceptingPackets,
      );

      // Native may already have emitted another event while its frame is
      // still in transit to Dart. Only one non-receipt command can safely
      // occupy the endpoint's bounded deferred cell in that window.
      harness.transport.beginUnseenCreditWindow();
      final end = harness.session.source.utf16Length;
      harness.session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: harness.session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: end,
            endUtf16: end,
            replacement: '!',
          ),
        ),
      );
      harness.executor.sourceChanged();
      expect(harness.transport.speculativeCommands, isEmpty);

      harness.scheduler.runAll();

      expect(harness.transport.speculativeCommands, hasLength(1));
      expect(
        harness.transport.speculativeCommands.single,
        isA<FlarkV3ParserSynchronizeSource>(),
      );
      expect(
        harness.transport.commands.whereType<FlarkV3ParserSupersede>(),
        isEmpty,
      );
    });

    test('terminal parser failure can start an exact recovery generation', () {
      final harness = _ExecutorHarness();
      addTearDown(harness.dispose);
      harness.openAndSynchronize();
      harness.transport.emit(
        FlarkV3ParserFailed(eventId: 3, workerGeneration: 1, failureCode: 4),
      );
      harness.scheduler.runAll();
      expect(harness.executor.state, FlarkV3SessionDriverState.faulted);

      final restart = harness.executor.restart();

      expect(restart.workerGeneration, 2);
      expect(harness.executor.state, FlarkV3SessionDriverState.opening);
      final recovery = harness.transport.last<FlarkV3ParserOpen>();
      expect(recovery.binding.workerGeneration, 2);
      expect(recovery.mode, FlarkV3ParserOpenMode.recovery);
    });

    test(
      'emergency teardown settles close after the transport has faulted',
      () async {
        final harness = _ExecutorHarness();
        addTearDown(harness.dispose);
        harness.openAndSynchronize();
        harness.transport.failSends = true;

        harness.executor.emergencyDispose();

        expect(harness.executor.state, FlarkV3SessionDriverState.closed);
        expect(harness.transport.closed, isTrue);
        await expectLater(
          harness.executor.close().timeout(const Duration(seconds: 1)),
          completes,
        );
      },
    );
  });
}

final class _ExecutorHarness {
  _ExecutorHarness({int maximumActionsPerTurn = 8}) {
    final sourceSession = FlarkV3SourceSession.fromString('live');
    session = FlarkDocumentSession.attach(
      sourceSession: sourceSession,
      documentSession: _documentSession,
      hostStore: store,
    );
    executor = FlarkV3SessionExecutor.attach(
      session: session,
      transport: transport,
      parserBinding: FlarkV3ParserSessionBinding(
        documentSession: _documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      ),
      publicationAuthority: _publicationAuthority,
      scheduler: scheduler,
      maximumActionsPerTurn: maximumActionsPerTurn,
      onProgress: () => progressCount += 1,
    );
  }

  final _QueueScheduler scheduler = _QueueScheduler();
  final _ExecutorHostStore store = _ExecutorHostStore();
  final _ExecutorParserTransport transport = _ExecutorParserTransport();
  late final FlarkDocumentSession session;
  late final FlarkV3SessionExecutor executor;
  int progressCount = 0;

  void openAndSynchronize() {
    final open = transport.last<FlarkV3ParserOpen>();
    transport.emit(
      FlarkV3ParserOpened(eventId: 1, binding: open.binding, mode: open.mode),
    );
    scheduler.runAll();
    final lease = transport.last<FlarkV3ParserSynchronizeSource>().lease;
    transport.emit(_ackEvent(2, lease));
    scheduler.runAll();
    expect(session.sourceWorkerSynchronized, isTrue);
  }

  void dispose() => executor.emergencyDispose();
}

final _documentSession = FlarkV3DocumentSessionId(71, 72, 73, 74);
final _syntaxProfile = FlarkV3SyntaxProfileId(1);
final _publicationAuthority = FlarkV3ParserPublicationAuthority(
  grammarRevision: 7,
  syntaxProfile: _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
);

FlarkV3HostOfferBegin _publicationOffer(FlarkV3SourceVersion sourceVersion) =>
    FlarkV3HostOfferBegin(
      offerId: FlarkV3OfferId(11, sourceVersion.revision, 12, 13),
      publicationSession: FlarkV3PublicationSessionId(21, 22, 23, 24),
      targetHostRevision: FlarkV3HostRevisionId(1),
      sourceVersion: sourceVersion,
      sourceRoot: FlarkV3SourceRootId(0, 101),
      parseGeneration: 1,
      grammarRevision: 7,
      syntaxProfile: _syntaxProfile,
      authorityMask: FlarkV3StructuralAuthorityMask.complete,
      mode: FlarkV3PublicationMode.fullSnapshot,
      baseAck: null,
      transferredRecordCount: 1,
      targetRecordCount: 1,
      limits: FlarkV3HostOfferLimits(
        maximumFrameCount: 1,
        maximumEncodedFrameBytes: 64,
        maximumPacketBytes: 132,
        maximumFrameBytes: 64,
        maximumProgramChildren: 8,
      ),
    );

FlarkV3ParserSourceSynchronized _ackEvent(
  int eventId,
  FlarkV3SourceWorkerSyncLease lease,
) => FlarkV3ParserSourceSynchronized(
  eventId: eventId,
  workerGeneration: lease.workerGeneration,
  acknowledgement: switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
      observedReplica: lease.isLast ? _observedFor(lease) : null,
    ),
    FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
      observedReplica: _observedFor(lease),
    ),
  },
);

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  final utf8Length = switch (target) {
    FlarkV3KnownSourceStamp() => target.utf8Length,
    FlarkV3ProvisionalSourceStamp() => throw StateError(
      'Executor fixture only installs known source targets.',
    ),
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: utf8Length,
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

final class _QueueScheduler implements FlarkV3SessionTaskScheduler {
  final List<FlarkV3SessionExecutorCallback> _tasks =
      <FlarkV3SessionExecutorCallback>[];
  final List<Zone> schedulingZones = <Zone>[];

  int get pendingTasks => _tasks.length;

  @override
  void schedule(FlarkV3SessionExecutorCallback callback) {
    schedulingZones.add(Zone.current);
    _tasks.add(callback);
  }

  void runNext() {
    final task = _tasks.removeAt(0);
    task();
  }

  void runAll() {
    while (_tasks.isNotEmpty) {
      runNext();
    }
  }
}

final class _ExecutorParserTransport implements FlarkV3ParserTransport {
  final List<FlarkV3ParserCommand> commands = <FlarkV3ParserCommand>[];
  final List<FlarkV3ParserCommand> speculativeCommands =
      <FlarkV3ParserCommand>[];
  FlarkV3ParserEventCallback? _callback;
  bool _unseenCreditWindow = false;
  bool failSends = false;
  bool closed = false;

  @override
  void bind(FlarkV3ParserEventCallback onEvent) {
    if (_callback != null) throw StateError('already bound');
    _callback = onEvent;
  }

  @override
  void send(FlarkV3ParserCommand command) {
    if (failSends) throw StateError('transport is faulted');
    if (_unseenCreditWindow && command is! FlarkV3ParserEventReceipt) {
      speculativeCommands.add(command);
      if (speculativeCommands.length > 1) {
        throw StateError('Exceeded one speculative command credit.');
      }
    }
    commands.add(command);
  }

  void beginUnseenCreditWindow() {
    _unseenCreditWindow = true;
    speculativeCommands.clear();
  }

  void emit(FlarkV3ParserEvent event) => _callback!(event);

  T last<T extends FlarkV3ParserCommand>() => commands.whereType<T>().last;

  @override
  void close() => closed = true;
}

final class _ExecutorHostStore implements FlarkV3HostStore {
  bool closing = false;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => _accepted;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closing = true;
    return _accepted;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => FlarkV3HostAccepted(
    closing ? const FlarkV3HostClosed() : const FlarkV3HostPollPending(),
  );

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      throw UnsupportedError('no publication');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => throw UnsupportedError('no publication');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => throw UnsupportedError('no publication');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => _accepted;

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => throw UnsupportedError('no query');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => throw UnsupportedError('no publication');
}

const _accepted = FlarkV3HostAccepted<FlarkV3HostUnit>(
  FlarkV3HostUnit.accepted,
);
