import 'dart:typed_data';

import 'flark_v3_host_protocol.dart';

export 'flark_v3_host_protocol.dart' show FlarkV3ProtocolU64;

/// Full 256-bit engine commitment retained as eight exact wire lanes.
final class FlarkV3ProtocolDigest256 {
  factory FlarkV3ProtocolDigest256(
    int word0,
    int word1,
    int word2,
    int word3,
    int word4,
    int word5,
    int word6,
    int word7,
  ) {
    for (final word in [
      word0,
      word1,
      word2,
      word3,
      word4,
      word5,
      word6,
      word7,
    ]) {
      _checkU32(word, 'digest256');
    }
    return FlarkV3ProtocolDigest256._(
      word0,
      word1,
      word2,
      word3,
      word4,
      word5,
      word6,
      word7,
    );
  }

  const FlarkV3ProtocolDigest256._(
    this.word0,
    this.word1,
    this.word2,
    this.word3,
    this.word4,
    this.word5,
    this.word6,
    this.word7,
  );

  static const zero = FlarkV3ProtocolDigest256._(0, 0, 0, 0, 0, 0, 0, 0);

  final int word0;
  final int word1;
  final int word2;
  final int word3;
  final int word4;
  final int word5;
  final int word6;
  final int word7;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ProtocolDigest256 &&
      other.word0 == word0 &&
      other.word1 == word1 &&
      other.word2 == word2 &&
      other.word3 == word3 &&
      other.word4 == word4 &&
      other.word5 == word5 &&
      other.word6 == word6 &&
      other.word7 == word7;

  @override
  int get hashCode =>
      Object.hash(word0, word1, word2, word3, word4, word5, word6, word7);
}

/// Sibling publication target for one parser-certified hot-inline root.
///
/// It is deliberately not a structural publication mode.
enum FlarkV3HotInlineSidecarMode { hotInlineSidecar }

/// Exact source and generation fence for one HIO1 root.
final class FlarkV3HotInlineSidecarBinding {
  FlarkV3HotInlineSidecarBinding({
    required this.parserProfile,
    required this.refinementGeneration,
    required this.blockOrdinal,
    required this.physicalStartUtf8,
    required this.physicalEndUtf8,
    required this.visibleStartUtf8,
    required this.visibleEndUtf8,
    required this.physicalStartUtf16,
    required this.physicalEndUtf16,
    required this.visibleStartUtf16,
    required this.visibleEndUtf16,
  }) {
    if (refinementGeneration.isZero) {
      throw RangeError('Refinement generation must be non-zero.');
    }
    for (final (name, value) in [
      ('physicalStartUtf8', physicalStartUtf8),
      ('physicalEndUtf8', physicalEndUtf8),
      ('visibleStartUtf8', visibleStartUtf8),
      ('visibleEndUtf8', visibleEndUtf8),
      ('physicalStartUtf16', physicalStartUtf16),
      ('physicalEndUtf16', physicalEndUtf16),
      ('visibleStartUtf16', visibleStartUtf16),
      ('visibleEndUtf16', visibleEndUtf16),
    ]) {
      _checkU32(value, name);
    }
    if (physicalStartUtf8 >= physicalEndUtf8 ||
        visibleStartUtf8 >= visibleEndUtf8 ||
        visibleStartUtf8 < physicalStartUtf8 ||
        visibleEndUtf8 > physicalEndUtf8 ||
        physicalStartUtf16 >= physicalEndUtf16 ||
        visibleStartUtf16 >= visibleEndUtf16 ||
        visibleStartUtf16 < physicalStartUtf16 ||
        visibleEndUtf16 > physicalEndUtf16) {
      throw RangeError('Hot-inline physical and visible ranges are invalid.');
    }
  }

