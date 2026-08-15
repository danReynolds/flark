@TestOn('browser')
library;

import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/web/flark_v3_web_host_store.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';

void main() {
  test(
    'main-context Wasm host observes source and truthfully reclaims',
    () async {
      final documentSession = FlarkV3DocumentSessionId(0x464c4b33, 2, 3, 4);
      final store = await FlarkV3WebHostStore.create(
        wasmUri: Uri.base.resolve(
          'packages/flark/assets/wasm/flark_comrak_bridge.wasm',
        ),
        documentSession: documentSession,
      );

      expect(
        store.observeSourceVersion(FlarkV3SourceVersion.empty(documentSession)),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
      );
      expect(store.close(), isA<FlarkV3HostAccepted<FlarkV3HostUnit>>());

      FlarkV3HostPollOutcome? terminal;
      for (
        var turn = 0;
        turn < 8 && terminal is! FlarkV3HostClosed;
        turn += 1
      ) {
        final polled = store.poll(
          FlarkV3HostWorkGrant(
            inspectBytes: 64 * 1024,
            copyBytes: 64 * 1024,
            transitions: 128,
          ),
        );
        expect(polled, isA<FlarkV3HostAccepted<FlarkV3HostPollOutcome>>());
        terminal =
            (polled as FlarkV3HostAccepted<FlarkV3HostPollOutcome>).value;
      }
      expect(terminal, isA<FlarkV3HostClosed>());
      expect(
        store.close(),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
        reason: 'close remains idempotent after proven Wasm-slot removal',
      );
    },
  );

  test(
    'main-context Wasm host keeps hot-inline lifecycle and query parity',
    () async {
      final store = await FlarkV3WebHostStore.create(
        wasmUri: Uri.base.resolve(
          'packages/flark/assets/wasm/flark_comrak_bridge.wasm',
        ),
        documentSession: _sidecarDocumentSession,
      );
      _expectAcceptedUnit(store.observeSourceVersion(_sidecarSource));

      final begin = _sidecarBegin();
      _expectRejected(
        store.beginInlineSidecarOffer(begin),
        FlarkV3HostRejectReason.baseMismatch,
      );

      final packet = testPublicationPacket(
        offerId: begin.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: FlarkV3ProtocolDigest128.zero,
        frameBytes: Uint8List.fromList(const <int>[0]),
      );
      _expectRejected(
        store.admitInlineSidecarPacket(packet),
        FlarkV3HostRejectReason.wrongOffer,
      );
      _expectRejected(
        store.requestInlineSidecarCommit(
          FlarkV3HotInlineSidecarCommitRequest(
            offerId: begin.offerId,
            actualFrameCount: 2,
            actualEncodedFrameBytes: 1,
            rollingTransportDigest: FlarkV3ProtocolDigest128.zero,
            rootStreamDigest: FlarkV3ProtocolDigest128.zero,
          ),
        ),
        FlarkV3HostRejectReason.wrongOffer,
      );
      _expectRejected(
        store.abortInlineSidecarOffer(begin.offerId),
        FlarkV3HostRejectReason.wrongOffer,
      );

      final sidecarPoll = store.pollInlineSidecar(_sidecarGrant);
      expect(
        (sidecarPoll
                as FlarkV3HostAccepted<FlarkV3InlineSidecarHostPollOutcome>)
            .value,
        isA<FlarkV3InlineSidecarHostPollPending>(),
      );
      _expectRejected(
        store.acknowledgeInlineSidecarDelivery(_sidecarAck(begin)),
        FlarkV3HostRejectReason.invalid,
      );

      final query = store.queryInlineSidecar(
        FlarkV3InlineSidecarQuery(
          binding: begin.binding,
          maximumEncodedBytes: 1024,
        ),
      );
      expect(
        (query as FlarkV3HostAccepted<FlarkV3InlineSidecarQueryOutcome>).value,
        isA<FlarkV3InlineSidecarQueryUnavailable>(),
      );

      _expectAcceptedUnit(store.close());
      final closingSidecar = store.pollInlineSidecar(_sidecarGrant);
      expect(
        (closingSidecar
                as FlarkV3HostAccepted<FlarkV3InlineSidecarHostPollOutcome>)
            .value,
        isA<FlarkV3InlineSidecarHostClosed>(),
        reason:
            'sidecar terminal observation must not reclaim the structural host',
      );

      final closed = store.poll(_sidecarGrant);
      expect(
        (closed as FlarkV3HostAccepted<FlarkV3HostPollOutcome>).value,
        isA<FlarkV3HostClosed>(),
      );
      _expectRejected(
        store.pollInlineSidecar(_sidecarGrant),
        FlarkV3HostRejectReason.closed,
      );
    },
  );
}

