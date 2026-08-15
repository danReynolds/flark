import 'dart:typed_data';

import '../source/source.dart';

/// Internal v3 host protocol.
///
/// These types are deliberately not exported by Flark's public package
/// barrel. They are the platform-neutral values shared by the Dart controller,
/// the native Rust host-store adapter, and the main-context WebAssembly
/// host-store adapter.

sealed class FlarkV3ProtocolId128 {
  const FlarkV3ProtocolId128._(this.word0, this.word1, this.word2, this.word3);

  final int word0;
  final int word1;
  final int word2;
  final int word3;

  @override
  bool operator ==(Object other) =>
      other.runtimeType == runtimeType &&
      other is FlarkV3ProtocolId128 &&
      other.word0 == word0 &&
      other.word1 == word1 &&
      other.word2 == word2 &&
      other.word3 == word3;

  @override
  int get hashCode => Object.hash(runtimeType, word0, word1, word2, word3);
}

/// Largest scalar carried by the version-1 Dart/Rust transport ABI.
///
/// V1 deliberately uses exact unsigned 32-bit lanes on native and JavaScript
/// runtimes. Supporting documents or counters beyond this ceiling requires a
/// versioned ABI rather than an implicit lossy widening.
const int flarkV3TransportV1Maximum = 0xFFFFFFFF;

void _checkTransportU32(int value, String name, {bool positive = false}) {
  final minimum = positive ? 1 : 0;
  if (value < minimum || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, minimum, flarkV3TransportV1Maximum, name);
  }
}

/// One exact unsigned 64-bit protocol scalar represented as two 32-bit lanes.
///
/// Dart's JavaScript runtime cannot represent every `u64` as an [int]. Keeping
/// the little-endian low and high words separate makes host ordinals lossless
/// on native and web transports.
final class FlarkV3ProtocolU64 implements Comparable<FlarkV3ProtocolU64> {
  factory FlarkV3ProtocolU64({required int lowWord, required int highWord}) {
    _checkTransportU32(lowWord, 'lowWord');
    _checkTransportU32(highWord, 'highWord');
    return FlarkV3ProtocolU64._(lowWord: lowWord, highWord: highWord);
  }

  factory FlarkV3ProtocolU64.fromU32(int value) {
    _checkTransportU32(value, 'value');
    return FlarkV3ProtocolU64._(lowWord: value, highWord: 0);
  }

  const FlarkV3ProtocolU64._({required this.lowWord, required this.highWord});

  static const zero = FlarkV3ProtocolU64._(lowWord: 0, highWord: 0);

  final int lowWord;
  final int highWord;

  bool get isZero => lowWord == 0 && highWord == 0;
  bool get fitsU32 => highWord == 0;

  @override
  int compareTo(FlarkV3ProtocolU64 other) {
    final highComparison = highWord.compareTo(other.highWord);
    return highComparison == 0
        ? lowWord.compareTo(other.lowWord)
        : highComparison;
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ProtocolU64 &&
      other.lowWord == lowWord &&
      other.highWord == highWord;

  @override
  int get hashCode => Object.hash(lowWord, highWord);
}

final class FlarkV3DocumentSessionId extends FlarkV3ProtocolId128 {
  factory FlarkV3DocumentSessionId(int word0, int word1, int word2, int word3) {
    _checkWords(word0, word1, word2, word3, 'documentSession');
    return FlarkV3DocumentSessionId._(word0, word1, word2, word3);
  }

  const FlarkV3DocumentSessionId._(
    super.word0,
    super.word1,
    super.word2,
    super.word3,
  ) : super._();
}

final class FlarkV3PublicationSessionId extends FlarkV3ProtocolId128 {
  factory FlarkV3PublicationSessionId(
    int word0,
    int word1,
    int word2,
    int word3,
  ) {
    _checkWords(word0, word1, word2, word3, 'publicationSession');
    return FlarkV3PublicationSessionId._(word0, word1, word2, word3);
  }

  const FlarkV3PublicationSessionId._(
    super.word0,
    super.word1,
    super.word2,
    super.word3,
  ) : super._();
}

final class FlarkV3OfferId extends FlarkV3ProtocolId128 {
  factory FlarkV3OfferId(int word0, int word1, int word2, int word3) {
    _checkWords(word0, word1, word2, word3, 'offerId');
    return FlarkV3OfferId._(word0, word1, word2, word3);
  }

  const FlarkV3OfferId._(super.word0, super.word1, super.word2, super.word3)
    : super._();
}

final class FlarkV3ProtocolDigest128 extends FlarkV3ProtocolId128 {
  factory FlarkV3ProtocolDigest128(int word0, int word1, int word2, int word3) {
    _checkWords(word0, word1, word2, word3, 'protocolDigest');
    return FlarkV3ProtocolDigest128._(word0, word1, word2, word3);
  }

  const FlarkV3ProtocolDigest128._(
    super.word0,
    super.word1,
    super.word2,
    super.word3,
  ) : super._();

  static const zero = FlarkV3ProtocolDigest128._(0, 0, 0, 0);
}

/// Opaque identity of the immutable parser-worker source root.
///
/// This is intentionally distinct from [FlarkV3SourceVersion.contentHash]. A
/// content hash proves source bytes; this identity proves which persistent
/// worker root the candidate read. Two 32-bit words keep the value exact on
/// Dart's JavaScript runtimes.
final class FlarkV3SourceRootId {
  factory FlarkV3SourceRootId(int highWord, int lowWord) {
    _checkWords(highWord, lowWord, 0, 0, 'sourceRoot');
    if (highWord == 0 && lowWord == 0) {
      throw RangeError('Source-root identity must be non-zero.');
    }
    return FlarkV3SourceRootId._(highWord, lowWord);
  }

  const FlarkV3SourceRootId._(this.highWord, this.lowWord);

