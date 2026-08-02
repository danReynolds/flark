@TestOn('browser')
library;

import 'dart:async';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/flark_v3_byte_endpoint.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_publication_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_session_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_viewport_presentation_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_runtime.dart'
    show FlarkV3DocumentRuntimePlatformAttachment;
import 'package:flark/src/v3/runtime/public/flark_v3_platform_endpoint_handle.dart';
import 'package:flark/src/v3/runtime/web/flark_v3_web_host_store.dart';
import 'package:flark/src/v3/runtime/web/flark_v3_web_worker_byte_endpoint.dart';
import 'package:test/test.dart';

const Duration _functionalTimeout = Duration(seconds: 20);
const int _invalidSeedFailureCode = 2;

Future<FlarkV3DocumentRuntimeStatus> _awaitStatus(
  FlarkV3DocumentRuntime runtime,
  bool Function(FlarkV3DocumentRuntimeStatus status) predicate,
) {
  final current = runtime.status;
  if (predicate(current)) {
    return Future<FlarkV3DocumentRuntimeStatus>.value(current);
  }
  return runtime.statuses.firstWhere(predicate).timeout(_functionalTimeout);
}

void main() {
  test('real Worker uses the dedicated inline-sidecar Wasm dispatch', () async {
    final assets = FlarkV3WebRuntimeAssets.packageDefaults();
    final worker = await FlarkV3WebWorkerByteEndpoint.start(
      workerUri: assets.workerUri,
      wasmUri: assets.wasmUri,
    ).timeout(_functionalTimeout);
    final failed = Completer<FlarkV3WebEndpointException>();
    worker.bind(
      onFrame: (_) => fail('an unopened endpoint cannot emit an event'),
      onFailure: (error, stackTrace) {
        if (!failed.isCompleted) {
          failed.complete(error as FlarkV3WebEndpointException);
        }
      },
    );
    final done = expectLater(
      worker.done,
      throwsA(
        isA<FlarkV3WebEndpointException>().having(
          (error) => error.operation,
          'operation',
          'dispatchInlineSidecarHostPoll',
        ),
      ),
    );

    worker.sendInlineSidecarHostPoll(
      _inlineSidecarAbortPollFrame(
        FlarkV3ParserSessionBinding(
          documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
          sourceSessionIdentity: 95,
          workerGeneration: 1,
        ),
      ),
    );

    expect(
      (await failed.future.timeout(_functionalTimeout)).operation,
      'dispatchInlineSidecarHostPoll',
    );
    await done.timeout(_functionalTimeout);
  });

  test('real Worker uses the dedicated viewport Wasm dispatch', () async {
    final assets = FlarkV3WebRuntimeAssets.packageDefaults();
    final worker = await FlarkV3WebWorkerByteEndpoint.start(
      workerUri: assets.workerUri,
      wasmUri: assets.wasmUri,
    ).timeout(_functionalTimeout);
    final failed = Completer<FlarkV3WebEndpointException>();
    worker.bind(
      onFrame: (_) => fail('an unopened endpoint cannot emit an event'),
      onFailure: (error, stackTrace) {
        if (!failed.isCompleted) {
          failed.complete(error as FlarkV3WebEndpointException);
        }
      },
    );
    final done = expectLater(
      worker.done,
      throwsA(
        isA<FlarkV3WebEndpointException>().having(
          (error) => error.operation,
          'operation',
          'dispatchViewportPresentationHostPoll',
        ),
      ),
    );
    final offerId = FlarkV3OfferId(96, 97, 98, 99);
    worker.sendViewportPresentationHostPoll(
      FlarkV3ViewportPresentationWireCodec.encodeCommand(
        FlarkV3ParserViewportPresentationHostPollCompleted(
          ticket: FlarkV3ParserViewportPresentationHostPollTicket(
            binding: FlarkV3ParserSessionBinding(
              documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
              sourceSessionIdentity: 95,
              workerGeneration: 1,
            ),
            pollTicket: 100,
            offerId: offerId,
            phase: FlarkV3ParserViewportPresentationHostPollPhase.abort,
          ),
          outcome: FlarkV3ViewportPresentationHostAbortComplete(offerId),
        ),
      ),
    );
    expect(
      (await failed.future.timeout(_functionalTimeout)).operation,
      'dispatchViewportPresentationHostPoll',
    );
    await done.timeout(_functionalTimeout);
  });

  test(
    'real Worker recovers from Rust InvalidSeed with a full exact reseed',
    () async {
      final markdown = List<String>.generate(
        900,
        (index) =>
            '## Section $index\n\nParagraph **bold $index** with 世界.\n\n',
      ).join();
      expect(markdown.length, greaterThan(2 * 8192));

      final assets = FlarkV3WebRuntimeAssets.packageDefaults();
      final documentSession = FlarkV3DocumentSessionId(
        0x464c4b33,
        0x5245434f,
        0x56455259,
        1,
      );
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(
        markdown,
      );
      final hostStore = await FlarkV3WebHostStore.create(
        wasmUri: assets.wasmUri,
        documentSession: documentSession,
      ).timeout(_functionalTimeout);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(documentSession),
      );
      final initialBinding = FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final worker = await FlarkV3WebWorkerByteEndpoint.start(
        workerUri: assets.workerUri,
        wasmUri: assets.wasmUri,
      ).timeout(_functionalTimeout);
      final endpoint = _SnapshotReplayEndpoint(
        delegate: worker,
        initialBinding: initialBinding,
      );
      final runtime = await FlarkV3DocumentRuntimePlatformAttachment.attach(
        document: document,
        parserBinding: initialBinding,
        platformEndpoint: FlarkV3PlatformEndpointHandle(
          endpoint: endpoint,
          done: worker.done,
        ),
      ).timeout(_functionalTimeout);
      addTearDown(() async {
        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runtime.close().timeout(_functionalTimeout);
        }
      });

      await runtime.initialReady.timeout(_functionalTimeout);
      final initiallyCurrent = await _awaitStatus(
        runtime,
        (status) => status.structureCurrent,
      );
      expect(initiallyCurrent.sourceCurrent, isTrue);
      expect(
        endpoint.snapshotPages(initialBinding.workerGeneration),
        isNotEmpty,
      );
      const rangeBudget = FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 32 + 2 * 160,
        maximumBlockCount: 2,
      );
      final preRecoveryRange =
          runtime.queryBlockRange(
                0,
                runtime.sourceLengthUtf16,
                budget: rangeBudget,
              )
              as FlarkV3DocumentStructuralBlockRange;
      expect(preRecoveryRange.continuation, isNotNull);
      final visibleDemand = FlarkV3VisibleBlockDemand(
        sourceRevision: runtime.sourceRevision,
        structureGeneration: runtime.status.structureGeneration,
        startUtf16: 0,
        endUtf16: runtime.sourceLengthUtf16,
        maximumBlocks: 4,
      );
      final materializer = FlarkV3VisibleBlockSetMaterializer(runtime);
      expect(
        (materializer.advance(visibleDemand, budget: rangeBudget)
                as FlarkV3ExactVisibleBlockSet)
            .blocks,
        hasLength(2),
      );

      endpoint.replayCapturedSnapshotPage();

      final faulted = await _awaitStatus(
        runtime,
        (status) => status.state == FlarkV3DocumentRuntimeState.faulted,
      );
      expect(faulted.recoveryAvailable, isTrue);
      expect(endpoint.failureCodes, contains(_invalidSeedFailureCode));

      runtime.recover();
      final recoveredGeneration = initialBinding.workerGeneration + 1;
      expect(endpoint.recoveredFrom, <FlarkV3ByteEndpointBinding>[
        FlarkV3ByteEndpointBinding(
          documentSessionWords: <int>[
            documentSession.word0,
            documentSession.word1,
            documentSession.word2,
            documentSession.word3,
          ],
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: initialBinding.workerGeneration,
        ),
      ]);
      await endpoint
          .waitForStructuralCommit(recoveredGeneration)
          .timeout(_functionalTimeout);

      final recovered = await _awaitStatus(
        runtime,
        (status) => status.sourceCurrent && status.structureCurrent,
      );
      expect(recovered.structureRevision, runtime.sourceRevision);
      expect(
        recovered.structureGeneration,
        greaterThan(visibleDemand.structureGeneration),
        reason: 'same-source recovery installs a distinct semantic authority',
      );

      final recoveryPages = endpoint.snapshotPages(recoveredGeneration);
      expect(recoveryPages.length, greaterThan(1));
      expect(recoveryPages.first.isSeed, isTrue);
      var nextStartUtf16 = 0;
      final reconstructed = StringBuffer();
      for (final page in recoveryPages) {
        expect(page.binding.workerGeneration, recoveredGeneration);
        expect(page.startUtf16, nextStartUtf16);
        expect(page.totalUtf16Length, markdown.length);
        reconstructed.write(page.source);
        nextStartUtf16 = page.endUtf16;
      }
      expect(nextStartUtf16, markdown.length);
      expect(reconstructed.toString(), markdown);

      final staleContinuation = runtime.continueBlockRange(
        preRecoveryRange.continuation!,
        budget: rangeBudget,
      );
      expect(staleContinuation, isA<FlarkV3DocumentPendingBlockRange>());
      expect(
        (staleContinuation as FlarkV3DocumentPendingBlockRange).reason,
        FlarkV3DocumentPendingReason.structurePending,
        reason:
            'a same-source replacement publication invalidates its old '
            'resume claim without turning the expected race into corruption',
      );
      final staleVisible = materializer.advance(
        visibleDemand,
        budget: rangeBudget,
      );
      expect(staleVisible, isA<FlarkV3PendingVisibleBlockSet>());
      expect(
        (staleVisible as FlarkV3PendingVisibleBlockSet).reason,
        FlarkV3DocumentPendingReason.structurePending,
      );
      final recoveredVisibleDemand = FlarkV3VisibleBlockDemand(
        sourceRevision: runtime.sourceRevision,
        structureGeneration: recovered.structureGeneration,
        startUtf16: 0,
        endUtf16: runtime.sourceLengthUtf16,
        maximumBlocks: 4,
      );
      expect(
        (materializer.advance(recoveredVisibleDemand, budget: rangeBudget)
                as FlarkV3ExactVisibleBlockSet)
            .blocks,
        hasLength(2),
        reason:
            'a newly fenced demand restarts from the replacement publication',
      );

      expect(runtime.exportMarkdown(), markdown);
      expect(
        runtime.queryAtUtf16(markdown.indexOf('**bold 450**')),
        isA<FlarkV3DocumentStructuralQuery>(),
      );

      await runtime.close().timeout(_functionalTimeout);
      await worker.done.timeout(_functionalTimeout);
      expect(endpoint.closeRequested, isTrue);
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
    },
    timeout: const Timeout(Duration(minutes: 1)),
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