  final FlarkV3SyntaxProfileId parserProfile;
  final FlarkV3ProtocolU64 refinementGeneration;
  final FlarkV3ProtocolU64 blockOrdinal;
  final int physicalStartUtf8;
  final int physicalEndUtf8;
  final int visibleStartUtf8;
  final int visibleEndUtf8;
  final int physicalStartUtf16;
  final int physicalEndUtf16;
  final int visibleStartUtf16;
  final int visibleEndUtf16;

  void requireBase(FlarkV3StructuralAck baseAck) {
    if (parserProfile != baseAck.syntaxProfile ||
        physicalEndUtf8 > baseAck.sourceVersion.metric.bytes ||
        physicalEndUtf16 > baseAck.sourceVersion.metric.utf16) {
      throw ArgumentError(
        'Hot-inline binding exceeds or disagrees with its structural base.',
      );
    }
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HotInlineSidecarBinding &&
      other.parserProfile == parserProfile &&
      other.refinementGeneration == refinementGeneration &&
      other.blockOrdinal == blockOrdinal &&
      other.physicalStartUtf8 == physicalStartUtf8 &&
      other.physicalEndUtf8 == physicalEndUtf8 &&
      other.visibleStartUtf8 == visibleStartUtf8 &&
      other.visibleEndUtf8 == visibleEndUtf8 &&
      other.physicalStartUtf16 == physicalStartUtf16 &&
      other.physicalEndUtf16 == physicalEndUtf16 &&
      other.visibleStartUtf16 == visibleStartUtf16 &&
      other.visibleEndUtf16 == visibleEndUtf16;

  @override
  int get hashCode => Object.hash(
    parserProfile,
    refinementGeneration,
    blockOrdinal,
    physicalStartUtf8,
    physicalEndUtf8,
    visibleStartUtf8,
    visibleEndUtf8,
    physicalStartUtf16,
    physicalEndUtf16,
    visibleStartUtf16,
    visibleEndUtf16,
  );
}

sealed class FlarkV3HotInlineSidecarDisposition {
  const FlarkV3HotInlineSidecarDisposition();
}

final class FlarkV3HotInlineSidecarAuthoritative
    extends FlarkV3HotInlineSidecarDisposition {
  FlarkV3HotInlineSidecarAuthoritative({
    required this.logicalPageCount,
    required this.factCount,
    required this.storagePageCount,
    required this.linkValueEntryCount,
    required this.linkValueStoragePageCount,
    required this.linkValueEncodedBytes,
    required this.orderedCommitment256,
  }) {
    _checkU32(linkValueEntryCount, 'linkValueEntryCount');
    _checkU32(linkValueEncodedBytes, 'linkValueEncodedBytes');
    final empty = logicalPageCount.isZero;
    if (empty != (factCount.isZero && storagePageCount.isZero) ||
        (!empty &&
            (factCount.compareTo(logicalPageCount) < 0 ||
                storagePageCount.isZero)) ||
        (factCount.fitsU32 && linkValueEntryCount > factCount.lowWord) ||
        (linkValueEntryCount == 0) !=
            (linkValueStoragePageCount.isZero && linkValueEncodedBytes == 0) ||
        (linkValueEntryCount > 0 &&
            (linkValueStoragePageCount.isZero ||
                linkValueEncodedBytes < 16 + 32 * linkValueEntryCount ||
                linkValueEncodedBytes > 64 * 1024))) {
      throw ArgumentError('Authoritative sidecar counts are inconsistent.');
    }
  }

  final FlarkV3ProtocolU64 logicalPageCount;
  final FlarkV3ProtocolU64 factCount;
  final FlarkV3ProtocolU64 storagePageCount;
  final int linkValueEntryCount;
  final FlarkV3ProtocolU64 linkValueStoragePageCount;
  final int linkValueEncodedBytes;
  final FlarkV3ProtocolDigest256 orderedCommitment256;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HotInlineSidecarAuthoritative &&
      other.logicalPageCount == logicalPageCount &&
      other.factCount == factCount &&
      other.storagePageCount == storagePageCount &&
      other.linkValueEntryCount == linkValueEntryCount &&
      other.linkValueStoragePageCount == linkValueStoragePageCount &&
      other.linkValueEncodedBytes == linkValueEncodedBytes &&
      other.orderedCommitment256 == orderedCommitment256;

  @override
  int get hashCode => Object.hash(
    logicalPageCount,
    factCount,
    storagePageCount,
    linkValueEntryCount,
    linkValueStoragePageCount,
    linkValueEncodedBytes,
    orderedCommitment256,
  );
}

final class FlarkV3HotInlineSidecarUnsupported
    extends FlarkV3HotInlineSidecarDisposition {
  FlarkV3HotInlineSidecarUnsupported({
    required this.reason,
    required this.metadataCommitment256,
  }) {
    _checkU32(reason, 'reason', positive: true);
  }

  final int reason;
  final FlarkV3ProtocolDigest256 metadataCommitment256;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HotInlineSidecarUnsupported &&
      other.reason == reason &&
      other.metadataCommitment256 == metadataCommitment256;

  @override
  int get hashCode => Object.hash(reason, metadataCommitment256);
}

/// HIO1 envelope facts authenticated before its persistent root is transferred.
final class FlarkV3HotInlineSidecarEnvelopeMetrics {
  FlarkV3HotInlineSidecarEnvelopeMetrics({
    this.hio1EncodedBytes = hio1EnvelopeBytes,
    required this.ipr2DescriptorBytes,
    required this.transferredNodeCount,
    required this.hio1EnvelopeDigest256,
    required this.disposition,
  }) {
    _checkU32(hio1EncodedBytes, 'hio1EncodedBytes');
    _checkU32(ipr2DescriptorBytes, 'ipr2DescriptorBytes');
    _checkU32(transferredNodeCount, 'transferredNodeCount');
    if (hio1EncodedBytes != hio1EnvelopeBytes) {
      throw ArgumentError('HIO1 envelope width must be exactly 256 bytes.');
    }
    switch (disposition) {
      case FlarkV3HotInlineSidecarAuthoritative():
        if (ipr2DescriptorBytes != ipr2FixedDescriptorBytes &&
            ipr2DescriptorBytes != inlineBundleDescriptorBytes &&
            ipr2DescriptorBytes != projectedInlineBundleDescriptorBytes &&
            ipr2DescriptorBytes != blockQuoteDescriptorBytes) {
          throw ArgumentError(
            'An authoritative sidecar requires one supported fixed '
            'projection descriptor.',
          );
        }
      case FlarkV3HotInlineSidecarUnsupported():
        if (ipr2DescriptorBytes != 0 || transferredNodeCount != 1) {
          throw ArgumentError(
            'An unsupported sidecar transfers one metadata node and no IPR2.',
          );
        }
    }
  }

