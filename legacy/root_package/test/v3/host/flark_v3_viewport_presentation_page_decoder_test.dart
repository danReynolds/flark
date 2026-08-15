import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:test/test.dart';

import '../support/flark_v3_viewport_page_fixture.dart';

void main() {
  group('FLKVP001 schema-8 aggregate decoder', () {
    test('decodes one owned page into ordered opaque typed payloads', () {
      final fixture = buildFlarkV3ViewportPageFixture();
      final page = FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
        ack: fixture.ack,
        encodedPage: fixture.bytes,
      );

      expect(identical(page.encodedPage, fixture.bytes), isTrue);
      expect(page.ack, fixture.ack);
      expect(page.payloadStart, 448);
      expect(page.entryCount, 2);
      expect(page.pageBindingDigest256.word0, 0xa0);
      expect(page.pageBindingDigest256.word7, 0xa7);

      final first = page.entries[0];
      expect(first.orderedChildIndex, 0);
      expect(first.sourceVersion, fixture.ack.baseAck.sourceVersion);
      expect(first.sourceRoot, fixture.ack.baseAck.sourceRoot);
      expect(first.parseGeneration, fixture.ack.baseAck.parseGeneration);
      expect(first.binding.blockOrdinal, FlarkV3ProtocolU64.fromU32(10));
      expect(first.payloadKind, FlarkV3ViewportPresentationPayloadKind.inline);
      expect(
        first.disposition,
        FlarkV3ViewportPresentationPayloadDisposition.authoritative,
      );
      expect(first.recordCount, 1);
      expect(first.payloadOffset, 448);
      expect(first.payloadLength, 20);
      expect(first.unsupportedReason, 0);
      expect(first.isAuthoritative, isTrue);
      expect(() => first.payload[0] = 0, throwsUnsupportedError);

      final second = page.entries[1];
      expect(second.binding.blockOrdinal, FlarkV3ProtocolU64.fromU32(11));
      expect(
        second.payloadKind,
        FlarkV3ViewportPresentationPayloadKind.bulletList,
      );
      expect(second.payloadOffset, first.payloadOffset + first.payloadLength);
      expect(second.payloadLength, 28);
    });

    test('admits exact variable-width unsupported metadata only', () {
      final fixture = buildFlarkV3ViewportPageFixture(
        unsupportedSecondEntry: true,
        unsupportedMetadataBytes: 17,
      );
      final page = FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
        ack: fixture.ack,
        encodedPage: fixture.bytes,
      );
      final unsupported = page.entries[1];

      expect(
        unsupported.payloadKind,
        FlarkV3ViewportPresentationPayloadKind.unsupported,
      );
      expect(
        unsupported.disposition,
        FlarkV3ViewportPresentationPayloadDisposition.unsupported,
      );
      expect(unsupported.recordCount, 0);
      expect(unsupported.payloadLength, 17);
      expect(unsupported.unsupportedReason, 91);
      expect(unsupported.isAuthoritative, isFalse);
    });

    for (final corruption in <_PageCorruption>[
      _PageCorruption('magic', (bytes) => bytes[0] ^= 0xff),
      _PageCorruption('schema', (bytes) => _u32(bytes, 8, 9)),
      _PageCorruption('header width', (bytes) => _u32(bytes, 12, 159)),
      _PageCorruption('directory width', (bytes) => _u32(bytes, 16, 143)),
      _PageCorruption('entry count', (bytes) => _u32(bytes, 20, 1)),
      _PageCorruption('payload boundary', (bytes) => _u32(bytes, 24, 449)),
      _PageCorruption(
        'encoded length',
        (bytes) => _u32(bytes, 28, bytes.length - 1),
      ),
      _PageCorruption('viewport publication', (bytes) => _u32(bytes, 32, 99)),
      _PageCorruption('base publication', (bytes) => _u32(bytes, 48, 99)),
      _PageCorruption('viewport generation', (bytes) => _u32(bytes, 64, 99)),
      _PageCorruption('completion flags', (bytes) => _u32(bytes, 68, 2)),
      _PageCorruption('requested range', (bytes) => _u32(bytes, 72, 1)),
      _PageCorruption('covered range', (bytes) => _u32(bytes, 88, 1)),
      _PageCorruption('frame count', (bytes) => _u32(bytes, 104, 10)),
      _PageCorruption('frame bytes', (bytes) => _u32(bytes, 108, 701)),
      _PageCorruption('root digest', (bytes) => _u32(bytes, 112, 99)),
    ]) {
      test('rejects corrupt header ${corruption.label}', () {
        final fixture = buildFlarkV3ViewportPageFixture();
        final bytes = Uint8List.fromList(fixture.bytes);
        corruption.mutate(bytes);
        _expectDecodeFailure(fixture.ack, bytes);
      });
    }

    test('rejects truncated and trailing page storage', () {
      final fixture = buildFlarkV3ViewportPageFixture();
      _expectDecodeFailure(
        fixture.ack,
        Uint8List.fromList(fixture.bytes.sublist(0, fixture.bytes.length - 1)),
      );
      _expectDecodeFailure(
        fixture.ack,
        Uint8List.fromList([...fixture.bytes, 0]),
      );
    });

    for (final corruption in <_PageCorruption>[
      _PageCorruption('child index', (bytes) => _u32(bytes, 160, 1)),
      _PageCorruption('source revision', (bytes) => _u32(bytes, 164, 12)),
      _PageCorruption('document session', (bytes) => _u32(bytes, 168, 99)),
      _PageCorruption('source root', (bytes) => _u32(bytes, 184, 99)),
      _PageCorruption('content hash', (bytes) => _u32(bytes, 192, 99)),
      _PageCorruption('UTF-8 source length', (bytes) => _u32(bytes, 208, 13)),
      _PageCorruption('UTF-16 source length', (bytes) => _u32(bytes, 212, 13)),
      _PageCorruption('parse generation', (bytes) => _u32(bytes, 216, 99)),
      _PageCorruption('source reserved bytes', (bytes) => bytes[220] = 1),
      _PageCorruption('parser profile', (bytes) => _u32(bytes, 224, 8)),
      _PageCorruption(
        'parser profile high lane',
        (bytes) => _u32(bytes, 228, 1),
      ),
      _PageCorruption('refinement generation', (bytes) => _u32(bytes, 232, 7)),
      _PageCorruption('refinement high lane', (bytes) => _u32(bytes, 236, 1)),
      _PageCorruption(
        'duplicate block ordinal',
        (bytes) => _u32(bytes, 160 + 144 + 80, 10),
      ),
      _PageCorruption(
        'overlapping physical ranges',
        (bytes) => _u32(bytes, 160 + 144 + 88, 5),
      ),
      _PageCorruption(
        'escaping physical range',
        (bytes) => _u32(bytes, 160 + 92, 13),
      ),
      _PageCorruption(
        'empty visible range',
        (bytes) => _u32(bytes, 160 + 96, 6),
      ),
      _PageCorruption(
        'payload reserved bytes',
        (bytes) => bytes[160 + 122] = 1,
      ),
      _PageCorruption(
        'terminal reserved bytes',
        (bytes) => bytes[160 + 140] = 1,
      ),
    ]) {
      test('rejects corrupt directory ${corruption.label}', () {
        final fixture = buildFlarkV3ViewportPageFixture();
        final bytes = Uint8List.fromList(fixture.bytes);
        corruption.mutate(bytes);
        _expectDecodeFailure(fixture.ack, bytes);
      });
    }

    for (final corruption in <_PageCorruption>[
      _PageCorruption('unknown kind', (bytes) => bytes[160 + 120] = 5),
      _PageCorruption('unknown disposition', (bytes) => bytes[160 + 121] = 3),
      _PageCorruption(
        'authoritative reason',
        (bytes) => _u32(bytes, 160 + 136, 1),
      ),
      _PageCorruption(
        'inline record width',
        (bytes) => _u32(bytes, 160 + 132, 19),
      ),
      _PageCorruption('payload gap', (bytes) => _u32(bytes, 160 + 128, 449)),
      _PageCorruption(
        'payload overlap',
        (bytes) => _u32(bytes, 160 + 144 + 128, 467),
      ),
      _PageCorruption(
        'payload escape',
        (bytes) => _u32(bytes, 160 + 144 + 128, bytes.length),
      ),
    ]) {
      test('rejects corrupt payload ${corruption.label}', () {
        final fixture = buildFlarkV3ViewportPageFixture();
        final bytes = Uint8List.fromList(fixture.bytes);
        corruption.mutate(bytes);
        _expectDecodeFailure(fixture.ack, bytes);
      });
    }

    for (final corruption in <_PageCorruption>[
      _PageCorruption(
        'authoritative unsupported disposition',
        (bytes) => bytes[160 + 144 + 121] = 1,
      ),
      _PageCorruption(
        'unsupported record count',
        (bytes) => _u32(bytes, 160 + 144 + 124, 1),
      ),
      _PageCorruption(
        'zero unsupported reason',
        (bytes) => _u32(bytes, 160 + 144 + 136, 0),
      ),
      _PageCorruption(
        'oversized unsupported metadata',
        (bytes) => _u32(bytes, 160 + 144 + 132, 49),
      ),
    ]) {
      test('rejects corrupt unsupported payload ${corruption.label}', () {
        final fixture = buildFlarkV3ViewportPageFixture(
          unsupportedSecondEntry: true,
        );
        final bytes = Uint8List.fromList(fixture.bytes);
        corruption.mutate(bytes);
        _expectDecodeFailure(fixture.ack, bytes);
      });
    }

    test('rejects aggregate fact/source totals from a different ACK', () {
      final fixture = buildFlarkV3ViewportPageFixture();
      final factMismatch = _copyAck(
        fixture.ack,
        inlineSourceBytes: 12,
        factCount: 3,
      );
      final sourceMismatch = _copyAck(
        fixture.ack,
        inlineSourceBytes: 11,
        factCount: 2,
      );

      _expectDecodeFailure(factMismatch, fixture.bytes);
      _expectDecodeFailure(sourceMismatch, fixture.bytes);
    });

    test('rejects an exact-base source identity mismatch', () {
      final fixture = buildFlarkV3ViewportPageFixture();
      final base = fixture.ack.baseAck;
      final differentBase = FlarkV3StructuralAck(
        publicationSession: base.publicationSession,
        hostRevision: base.hostRevision,
        sourceVersion: base.sourceVersion,
        sourceRoot: FlarkV3SourceRootId(99, 100),
        parseGeneration: base.parseGeneration,
        grammarRevision: base.grammarRevision,
        syntaxProfile: base.syntaxProfile,
        authorityMask: base.authorityMask,
        recordCount: base.recordCount,
        sequenceDigest: base.sequenceDigest,
        manifestDigest: base.manifestDigest,
      );
      final ack = FlarkV3ViewportPresentationAck(
        publicationSession: fixture.ack.publicationSession,
        baseAck: differentBase,
        binding: fixture.ack.binding,
        envelope: fixture.ack.envelope,
        actualFrameCount: fixture.ack.actualFrameCount,
        actualEncodedFrameBytes: fixture.ack.actualEncodedFrameBytes,
        aggregateRootStreamDigest: fixture.ack.aggregateRootStreamDigest,
      );

      _expectDecodeFailure(ack, fixture.bytes);
    });
  });
}

