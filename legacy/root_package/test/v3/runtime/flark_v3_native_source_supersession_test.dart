@TestOn('vm')
library;

import 'dart:convert';
import 'dart:io';

import 'package:flark/flark_adapter.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_host_store.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_library_locator.dart';
import 'package:test/test.dart';

void main() {
  test(
    'certified source adoption supersedes provisional in-flight staging',
    () {
      final libraryPath = _nativeLibraryPath;
      if (libraryPath == null) return;
      expect(
        File(libraryPath).existsSync(),
        isTrue,
        reason: 'Build the native bridge before running native contracts.',
      );

      final store = FlarkV3NativeHostStore.create(
        library: openFlarkV3NativeLibrary(
          overrideLibraryPath: File(libraryPath).absolute.path,
        ),
        documentSession: _documentSession,
      );
      final session = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(
          'live',
          ordinaryReplacementUtf16Limit: 1,
        ),
        documentSession: _documentSession,
        hostStore: store,
      );
      addTearDown(() => _closeAndDrain(session, store));
      _acknowledgeAllSourceWorkerSync(session);

      final first = _offer(session.sourceVersion, identity: 1);
      expect(session.beginOffer(first), isA<FlarkV3HostAccepted>());

      final edit = session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 4,
            endUtf16: 4,
            replacement: '!!',
          ),
        ),
      );
      expect(edit.provisional, isTrue);
      expect(edit.uiAdvance!.activeOfferAbort, isNull);

      _acknowledgeAllSourceWorkerSync(session);
      final certification = FlarkV3SourceCertificationReceipt.scan(
        session.beginSourceCertification(),
        sourceReplica: session.source,
      );
      final promoted = session.applySourceCertification(certification);
      expect(promoted.promoted, isTrue);
      expect(promoted.hostAdoption!.storeSynchronized, isTrue);

      final replacement = session.beginOffer(
        _offer(session.sourceVersion, identity: 2),
      );
      expect(
        replacement,
        isA<FlarkV3HostAccepted>(),
        reason:
            'The newer exact source must atomically retire stale staging '
            'without an orphaned abort-completion poll: '
            '${switch (replacement) {
              FlarkV3HostAccepted<FlarkV3HostUnit>() => 'accepted',
              FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection) => '${rejection.reason.name}: ${rejection.message}',
            }}',
      );
    },
    skip: _nativeLibraryPath == null
        ? 'Native host contract is unsupported on this platform.'
        : false,
  );
}

final _documentSession = FlarkV3DocumentSessionId(31, 32, 33, 34);

FlarkV3HostOfferBegin _offer(
  FlarkV3SourceVersion source, {
  required int identity,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(identity, 1, 2, 3),
  publicationSession: FlarkV3PublicationSessionId(identity, 4, 5, 6),
  targetHostRevision: FlarkV3HostRevisionId(identity),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(identity, source.revision + 1),
  parseGeneration: identity,
  grammarRevision: 1,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: 1,
  targetRecordCount: 1,
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 1,
    maximumEncodedFrameBytes: 1024,
    maximumPacketBytes: 1092,
    maximumFrameBytes: 1024,
    maximumProgramChildren: 1,
  ),
);

void _acknowledgeAllSourceWorkerSync(FlarkDocumentSession session) {
  while (session.hasPendingSourceWorkerSync) {
    final lease = session.beginSourceWorkerSync();
    final acknowledgement = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
        observedReplica: lease.isLast ? _observedFor(session, lease) : null,
      ),
      FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
        observedReplica: _observedFor(session, lease),
      ),
    };
    expect(
      session.acknowledgeSourceWorkerSync(acknowledgement).disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  }
}

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkDocumentSession session,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: switch (target) {
      FlarkV3KnownSourceStamp() => target.utf8Length,
      FlarkV3ProvisionalSourceStamp() =>
        utf8.encode(session.source.toString()).length,
    },
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

Future<void> _closeAndDrain(
  FlarkDocumentSession session,
  FlarkV3NativeHostStore store,
) async {
  session.close();
  for (var poll = 0; poll < 1024; poll += 1) {
    final result = store.poll(
      FlarkV3HostWorkGrant(inspectBytes: 0, copyBytes: 0, transitions: 64),
    );
    if (result case FlarkV3HostAccepted<FlarkV3HostPollOutcome>(
      value: FlarkV3HostClosed(),
    )) {
      return;
    }
    await Future<void>.delayed(Duration.zero);
  }
  throw StateError('Native host did not drain within its bounded close gate.');
}

String? get _nativeLibraryPath => switch (Platform.operatingSystem) {
  'macos' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
  'linux' => 'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
  'windows' => 'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
  _ => null,
};
