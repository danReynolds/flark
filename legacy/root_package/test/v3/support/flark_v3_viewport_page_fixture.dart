import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/source/source.dart';

final class FlarkV3ViewportPageFixture {
  const FlarkV3ViewportPageFixture({required this.ack, required this.bytes});

  final FlarkV3ViewportPresentationAck ack;
  final Uint8List bytes;
}

FlarkV3ViewportPageFixture buildFlarkV3ViewportPageFixture({
  bool unsupportedSecondEntry = false,
  int unsupportedMetadataBytes = 17,
}) {
  if (unsupportedMetadataBytes < 0 || unsupportedMetadataBytes > 48) {
    throw RangeError.range(unsupportedMetadataBytes, 0, 48);
  }
  final documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
  final source = FlarkV3SourceVersion(
    documentSession: documentSession,
    revision: 11,
    metric: FlarkV3SourceMetric(bytes: 12, utf16: 12),
    contentHash: const FlarkV3ContentHash128(31, 32, 33, 34),
  );
  final baseAck = FlarkV3StructuralAck(
    publicationSession: FlarkV3PublicationSessionId(5, 6, 7, 8),
    hostRevision: FlarkV3HostRevisionId(3),
    sourceVersion: source,
    sourceRoot: FlarkV3SourceRootId(9, 10),
    parseGeneration: 4,
    grammarRevision: 5,
    syntaxProfile: FlarkV3SyntaxProfileId(7),
    authorityMask: FlarkV3StructuralAuthorityMask.complete,
    recordCount: 8,
    sequenceDigest: _digest128(40),
    manifestDigest: _digest128(50),
  );
  final binding = FlarkV3ViewportPresentationBinding(
    viewportGeneration: 6,
    requestedRange: FlarkV3ViewportPresentationMetricRange(
      startUtf8: 0,
      startUtf16: 0,
      endUtf8: 12,
      endUtf16: 12,
    ),
    coveredRange: FlarkV3ViewportPresentationMetricRange(
      startUtf8: 0,
      startUtf16: 0,
      endUtf8: 12,
      endUtf16: 12,
    ),
    start: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(10),
      utf8Offset: 0,
      utf16Offset: 0,
    ),
    next: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(12),
      utf8Offset: 12,
      utf16Offset: 12,
    ),
    complete: true,
  );
  final envelope = FlarkV3ViewportPresentationEnvelopeMetrics(
    visitedStructuralEntries: 2,
    visitedStoragePages: 2,
    orderedLeafCount: 2,
    inlineSourceBytes: 12,
    factCount: unsupportedSecondEntry ? 1 : 2,
    transferredNodeCount: 2,
    parserTransitions: 19,
    aggregateEnvelopeDigest256: _digest256(60),
  );
  final ack = FlarkV3ViewportPresentationAck(
    publicationSession: FlarkV3PublicationSessionId(21, 22, 23, 24),
    baseAck: baseAck,
    binding: binding,
    envelope: envelope,
    actualFrameCount: 9,
    actualEncodedFrameBytes: 700,
    aggregateRootStreamDigest: _digest128(70),
  );

  const headerBytes = FlarkV3ViewportPresentationAggregatePage.headerBytes;
  const entryBytes =
      FlarkV3ViewportPresentationAggregatePage.directoryEntryBytes;
  const entryCount = 2;
  const payloadStart = headerBytes + entryBytes * entryCount;
  const firstPayloadBytes = 20;
  final secondPayloadBytes = unsupportedSecondEntry
      ? unsupportedMetadataBytes
      : 28;
  final totalBytes = payloadStart + firstPayloadBytes + secondPayloadBytes;
  final bytes = Uint8List(totalBytes);
  final data = ByteData.sublistView(bytes);

  bytes.setRange(
    0,
    FlarkV3ViewportPresentationAggregatePage.magicBytes.length,
    FlarkV3ViewportPresentationAggregatePage.magicBytes,
  );
  data
    ..setUint32(
      8,
      FlarkV3ViewportPresentationAggregatePage.schema,
      Endian.little,
    )
    ..setUint32(12, headerBytes, Endian.little)
    ..setUint32(16, entryBytes, Endian.little)
    ..setUint32(20, entryCount, Endian.little)
    ..setUint32(24, payloadStart, Endian.little)
    ..setUint32(28, totalBytes, Endian.little);
  _writeId128(data, 32, ack.publicationSession);
  _writeId128(data, 48, ack.baseAck.publicationSession);
  data
    ..setUint32(64, binding.viewportGeneration, Endian.little)
    ..setUint32(68, 1, Endian.little);
  _writeRange(data, 72, binding.requestedRange);
  _writeRange(data, 88, binding.coveredRange);
  data
    ..setUint32(104, ack.actualFrameCount, Endian.little)
    ..setUint32(108, ack.actualEncodedFrameBytes, Endian.little);
  _writeId128(data, 112, ack.aggregateRootStreamDigest);
  for (var index = 0; index < 8; index += 1) {
    data.setUint32(128 + index * 4, 0xa0 + index, Endian.little);
  }

  _writeEntry(
    data,
    offset: headerBytes,
    index: 0,
    ack: ack,
    blockOrdinal: 10,
    physicalStart: 0,
    physicalEnd: 6,
    payloadKind: 1,
    disposition: 1,
    recordCount: 1,
    payloadOffset: payloadStart,
    payloadLength: firstPayloadBytes,
    unsupportedReason: 0,
  );
  _writeEntry(
    data,
    offset: headerBytes + entryBytes,
    index: 1,
    ack: ack,
    blockOrdinal: 11,
    physicalStart: 6,
    physicalEnd: 12,
    payloadKind: unsupportedSecondEntry ? 0xff : 4,
    disposition: unsupportedSecondEntry ? 2 : 1,
    recordCount: unsupportedSecondEntry ? 0 : 1,
    payloadOffset: payloadStart + firstPayloadBytes,
    payloadLength: secondPayloadBytes,
    unsupportedReason: unsupportedSecondEntry ? 91 : 0,
  );
  for (var index = payloadStart; index < totalBytes; index += 1) {
    bytes[index] = index & 0xff;
  }
  return FlarkV3ViewportPageFixture(ack: ack, bytes: bytes);
}

