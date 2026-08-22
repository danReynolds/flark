import 'dart:io';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_host_store.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_library_locator.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';
import '../support/flark_v3_viewport_page_fixture.dart';

void main() {
  test(
    'native adapter keeps all seven VPB1 host calls typed and fail-closed',
    () {
      final libraryPath = _nativeLibraryPath;
      if (libraryPath == null) return;
      expect(
        File(libraryPath).existsSync(),
        isTrue,
        reason: 'Build the native bridge before running native contracts.',
      );

      final fixture = buildFlarkV3ViewportPageFixture();
      final store = FlarkV3NativeHostStore.create(
        library: openFlarkV3NativeLibrary(
          overrideLibraryPath: File(libraryPath).absolute.path,
        ),
        documentSession: fixture.ack.baseAck.sourceVersion.documentSession,
      );
      _expectAcceptedUnit(
        store.observeSourceVersion(fixture.ack.baseAck.sourceVersion),
      );

      final begin = _begin(fixture.ack);
      _expectRejected(
        store.beginViewportPresentationOffer(begin),
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
        store.admitViewportPresentationPacket(packet),
        FlarkV3HostRejectReason.wrongOffer,
      );
      _expectRejected(
        store.requestViewportPresentationCommit(
          FlarkV3ViewportPresentationCommitRequest(
            offerId: begin.offerId,
            actualFrameCount: fixture.ack.actualFrameCount,
            actualEncodedFrameBytes: fixture.ack.actualEncodedFrameBytes,
            rollingTransportDigest: FlarkV3ProtocolDigest128.zero,
            aggregateRootStreamDigest: FlarkV3ProtocolDigest128.zero,
          ),
        ),
        FlarkV3HostRejectReason.wrongOffer,
      );
      _expectRejected(
        store.abortViewportPresentationOffer(begin.offerId),
        FlarkV3HostRejectReason.wrongOffer,
      );

      final poll = store.pollViewportPresentation(_grant);
      expect(
        (poll
                as FlarkV3HostAccepted<
                  FlarkV3ViewportPresentationHostPollOutcome
                >)
            .value,
        isA<FlarkV3ViewportPresentationHostPollPending>(),
      );
      _expectRejected(
        store.acknowledgeViewportPresentationDelivery(fixture.ack),
        FlarkV3HostRejectReason.invalid,
      );
      _expectRejected(
        store.queryViewportPresentation(
          FlarkV3ViewportPresentationQuery(
            ack: fixture.ack,
            maximumEncodedBytes: 4096,
          ),
        ),
        FlarkV3HostRejectReason.baseMismatch,
      );
      _expectRejected(
        store.queryViewportPresentation(
          FlarkV3ViewportPresentationQuery(
            ack: fixture.ack,
            maximumEncodedBytes:
                flarkV3NativeHostViewportMaximumQueryBytes + 1,
          ),
        ),
        FlarkV3HostRejectReason.queryBoundExceeded,
      );

      _expectAcceptedUnit(store.close());
      final closingViewport = store.pollViewportPresentation(_grant);
      expect(
        (closingViewport
                as FlarkV3HostAccepted<
                  FlarkV3ViewportPresentationHostPollOutcome
                >)
            .value,
        isA<FlarkV3ViewportPresentationHostClosed>(),
        reason:
            'viewport terminal observation must not reclaim the structural host',
      );
      final closed = store.poll(_grant);
      expect(
        (closed as FlarkV3HostAccepted<FlarkV3HostPollOutcome>).value,
        isA<FlarkV3HostClosed>(),
      );
      _expectRejected(
        store.pollViewportPresentation(_grant),
        FlarkV3HostRejectReason.closed,
      );
    },
    skip: _nativeLibraryPath == null
        ? 'Native VPB1 host contract is unsupported on this platform.'
        : false,
  );
}

final _grant = FlarkV3HostWorkGrant(
  inspectBytes: 64 * 1024,
  copyBytes: 64 * 1024,
  transitions: 128,
);

FlarkV3ViewportPresentationOfferBegin _begin(
  FlarkV3ViewportPresentationAck ack,
) => FlarkV3ViewportPresentationOfferBegin(
  offerId: FlarkV3OfferId(71, 72, 73, 74),
  publicationSession: ack.publicationSession,
  baseAck: ack.baseAck,
  binding: ack.binding,
  envelope: ack.envelope,
  queryLimits: FlarkV3ViewportPresentationQueryLimits(
    maximumStructuralEntries: 8,
    maximumStoragePages: 8,
    maximumInlineLeaves: 8,
    maximumInlineLeafSourceBytes: 1024,
    maximumInlineSourceBytes: 4096,
    maximumFactRecords: 32,
    maximumEncodedFrameBytes: 4096,
    maximumParserTransitions: 1000,
  ),
  limits: FlarkV3ViewportPresentationOfferLimits(
    maximumFrameCount: 16,
    maximumEncodedFrameBytes: 4096,
    maximumPacketBytes: 2048,
    maximumFrameBytes: 1024,
    maximumProgramChildren: 32,
  ),
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

String? get _nativeLibraryPath => switch (Platform.operatingSystem) {
  'macos' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
  'linux' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
  'windows' => 'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
  _ => null,
};