  final int highWord;
  final int lowWord;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceRootId &&
      other.highWord == highWord &&
      other.lowWord == lowWord;

  @override
  int get hashCode => Object.hash(highWord, lowWord);
}

/// Pinned syntax-profile identity selected by the session owner.
///
/// The protocol keeps this opaque so adding a profile does not require a Dart
/// grammar implementation. The session driver still admits only the exact
/// configured value, so an unconfigured profile fails closed.
final class FlarkV3SyntaxProfileId {
  factory FlarkV3SyntaxProfileId(int value) {
    if (value <= 0 || value > 0xFFFFFFFF) {
      throw RangeError.range(value, 1, 0xFFFFFFFF, 'syntaxProfile');
    }
    return FlarkV3SyntaxProfileId._(value);
  }

  const FlarkV3SyntaxProfileId._(this.value);

  final int value;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SyntaxProfileId && other.value == value;

  @override
  int get hashCode => value.hashCode;
}

/// Roots whose facts are authoritative in one atomic structural publication.
///
/// Unknown bits are rejected at construction. A session may deliberately
/// configure a narrower supported vertical, but a worker cannot add or omit
/// authority relative to that configured mask during publication.
final class FlarkV3StructuralAuthorityMask {
  factory FlarkV3StructuralAuthorityMask(int bits) {
    if (bits <= 0 || (bits & ~knownBits) != 0) {
      throw RangeError.value(bits, 'bits', 'Unknown structural authority.');
    }
    return FlarkV3StructuralAuthorityMask._(bits);
  }

  const FlarkV3StructuralAuthorityMask._(this.bits);

  static const source = 1 << 0;
  static const projection = 1 << 1;
  static const green = 1 << 2;
  static const checkpoint = 1 << 3;
  static const references = 1 << 4;
  static const knownBits =
      source | projection | green | checkpoint | references;

  static const complete = FlarkV3StructuralAuthorityMask._(knownBits);

  final int bits;

  bool contains(int authority) => (bits & authority) == authority;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3StructuralAuthorityMask && other.bits == bits;

  @override
  int get hashCode => bits.hashCode;
}

final class FlarkV3SourceMetric {
  factory FlarkV3SourceMetric({required int bytes, required int utf16}) {
    _checkTransportU32(bytes, 'bytes');
    _checkTransportU32(utf16, 'utf16');
    return FlarkV3SourceMetric._(bytes: bytes, utf16: utf16);
  }

  const FlarkV3SourceMetric._({required this.bytes, required this.utf16});

  static const zero = FlarkV3SourceMetric._(bytes: 0, utf16: 0);

  final int bytes;
  final int utf16;

  bool contains(FlarkV3SourceMetric other) =>
      other.bytes <= bytes && other.utf16 <= utf16;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceMetric &&
      other.bytes == bytes &&
      other.utf16 == utf16;

  @override
  int get hashCode => Object.hash(bytes, utf16);

  @override
  String toString() => 'FlarkV3SourceMetric(bytes: $bytes, utf16: $utf16)';
}

final class FlarkV3MetricRange {
  FlarkV3MetricRange({required this.start, required this.end}) {
    if (!end.contains(start)) {
      throw RangeError('Metric range end must contain its start.');
    }
  }

  final FlarkV3SourceMetric start;
  final FlarkV3SourceMetric end;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3MetricRange && other.start == start && other.end == end;

  @override
  int get hashCode => Object.hash(start, end);
}

/// Exact UI-owned source lineage before derived byte/hash facts necessarily
/// exist.
///
/// This identity is deliberately too small to authorize parser publication or
/// structural queries. It only proves which UTF-16 source the editor owns now;
/// [FlarkV3SourceVersion] remains the certified host authority.
final class FlarkV3UiSourceIdentity {
  FlarkV3UiSourceIdentity({
    required this.documentSession,
    required this.uiRevision,
    required this.utf16Length,
  }) {
    _checkTransportU32(uiRevision, 'uiRevision');
    _checkTransportU32(utf16Length, 'utf16Length');
  }

  factory FlarkV3UiSourceIdentity.fromCertified(
    FlarkV3SourceVersion sourceVersion,
  ) => FlarkV3UiSourceIdentity(
    documentSession: sourceVersion.documentSession,
    uiRevision: sourceVersion.revision,
    utf16Length: sourceVersion.metric.utf16,
  );

  final FlarkV3DocumentSessionId documentSession;
  final int uiRevision;
  final int utf16Length;

  bool bindsCertified(FlarkV3SourceVersion sourceVersion) =>
      documentSession == sourceVersion.documentSession &&
      uiRevision == sourceVersion.revision &&
      utf16Length == sourceVersion.metric.utf16;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3UiSourceIdentity &&
      other.documentSession == documentSession &&
      other.uiRevision == uiRevision &&
      other.utf16Length == utf16Length;

  @override
  int get hashCode => Object.hash(documentSession, uiRevision, utf16Length);

  @override
  String toString() =>
      'FlarkV3UiSourceIdentity(session: $documentSession, '
      'uiRevision: $uiRevision, utf16: $utf16Length)';
}

final class FlarkV3SourceVersion {
  FlarkV3SourceVersion({
    required this.documentSession,
    required this.revision,
    required this.metric,
    required this.contentHash,
  }) {
    _checkTransportU32(revision, 'revision');
  }

  factory FlarkV3SourceVersion.fromDocument({
    required FlarkV3DocumentSessionId documentSession,
    required FlarkV3SourceDocument document,
  }) => FlarkV3SourceVersion(
    documentSession: documentSession,
    revision: document.revision,
    metric: FlarkV3SourceMetric(
      bytes: document.utf8Length,
      utf16: document.utf16Length,
    ),
    contentHash: document.contentHash128,
  );