  static const int hio1EnvelopeBytes = 256;
  static const int ipr2FixedDescriptorBytes = 160;
  static const int inlineBundleDescriptorBytes = 280;
  static const int projectedInlineBundleDescriptorBytes = 328;
  static const int blockQuoteDescriptorBytes = 168;

  final int hio1EncodedBytes;
  final int ipr2DescriptorBytes;
  final int transferredNodeCount;
  final FlarkV3ProtocolDigest256 hio1EnvelopeDigest256;
  final FlarkV3HotInlineSidecarDisposition disposition;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HotInlineSidecarEnvelopeMetrics &&
      other.hio1EncodedBytes == hio1EncodedBytes &&
      other.ipr2DescriptorBytes == ipr2DescriptorBytes &&
      other.transferredNodeCount == transferredNodeCount &&
      other.hio1EnvelopeDigest256 == hio1EnvelopeDigest256 &&
      other.disposition == disposition;

  @override
  int get hashCode => Object.hash(
    hio1EncodedBytes,
    ipr2DescriptorBytes,
    transferredNodeCount,
    hio1EnvelopeDigest256,
    disposition,
  );
}

/// Sidecar offer tied to one complete, exact structural base ACK.
final class FlarkV3HotInlineSidecarOfferBegin {
  FlarkV3HotInlineSidecarOfferBegin({
    this.schema = supportedSchema,
    this.mode = FlarkV3HotInlineSidecarMode.hotInlineSidecar,
    required this.offerId,
    required this.publicationSession,
    required this.baseAck,
    required this.binding,
    required this.envelope,
    required this.limits,
  }) {
    if (schema != supportedSchema) {
      throw ArgumentError.value(
        schema,
        'schema',
        'Unsupported sidecar schema.',
      );
    }
    _requireNonZeroId(offerId, 'offerId');
    _requireNonZeroId(publicationSession, 'publicationSession');
    if (publicationSession == baseAck.publicationSession) {
      throw ArgumentError('A sidecar publication identity must be fresh.');
    }
    binding.requireBase(baseAck);
    if (envelope.transferredNodeCount + 2 > limits.maximumFrameCount) {
      throw ArgumentError(
        'Sidecar closure exceeds its advertised frame limit.',
      );
    }
  }