/// Test-only decorator around the production Web endpoint.
///
/// It records frames without interpreting or changing them on the production
/// path. The one explicit replay sends a byte-for-byte valid seed page after
/// that seed is already installed, which makes the real Rust endpoint emit
/// its typed `InvalidSeed` failure. There is deliberately no shipping fault
/// injection API and this receipt does not simulate a physical Worker crash.
final class _SnapshotReplayEndpoint implements FlarkV3ByteEndpoint {
  _SnapshotReplayEndpoint({
    required FlarkV3WebWorkerByteEndpoint delegate,
    required FlarkV3ParserSessionBinding initialBinding,
  }) : _delegate = delegate,
       _establishedBinding = null,
       _initialBinding = initialBinding;

  final FlarkV3WebWorkerByteEndpoint _delegate;
  final FlarkV3ParserSessionBinding _initialBinding;
  FlarkV3ParserSessionBinding? _establishedBinding;
  Uint8List? _capturedSnapshotPage;
  final Map<int, List<FlarkV3ParserSessionSnapshotCommand>>
  _snapshotPagesByGeneration =
      <int, List<FlarkV3ParserSessionSnapshotCommand>>{};

  final List<int> failureCodes = <int>[];
  final List<FlarkV3ByteEndpointBinding> recoveredFrom =
      <FlarkV3ByteEndpointBinding>[];
  final Set<int> _structurallyCommittedGenerations = <int>{};
  final Map<int, Completer<void>> _structuralCommitWaiters =
      <int, Completer<void>>{};
  bool closeRequested = false;