  /// Real certified revision-zero worker base for a session that starts from
  /// empty source and adopts a non-empty provisional UI value.
  ///
  /// This is never the hash identity of that newer provisional UI value.
  factory FlarkV3SourceVersion.empty(
    FlarkV3DocumentSessionId documentSession,
  ) => FlarkV3SourceVersion(
    documentSession: documentSession,
    revision: 0,
    metric: FlarkV3SourceMetric.zero,
    contentHash: FlarkV3ContentHash128.zero,
  );

  final FlarkV3DocumentSessionId documentSession;
  final int revision;
  final FlarkV3SourceMetric metric;
  final FlarkV3ContentHash128 contentHash;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceVersion &&
      other.documentSession == documentSession &&
      other.revision == revision &&
      other.metric == metric &&
      other.contentHash == contentHash;

  @override
  int get hashCode =>
      Object.hash(documentSession, revision, metric, contentHash);

  @override
  String toString() =>
      'FlarkV3SourceVersion(session: $documentSession, revision: $revision, '
      'metric: $metric)';
}

final class FlarkV3HostRevisionId {
  FlarkV3HostRevisionId(this.value) {
    _checkTransportU32(value, 'value', positive: true);
  }

  final int value;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HostRevisionId && other.value == value;

  @override
  int get hashCode => value.hashCode;
}

enum FlarkV3PublicationMode {
  fullSnapshot,

  /// Reuses only the canonical References root named by the exact base ACK.
  ///
  /// Target role wrappers and the target manifest are always fresh.
  exactBaseReferencesDelta,

  /// Reuses exact installed References and incrementally splices persistent
  /// SourceFacts before fresh target nodes are admitted.
  exactBaseDelta,
}

final class FlarkV3StructuralAck {
  FlarkV3StructuralAck({
    required this.publicationSession,
    required this.hostRevision,
    required this.sourceVersion,
    required this.sourceRoot,
    required this.parseGeneration,
    required this.grammarRevision,
    required this.syntaxProfile,
    required this.authorityMask,
    required this.recordCount,
    required this.sequenceDigest,
    required this.manifestDigest,
  }) {
    _checkTransportU32(parseGeneration, 'parseGeneration', positive: true);
    _checkTransportU32(grammarRevision, 'grammarRevision', positive: true);
    _checkTransportU32(recordCount, 'recordCount');
  }

  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3HostRevisionId hostRevision;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3SourceRootId sourceRoot;
  final int parseGeneration;
  final int grammarRevision;
  final FlarkV3SyntaxProfileId syntaxProfile;
  final FlarkV3StructuralAuthorityMask authorityMask;
  final int recordCount;
  final FlarkV3ProtocolDigest128 sequenceDigest;
  final FlarkV3ProtocolDigest128 manifestDigest;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3StructuralAck &&
      other.publicationSession == publicationSession &&
      other.hostRevision == hostRevision &&
      other.sourceVersion == sourceVersion &&
      other.sourceRoot == sourceRoot &&
      other.parseGeneration == parseGeneration &&
      other.grammarRevision == grammarRevision &&
      other.syntaxProfile == syntaxProfile &&
      other.authorityMask == authorityMask &&
      other.recordCount == recordCount &&
      other.sequenceDigest == sequenceDigest &&
      other.manifestDigest == manifestDigest;

  @override
  int get hashCode => Object.hash(
    publicationSession,
    hostRevision,
    sourceVersion,
    sourceRoot,
    parseGeneration,
    grammarRevision,
    syntaxProfile,
    authorityMask,
    recordCount,
    sequenceDigest,
    manifestDigest,
  );
}

/// The fixed-size declaration accepted before any staged publication bytes.
///
/// Manifest records and root closures stay
/// entirely inside the shared Rust host store. Dart never mirrors them.
final class FlarkV3HostOfferBegin {
  FlarkV3HostOfferBegin({
    this.schema = supportedManifestSchema,
    required this.offerId,
    required this.publicationSession,
    required this.targetHostRevision,
    required this.sourceVersion,
    required this.sourceRoot,
    required this.parseGeneration,
    required this.grammarRevision,
    required this.syntaxProfile,
    required this.authorityMask,
    required this.mode,
    required this.baseAck,
    required this.transferredRecordCount,
    required this.targetRecordCount,
    required this.limits,
  }) {
    if (schema != supportedManifestSchema) {
      throw ArgumentError.value(
        schema,
        'schema',
        'Only the exact supported manifest schema is authoritative.',
      );
    }
    _checkTransportU32(parseGeneration, 'parseGeneration', positive: true);
    _checkTransportU32(grammarRevision, 'grammarRevision', positive: true);
    _checkTransportU32(
      transferredRecordCount,
      'transferredRecordCount',
      positive: true,
    );
    _checkTransportU32(targetRecordCount, 'targetRecordCount');
    switch (mode) {
      case FlarkV3PublicationMode.fullSnapshot:
        if (baseAck != null || transferredRecordCount != targetRecordCount) {
          throw ArgumentError('A full snapshot cannot bind a base ACK.');
        }
        break;
      case FlarkV3PublicationMode.exactBaseReferencesDelta:
        final base = baseAck;
        if (base == null) {
          throw ArgumentError(
            'An exact-base References delta requires a base ACK.',
          );
        }
        if (base.publicationSession == publicationSession) {
          throw ArgumentError('The target publication identity must be fresh.');
        }
        if (base.grammarRevision != grammarRevision ||
            base.syntaxProfile != syntaxProfile ||
            base.authorityMask != authorityMask ||
            base.parseGeneration >= parseGeneration) {
          throw ArgumentError(
            'A delta must preserve grammar/profile/authority and advance its '
            'parse generation.',
          );
        }
        if (base.sourceVersion.documentSession !=
                sourceVersion.documentSession ||
            base.sourceVersion.revision >= sourceVersion.revision) {
          throw ArgumentError('A delta must advance its exact document base.');
        }
        if (transferredRecordCount >= targetRecordCount) {
          throw ArgumentError(
            'A References delta must omit exact reused role records.',
          );
        }
        break;
      case FlarkV3PublicationMode.exactBaseDelta:
        final base = baseAck;
        if (base == null) {
          throw ArgumentError('An exact-base delta requires a base ACK.');
        }
        if (base.publicationSession == publicationSession) {
          throw ArgumentError('The target publication identity must be fresh.');
        }
        if (base.grammarRevision != grammarRevision ||
            base.syntaxProfile != syntaxProfile ||
            base.authorityMask != authorityMask ||
            base.parseGeneration >= parseGeneration) {
          throw ArgumentError(
            'A delta must preserve grammar/profile/authority and advance its '
            'parse generation.',
          );
        }
        if (base.sourceVersion.documentSession !=
                sourceVersion.documentSession ||
            base.sourceVersion.revision >= sourceVersion.revision) {
          throw ArgumentError('A delta must advance its exact document base.');
        }
        if (transferredRecordCount > targetRecordCount) {
          throw ArgumentError(
            'An exact-base delta cannot transfer more than its target records.',
          );
        }
        break;
    }
  }