  static const int supportedSchema = 3;

  final int schema;
  final FlarkV3HotInlineSidecarMode mode;
  final FlarkV3OfferId offerId;
  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3StructuralAck baseAck;
  final FlarkV3HotInlineSidecarBinding binding;
  final FlarkV3HotInlineSidecarEnvelopeMetrics envelope;
  final FlarkV3HostOfferLimits limits;

  bool hasExactBase(FlarkV3StructuralAck installed) => baseAck == installed;
}

/// Sidecar-specific transport totals. Its final digest is a root stream, not a
/// canonical structural stream.
final class FlarkV3HotInlineSidecarCommitRequest {
  FlarkV3HotInlineSidecarCommitRequest({
    required this.offerId,
    required this.actualFrameCount,
    required this.actualEncodedFrameBytes,
    required this.rollingTransportDigest,
    required this.rootStreamDigest,
  }) {
    _requireNonZeroId(offerId, 'offerId');
    _checkU32(actualFrameCount, 'actualFrameCount');
    _checkU32(actualEncodedFrameBytes, 'actualEncodedFrameBytes');
    if (actualFrameCount < 2) {
      throw RangeError.range(
        actualFrameCount,
        2,
        flarkV3TransportV1Maximum,
        'actualFrameCount',
      );
    }
  }

  final FlarkV3OfferId offerId;
  final int actualFrameCount;
  final int actualEncodedFrameBytes;
  final FlarkV3ProtocolDigest128 rollingTransportDigest;
  final FlarkV3ProtocolDigest128 rootStreamDigest;
}

enum FlarkV3InlineSidecarAckDisposition { authoritative, unsupported }

/// Receipt for one installed inline generation. This never advances or
/// replaces the embedded structural ACK.
final class FlarkV3InlineSidecarAck {
  FlarkV3InlineSidecarAck({
    required this.publicationSession,
    required this.baseAck,
    required this.refinementGeneration,
    required this.blockOrdinal,
    required this.transferredNodeCount,
    required this.disposition,
    required this.hio1EnvelopeDigest256,
    required this.rootStreamDigest,
  }) {
    _requireNonZeroId(publicationSession, 'publicationSession');
    if (publicationSession == baseAck.publicationSession) {
      throw ArgumentError('A sidecar ACK cannot alias its structural base.');
    }
    if (refinementGeneration.isZero) {
      throw RangeError('Refinement generation must be non-zero.');
    }
    _checkU32(transferredNodeCount, 'transferredNodeCount');
    if (disposition == FlarkV3InlineSidecarAckDisposition.unsupported &&
        transferredNodeCount != 1) {
      throw ArgumentError(
        'An unsupported sidecar ACK must receipt one metadata node.',
      );
    }
  }

  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3StructuralAck baseAck;
  final FlarkV3ProtocolU64 refinementGeneration;
  final FlarkV3ProtocolU64 blockOrdinal;
  final int transferredNodeCount;
  final FlarkV3InlineSidecarAckDisposition disposition;
  final FlarkV3ProtocolDigest256 hio1EnvelopeDigest256;
  final FlarkV3ProtocolDigest128 rootStreamDigest;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3InlineSidecarAck &&
      other.publicationSession == publicationSession &&
      other.baseAck == baseAck &&
      other.refinementGeneration == refinementGeneration &&
      other.blockOrdinal == blockOrdinal &&
      other.transferredNodeCount == transferredNodeCount &&
      other.disposition == disposition &&
      other.hio1EnvelopeDigest256 == hio1EnvelopeDigest256 &&
      other.rootStreamDigest == rootStreamDigest;