final _sidecarDocumentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _sidecarSource = FlarkV3SourceVersion(
  documentSession: _sidecarDocumentSession,
  revision: 1,
  metric: FlarkV3SourceMetric(bytes: 4, utf16: 4),
  contentHash: const FlarkV3ContentHash128(5, 6, 7, 8),
);
final _sidecarBaseAck = FlarkV3StructuralAck(
  publicationSession: FlarkV3PublicationSessionId(9, 0, 0, 0),
  hostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: _sidecarSource,
  sourceRoot: FlarkV3SourceRootId(10, 11),
  parseGeneration: 1,
  grammarRevision: 1,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 1,
  sequenceDigest: FlarkV3ProtocolDigest128.zero,
  manifestDigest: FlarkV3ProtocolDigest128.zero,
);
final _sidecarGrant = FlarkV3HostWorkGrant(
  inspectBytes: 1024,
  copyBytes: 1024,
  transitions: 64,
);

FlarkV3HotInlineSidecarOfferBegin _sidecarBegin() =>
    FlarkV3HotInlineSidecarOfferBegin(
      offerId: FlarkV3OfferId(12, 0, 0, 0),
      publicationSession: FlarkV3PublicationSessionId(13, 0, 0, 0),
      baseAck: _sidecarBaseAck,
      binding: FlarkV3HotInlineSidecarBinding(
        parserProfile: FlarkV3SyntaxProfileId(1),
        refinementGeneration: FlarkV3ProtocolU64.fromU32(1),
        blockOrdinal: FlarkV3ProtocolU64.fromU32(0),
        physicalStartUtf8: 0,
        physicalEndUtf8: 4,
        visibleStartUtf8: 0,
        visibleEndUtf8: 4,
        physicalStartUtf16: 0,
        physicalEndUtf16: 4,
        visibleStartUtf16: 0,
        visibleEndUtf16: 4,
      ),
      envelope: FlarkV3HotInlineSidecarEnvelopeMetrics(
        hio1EncodedBytes:
            FlarkV3HotInlineSidecarEnvelopeMetrics.hio1EnvelopeBytes,
        ipr2DescriptorBytes: 0,
        transferredNodeCount: 1,
        hio1EnvelopeDigest256: FlarkV3ProtocolDigest256.zero,
        disposition: FlarkV3HotInlineSidecarUnsupported(
          reason: 7,
          metadataCommitment256: FlarkV3ProtocolDigest256.zero,
        ),
      ),
      limits: FlarkV3HostOfferLimits(
        maximumFrameCount: 4,
        maximumEncodedFrameBytes: 1024,
        maximumPacketBytes: 580,
        maximumFrameBytes: 512,
        maximumProgramChildren: 8,
      ),
    );

FlarkV3InlineSidecarAck _sidecarAck(FlarkV3HotInlineSidecarOfferBegin begin) =>
    FlarkV3InlineSidecarAck(
      publicationSession: begin.publicationSession,
      baseAck: begin.baseAck,
      refinementGeneration: begin.binding.refinementGeneration,
      blockOrdinal: begin.binding.blockOrdinal,
      transferredNodeCount: 1,
      disposition: FlarkV3InlineSidecarAckDisposition.unsupported,
      hio1EnvelopeDigest256: begin.envelope.hio1EnvelopeDigest256,
      rootStreamDigest: FlarkV3ProtocolDigest128.zero,
    );

void _expectAcceptedUnit(FlarkV3HostCallResult<FlarkV3HostUnit> result) =>
    expect(result, isA<FlarkV3HostAccepted<FlarkV3HostUnit>>());

void _expectRejected<T>(
  FlarkV3HostCallResult<T> result,
  FlarkV3HostRejectReason reason,
) {
  expect(result, isA<FlarkV3HostRejected<T>>());
  expect((result as FlarkV3HostRejected<T>).rejection.reason, reason);
}
