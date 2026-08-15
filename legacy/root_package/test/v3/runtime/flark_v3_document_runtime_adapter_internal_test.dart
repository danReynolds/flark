import 'dart:async';

import 'package:flark/flark_adapter.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_runtime.dart'
    show FlarkV3DocumentRuntimeState, FlarkV3RuntimeClosedBeforeReady;
import 'package:flark/src/v3/runtime/public/flark_v3_platform_host_store_factory.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  test(
    'adapter runtime owns native synchronization, edits, and close',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('live');
      final documentSession = FlarkV3DocumentSessionId(91, 92, 93, 94);
      final hostStore = await _nativeHostStore(documentSession);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final runtime = await FlarkV3DocumentRuntimeAdapter.attach(
        document: document,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
      );
      addTearDown(() async {
        await runtime.close().timeout(const Duration(seconds: 5));
      });

      final initialReady = runtime.initialReady;
      await initialReady.timeout(const Duration(seconds: 5));
      expect(runtime.status.sourceCurrent, isTrue);

      final passiveLease = FlarkV3DocumentRuntimeAdapter.borrow(runtime);
      final inlineOwner = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      final exactQuery =
          passiveLease.queryAtUtf16(2) as FlarkV3DocumentStructuralQuery;
      expect(
        () => passiveLease.ensureInlineAtUtf16(2, structuralQuery: exactQuery),
        throwsStateError,
      );
      expect(
        () => FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        ),
        throwsStateError,
      );
      inlineOwner.release();
      final replacementOwner = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      replacementOwner.release();
      passiveLease.release();

      final synchronizedEdit = runtime.statuses.firstWhere(
        (status) => status.sourceRevision == 1 && status.sourceCurrent,
      );
      final receipt = runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 4,
            endUtf16: 4,
            replacement: ' **Markdown**',
          ),
        ),
      );
      expect(receipt.changed, isTrue);
      expect(runtime.exportMarkdown(), 'live **Markdown**');
      expect(runtime.readSourceRange(5, 17), '**Markdown**');
      expect(
        runtime.status.sourceCurrent,
        isFalse,
        reason: 'current readiness must not inherit the startup receipt',
      );
      await initialReady.timeout(Duration.zero);

      await synchronizedEdit.timeout(const Duration(seconds: 5));
      expect(runtime.status.sourceCurrent, isTrue);

      await runtime.close().timeout(const Duration(seconds: 5));
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
      expect(hostStore.closing, isTrue);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'close intent rejects apply and undo before either can mutate source',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('undoable');
      final documentSession = FlarkV3DocumentSessionId(101, 102, 103, 104);
      final hostStore = await _nativeHostStore(documentSession);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final runtime = await FlarkV3DocumentRuntimeAdapter.attach(
        document: document,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
      );

      await runtime.initialReady.timeout(const Duration(seconds: 5));
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 8,
            endUtf16: 8,
            replacement: ' source',
          ),
        ),
      );
      final beforeClose = runtime.exportMarkdown();
      final close = runtime.close();

      expect(
        () => runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: runtime.sourceRevision,
            operation: const FlarkV3SourceEdit(
              startUtf16: 0,
              endUtf16: 0,
              replacement: 'late ',
            ),
          ),
        ),
        throwsStateError,
      );
      expect(runtime.undo, throwsStateError);
      expect(runtime.exportMarkdown(), beforeClose);

      await close.timeout(const Duration(seconds: 5));
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'close before startup settles initialReady with an explicit outcome',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('closing');
      final documentSession = FlarkV3DocumentSessionId(111, 112, 113, 114);
      final hostStore = await _nativeHostStore(documentSession);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final runtime = await FlarkV3DocumentRuntimeAdapter.attach(
        document: document,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
      );
      expect(runtime.status.structureCurrent, isFalse);

      final startup = expectLater(
        runtime.initialReady,
        throwsA(isA<FlarkV3RuntimeClosedBeforeReady>()),
      );
      await runtime.close().timeout(const Duration(seconds: 5));
      await startup;
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'background startup failure is owned without an initialReady listener',
    () async {
      final unhandled = <Object>[];
      final body = runZonedGuarded<Future<void>>(() async {
        final sourceSession = FlarkV3SourceSession.fromString('failure');
        final documentSession = FlarkV3DocumentSessionId(121, 122, 123, 124);
        final hostStore = await _nativeHostStore(documentSession);
        final document = FlarkDocumentSession.attach(
          sourceSession: sourceSession,
          documentSession: documentSession,
          hostStore: hostStore,
        );
        final runtime = await FlarkV3DocumentRuntimeAdapter.attach(
          document: document,
          parserBinding: FlarkV3ParserSessionBinding(
            documentSession: documentSession,
            sourceSessionIdentity: sourceSession.sourceSessionIdentity + 1,
            workerGeneration: sourceSession.workerGeneration,
          ),
        );

        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runtime.statuses
              .firstWhere(
                (status) => status.state == FlarkV3DocumentRuntimeState.closed,
              )
              .timeout(const Duration(seconds: 5));
        }
        await expectLater(runtime.close(), throwsStateError);
        await Future<void>.delayed(Duration.zero);
      }, (error, stackTrace) => unhandled.add(error));

      await body;
      expect(unhandled, isEmpty);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

Future<_PublicHostStore> _nativeHostStore(
  FlarkV3DocumentSessionId documentSession,
) async => _PublicHostStore(
  await createFlarkV3DefaultPlatformHostStore(
    documentSession: documentSession,
    nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
  ),
);

final class _PublicHostStore implements FlarkV3HostStore {
  _PublicHostStore(this._delegate);

  final FlarkV3HostStore _delegate;
  bool closing = false;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => _delegate.observeSourceVersion(sourceVersion);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closing = true;
    return _delegate.close();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => _delegate.poll(grant);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => _delegate.acknowledgeDelivery(ack);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      _delegate.abortOffer(offerId);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _delegate.admitPacket(packet);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => _delegate.beginOffer(begin);

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => _delegate.queryStructural(query);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => _delegate.requestCommit(request);
}