  List<FlarkV3ParserSessionSnapshotCommand> snapshotPages(int generation) =>
      List<FlarkV3ParserSessionSnapshotCommand>.unmodifiable(
        _snapshotPagesByGeneration[generation] ??
            const <FlarkV3ParserSessionSnapshotCommand>[],
      );

  Future<void> waitForStructuralCommit(int generation) {
    if (_structurallyCommittedGenerations.contains(generation)) {
      return Future<void>.value();
    }
    return _structuralCommitWaiters
        .putIfAbsent(generation, Completer<void>.new)
        .future;
  }

  @override
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  }) {
    _delegate.bind(
      onFrame: (frame) {
        _recordFailure(frame);
        onFrame(frame);
      },
      onFailure: onFailure,
    );
  }

  @override
  void recover(FlarkV3ByteEndpointBinding previousBinding) {
    recoveredFrom.add(previousBinding);
    _delegate.recover(previousBinding);
  }

  @override
  void send(Uint8List frame) {
    _recordCommand(frame);
    _delegate.send(frame);
  }

  @override
  void sendHostPoll(Uint8List frame) {
    _recordHostPoll(frame);
    _delegate.sendHostPoll(frame);
  }

  @override
  void sendInlineSidecarHostPoll(Uint8List frame) =>
      _delegate.sendInlineSidecarHostPoll(frame);

  @override
  void sendViewportPresentationHostPoll(Uint8List frame) =>
      _delegate.sendViewportPresentationHostPoll(frame);

  @override
  void sendClose(Uint8List frame) {
    _recordCommand(frame);
    _delegate.sendClose(frame);
  }

  void replayCapturedSnapshotPage() {
    final frame = _capturedSnapshotPage;
    if (frame == null) {
      throw StateError('No valid SnapshotPage has been captured.');
    }
    _delegate.send(Uint8List.fromList(frame));
  }

  void _recordCommand(Uint8List frame) {
    final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
      frame,
      establishedBinding: _establishedBinding,
    );
    final command = decoded.command;
    if (command is FlarkV3ParserSessionOpenCommand) {
      _establishedBinding = command.binding;
      if (command.mode == FlarkV3ParserOpenMode.fresh &&
          command.binding != _initialBinding) {
        throw StateError('Fresh open crossed the test binding.');
      }
      return;
    }
    if (command is FlarkV3ParserSessionSnapshotCommand) {
      _capturedSnapshotPage ??= Uint8List.fromList(frame);
      _snapshotPagesByGeneration
          .putIfAbsent(
            command.binding.workerGeneration,
            () => <FlarkV3ParserSessionSnapshotCommand>[],
          )
          .add(command);
    }
  }

  void _recordFailure(Uint8List frame) {
    try {
      final envelope = FlarkV3WireProtocol.decode(
        frame,
        kind: FlarkV3WireFrameKind.request,
      );
      if (envelope.opcode != FlarkV3WireOpcode.parserPoll) return;
      final binding = _establishedBinding;
      if (binding == null) return;
      final event = FlarkV3ParserSessionWireCodec.decodeEvent(
        frame,
        expectedBinding: binding,
        requireDrainGrant: false,
      );
      if (event is FlarkV3ParserSessionFailedEvent) {
        failureCodes.add(event.failureCode);
      }
    } on Object {
      // Observation must never perturb a valid production frame. The typed
      // wire transport remains the authority that validates and routes it.
    }
  }

  void _recordHostPoll(Uint8List frame) {
    final binding = _establishedBinding;
    if (binding == null) return;
    try {
      final command = FlarkV3ParserPublicationWireCodec.decodeCommand(
        frame,
        expectedBinding: binding,
      ).command;
      if (command case FlarkV3ParserHostPollCompleted(
        outcome: FlarkV3HostCommitted(),
      )) {
        final generation = binding.workerGeneration;
        _structurallyCommittedGenerations.add(generation);
        final waiter = _structuralCommitWaiters[generation];
        if (waiter != null && !waiter.isCompleted) waiter.complete();
      }
    } on Object {
      // Observation must not perturb the production publication transport.
    }
  }

  @override
  void close() {
    closeRequested = true;
    _delegate.close();
  }
}