  @override
  int get hashCode => Object.hash(
    publicationSession,
    baseAck,
    refinementGeneration,
    blockOrdinal,
    transferredNodeCount,
    disposition,
    hio1EnvelopeDigest256,
    rootStreamDigest,
  );
}

sealed class FlarkV3InlineSidecarHostPollOutcome {
  const FlarkV3InlineSidecarHostPollOutcome();
}

final class FlarkV3InlineSidecarHostPollPending
    extends FlarkV3InlineSidecarHostPollOutcome {
  const FlarkV3InlineSidecarHostPollPending();
}

final class FlarkV3InlineSidecarHostPacketCredit
    extends FlarkV3InlineSidecarHostPollOutcome {
  FlarkV3InlineSidecarHostPacketCredit({
    required this.offerId,
    required this.nextFrameOrdinal,
  }) {
    _requireNonZeroId(offerId, 'offerId');
    _checkU32(nextFrameOrdinal, 'nextFrameOrdinal', positive: true);
  }

  final FlarkV3OfferId offerId;
  final int nextFrameOrdinal;
}

final class FlarkV3InlineSidecarHostCommitted
    extends FlarkV3InlineSidecarHostPollOutcome {
  const FlarkV3InlineSidecarHostCommitted(this.ack);

  final FlarkV3InlineSidecarAck ack;
}

final class FlarkV3InlineSidecarHostAbortComplete
    extends FlarkV3InlineSidecarHostPollOutcome {
  FlarkV3InlineSidecarHostAbortComplete(this.offerId) {
    _requireNonZeroId(offerId, 'offerId');
  }

  final FlarkV3OfferId offerId;
}

final class FlarkV3InlineSidecarHostClosed
    extends FlarkV3InlineSidecarHostPollOutcome {
  const FlarkV3InlineSidecarHostClosed();
}

/// Exact installed binding and copy ceiling for a narrow sidecar query.
///
/// The ordinary structural point query is the product-facing join. This
/// sibling query remains useful at the native ABI seam for independently
/// validating an installed sidecar and for hosts that have not yet joined it
/// into their structural viewport.
final class FlarkV3InlineSidecarQuery {
  FlarkV3InlineSidecarQuery({
    required this.binding,
    required this.maximumEncodedBytes,
  }) {
    _checkU32(maximumEncodedBytes, 'maximumEncodedBytes', positive: true);
    if (maximumEncodedBytes > maximumQueryBytes) {
      throw RangeError.range(
        maximumEncodedBytes,
        1,
        maximumQueryBytes,
        'maximumEncodedBytes',
      );
    }
  }

  /// The fact and FLKIV lanes retain independent 64 KiB ceilings; this
  /// combined caller-owned buffer fits both without weakening either bound.
  static const int maximumQueryBytes = 128 * 1024;

  final FlarkV3HotInlineSidecarBinding binding;
  final int maximumEncodedBytes;
}

sealed class FlarkV3InlineSidecarQueryOutcome {
  const FlarkV3InlineSidecarQueryOutcome();
}

/// Stable record format carried by an authoritative direct-sidecar query.
enum FlarkV3InlineSidecarPayloadKind {
  inline(1, 20),
  indentedCode(2, 20),
  blockQuote(3, 20),
  bulletList(4, 28),
  blockQuoteInline(5, 20),
  orderedListItem(6, 48);

