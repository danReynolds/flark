import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_driver.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_parser_transport.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_host_store.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_isolate_byte_endpoint.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_library_locator.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/session/session.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test(
    'real endpoint publishes one 24-leaf VPB1 page through the native host',
    () async {
      final libraryPath = _nativeLibraryPath;
      if (libraryPath == null) return;
      final libraryFile = File(libraryPath).absolute;
      expect(
        libraryFile.existsSync(),
        isTrue,
        reason: 'Build the release native bridge before this integration gate.',
      );

      final markdown = _viewportMarkdown();
      final sourceSession = FlarkV3SourceSession.fromString(markdown);
      final documentSession = FlarkV3DocumentSessionId(801, 802, 803, 804);
      final hostStore = _PoisoningViewportHostStore(
        FlarkV3NativeHostStore.create(
          library: openFlarkV3NativeLibrary(
            overrideLibraryPath: libraryFile.path,
          ),
          documentSession: documentSession,
        ),
      );
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final endpoint = await FlarkV3NativeIsolateByteEndpoint.start(
        overrideLibraryPath: libraryFile.path,
      );
      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) {},
      );
      final published = Completer<FlarkV3ViewportPresentationAck>();
      final failures = <Object>[];
      var requested = false;
      late final FlarkV3SessionExecutor executor;

      void failPipeline(Object error, StackTrace stackTrace) {
        failures.add(error);
        if (!published.isCompleted) {
          published.completeError(error, stackTrace);
        }
      }

      void observeProgress() {
        if (!requested &&
            executor.state == FlarkV3SessionDriverState.open &&
            session.sourceWorkerSynchronized &&
            session.presentationState is FlarkV3ExactStructuralPresentation) {
          requested = true;
          executor.requestViewportPresentation(
            requestedStartUtf8: 0,
            requestedStartUtf16: 0,
            requestedEndUtf8: session.source.utf8Length,
            requestedEndUtf16: session.source.utf16Length,
            startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          );
        }
        final ack = session.installedViewportPresentationAck;
        if (ack != null &&
            session.pendingViewportPresentationDeliveryAck == null &&
            !published.isCompleted) {
          published.complete(ack);
        }
      }

      executor = FlarkV3SessionExecutor.attach(
        session: session,
        transport: transport,
        parserBinding: FlarkV3ParserSessionBinding(
          documentSession: documentSession,
          sourceSessionIdentity: sourceSession.sourceSessionIdentity,
          workerGeneration: sourceSession.workerGeneration,
        ),
        publicationAuthority: FlarkV3ParserPublicationAuthority(
          grammarRevision: flarkV3CurrentGrammarRevision,
          syntaxProfile: FlarkV3SyntaxProfileId(1),
          authorityMask: FlarkV3StructuralAuthorityMask.complete,
        ),
        onProgress: observeProgress,
        onFailure: failPipeline,
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(const Duration(seconds: 5));
      });

      observeProgress();
      final ack = await published.future.timeout(
        const Duration(seconds: 10),
        onTimeout: () => throw StateError(
          'VPB1 pipeline stalled: requested=$requested, '
          'executor=${executor.state}, '
          'sourceSynchronized=${session.sourceWorkerSynchronized}, '
          'presentation=${session.presentationState.runtimeType}, '
          'publication=${executor.viewportPresentationPublicationState}, '
          'unavailable=${executor.lastViewportPresentationUnavailableReason}, '
          'parserFailure=${executor.lastViewportPresentationFailure}, '
          'hostRejection=${executor.lastHostRejection}, '
          'installed=${session.installedViewportPresentationAck != null}, '
          'pendingDelivery='
          '${session.pendingViewportPresentationDeliveryAck != null}, '
          'failures=$failures',
        ),
      );
      expect(requested, isTrue);
      expect(failures, isEmpty);
      expect(ack.binding.complete, isTrue);
      expect(ack.envelope.orderedLeafCount, 24);
      expect(ack.envelope.factCount, 23 * 3);

      final query = session.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: flarkV3NativeHostMaximumQueryBytes,
        ),
      );
      expect(
        query,
        isA<FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>>(),
      );
      final outcome =
          (query
                  as FlarkV3HostAccepted<
                    FlarkV3ViewportPresentationQueryOutcome
                  >)
              .value;
      expect(outcome, isA<FlarkV3ViewportPresentationQueryAvailable>());
      final page = (outcome as FlarkV3ViewportPresentationQueryAvailable).page;
      expect(page.ack, ack);
      expect(page.entryCount, 24);
      expect(
        hostStore.poisonedPacketCount,
        greaterThan(0),
        reason:
            'The successful commit must survive mutation of caller-owned '
            'FPK3 bytes immediately after every accepted native admission.',
      );
      expect(
        _containsBytes(page.encodedPage, utf8.encode('HIO1')),
        isFalse,
        reason: 'Opaque engine HIO1 wrappers must not cross the host boundary.',
      );

      final authoritative = page.entries
          .where((entry) => entry.isAuthoritative)
          .toList(growable: false);
      final unsupported = page.entries
          .where((entry) => !entry.isAuthoritative)
          .toList(growable: false);
      expect(authoritative, hasLength(23));
      expect(unsupported, hasLength(1));
      expect(unsupported.single.orderedChildIndex, 12);
      expect(
        unsupported.single.payloadKind,
        FlarkV3ViewportPresentationPayloadKind.unsupported,
      );

      final first = authoritative.first;
      final firstLeaf = FlarkV3SourceSpan(
        startUtf8: first.binding.visibleStartUtf8,
        endUtf8: first.binding.visibleEndUtf8,
        startUtf16: first.binding.visibleStartUtf16,
        endUtf16: first.binding.visibleEndUtf16,
      );
      final inlineFacts = FlarkV3InlineFactsDecoder.decode(
        sourceDocument: session.source,
        expectedSource: ack.baseAck.sourceVersion,
        factSource: first.sourceVersion,
        expectedProfilePartition: ack.baseAck.syntaxProfile.value,
        profilePartition: first.binding.parserProfile.value,
        expectedLeaf: firstLeaf,
        factLeaf: firstLeaf,
        disposition: FlarkV3InlineFactsDisposition.authoritative,
        factCount: first.recordCount,
        encodedFacts: first.payload,
      );
      expect(
        inlineFacts.facts.map((fact) => fact.kind),
        <FlarkV3InlineFactKind>[
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.code,
        ],
      );

      final undersizedQuery = session.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: page.encodedPage.lengthInBytes - 1,
        ),
      );
      expect(
        undersizedQuery,
        isA<FlarkV3HostRejected<FlarkV3ViewportPresentationQueryOutcome>>(),
      );
      expect(
        (undersizedQuery
                as FlarkV3HostRejected<FlarkV3ViewportPresentationQueryOutcome>)
            .rejection
            .reason,
        FlarkV3HostRejectReason.queryBoundExceeded,
      );
      final exactQuery = session.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: page.encodedPage.lengthInBytes,
        ),
      );
      expect(
        exactQuery,
        isA<FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>>(),
      );
      final exactPage =
          ((exactQuery
                          as FlarkV3HostAccepted<
                            FlarkV3ViewportPresentationQueryOutcome
                          >)
                      .value
                  as FlarkV3ViewportPresentationQueryAvailable)
              .page;
      expect(exactPage.entryCount, page.entryCount);
      expect(exactPage.encodedPage, orderedEquals(page.encodedPage));

      await executor.close().timeout(const Duration(seconds: 5));
      await endpoint.done.timeout(const Duration(seconds: 5));
      expect(executor.state, FlarkV3SessionDriverState.closed);
      expect(failures, isEmpty);
    },
    skip: _nativeLibraryPath == null
        ? 'Native viewport integration is unsupported on this platform.'
        : false,
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

