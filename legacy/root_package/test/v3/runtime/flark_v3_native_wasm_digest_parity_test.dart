import 'dart:async';
import 'dart:convert';

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_platform_host_store_factory.dart';
import 'package:test/test.dart';

import 'support/flark_v3_digest_parity_platform_native.dart'
    if (dart.library.js_interop) 'support/flark_v3_digest_parity_platform_web.dart'
    as platform;

const _fixture = '[x]: /target\nCafé 😀 [x]\n';
const _expectedDigests = (
  canonical: '98bdb389d73432ea7b1d5a03ff1a3cea',
  sequence: '35b8bbf902a2574d828cc7e10b1fc49d',
  manifest: '9add2baf777de19103158a44270fda36',
);
const _expectedCheckpointBParityDigest =
    '2233974cc604304e5e2e8bc28f0f36ea25cf51f00ef8470338aad783baab11d4';

// The native gate runs this file against isolate + FFI and the Web gate runs
// the same compiled fixture against a real Worker plus independent host Wasm.
// The shared golden therefore joins the two otherwise separate processes.
// Keep the authority-bound golden first: later diagnostics intentionally
// allocate process-local identities that are not part of semantic parity.

void main() {
  test(
    'native and real Wasm produce the same canonical, sequence, and manifest digests',
    () async {
      final documentSession = FlarkV3DocumentSessionId(
        0x464c4b33,
        0x10203040,
        0x50607080,
        0x90a0b0c0,
      );
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(
        _fixture,
      );
      expect(
        sourceSession.sourceSessionIdentity,
        1,
        reason: 'the fixed receipt requires an isolated test session',
      );

      final hostStore = _DigestCapturingHostStore(
        await createFlarkV3DefaultPlatformHostStore(
          documentSession: documentSession,
          nativeLibraryPath: platform.flarkV3DigestParityNativeLibraryPath,
          webAssets: platform.flarkV3DigestParityWebAssets,
        ),
      );
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
        certifiedSourceVersion: FlarkV3SourceVersion.empty(documentSession),
      );
      final runtime = await FlarkV3DocumentRuntimeAdapter.attach(
        document: document,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        nativeLibraryPath: platform.flarkV3DigestParityNativeLibraryPath,
        webAssets: platform.flarkV3DigestParityWebAssets,
      );
      addTearDown(() => runtime.close());

      await runtime.initialReady.timeout(const Duration(seconds: 20));
      final receipt = await hostStore
          .receiptForRevision(1)
          .timeout(const Duration(seconds: 20));
      final actual = (
        canonical: _digestHex(receipt.canonical),
        sequence: _digestHex(receipt.ack.sequenceDigest),
        manifest: _digestHex(receipt.ack.manifestDigest),
      );

      expect(
        receipt.canonical,
        isNot(receipt.ack.sequenceDigest),
        reason:
            'the canonical stream and ACK sequence are separate '
            'domain-bound proofs',
      );
      expect(actual, _expectedDigests);
      expect(runtime.status.structureCurrent, isTrue);
      expect(runtime.exportMarkdown(), _fixture);

      await runtime.close().timeout(const Duration(seconds: 10));
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'native and real Wasm decode an admitted recursive-Green budget gap',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _fixture,
        nativeLibraryPath: platform.flarkV3DigestParityNativeLibraryPath,
        webAssets: platform.flarkV3DigestParityWebAssets,
      );
      addTearDown(() => runtime.close());
      await runtime.initialReady.timeout(const Duration(seconds: 20));

      final boundedGap = runtime.queryAtUtf16(
        _fixture.indexOf('Café'),
        budget: const FlarkV3DocumentQueryBudget(maximumTreeNodesVisited: 1),
      );
      expect(
        boundedGap,
        isA<FlarkV3DocumentSourceGapQuery>(),
        reason:
            'Both native FFI and Web Wasm decoders must accept a typed budget '
            'gap whose receipt stays inside caller authority.',
      );
      expect(
        (boundedGap as FlarkV3DocumentSourceGapQuery).reason,
        FlarkV3DocumentQueryGapReason.treeNodeLimit,
      );

      await runtime.close().timeout(const Duration(seconds: 10));
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'native and real Wasm produce the same Checkpoint B parity projection',
    () async {
      final encoded = await runFlarkV3CheckpointBProbeJson(
        nativeLibraryPath: platform.flarkV3DigestParityNativeLibraryPath,
        webAssets: platform.flarkV3DigestParityWebAssets,
      );
      final receipt = jsonDecode(encoded) as Map<String, Object?>;

      expect(receipt['schema'], 1);
      expect(receipt['allChecksPassed'], isTrue);
      expect(receipt['parityDigest'], _expectedCheckpointBParityDigest);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

String _digestHex(FlarkV3ProtocolDigest128 digest) => <int>[
  digest.word0,
  digest.word1,
  digest.word2,
  digest.word3,
].map((word) => word.toRadixString(16).padLeft(8, '0')).join();

typedef _DigestReceipt = ({
  FlarkV3ProtocolDigest128 canonical,
  FlarkV3StructuralAck ack,
});

final class _DigestCapturingHostStore implements FlarkV3HostStore {
  _DigestCapturingHostStore(this._delegate);

  final FlarkV3HostStore _delegate;
  final Map<FlarkV3OfferId, FlarkV3HostOfferBegin> _offers = {};
  final Map<int, FlarkV3ProtocolDigest128> _canonicalByRevision = {};
  final Map<int, FlarkV3StructuralAck> _ackByRevision = {};
  final Map<int, FlarkV3StructuralAck> _deliveredByRevision = {};
  final Map<int, Completer<_DigestReceipt>> _receiptWaiters = {};

  Future<_DigestReceipt> receiptForRevision(int revision) {
    final receipt = _completeReceipt(revision);
    if (receipt != null) return Future.value(receipt);
    return _receiptWaiters
        .putIfAbsent(revision, Completer<_DigestReceipt>.sync)
        .future;
  }

  _DigestReceipt? _completeReceipt(int revision) {
    final canonical = _canonicalByRevision[revision];
    final ack = _ackByRevision[revision];
    final delivered = _deliveredByRevision[revision];
    if (canonical == null || ack == null || delivered == null) return null;
    if (delivered != ack) {
      throw StateError('Parser delivery ACK differs from the installed ACK.');
    }
    return (canonical: canonical, ack: ack);
  }

  void _notifyReceipt(int revision) {
    final waiter = _receiptWaiters[revision];
    if (waiter == null || waiter.isCompleted) return;
    try {
      final receipt = _completeReceipt(revision);
      if (receipt != null) waiter.complete(receipt);
    } on Object catch (error, stackTrace) {
      waiter.completeError(error, stackTrace);
    }
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => _delegate.observeSourceVersion(sourceVersion);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    final result = _delegate.beginOffer(begin);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _offers[begin.offerId] = begin;
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _delegate.admitPacket(packet);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    final result = _delegate.requestCommit(request);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      final offer = _offers[request.offerId];
      if (offer == null) {
        throw StateError('Accepted commit has no captured offer.');
      }
      _canonicalByRevision[offer.sourceVersion.revision] =
          request.canonicalStreamDigest;
      _notifyReceipt(offer.sourceVersion.revision);
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      _delegate.abortOffer(offerId);

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final result = _delegate.poll(grant);
    if (result is FlarkV3HostAccepted<FlarkV3HostPollOutcome>) {
      final outcome = result.value;
      if (outcome is FlarkV3HostCommitted) {
        _ackByRevision[outcome.ack.sourceVersion.revision] = outcome.ack;
        _notifyReceipt(outcome.ack.sourceVersion.revision);
      }
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    final result = _delegate.acknowledgeDelivery(ack);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _deliveredByRevision[ack.sourceVersion.revision] = ack;
      _notifyReceipt(ack.sourceVersion.revision);
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => _delegate.queryStructural(query);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() => _delegate.close();
}