final class _PageCorruption {
  const _PageCorruption(this.label, this.mutate);

  final String label;
  final void Function(Uint8List bytes) mutate;
}

void _u32(Uint8List bytes, int offset, int value) {
  ByteData.sublistView(bytes).setUint32(offset, value, Endian.little);
}

void _expectDecodeFailure(FlarkV3ViewportPresentationAck ack, Uint8List bytes) {
  expect(
    () => FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
      ack: ack,
      encodedPage: bytes,
    ),
    throwsA(isA<FlarkV3ViewportPresentationPageDecodeException>()),
  );
}

FlarkV3ViewportPresentationAck _copyAck(
  FlarkV3ViewportPresentationAck ack, {
  required int inlineSourceBytes,
  required int factCount,
}) {
  final envelope = FlarkV3ViewportPresentationEnvelopeMetrics(
    visitedStructuralEntries: ack.envelope.visitedStructuralEntries,
    visitedStoragePages: ack.envelope.visitedStoragePages,
    orderedLeafCount: ack.envelope.orderedLeafCount,
    inlineSourceBytes: inlineSourceBytes,
    factCount: factCount,
    transferredNodeCount: ack.envelope.transferredNodeCount,
    parserTransitions: ack.envelope.parserTransitions,
    aggregateEnvelopeDigest256: ack.envelope.aggregateEnvelopeDigest256,
  );
  return FlarkV3ViewportPresentationAck(
    publicationSession: ack.publicationSession,
    baseAck: ack.baseAck,
    binding: ack.binding,
    envelope: envelope,
    actualFrameCount: ack.actualFrameCount,
    actualEncodedFrameBytes: ack.actualEncodedFrameBytes,
    aggregateRootStreamDigest: ack.aggregateRootStreamDigest,
  );
}
