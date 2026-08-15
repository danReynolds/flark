import 'dart:async';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_driver.dart';
import 'package:flark/src/v3/runtime/flark_v3_viewport_presentation_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_parser_transport.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_endpoint_bindings.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_isolate_byte_endpoint.dart';
import 'package:flark/src/v3/session/session.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('Dart poll admission exactly matches the native ABI bounds', () {
    expect(flarkV3NativeMaximumPollSourceBytes, 65536);
    expect(flarkV3NativeMaximumPollCheckpoints, 64);
    expect(flarkV3NativeMaximumCandidateTransitions, 256);
    expect(const FlarkV3NativePollFuel().validate, returnsNormally);
    expect(const FlarkV3NativeCandidatePollFuel().validate, returnsNormally);
    expect(flarkV3NativeEndpointAbiVersion, 0x00020002);
    expect(flarkV3NativeEventKindViewportPublicationBegin, 21);
    expect(flarkV3NativeEventKindViewportPublicationPacket, 22);
    expect(flarkV3NativeEventKindViewportPublicationCommit, 23);
    expect(flarkV3NativeEventKindViewportPublicationDeliveryAcknowledged, 24);
    expect(
      const FlarkV3NativePollFuel(maximumSourceBytes: 0).validate,
      throwsRangeError,
    );
    expect(
      const FlarkV3NativePollFuel(maximumCheckpoints: 0).validate,
      throwsRangeError,
    );
    expect(
      const FlarkV3NativeCandidatePollFuel(maximumTransitions: 0).validate,
      throwsRangeError,
    );
    expect(
      const FlarkV3NativeCandidatePollFuel(maximumTransitions: 257).validate,
      throwsRangeError,
    );
  });

  test(
    'deferred dispatch is invalidated by close and terminal generations',
    () {
      final slot = FlarkV3NativeDeferredDispatchSlot();

      slot.defer(Uint8List.fromList(<int>[1]), strictClose: false);
      expect(slot.isOccupied, isTrue);
      slot.supersedeForClose();
      expect(slot.isOccupied, isFalse);

      slot.defer(Uint8List.fromList(<int>[2]), strictClose: false);
      expect(slot.takeAfterReceipt(flarkV3NativeEventKindFailed), isNull);
      expect(slot.isOccupied, isFalse);

      // A replacement generation may use the same bounded cell, but it can
      // never receive the prior generation's deferred command.
      slot.defer(Uint8List.fromList(<int>[3]), strictClose: false);
      final replacement = slot.takeAfterReceipt(3);
      expect(replacement?.frame, <int>[3]);
      expect(replacement?.strictClose, isFalse);
      expect(replacement?.hostPoll, isFalse);

      slot.defer(Uint8List.fromList(<int>[4]), strictClose: false);
      expect(slot.takeAfterReceipt(flarkV3NativeEventKindClosed), isNull);
    },
  );

  test('deferred host-poll routing remains typed across event credit', () {
    final slot = FlarkV3NativeDeferredDispatchSlot();
    final frame = Uint8List.fromList(<int>[0x20, 0x01]);
    slot.defer(frame, strictClose: false, hostPoll: true);

    final replay = slot.takeAfterReceipt(3);
    expect(replay?.frame, frame);
    expect(replay?.strictClose, isFalse);
    expect(replay?.hostPoll, isTrue);
  });

  test(
    'deferred inline-sidecar host-poll routing remains disjoint across credit',
    () {
      final slot = FlarkV3NativeDeferredDispatchSlot();
      final frame = Uint8List.fromList(<int>[0x20, 0x01]);
      slot.defer(frame, strictClose: false, inlineSidecarHostPoll: true);

      final replay = slot.takeAfterReceipt(
        flarkV3NativeEventKindInlinePublicationPacket,
      );
      expect(replay?.frame, frame);
      expect(replay?.strictClose, isFalse);
      expect(replay?.hostPoll, isFalse);
      expect(replay?.inlineSidecarHostPoll, isTrue);
    },
  );

  test(
    'deferred viewport host-poll routing remains disjoint across credit',
    () {
      final slot = FlarkV3NativeDeferredDispatchSlot();
      final frame = Uint8List.fromList(<int>[0x20, 0x02]);
      slot.defer(frame, strictClose: false, viewportPresentationHostPoll: true);

      final replay = slot.takeAfterReceipt(
        flarkV3NativeEventKindViewportPublicationPacket,
      );
      expect(replay?.frame, frame);
      expect(replay?.strictClose, isFalse);
      expect(replay?.hostPoll, isFalse);
      expect(replay?.inlineSidecarHostPoll, isFalse);
      expect(replay?.viewportPresentationHostPoll, isTrue);
    },
  );

  test('one unseen-credit window admits one coalesced source command', () {
    final slot = FlarkV3NativeDeferredDispatchSlot();
    final sourceSync = Uint8List.fromList(<int>[0x20, 0x01]);
    final separateSupersede = Uint8List.fromList(<int>[0x21, 0x01]);

    slot.defer(sourceSync, strictClose: false);
    expect(
      () => slot.defer(separateSupersede, strictClose: false),
      throwsStateError,
      reason: 'Supersede plus source sync would exceed the bounded cell.',
    );

    final replay = slot.takeAfterReceipt(3);
    expect(replay?.frame, sourceSync);
    expect(replay?.strictClose, isFalse);
    expect(slot.isOccupied, isFalse);
  });

  test('direct endpoint disposal proves emergency revocation', () async {
    final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
    endpoint.bind(onFrame: (_) {}, onFailure: (error, stackTrace) {});

    endpoint.close();
    await endpoint.done.timeout(const Duration(seconds: 5));
  });

  test('native byte endpoint uses the dedicated inline-sidecar ABI', () async {
    final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
    final failed = Completer<FlarkV3NativeEndpointException>();
    endpoint.bind(
      onFrame: (_) => fail('an unopened endpoint cannot emit an event'),
      onFailure: (error, stackTrace) {
        if (!failed.isCompleted) {
          failed.complete(error as FlarkV3NativeEndpointException);
        }
      },
    );
    final done = expectLater(
      endpoint.done,
      throwsA(
        isA<FlarkV3NativeEndpointException>().having(
          (error) => error.operation,
          'operation',
          'dispatchInlineSidecarHostPoll',
        ),
      ),
    );

    endpoint.sendInlineSidecarHostPoll(
      _inlineSidecarAbortPollFrame(
        FlarkV3ParserSessionBinding(
          documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
          sourceSessionIdentity: 95,
          workerGeneration: 1,
        ),
      ),
    );

    expect(
      (await failed.future.timeout(const Duration(seconds: 5))).operation,
      'dispatchInlineSidecarHostPoll',
    );
    await done.timeout(const Duration(seconds: 5));
  });

  test('native byte endpoint uses the dedicated viewport ABI', () async {
    final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
    final failed = Completer<FlarkV3NativeEndpointException>();
    endpoint.bind(
      onFrame: (_) => fail('an unopened endpoint cannot emit an event'),
      onFailure: (error, stackTrace) {
        if (!failed.isCompleted) {
          failed.complete(error as FlarkV3NativeEndpointException);
        }
      },
    );
    final done = expectLater(
      endpoint.done,
      throwsA(
        isA<FlarkV3NativeEndpointException>().having(
          (error) => error.operation,
          'operation',
          'dispatchViewportPresentationHostPoll',
        ),
      ),
    );
    endpoint.sendViewportPresentationHostPoll(
      _viewportAbortPollFrame(
        FlarkV3ParserSessionBinding(
          documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
          sourceSessionIdentity: 95,
          workerGeneration: 1,
        ),
      ),
    );
    expect(
      (await failed.future.timeout(const Duration(seconds: 5))).operation,
      'dispatchViewportPresentationHostPoll',
    );
    await done.timeout(const Duration(seconds: 5));
  });

  test(
    'microdeadline startup failures reclaim before the next lifecycle',
    () async {
      for (var attempt = 0; attempt < 3; attempt += 1) {
        await expectLater(
          FlarkV3NativeIsolateByteEndpoint.start(
            startupTimeout: const Duration(microseconds: 1),
          ),
          throwsA(isA<TimeoutException>()),
        );
      }

      // Depending on scheduling, the microdeadline can expire before the
      // control handshake or just after initialization was authorized. Both
      // paths must reclaim truthfully before the failure is returned. A normal
      // lifecycle immediately afterwards proves startup did not poison native
      // bootstrap state or leave its registry admission occupied.
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      endpoint.bind(onFrame: (_) {}, onFailure: (error, stackTrace) {});
      endpoint.close();
      await endpoint.done.timeout(const Duration(seconds: 5));
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'post-handshake startup error waits for truthful teardown receipt',
    () async {
      Object? startupFailure;
      try {
        await FlarkV3NativeIsolateByteEndpoint.start(
          overrideLibraryPath:
              '/definitely/not/a/flark/native/library-for-bootstrap-test',
        );
      } catch (error) {
        startupFailure = error;
      }

      expect(startupFailure, isA<FlarkV3NativeEndpointException>());
      expect(
        (startupFailure! as FlarkV3NativeEndpointException).operation,
        'startup',
      );

      // The original startup error is returned only after the worker has
      // truthfully receipted its no-handle teardown, so a new real lifecycle
      // must remain independently usable.
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      endpoint.bind(onFrame: (_) {}, onFailure: (error, stackTrace) {});
      endpoint.close();
      await endpoint.done.timeout(const Duration(seconds: 5));
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'native isolate owns fresh, recovery, strict close, and removal lifecycle',
    () async {
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final events = _EventInbox();
      final failures = <Object>[];
      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => failures.add(error),
      )..bind(events.add);
      addTearDown(() async {
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );
      final fresh = await events.take<FlarkV3ParserOpened>();
      expect(fresh.binding, _binding1);
      expect(fresh.mode, FlarkV3ParserOpenMode.fresh);
      transport.send(_acceptEvent(fresh, _binding1));

      transport.send(
        FlarkV3ParserOpen(
          binding: _binding2,
          mode: FlarkV3ParserOpenMode.recovery,
        ),
      );
      final recovery = await events.take<FlarkV3ParserOpened>();
      expect(recovery.binding, _binding2);
      expect(recovery.mode, FlarkV3ParserOpenMode.recovery);
      transport.send(_acceptEvent(recovery, _binding2));

      transport.send(FlarkV3ParserBeginClose(2));
      transport.send(
        FlarkV3ParserDrainGrant(
          binding: _binding2,
          drainId: 1,
          maximumTransitions: 1,
        ),
      );
      final drained = await events.take<FlarkV3ParserDrainProgress>();
      expect(drained.binding, _binding2);
      expect(drained.complete, isTrue);
      transport.send(_acceptEvent(drained, _binding2));

      final closed = await events.take<FlarkV3ParserClosed>();
      expect(closed.workerGeneration, 2);
      transport.send(_acceptEvent(closed, _binding2));
      transport.close();
      await endpoint.done.timeout(const Duration(seconds: 5));

      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'Dart-only executor synchronizes and closes the real native endpoint',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('live **text**');
      final hostStore = _ClosingHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _binding1.documentSession,
        hostStore: hostStore,
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final failures = <Object>[];
      final failure = Completer<void>();
      void recordFailure(Object error) {
        failures.add(error);
        if (!failure.isCompleted) failure.completeError(error);
      }

      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      final synchronized = Completer<void>();
      late final FlarkV3SessionExecutor executor;
      void observeProgress() {
        if (!synchronized.isCompleted &&
            executor.state == FlarkV3SessionDriverState.open &&
            session.sourceWorkerSynchronized) {
          synchronized.complete();
        }
      }

      executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: _binding1.documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        onProgress: observeProgress,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      observeProgress();
      await Future.any<void>(<Future<void>>[
        synchronized.future,
        failure.future,
      ]).timeout(const Duration(seconds: 5));
      expect(session.source.toString(), 'live **text**');

      try {
        await executor.close().timeout(const Duration(seconds: 5));
      } catch (error) {
        fail('close failed with $error; causal failures: $failures');
      }
      await endpoint.done.timeout(const Duration(seconds: 5));

      expect(executor.state, FlarkV3SessionDriverState.closed);
      expect(hostStore.closing, isTrue);
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'Dart-only executor can close before native Opened delivery',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('close now');
      final hostStore = _ClosingHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _binding1.documentSession,
        hostStore: hostStore,
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final failures = <Object>[];
      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => failures.add(error),
      );
      final executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: _binding1.documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        onFailure: (error, stackTrace) => failures.add(error),
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      await executor.close().timeout(const Duration(seconds: 5));
      await endpoint.done.timeout(const Duration(seconds: 5));

      expect(executor.state, FlarkV3SessionDriverState.closed);
      expect(hostStore.closing, isTrue);
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'Dart-only executor can close during provisional SourceFacts work',
    () async {
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(
        List<String>.filled(10000, 'line\r\n').join(),
      );
      final hostStore = _ClosingHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _binding1.documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(
          _binding1.documentSession,
        ),
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final failures = <Object>[];
      final terminalFailure = Completer<void>();
      void recordFailure(Object error) {
        failures.add(error);
        if (!terminalFailure.isCompleted) terminalFailure.completeError(error);
      }

      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      final factsInFlight = Completer<void>();
      late final FlarkV3SessionExecutor executor;
      void observeProgress() {
        if (!factsInFlight.isCompleted &&
            executor.state == FlarkV3SessionDriverState.open &&
            session.sourceWorkerSynchronized &&
            !session.currentUiSourceCertified) {
          factsInFlight.complete();
        }
      }

      executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: _binding1.documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        onProgress: observeProgress,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      await Future.any<void>(<Future<void>>[
        factsInFlight.future,
        terminalFailure.future,
      ]).timeout(const Duration(seconds: 5));
      await executor.close().timeout(const Duration(seconds: 5));
      await endpoint.done.timeout(const Duration(seconds: 5));

      expect(executor.state, FlarkV3SessionDriverState.closed);
      expect(hostStore.closing, isTrue);
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'new source cancels old derived facts without a separate supersede',
    () async {
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(
        List<String>.filled(10000, 'line\r\n').join(),
      );
      final hostStore = _ClosingHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _binding1.documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(
          _binding1.documentSession,
        ),
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final failures = <Object>[];
      final terminalFailure = Completer<void>();
      void recordFailure(Object error) {
        failures.add(error);
        if (!terminalFailure.isCompleted) terminalFailure.completeError(error);
      }

      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      final oldFactsInFlight = Completer<void>();
      final editLeaseInFlight = Completer<void>();
      final latestCertified = Completer<void>();
      var firstEditRequested = false;
      var laterEditsApplied = false;
      late final FlarkV3SessionExecutor executor;

      void append(String text) {
        final end = session.source.utf16Length;
        session.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: session.uiRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: end,
              endUtf16: end,
              replacement: text,
            ),
          ),
        );
        executor.sourceChanged();
      }

      void observeProgress() {
        if (!oldFactsInFlight.isCompleted &&
            executor.state == FlarkV3SessionDriverState.open &&
            session.sourceWorkerSynchronized &&
            !session.currentUiSourceCertified) {
          oldFactsInFlight.complete();
        }
        if (firstEditRequested &&
            !laterEditsApplied &&
            sourceSession.workerSyncDiagnostics.liveLeaseCount == 1) {
          laterEditsApplied = true;
          append('?');
          append('#');
          editLeaseInFlight.complete();
        }
        if (laterEditsApplied &&
            !latestCertified.isCompleted &&
            session.sourceWorkerSynchronized &&
            session.currentUiSourceCertified) {
          latestCertified.complete();
        }
      }

      executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: _binding1.documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        onProgress: observeProgress,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      await Future.any<void>(<Future<void>>[
        oldFactsInFlight.future,
        terminalFailure.future,
      ]).timeout(const Duration(seconds: 5));
      firstEditRequested = true;
      append('!');
      await Future.any<void>(<Future<void>>[
        editLeaseInFlight.future,
        terminalFailure.future,
      ]).timeout(const Duration(seconds: 5));
      await Future.any<void>(<Future<void>>[
        latestCertified.future,
        terminalFailure.future,
      ]).timeout(const Duration(seconds: 5));

      expect(session.source.toString(), endsWith('!?#'));
      expect(session.sourceWorkerSynchronized, isTrue);
      expect(session.currentUiSourceCertified, isTrue);
      expect(failures, isEmpty);

      await executor.close().timeout(const Duration(seconds: 5));
      await endpoint.done.timeout(const Duration(seconds: 5));
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'strict close supersedes deferred source work and defers only its drain',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString(
        List<String>.filled(1000, 'line\n').join(),
      );
      final binding = FlarkV3ParserSessionBinding(
        documentSession: _binding1.documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final events = _EventInbox();
      final failures = <Object>[];
      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => failures.add(error),
      )..bind(events.add);
      FlarkV3SourceWorkerSyncLease? deferredLease;
      addTearDown(() async {
        final lease = deferredLease;
        if (lease != null && sourceSession.ownsWorkerSyncLease(lease.leaseId)) {
          sourceSession.releaseWorkerSyncLease(lease.leaseId);
        }
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      transport.send(
        FlarkV3ParserOpen(binding: binding, mode: FlarkV3ParserOpenMode.fresh),
      );
      final opened = await events.take<FlarkV3ParserOpened>();
      transport.send(_acceptEvent(opened, binding));

      final initialLease = sourceSession.beginWorkerSync();
      transport.send(FlarkV3ParserSynchronizeSource(initialLease));
      final synchronized = await events.take<FlarkV3ParserSourceSynchronized>();
      transport.send(
        FlarkV3ParserEventReceipt(
          eventId: synchronized.eventId,
          binding: binding,
          disposition: FlarkV3ParserEventDisposition.accepted,
          sourceSync: sourceSession.acknowledgeWorkerSync(
            synchronized.acknowledgement,
          ),
        ),
      );

      final facts = await events.take<FlarkV3ParserSourceFactsPage>();
      sourceSession.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: sourceSession.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: sourceSession.document.utf16Length,
            endUtf16: sourceSession.document.utf16Length,
            replacement: 'later',
          ),
        ),
      );
      final nextLease = sourceSession.beginWorkerSync();
      deferredLease = nextLease;

      // The source edit is backpressured behind `facts`. Strict close must
      // invalidate it before the drain itself occupies the one deferred cell.
      transport.send(FlarkV3ParserSynchronizeSource(nextLease));
      transport.send(FlarkV3ParserBeginClose(binding.workerGeneration));
      transport.send(
        FlarkV3ParserDrainGrant(
          binding: binding,
          drainId: 1,
          maximumTransitions: 1,
        ),
      );
      transport.send(_acceptEvent(facts, binding));

      var drained = await events.take<FlarkV3ParserDrainProgress>();
      var drainPolls = 1;
      while (!drained.complete) {
        transport.send(_acceptEvent(drained, binding));
        drainPolls += 1;
        expect(drainPolls, lessThanOrEqualTo(64));
        transport.send(
          FlarkV3ParserDrainGrant(
            binding: binding,
            drainId: drainPolls,
            maximumTransitions: 1,
          ),
        );
        drained = await events.take<FlarkV3ParserDrainProgress>();
      }
      transport.send(_acceptEvent(drained, binding));
      final closed = await events.take<FlarkV3ParserClosed>();
      transport.send(_acceptEvent(closed, binding));
      transport.close();
      await endpoint.done.timeout(const Duration(seconds: 5));

      expect(failures, isEmpty);
      expect(sourceSession.ownsWorkerSyncLease(nextLease.leaseId), isTrue);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'large provisional Dart source promotes from canonical native facts',
    () async {
      const repetitions = 50000;
      final source = List<String>.filled(repetitions, 'a🌍b\r\n').join();
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(source);
      final hostStore = _ClosingHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _binding1.documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(
          _binding1.documentSession,
        ),
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start();
      final failures = <Object>[];
      final terminalFailure = Completer<void>();
      void recordFailure(Object error) {
        failures.add(error);
        if (!terminalFailure.isCompleted) {
          terminalFailure.completeError(error);
        }
      }

      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      final promoted = Completer<void>();
      late final FlarkV3SessionExecutor executor;
      void observeProgress() {
        if (!promoted.isCompleted &&
            executor.state == FlarkV3SessionDriverState.open &&
            session.sourceWorkerSynchronized &&
            session.currentUiSourceCertified) {
          promoted.complete();
        }
      }

      executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: _binding1.documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        onProgress: observeProgress,
        onFailure: (error, stackTrace) => recordFailure(error),
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      observeProgress();
      await Future.any<void>(<Future<void>>[
        promoted.future,
        terminalFailure.future,
      ]).timeout(const Duration(seconds: 10));

      expect(session.source.hasCertifiedFacts, isTrue);
      expect(session.source.utf16Length, repetitions * 6);
      expect(session.source.utf8Length, repetitions * 8);
      expect(session.source.lineCount, repetitions + 1);
      expect(session.source.utf16ToUtf8(6), 8);
      expect(session.source.utf8ToUtf16(8), 6);
      expect(session.source.lineStartUtf16(1), 6);
      expect(session.sourceVersion.revision, 1);
      expect(session.sourceVersion.metric.bytes, repetitions * 8);
      expect(hostStore.lastObserved, session.sourceVersion);

      await executor.close().timeout(const Duration(seconds: 10));
      await endpoint.done.timeout(const Duration(seconds: 5));
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );
}