  static const int supportedManifestSchema = 1;

  final int schema;
  final FlarkV3OfferId offerId;
  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3HostRevisionId targetHostRevision;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3SourceRootId sourceRoot;
  final int parseGeneration;
  final int grammarRevision;
  final FlarkV3SyntaxProfileId syntaxProfile;
  final FlarkV3StructuralAuthorityMask authorityMask;
  final FlarkV3PublicationMode mode;
  final FlarkV3StructuralAck? baseAck;
  final int transferredRecordCount;
  final int targetRecordCount;
  final FlarkV3HostOfferLimits limits;
}

/// Hard admission ceilings. Exact stream totals are supplied only after the
/// one-pass encoder closes the stream.
final class FlarkV3HostOfferLimits {
  FlarkV3HostOfferLimits({
    required this.maximumFrameCount,
    required this.maximumEncodedFrameBytes,
    required this.maximumPacketBytes,
    required this.maximumFrameBytes,
    required this.maximumProgramChildren,
  }) {
    _checkTransportU32(maximumFrameCount, 'maximumFrameCount', positive: true);
    _checkTransportU32(
      maximumEncodedFrameBytes,
      'maximumEncodedFrameBytes',
      positive: true,
    );
    _checkTransportU32(
      maximumPacketBytes,
      'maximumPacketBytes',
      positive: true,
    );
    _checkTransportU32(maximumFrameBytes, 'maximumFrameBytes', positive: true);
    _checkTransportU32(
      maximumProgramChildren,
      'maximumProgramChildren',
      positive: true,
    );
    final minimumPacketBytes =
        FlarkV3HostPublicationPacket.wireHeaderBytes +
        FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes +
        maximumFrameBytes;
    if (maximumFrameBytes > maximumEncodedFrameBytes ||
        minimumPacketBytes > maximumPacketBytes ||
        maximumPacketBytes > productMaximumPacketBytes ||
        maximumFrameBytes > productMaximumFrameBytes ||
        maximumProgramChildren > productMaximumProgramChildren) {
      throw RangeError('Host offer limits are inconsistent.');
    }
  }

  static const productMaximumPacketBytes =
      FlarkV3HostPublicationPacket.maximumRawBytes;
  static const productMaximumFrameBytes = 5_140;
  static const productMaximumProgramChildren = 128;

  final int maximumFrameCount;
  final int maximumEncodedFrameBytes;
  final int maximumPacketBytes;
  final int maximumFrameBytes;
  final int maximumProgramChildren;
}

/// Exact transport totals supplied after the worker's one-pass encoder closes.
final class FlarkV3HostCommitRequest {
  FlarkV3HostCommitRequest({
    required this.offerId,
    required this.actualFrameCount,
    required this.actualEncodedFrameBytes,
    required this.rollingTransportDigest,
    required this.canonicalStreamDigest,
  }) {
    _checkTransportU32(actualFrameCount, 'actualFrameCount');
    _checkTransportU32(actualEncodedFrameBytes, 'actualEncodedFrameBytes');
  }

  final FlarkV3OfferId offerId;
  final int actualFrameCount;
  final int actualEncodedFrameBytes;
  final FlarkV3ProtocolDigest128 rollingTransportDigest;
  final FlarkV3ProtocolDigest128 canonicalStreamDigest;
}

/// One transfer-owned FPK3 packet carried by publication schema 4.
///
/// [rawBytes] is the exact packet body: its aggregate header, fixed-width
/// frame directory, and concatenated frame bodies. Dart validates aggregate
/// bounds in-place but never allocates or retains per-frame objects. Callers
/// must not mutate or reuse this buffer after transferring the packet.
final class FlarkV3HostPublicationPacket {
  factory FlarkV3HostPublicationPacket.fromOwnedBytes(Uint8List rawBytes) {
    final header = _validatePublicationPacket(rawBytes);
    return FlarkV3HostPublicationPacket._(
      offerId: header.offerId,
      firstFrameOrdinal: header.firstFrameOrdinal,
      firstRecordOrdinal: header.firstRecordOrdinal,
      frameCount: header.frameCount,
      aggregateRecordCount: header.aggregateRecordCount,
      aggregateFrameBytes: header.aggregateFrameBytes,
      rawBytes: rawBytes,
    );
  }

  factory FlarkV3HostPublicationPacket.fromCopiedBytes(Uint8List rawBytes) =>
      FlarkV3HostPublicationPacket.fromOwnedBytes(Uint8List.fromList(rawBytes));

  const FlarkV3HostPublicationPacket._({
    required this.offerId,
    required this.firstFrameOrdinal,
    required this.firstRecordOrdinal,
    required this.frameCount,
    required this.aggregateRecordCount,
    required this.aggregateFrameBytes,
    required this.rawBytes,
  });