void _writeEntry(
  ByteData data, {
  required int offset,
  required int index,
  required FlarkV3ViewportPresentationAck ack,
  required int blockOrdinal,
  required int physicalStart,
  required int physicalEnd,
  required int payloadKind,
  required int disposition,
  required int recordCount,
  required int payloadOffset,
  required int payloadLength,
  required int unsupportedReason,
}) {
  final source = ack.baseAck.sourceVersion;
  data
    ..setUint32(offset, index, Endian.little)
    ..setUint32(offset + 4, source.revision, Endian.little);
  _writeId128(data, offset + 8, source.documentSession);
  data
    ..setUint32(offset + 24, ack.baseAck.sourceRoot.highWord, Endian.little)
    ..setUint32(offset + 28, ack.baseAck.sourceRoot.lowWord, Endian.little)
    ..setUint32(offset + 32, source.contentHash.word0, Endian.little)
    ..setUint32(offset + 36, source.contentHash.word1, Endian.little)
    ..setUint32(offset + 40, source.contentHash.word2, Endian.little)
    ..setUint32(offset + 44, source.contentHash.word3, Endian.little)
    ..setUint32(offset + 48, source.metric.bytes, Endian.little)
    ..setUint32(offset + 52, source.metric.utf16, Endian.little)
    ..setUint32(offset + 56, ack.baseAck.parseGeneration, Endian.little)
    ..setUint32(offset + 64, ack.baseAck.syntaxProfile.value, Endian.little)
    ..setUint32(offset + 68, 0, Endian.little)
    ..setUint32(offset + 72, ack.binding.viewportGeneration, Endian.little)
    ..setUint32(offset + 76, 0, Endian.little)
    ..setUint32(offset + 80, blockOrdinal, Endian.little)
    ..setUint32(offset + 84, 0, Endian.little)
    ..setUint32(offset + 88, physicalStart, Endian.little)
    ..setUint32(offset + 92, physicalEnd, Endian.little)
    ..setUint32(offset + 96, physicalStart, Endian.little)
    ..setUint32(offset + 100, physicalEnd, Endian.little)
    ..setUint32(offset + 104, physicalStart, Endian.little)
    ..setUint32(offset + 108, physicalEnd, Endian.little)
    ..setUint32(offset + 112, physicalStart, Endian.little)
    ..setUint32(offset + 116, physicalEnd, Endian.little)
    ..setUint8(offset + 120, payloadKind)
    ..setUint8(offset + 121, disposition)
    ..setUint32(offset + 124, recordCount, Endian.little)
    ..setUint32(offset + 128, payloadOffset, Endian.little)
    ..setUint32(offset + 132, payloadLength, Endian.little)
    ..setUint32(offset + 136, unsupportedReason, Endian.little);
}

void _writeRange(
  ByteData data,
  int offset,
  FlarkV3ViewportPresentationMetricRange range,
) {
  data
    ..setUint32(offset, range.startUtf8, Endian.little)
    ..setUint32(offset + 4, range.startUtf16, Endian.little)
    ..setUint32(offset + 8, range.endUtf8, Endian.little)
    ..setUint32(offset + 12, range.endUtf16, Endian.little);
}

void _writeId128(ByteData data, int offset, FlarkV3ProtocolId128 value) {
  data
    ..setUint32(offset, value.word0, Endian.little)
    ..setUint32(offset + 4, value.word1, Endian.little)
    ..setUint32(offset + 8, value.word2, Endian.little)
    ..setUint32(offset + 12, value.word3, Endian.little);
}

FlarkV3ProtocolDigest128 _digest128(int first) =>
    FlarkV3ProtocolDigest128(first, first + 1, first + 2, first + 3);

FlarkV3ProtocolDigest256 _digest256(int first) => FlarkV3ProtocolDigest256(
  first,
  first + 1,
  first + 2,
  first + 3,
  first + 4,
  first + 5,
  first + 6,
  first + 7,
);