Uint8List _inlineSidecarAbortPollFrame(FlarkV3ParserSessionBinding binding) {
  final offerId = FlarkV3OfferId(96, 97, 98, 99);
  return FlarkV3HotInlineSidecarWireCodec.encodeCommand(
    FlarkV3ParserInlineSidecarHostPollCompleted(
      ticket: FlarkV3ParserInlineSidecarHostPollTicket(
        binding: binding,
        pollTicket: 100,
        offerId: offerId,
        phase: FlarkV3ParserInlineSidecarHostPollPhase.abort,
      ),
      outcome: FlarkV3InlineSidecarHostAbortComplete(offerId),
    ),
  );
}

Uint8List _viewportAbortPollFrame(FlarkV3ParserSessionBinding binding) {
  final offerId = FlarkV3OfferId(106, 107, 108, 109);
  return FlarkV3ViewportPresentationWireCodec.encodeCommand(
    FlarkV3ParserViewportPresentationHostPollCompleted(
      ticket: FlarkV3ParserViewportPresentationHostPollTicket(
        binding: binding,
        pollTicket: 110,
        offerId: offerId,
        phase: FlarkV3ParserViewportPresentationHostPollPhase.abort,
      ),
      outcome: FlarkV3ViewportPresentationHostAbortComplete(offerId),
    ),
  );
}