  static const int wireVersion = 1;
  static const int wireFlags = 0;
  static const int wireHeaderBytes = 44;
  static const int wireFrameDirectoryEntryBytes = 24;
  static const int maximumFrameCount = 256;
  static const int maximumAggregateFrameBytes = 64 * 1024;
  static const int maximumRawBytes =
      wireHeaderBytes +
      maximumFrameCount * wireFrameDirectoryEntryBytes +
      maximumAggregateFrameBytes;

  final FlarkV3OfferId offerId;
  final int firstFrameOrdinal;
  final int firstRecordOrdinal;
  final int frameCount;
  final int aggregateRecordCount;
  final int aggregateFrameBytes;
  final Uint8List rawBytes;
}

final class _PublicationPacketHeader {
  const _PublicationPacketHeader({
    required this.offerId,
    required this.firstFrameOrdinal,
    required this.firstRecordOrdinal,
    required this.frameCount,
    required this.aggregateRecordCount,
    required this.aggregateFrameBytes,
  });

  final FlarkV3OfferId offerId;
  final int firstFrameOrdinal;
  final int firstRecordOrdinal;
  final int frameCount;
  final int aggregateRecordCount;
  final int aggregateFrameBytes;
}

_PublicationPacketHeader _validatePublicationPacket(Uint8List rawBytes) {
  if (rawBytes.length < FlarkV3HostPublicationPacket.wireHeaderBytes) {
    throw ArgumentError.value(rawBytes.length, 'rawBytes.length');
  }
  final data = ByteData.sublistView(rawBytes);
  if (rawBytes[0] != 0x46 ||
      rawBytes[1] != 0x50 ||
      rawBytes[2] != 0x4b ||
      rawBytes[3] != 0x33) {
    throw ArgumentError('Publication packet magic must be FPK3.');
  }
  final version = data.getUint16(4, Endian.little);
  final flags = data.getUint16(6, Endian.little);
  if (version != FlarkV3HostPublicationPacket.wireVersion ||
      flags != FlarkV3HostPublicationPacket.wireFlags) {
    throw ArgumentError('Publication packet version or flags are unsupported.');
  }
  final offerId = FlarkV3OfferId(
    data.getUint32(8, Endian.little),
    data.getUint32(12, Endian.little),
    data.getUint32(16, Endian.little),
    data.getUint32(20, Endian.little),
  );
  final firstFrameOrdinal = data.getUint32(24, Endian.little);
  final firstRecordOrdinal = data.getUint32(28, Endian.little);
  final frameCount = data.getUint32(32, Endian.little);
  final aggregateRecordCount = data.getUint32(36, Endian.little);
  final aggregateFrameBytes = data.getUint32(40, Endian.little);

  if (frameCount == 0 ||
      frameCount > FlarkV3HostPublicationPacket.maximumFrameCount ||
      firstFrameOrdinal + frameCount > flarkV3TransportV1Maximum ||
      firstRecordOrdinal + aggregateRecordCount > flarkV3TransportV1Maximum ||
      aggregateFrameBytes >
          FlarkV3HostPublicationPacket.maximumAggregateFrameBytes) {
    throw ArgumentError('Publication packet aggregate bounds are invalid.');
  }
  final directoryBytes =
      frameCount * FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  final expectedBytes =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      directoryBytes +
      aggregateFrameBytes;
  if (rawBytes.length != expectedBytes) {
    throw ArgumentError.value(
      rawBytes.length,
      'rawBytes.length',
      'Expected $expectedBytes bytes from the packet aggregate header.',
    );
  }

  return _PublicationPacketHeader(
    offerId: offerId,
    firstFrameOrdinal: firstFrameOrdinal,
    firstRecordOrdinal: firstRecordOrdinal,
    frameCount: frameCount,
    aggregateRecordCount: aggregateRecordCount,
    aggregateFrameBytes: aggregateFrameBytes,
  );
}

final class FlarkV3HostWorkGrant {
  FlarkV3HostWorkGrant({
    required this.inspectBytes,
    required this.copyBytes,
    required this.transitions,
  }) {
    _checkTransportU32(inspectBytes, 'inspectBytes');
    _checkTransportU32(copyBytes, 'copyBytes');
    _checkTransportU32(transitions, 'transitions');
  }