final class _PoisoningViewportHostStore
    implements FlarkV3HostStore, FlarkV3ViewportPresentationHostStore {
  _PoisoningViewportHostStore(this.delegate);

  final FlarkV3NativeHostStore delegate;
  int poisonedPacketCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => delegate.observeSourceVersion(sourceVersion);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => delegate.beginOffer(begin);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => delegate.admitPacket(packet);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => delegate.requestCommit(request);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      delegate.abortOffer(offerId);

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => delegate.poll(grant);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => delegate.acknowledgeDelivery(ack);

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => delegate.queryStructural(query);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) => delegate.beginViewportPresentationOffer(begin);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final result = delegate.admitViewportPresentationPacket(packet);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      packet.rawBytes.fillRange(0, packet.rawBytes.lengthInBytes, 0xa5);
      poisonedPacketCount += 1;
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) => delegate.requestViewportPresentationCommit(request);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) => delegate.abortViewportPresentationOffer(offerId);

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) =>
      delegate.pollViewportPresentation(grant);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) =>
      delegate.acknowledgeViewportPresentationDelivery(ack);

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) =>
      delegate.queryViewportPresentation(query);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() => delegate.close();
}

String _viewportMarkdown() {
  const paragraphs = 24;
  const unsupportedOrdinal = 12;
  final output = StringBuffer();
  for (var ordinal = 0; ordinal < paragraphs; ordinal += 1) {
    if (ordinal != 0) output.write('\n\n');
    if (ordinal == unsupportedOrdinal) {
      output.write('before <tag>');
    } else {
      final suffix = ordinal.toString().padLeft(2, '0');
      output.write('**bold$suffix** *em$suffix* `code$suffix`');
    }
  }
  return output.toString();
}

bool _containsBytes(List<int> haystack, List<int> needle) {
  if (needle.isEmpty) return true;
  for (var start = 0; start + needle.length <= haystack.length; start += 1) {
    var matches = true;
    for (var offset = 0; offset < needle.length; offset += 1) {
      if (haystack[start + offset] != needle[offset]) {
        matches = false;
        break;
      }
    }
    if (matches) return true;
  }
  return false;
}

String? get _nativeLibraryPath => switch (Platform.operatingSystem) {
  'macos' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
  'linux' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
  'windows' => 'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
  _ => null,
};