FlarkV3ParserEventReceipt _acceptEvent(
  FlarkV3ParserEvent event,
  FlarkV3ParserSessionBinding binding,
) => FlarkV3ParserEventReceipt(
  eventId: event.eventId,
  binding: binding,
  disposition: FlarkV3ParserEventDisposition.accepted,
);

final _binding1 = FlarkV3ParserSessionBinding(
  documentSession: FlarkV3DocumentSessionId(101, 202, 303, 404),
  sourceSessionIdentity: 505,
  workerGeneration: 1,
);

final _binding2 = FlarkV3ParserSessionBinding(
  documentSession: FlarkV3DocumentSessionId(101, 202, 303, 404),
  sourceSessionIdentity: 505,
  workerGeneration: 2,
);

final class _EventInbox {
  final List<FlarkV3ParserEvent> _events = <FlarkV3ParserEvent>[];
  Completer<void>? _wake;

  void add(FlarkV3ParserEvent event) {
    _events.add(event);
    final wake = _wake;
    _wake = null;
    wake?.complete();
  }

  Future<T> take<T extends FlarkV3ParserEvent>() async {
    while (true) {
      if (_events.isNotEmpty) {
        final event = _events.removeAt(0);
        if (event is! T) {
          throw StateError('Expected $T but received ${event.runtimeType}.');
        }
        return event;
      }
      final wake = Completer<void>();
      _wake = wake;
      await wake.future.timeout(const Duration(seconds: 5));
    }
  }
}

final class _ClosingHostStore implements FlarkV3HostStore {
  bool closing = false;
  FlarkV3SourceVersion? lastObserved;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    lastObserved = sourceVersion;
    return _hostAccepted;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closing = true;
    return _hostAccepted;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => FlarkV3HostAccepted<FlarkV3HostPollOutcome>(
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
  ) => throw UnsupportedError('no publication');

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => throw UnsupportedError('no query');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => throw UnsupportedError('no publication');
}

const _hostAccepted = FlarkV3HostAccepted<FlarkV3HostUnit>(
  FlarkV3HostUnit.accepted,
);