  const FlarkV3InlineSidecarPayloadKind(this.wireValue, this.recordBytes);

  final int wireValue;
  final int recordBytes;

  static FlarkV3InlineSidecarPayloadKind? tryFromWireValue(int wireValue) {
    for (final kind in values) {
      if (kind.wireValue == wireValue) return kind;
    }
    return null;
  }
}

final class FlarkV3InlineSidecarQueryAuthoritative
    extends FlarkV3InlineSidecarQueryOutcome {
  FlarkV3InlineSidecarQueryAuthoritative({
    required this.payloadKind,
    required this.factCount,
    required this.valueEntryCount,
    required this.treeNodesVisited,
    required this.encodedFacts,
    required this.encodedValues,
  }) {
    _checkU32(factCount, 'factCount');
    _checkU32(valueEntryCount, 'valueEntryCount');
    _checkU32(treeNodesVisited, 'treeNodesVisited');
    if (encodedFacts.length != factCount * payloadKind.recordBytes) {
      throw ArgumentError(
        'Authoritative sidecar bytes do not match the fact count.',
      );
    }
    if (payloadKind != FlarkV3InlineSidecarPayloadKind.inline &&
        (valueEntryCount != 0 || encodedValues.isNotEmpty)) {
      throw ArgumentError('Only inline sidecars may carry a value lane.');
    }
    if (valueEntryCount == 0) {
      if (encodedValues.isNotEmpty) {
        throw ArgumentError(
          'An absent sidecar value lane must have no encoded bytes.',
        );
      }
    } else if (encodedValues.length < inlineValueHeaderBytes ||
        encodedValues.length > maximumInlineValueBytes ||
        !_hasInlineValueMagic(encodedValues) ||
        ByteData.sublistView(encodedValues).getUint32(8, Endian.little) !=
            inlineValueSchema ||
        ByteData.sublistView(encodedValues).getUint32(12, Endian.little) !=
            valueEntryCount) {
      throw ArgumentError(
        'Authoritative sidecar value bytes do not match their entry count.',
      );
    }
  }

  static const int inlineFactRecordBytes = 20;
  static const int inlineValueHeaderBytes = 16;
  static const int inlineValueSchema = 1;
  static const int maximumInlineValueBytes = 64 * 1024;

  final FlarkV3InlineSidecarPayloadKind payloadKind;
  final int factCount;
  final int valueEntryCount;
  final int treeNodesVisited;
  final Uint8List encodedFacts;
  final Uint8List encodedValues;

  int get encodedByteLength =>
      encodedFacts.lengthInBytes + encodedValues.lengthInBytes;
}

final class FlarkV3InlineSidecarQueryUnsupported
    extends FlarkV3InlineSidecarQueryOutcome {
  FlarkV3InlineSidecarQueryUnsupported({
    required this.reason,
    required this.metadata,
  }) {
    _checkU32(reason, 'reason', positive: true);
  }

  final int reason;
  final Uint8List metadata;
}

final class FlarkV3InlineSidecarQueryUnavailable
    extends FlarkV3InlineSidecarQueryOutcome {
  const FlarkV3InlineSidecarQueryUnavailable();
}

bool _hasInlineValueMagic(Uint8List bytes) {
  const magic = <int>[70, 76, 75, 73, 86, 48, 48, 49];
  if (bytes.lengthInBytes < magic.length) return false;
  for (var index = 0; index < magic.length; index += 1) {
    if (bytes[index] != magic[index]) return false;
  }
  return true;
}

void _checkU32(int value, String name, {bool positive = false}) {
  final minimum = positive ? 1 : 0;
  if (value < minimum || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, minimum, flarkV3TransportV1Maximum, name);
  }
}

void _requireNonZeroId(FlarkV3ProtocolId128 id, String name) {
  if (id.word0 == 0 && id.word1 == 0 && id.word2 == 0 && id.word3 == 0) {
    throw ArgumentError.value(id, name, 'Protocol identity must be non-zero.');
  }
}