  final int inspectBytes;
  final int copyBytes;
  final int transitions;
}

enum FlarkV3MetricAffinity { upstream, downstream }

final class FlarkV3HostPointQuery {
  const FlarkV3HostPointQuery({
    required this.sourceVersion,
    required this.position,
    required this.budget,
    this.affinity = FlarkV3MetricAffinity.downstream,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3SourceMetric position;
  final FlarkV3HostQueryBudget budget;
  final FlarkV3MetricAffinity affinity;
}

final class FlarkV3HostQueryBudget {
  FlarkV3HostQueryBudget({
    required this.maxEncodedBytes,
    required this.maxOpenDepth,
    required this.maxLeafCount,
    required this.maxTreeNodesVisited,
  }) {
    if (maxEncodedBytes <= 0 ||
        maxOpenDepth <= 0 ||
        maxLeafCount <= 0 ||
        maxTreeNodesVisited <= 0) {
      throw RangeError('Host query budgets must be positive.');
    }
    _checkTransportU32(maxEncodedBytes, 'maxEncodedBytes', positive: true);
    _checkTransportU32(maxOpenDepth, 'maxOpenDepth', positive: true);
    _checkTransportU32(maxLeafCount, 'maxLeafCount', positive: true);
    _checkTransportU32(
      maxTreeNodesVisited,
      'maxTreeNodesVisited',
      positive: true,
    );
  }

  final int maxEncodedBytes;
  final int maxOpenDepth;
  final int maxLeafCount;

  /// Hard cap on host-store tree visits/transitions for this query.
  final int maxTreeNodesVisited;
}

/// Hard work and copy limits for one consecutive structural-block query.
///
/// Range work is intentionally distinct from a point-query closure: output
/// block count and authenticated storage pages are independently bounded.
final class FlarkV3HostBlockRangeBudget {
  FlarkV3HostBlockRangeBudget({
    required this.maxEncodedBytes,
    required this.maxBlockCount,
    required this.maxStoragePagesVisited,
    required this.maxOpenDepth,
    required this.maxTreeNodesVisited,
  }) {
    if (maxEncodedBytes <= 0 ||
        maxBlockCount <= 0 ||
        maxStoragePagesVisited <= 0 ||
        maxOpenDepth <= 0 ||
        maxTreeNodesVisited <= 0) {
      throw RangeError('Host block-range budgets must be positive.');
    }
    _checkTransportU32(maxEncodedBytes, 'maxEncodedBytes', positive: true);
    _checkTransportU32(maxBlockCount, 'maxBlockCount', positive: true);
    _checkTransportU32(
      maxStoragePagesVisited,
      'maxStoragePagesVisited',
      positive: true,
    );
    _checkTransportU32(maxOpenDepth, 'maxOpenDepth', positive: true);
    _checkTransportU32(
      maxTreeNodesVisited,
      'maxTreeNodesVisited',
      positive: true,
    );
  }

  final int maxEncodedBytes;
  final int maxBlockCount;
  final int maxStoragePagesVisited;
  final int maxOpenDepth;
  final int maxTreeNodesVisited;
}

/// Maximum authenticated packed entries carried by one structural storage
/// page. Both legacy block leaves and RecursiveGreen event leaves fit this
/// shared host receipt bound.
const int flarkV3HostMaximumPackedEntriesPerStoragePage = 128;

bool flarkV3HostPackedEntryReceiptFitsStoragePages({
  required int storagePagesVisited,
  required int packedEntriesInspected,
}) =>
    packedEntriesInspected <=
    storagePagesVisited * flarkV3HostMaximumPackedEntriesPerStoragePage;

/// Hard host-work bounds for one ordinal-to-source structural lookup.
///
/// This locator returns only authenticated ordinal and source cuts. It never
/// copies block records or performs Markdown work in Dart.
final class FlarkV3HostStructuralOrdinalWindowBudget {
  FlarkV3HostStructuralOrdinalWindowBudget({
    required this.maximumEntries,
    required this.maximumStoragePagesVisited,
    required this.maximumTreeNodesVisited,
    required this.maximumPackedEntriesInspected,
  }) {
    if (maximumEntries <= 0 ||
        maximumEntries > maximumWindowEntries ||
        maximumStoragePagesVisited <= 0 ||
        maximumTreeNodesVisited <= 0 ||
        maximumPackedEntriesInspected <= 0) {
      throw RangeError(
        'Structural ordinal-window budgets must be positive and contain at '
        'most $maximumWindowEntries entries.',
      );
    }
    _checkTransportU32(maximumEntries, 'maximumEntries', positive: true);
    _checkTransportU32(
      maximumStoragePagesVisited,
      'maximumStoragePagesVisited',
      positive: true,
    );
    _checkTransportU32(
      maximumTreeNodesVisited,
      'maximumTreeNodesVisited',
      positive: true,
    );
    _checkTransportU32(
      maximumPackedEntriesInspected,
      'maximumPackedEntriesInspected',
      positive: true,
    );
  }

  static const int maximumWindowEntries = 4096;

  final int maximumEntries;
  final int maximumStoragePagesVisited;
  final int maximumTreeNodesVisited;
  final int maximumPackedEntriesInspected;
}

/// One exact-source ordinal window request.
final class FlarkV3HostStructuralOrdinalWindowQuery {
  const FlarkV3HostStructuralOrdinalWindowQuery({
    required this.sourceVersion,
    required this.startBlockOrdinal,
    required this.budget,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3ProtocolU64 startBlockOrdinal;
  final FlarkV3HostStructuralOrdinalWindowBudget budget;
}

/// Work receipt shared by successful and undecodable ordinal lookups.
final class FlarkV3HostStructuralOrdinalWindowWorkReceipt {
  FlarkV3HostStructuralOrdinalWindowWorkReceipt({
    required this.storagePagesVisited,
    required this.treeNodesVisited,
    required this.packedEntriesInspected,
    required this.summaryNodesSkipped,
  }) {
    _checkTransportU32(storagePagesVisited, 'storagePagesVisited');
    _checkTransportU32(treeNodesVisited, 'treeNodesVisited');
    _checkTransportU32(packedEntriesInspected, 'packedEntriesInspected');
    _checkTransportU32(summaryNodesSkipped, 'summaryNodesSkipped');
  }

  static final zero = FlarkV3HostStructuralOrdinalWindowWorkReceipt(
    storagePagesVisited: 0,
    treeNodesVisited: 0,
    packedEntriesInspected: 0,
    summaryNodesSkipped: 0,
  );

  final int storagePagesVisited;
  final int treeNodesVisited;
  final int packedEntriesInspected;
  final int summaryNodesSkipped;

  bool get isZero =>
      storagePagesVisited == 0 &&
      treeNodesVisited == 0 &&
      packedEntriesInspected == 0 &&
      summaryNodesSkipped == 0;

  bool fits(FlarkV3HostStructuralOrdinalWindowBudget budget) =>
      storagePagesVisited <= budget.maximumStoragePagesVisited &&
      treeNodesVisited <= budget.maximumTreeNodesVisited &&
      packedEntriesInspected <= budget.maximumPackedEntriesInspected;
}

enum FlarkV3HostStructuralOrdinalWindowFailureReason {
  unavailable,
  entryLimit,
  storagePageLimit,
  treeNodeLimit,
  packedEntryLimit,
  ordinalOutOfRange,
  undecodable,
}

sealed class FlarkV3HostStructuralOrdinalWindowOutcome {
  const FlarkV3HostStructuralOrdinalWindowOutcome();

  bool binds(FlarkV3HostStructuralOrdinalWindowQuery query);
}

/// Exact authenticated ordinal and source cuts for one bounded window.
final class FlarkV3HostStructuralOrdinalWindow
    extends FlarkV3HostStructuralOrdinalWindowOutcome {
  const FlarkV3HostStructuralOrdinalWindow({
    required this.sourceVersion,
    required this.totalBlockCount,
    required this.startBlockOrdinal,
    required this.nextBlockOrdinal,
    required this.startSource,
    required this.nextSource,
    required this.work,
    required this.complete,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3ProtocolU64 totalBlockCount;
  final FlarkV3ProtocolU64 startBlockOrdinal;
  final FlarkV3ProtocolU64 nextBlockOrdinal;
  final FlarkV3SourceMetric startSource;
  final FlarkV3SourceMetric nextSource;
  final FlarkV3HostStructuralOrdinalWindowWorkReceipt work;
  final bool complete;

  @override
  bool binds(FlarkV3HostStructuralOrdinalWindowQuery query) {
    final terminal = startBlockOrdinal == totalBlockCount;
    return sourceVersion == query.sourceVersion &&
        startBlockOrdinal == query.startBlockOrdinal &&
        startBlockOrdinal.compareTo(totalBlockCount) <= 0 &&
        nextBlockOrdinal.compareTo(startBlockOrdinal) >= 0 &&
        nextBlockOrdinal.compareTo(totalBlockCount) <= 0 &&
        _u64SpanAtMost(
          startBlockOrdinal,
          nextBlockOrdinal,
          query.budget.maximumEntries,
        ) &&
        work.fits(query.budget) &&
        sourceVersion.metric.contains(startSource) &&
        sourceVersion.metric.contains(nextSource) &&
        nextSource.contains(startSource) &&
        complete == (nextBlockOrdinal == totalBlockCount) &&
        (terminal
            ? nextBlockOrdinal == totalBlockCount &&
                  startSource == sourceVersion.metric &&
                  nextSource == sourceVersion.metric
            : nextBlockOrdinal.compareTo(startBlockOrdinal) > 0);
  }
}

/// Typed, exact-source failure from the ordinal locator.
final class FlarkV3HostStructuralOrdinalWindowFailure
    extends FlarkV3HostStructuralOrdinalWindowOutcome {
  const FlarkV3HostStructuralOrdinalWindowFailure({
    required this.sourceVersion,
    required this.totalBlockCount,
    required this.startBlockOrdinal,
    required this.reason,
    required this.work,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3ProtocolU64 totalBlockCount;
  final FlarkV3ProtocolU64 startBlockOrdinal;
  final FlarkV3HostStructuralOrdinalWindowFailureReason reason;
  final FlarkV3HostStructuralOrdinalWindowWorkReceipt work;

  @override
  bool binds(FlarkV3HostStructuralOrdinalWindowQuery query) {
    if (sourceVersion != query.sourceVersion ||
        startBlockOrdinal != query.startBlockOrdinal ||
        !work.fits(query.budget)) {
      return false;
    }
    return switch (reason) {
      FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable =>
        totalBlockCount.isZero && work.isZero,
      FlarkV3HostStructuralOrdinalWindowFailureReason.entryLimit ||
      FlarkV3HostStructuralOrdinalWindowFailureReason.storagePageLimit ||
      FlarkV3HostStructuralOrdinalWindowFailureReason.treeNodeLimit ||
      FlarkV3HostStructuralOrdinalWindowFailureReason.packedEntryLimit ||
      FlarkV3HostStructuralOrdinalWindowFailureReason.ordinalOutOfRange =>
        work.isZero,
      FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable =>
        totalBlockCount.isZero,
    };
  }
}

/// Opaque exact-revision continuation minted and consumed by the Rust host.
///
/// Dart never interprets these bytes. Both constructors copy so neither an
/// adapter scratch view nor a caller-owned list can mutate the resume claim.
final class FlarkV3HostBlockRangeContinuation {
  FlarkV3HostBlockRangeContinuation.owned(Uint8List encoded)
    : _encoded = Uint8List.fromList(encoded) {
    if (_encoded.length != encodedBytes) {
      throw ArgumentError.value(
        encoded.length,
        'encoded',
        'A block-range continuation must contain exactly $encodedBytes bytes.',
      );
    }
  }

  static const int encodedBytes = 64;

  final Uint8List _encoded;

  Uint8List copyEncoded() => Uint8List.fromList(_encoded);

  @override
  bool operator ==(Object other) =>
      other is FlarkV3HostBlockRangeContinuation &&
      _constantTimeBytesEqual(other._encoded, _encoded);

  @override
  int get hashCode => Object.hashAll(_encoded);
}

/// One exact requested source interval and optional host-minted continuation.
///
/// The original [requestedRange] is repeated on every continuation call. The
/// opaque token may advance traversal only within that exact range and source
/// version.
final class FlarkV3HostBlockRangeQuery {
  const FlarkV3HostBlockRangeQuery({
    required this.sourceVersion,
    required this.requestedRange,
    required this.budget,
    this.continuation,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange requestedRange;
  final FlarkV3HostBlockRangeBudget budget;
  final FlarkV3HostBlockRangeContinuation? continuation;
}

/// Bounded-work receipt for one structural range page.
final class FlarkV3HostBlockRangeReceipt {
  FlarkV3HostBlockRangeReceipt({
    required this.encodedBytes,
    required this.blockCount,
    required this.storagePagesVisited,
    required this.openDepth,
    required this.treeNodesVisited,
    required this.packedEntriesInspected,
    required this.summaryNodesSkipped,
    required this.complete,
  }) {
    _checkTransportU32(encodedBytes, 'encodedBytes');
    _checkTransportU32(blockCount, 'blockCount');
    _checkTransportU32(storagePagesVisited, 'storagePagesVisited');
    _checkTransportU32(openDepth, 'openDepth');
    _checkTransportU32(treeNodesVisited, 'treeNodesVisited');
    _checkTransportU32(packedEntriesInspected, 'packedEntriesInspected');
    _checkTransportU32(summaryNodesSkipped, 'summaryNodesSkipped');
  }

  final int encodedBytes;
  final int blockCount;
  final int storagePagesVisited;
  final int openDepth;
  final int treeNodesVisited;
  final int packedEntriesInspected;
  final int summaryNodesSkipped;
  final bool complete;
}

final class FlarkV3HostViewportReceipt {
  FlarkV3HostViewportReceipt({
    required this.encodedBytes,
    required this.leafCount,
    required this.openDepth,
    required this.treeNodesVisited,
    required this.summaryNodesSkipped,
  }) {
    _checkTransportU32(encodedBytes, 'encodedBytes');
    _checkTransportU32(leafCount, 'leafCount');
    _checkTransportU32(openDepth, 'openDepth');
    _checkTransportU32(treeNodesVisited, 'treeNodesVisited');
    _checkTransportU32(summaryNodesSkipped, 'summaryNodesSkipped');
  }

  final int encodedBytes;
  final int leafCount;
  final int openDepth;
  final int treeNodesVisited;
  final int summaryNodesSkipped;
}

enum FlarkV3HostSourceGapReason {
  openDepthLimit,
  encodedByteLimit,
  leafLimit,
  treeNodeLimit,
  undecodableClosure,
  unavailableFacts,
}

/// Exact structural fallback authored by the shared host store.
///
/// The current depth-cap implementation returns BOF-to-EOF. Keeping the typed
/// range and reason in the ABI permits a later proven narrower fallback without
/// conflating it with the controller's stable-pending state.
final class FlarkV3HostLocalSourceGap {
  const FlarkV3HostLocalSourceGap({
    required this.sourceVersion,
    required this.range,
    required this.reason,
    required this.receipt,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange range;
  final FlarkV3HostSourceGapReason reason;
  final FlarkV3HostViewportReceipt receipt;
}

/// One structure-only page of consecutive top-level block records.
///
/// [encoded] is one owned `FLKVR001` envelope. Leaf projections and point
/// paths never ride this lane; they remain independently demand-loaded by the
/// selected point query.
final class FlarkV3HostStructuralBlockRange {
  FlarkV3HostStructuralBlockRange.owned({
    required this.sourceVersion,
    required this.requestedRange,
    required this.coveredRange,
    required this.encoded,
    required this.receipt,
    required this.continuation,
  });

  factory FlarkV3HostStructuralBlockRange.copied({
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3MetricRange requestedRange,
    required FlarkV3MetricRange coveredRange,
    required Uint8List encoded,
    required FlarkV3HostBlockRangeReceipt receipt,
    required FlarkV3HostBlockRangeContinuation? continuation,
  }) => FlarkV3HostStructuralBlockRange.owned(
    sourceVersion: sourceVersion,
    requestedRange: requestedRange,
    coveredRange: coveredRange,
    encoded: Uint8List.fromList(encoded),
    receipt: receipt,
    continuation: continuation,
  );

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange requestedRange;
  final FlarkV3MetricRange coveredRange;
  final Uint8List encoded;
  final FlarkV3HostBlockRangeReceipt receipt;
  final FlarkV3HostBlockRangeContinuation? continuation;
}

/// Exact source fallback for a structural-range request that cannot fit.
final class FlarkV3HostBlockRangeSourceGap {
  const FlarkV3HostBlockRangeSourceGap({
    required this.sourceVersion,
    required this.requestedRange,
    required this.reason,
    required this.receipt,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange requestedRange;
  final FlarkV3HostSourceGapReason reason;
  final FlarkV3HostBlockRangeReceipt receipt;
}

/// Bounded, host-copied viewport data. The persistent sequence remains in
/// Rust; this is the only structural material allowed back into Dart.
final class FlarkV3HostStructuralViewport {
  FlarkV3HostStructuralViewport.owned({
    required this.sourceVersion,
    required this.range,
    required this.encoded,
    required this.receipt,
  });

  factory FlarkV3HostStructuralViewport.copied({
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3MetricRange range,
    required Uint8List encoded,
    required FlarkV3HostViewportReceipt receipt,
  }) => FlarkV3HostStructuralViewport.owned(
    sourceVersion: sourceVersion,
    range: range,
    encoded: Uint8List.fromList(encoded),
    receipt: receipt,
  );

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange range;

  /// Adapter-owned bounded buffer. The constructor deliberately does not copy.
  final Uint8List encoded;
  final FlarkV3HostViewportReceipt receipt;
}

void _checkWords(int word0, int word1, int word2, int word3, String name) {
  for (final word in [word0, word1, word2, word3]) {
    _checkTransportU32(word, name);
  }
}

bool _constantTimeBytesEqual(Uint8List left, Uint8List right) {
  if (left.length != right.length) return false;
  var difference = 0;
  for (var index = 0; index < left.length; index += 1) {
    difference |= left[index] ^ right[index];
  }
  return difference == 0;
}

bool _u64SpanAtMost(
  FlarkV3ProtocolU64 start,
  FlarkV3ProtocolU64 end,
  int maximum,
) {
  if (end.compareTo(start) < 0) return false;
  if (end.highWord == start.highWord) {
    return end.lowWord - start.lowWord <= maximum;
  }
  if (end.highWord == start.highWord + 1) {
    return (0x100000000 - start.lowWord) + end.lowWord <= maximum;
  }
  return false;
}
