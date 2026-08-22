@TestOn('browser')
library;

import 'dart:async';

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_driver.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_parser_transport.dart';
import 'package:flark/src/v3/runtime/web/flark_v3_web_host_store.dart';
import 'package:flark/src/v3/runtime/web/flark_v3_web_worker_byte_endpoint.dart';
import 'package:test/test.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'real Worker publishes one 24-leaf VPB1 page through the Wasm host',
    () async {
      final assets = FlarkV3WebRuntimeAssets.packageDefaults();
      final markdown = _viewportMarkdown();
      final sourceSession = FlarkV3SourceSession.fromString(markdown);
      final documentSession = FlarkV3DocumentSessionId(811, 812, 813, 814);
      final hostStore = _PoisoningViewportHostStore(
        await FlarkV3WebHostStore.create(
          wasmUri: assets.wasmUri,
          documentSession: documentSession,
        ).timeout(_functionalTimeout),
      );
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final endpoint = await FlarkV3WebWorkerByteEndpoint.start(
        workerUri: assets.workerUri,
        wasmUri: assets.wasmUri,
      ).timeout(_functionalTimeout);
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
        await endpoint.done.timeout(_closeTimeout);
      });

      observeProgress();
      final ack = await published.future.timeout(
        _functionalTimeout,
        onTimeout: () => throw StateError(
          'Web VPB1 pipeline stalled: requested=$requested, '
          'executor=${executor.state}, '
          'sourceSynchronized=${session.sourceWorkerSynchronized}, '
          'presentation=${session.presentationState.runtimeType}, '
          'publication=${executor.viewportPresentationPublicationState}, '
          'unavailable=${executor.lastViewportPresentationUnavailableReason}, '
          'parserFailure=${executor.lastViewportPresentationFailure}, '
          'hostRejection=${executor.lastHostRejection}, failures=$failures',
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
          maximumEncodedBytes: 64 * 1024,
        ),
      );
      expect(
        query,
        isA<FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>>(),
      );
      final page =
          ((query
                          as FlarkV3HostAccepted<
                            FlarkV3ViewportPresentationQueryOutcome
                          >)
                      .value
                  as FlarkV3ViewportPresentationQueryAvailable)
              .page;
      expect(page.entryCount, 24);
      expect(
        page.entries.where((entry) => entry.isAuthoritative),
        hasLength(23),
      );
      expect(
        page.entries
            .where((entry) => !entry.isAuthoritative)
            .single
            .orderedChildIndex,
        12,
      );
      expect(
        hostStore.poisonedPacketCount,
        greaterThan(0),
        reason:
            'The Wasm host must synchronously copy every accepted caller '
            'packet before returning.',
      );

      final undersized = session.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: page.encodedPage.lengthInBytes - 1,
        ),
      );
      expect(
        (undersized
                as FlarkV3HostRejected<FlarkV3ViewportPresentationQueryOutcome>)
            .rejection
            .reason,
        FlarkV3HostRejectReason.queryBoundExceeded,
      );
      final exact = session.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: page.encodedPage.lengthInBytes,
        ),
      );
      expect(
        ((exact as FlarkV3HostAccepted<FlarkV3ViewportPresentationQueryOutcome>)
                    .value
                as FlarkV3ViewportPresentationQueryAvailable)
            .page
            .encodedPage,
        orderedEquals(page.encodedPage),
      );

      await executor.close().timeout(_closeTimeout);
      await endpoint.done.timeout(_closeTimeout);
      expect(executor.state, FlarkV3SessionDriverState.closed);
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real Worker admits 64 moderately dense VPB1 leaves',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _moderatelyDenseViewportMarkdown(),
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        viewportPresentationDemandOwner: true,
      );
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(_closeTimeout);
      });

      await runtime.initialReady.timeout(_functionalTimeout);
      final current = runtime.status;
      final located = lease.queryBlockOrdinalWindow(
        FlarkV3DocumentOrdinalWindowDemand(
          sourceRevision: current.sourceRevision,
          structureGeneration: current.structureGeneration,
          startBlockOrdinal: 0,
        ),
        budget: const FlarkV3DocumentOrdinalWindowBudget(maximumEntries: 64),
      );
      expect(located, isA<FlarkV3ExactDocumentOrdinalWindow>());
      final exactWindow = located as FlarkV3ExactDocumentOrdinalWindow;
      expect(exactWindow.nextBlockOrdinal, 64);

      final demand = FlarkV3ViewportPresentationDemand(
        sourceRevision: current.sourceRevision,
        structureGeneration: current.structureGeneration,
        startUtf16: exactWindow.coveredSource.startUtf16,
        endUtf16: exactWindow.coveredSource.endUtf16,
        startBlockOrdinal: exactWindow.startBlockOrdinal,
      );
      final completion = runtime.statuses.firstWhere(
        (status) =>
            status.viewportPresentationAttemptOutcomeGeneration >
            current.viewportPresentationAttemptOutcomeGeneration,
      );
      final receipt = lease.ensureViewportPresentation(demand);
      expect(
        receipt.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );
      final completed = await completion.timeout(_functionalTimeout);
      expect(
        completed.viewportPresentationGeneration,
        receipt.viewportGeneration,
      );
      expect(completed.viewportPresentationUnavailableReason, isNull);

      final queried = lease.queryViewportPresentation(demand);
      expect(queried, isA<FlarkV3ExactViewportPresentationPage>());
      final page = (queried as FlarkV3ExactViewportPresentationPage).page;
      expect(page.ack.envelope.visitedStructuralEntries, 64);
      expect(page.ack.envelope.orderedLeafCount, 64);
      // 192 facts crossed the retired logical-page * maximum-record
      // approximation even though the actual bounded VPB1 transport fits.
      expect(page.ack.envelope.factCount, 64 * 3);
      expect(page.ack.envelope.parserTransitions, lessThanOrEqualTo(250000));
      expect(page.entryCount, 64);
      expect(page.encodedPage.lengthInBytes, lessThan(64 * 1024));
      expect(page.ack.actualEncodedFrameBytes, lessThanOrEqualTo(512 * 1024));
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real Worker publishes typed budget exhaustion for a simultaneous-max candidate',
    () async {
      final assets = FlarkV3WebRuntimeAssets.packageDefaults();
      final markdown = _maximumAdversarialViewportMarkdown();
      expect(markdown.length, greaterThan(60 * 1024));
      expect(markdown.length, lessThanOrEqualTo(64 * 1024));
      expect(RegExp(r'\*\*x\*\*').allMatches(markdown), hasLength(2048));
      final sourceSession = FlarkV3SourceSession.fromString(markdown);
      final documentSession = FlarkV3DocumentSessionId(821, 822, 823, 824);
      final hostStore = await FlarkV3WebHostStore.create(
        wasmUri: assets.wasmUri,
        documentSession: documentSession,
      ).timeout(_functionalTimeout);
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: hostStore,
      );
      final endpoint = await FlarkV3WebWorkerByteEndpoint.start(
        workerUri: assets.workerUri,
        wasmUri: assets.wasmUri,
      ).timeout(_functionalTimeout);
      final failures = <Object>[];
      final exhausted = Completer<int>();
      final transport = FlarkV3WireParserTransport(
        endpoint: endpoint,
        onFailure: (error, stackTrace) {
          failures.add(error);
          if (!exhausted.isCompleted) {
            exhausted.completeError(error, stackTrace);
          }
        },
      );
      var requested = false;
      late final FlarkV3SessionExecutor executor;

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
        final reason = executor.lastViewportPresentationUnavailableReason;
        if (reason != null && !exhausted.isCompleted) {
          exhausted.complete(reason);
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
        onFailure: (error, stackTrace) {
          failures.add(error);
          if (!exhausted.isCompleted) {
            exhausted.completeError(error, stackTrace);
          }
        },
      );
      addTearDown(() async {
        executor.emergencyDispose();
        transport.close();
        await endpoint.done.timeout(_closeTimeout);
      });

      observeProgress();
      final reason = await exhausted.future.timeout(_functionalTimeout);
      expect(requested, isTrue);
      expect(
        reason,
        FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
      );
      expect(executor.viewportPresentationAttemptOutcomeGeneration, 1);
      expect(session.installedViewportPresentationAck, isNull);
      expect(failures, isEmpty);

      await executor.close().timeout(_closeTimeout);
      await endpoint.done.timeout(_closeTimeout);
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );
}

final class _PoisoningViewportHostStore
    implements FlarkV3HostStore, FlarkV3ViewportPresentationHostStore {
  _PoisoningViewportHostStore(this.delegate);

  final FlarkV3WebHostStore delegate;
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

String _moderatelyDenseViewportMarkdown() =>
    List<String>.filled(64, '# **strong** *emphasis* `code`').join('\n');

String _maximumAdversarialViewportMarkdown() {
  final facts = List<String>.filled(32, '**x**').join(' ');
  final padding = List<String>.filled(800, 'p').join();
  return List<String>.filled(64, '# $facts $padding').join('\n');
}
