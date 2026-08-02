import 'dart:typed_data';

import 'flark_v3_host_protocol.dart';
import 'flark_v3_hot_inline_sidecar_protocol.dart'
    show
        FlarkV3HotInlineSidecarBinding,
        FlarkV3ProtocolDigest256,
        FlarkV3ProtocolU64;

/// Sibling publication target for one authenticated aggregate viewport page.
///
/// This is neither a structural publication mode nor a singleton HIO1
/// sidecar.
enum FlarkV3ViewportPresentationMode { aggregatePage }

/// Exact non-empty UTF-8/UTF-16 source range authenticated by one VPB1 page.
final class FlarkV3ViewportPresentationMetricRange {
  FlarkV3ViewportPresentationMetricRange({
    required this.startUtf8,
    required this.startUtf16,
    required this.endUtf8,
    required this.endUtf16,
  }) {
    for (final (name, value) in [
      ('startUtf8', startUtf8),
      ('startUtf16', startUtf16),
      ('endUtf8', endUtf8),
      ('endUtf16', endUtf16),
    ]) {
      _checkU32(value, name);
    }
    if (startUtf8 >= endUtf8 || startUtf16 >= endUtf16) {
      throw RangeError('A viewport-presentation range must be non-empty.');
    }
  }

  final int startUtf8;
  final int startUtf16;
  final int endUtf8;
  final int endUtf16;

  bool contains(FlarkV3ViewportPresentationMetricRange other) =>
      other.startUtf8 >= startUtf8 &&
      other.startUtf16 >= startUtf16 &&
      other.endUtf8 <= endUtf8 &&
      other.endUtf16 <= endUtf16;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationMetricRange &&
      other.startUtf8 == startUtf8 &&
      other.startUtf16 == startUtf16 &&
      other.endUtf8 == endUtf8 &&
      other.endUtf16 == endUtf16;

  @override
  int get hashCode => Object.hash(startUtf8, startUtf16, endUtf8, endUtf16);
}

/// Stable measured-sequence cut used to begin or resume one VPB1 page.
final class FlarkV3ViewportPresentationVisitStart {
  FlarkV3ViewportPresentationVisitStart({
    required this.blockOrdinal,
    required this.utf8Offset,
    required this.utf16Offset,
  }) {
    _checkU32(utf8Offset, 'utf8Offset');
    _checkU32(utf16Offset, 'utf16Offset');
  }

  final FlarkV3ProtocolU64 blockOrdinal;
  final int utf8Offset;
  final int utf16Offset;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationVisitStart &&
      other.blockOrdinal == blockOrdinal &&
      other.utf8Offset == utf8Offset &&
      other.utf16Offset == utf16Offset;

  @override
  int get hashCode => Object.hash(blockOrdinal, utf8Offset, utf16Offset);
}

/// Exact caller demand and producer-authenticated coverage for one VPB1 page.
///
/// [next] is present on complete pages as well as partial pages. [complete] is
/// true exactly when the covered end reaches the requested end in both source
/// metrics.
final class FlarkV3ViewportPresentationBinding {
  FlarkV3ViewportPresentationBinding({
    required this.viewportGeneration,
    required this.requestedRange,
    required this.coveredRange,
    required this.start,
    required this.next,
    required this.complete,
  }) {
    _checkU32(viewportGeneration, 'viewportGeneration', positive: true);
    if (!requestedRange.contains(coveredRange)) {
      throw RangeError(
        'Viewport-presentation coverage must stay inside the requested range.',
      );
    }
    if (start.utf8Offset != coveredRange.startUtf8 ||
        start.utf16Offset != coveredRange.startUtf16 ||
        next.utf8Offset != coveredRange.endUtf8 ||
        next.utf16Offset != coveredRange.endUtf16 ||
        next.blockOrdinal.compareTo(start.blockOrdinal) <= 0) {
      throw ArgumentError(
        'Viewport-presentation cuts must exactly fence ordered coverage.',
      );
    }
    final reachesRequestedEnd =
        coveredRange.endUtf8 == requestedRange.endUtf8 &&
        coveredRange.endUtf16 == requestedRange.endUtf16;
    if (complete != reachesRequestedEnd) {
      throw ArgumentError(
        'Viewport-presentation completion must match its authenticated end.',
      );
    }
  }

  final int viewportGeneration;
  final FlarkV3ViewportPresentationMetricRange requestedRange;
  final FlarkV3ViewportPresentationMetricRange coveredRange;
  final FlarkV3ViewportPresentationVisitStart start;
  final FlarkV3ViewportPresentationVisitStart next;
  final bool complete;

  void requireBase(FlarkV3StructuralAck baseAck) {
    if (requestedRange.endUtf8 > baseAck.sourceVersion.metric.bytes ||
        requestedRange.endUtf16 > baseAck.sourceVersion.metric.utf16) {
      throw ArgumentError(
        'Viewport-presentation demand exceeds its exact structural base.',
      );
    }
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationBinding &&
      other.viewportGeneration == viewportGeneration &&
      other.requestedRange == requestedRange &&
      other.coveredRange == coveredRange &&
      other.start == start &&
      other.next == next &&
      other.complete == complete;

  @override
  int get hashCode => Object.hash(
    viewportGeneration,
    requestedRange,
    coveredRange,
    start,
    next,
    complete,
  );
}

/// Caller-owned semantic work bounds repeated in VPB1 Begin.
///
/// These limits bound the parser-authored aggregate. Transport limits remain
/// independently owned by [FlarkV3ViewportPresentationOfferLimits].
final class FlarkV3ViewportPresentationQueryLimits {
  FlarkV3ViewportPresentationQueryLimits({
    required this.maximumStructuralEntries,
    required this.maximumStoragePages,
    required this.maximumInlineLeaves,
    required this.maximumInlineLeafSourceBytes,
    required this.maximumInlineSourceBytes,
    required this.maximumFactRecords,
    required this.maximumEncodedFrameBytes,
    required this.maximumParserTransitions,
  }) {
    for (final (name, value, maximum) in [
      (
        'maximumStructuralEntries',
        maximumStructuralEntries,
        productMaximumStructuralEntries,
      ),
      ('maximumStoragePages', maximumStoragePages, productMaximumStoragePages),
      ('maximumInlineLeaves', maximumInlineLeaves, productMaximumInlineLeaves),
      (
        'maximumInlineLeafSourceBytes',
        maximumInlineLeafSourceBytes,
        productMaximumInlineLeafSourceBytes,
      ),
      (
        'maximumInlineSourceBytes',
        maximumInlineSourceBytes,
        productMaximumInlineSourceBytes,
      ),
      ('maximumFactRecords', maximumFactRecords, productMaximumFactRecords),
      (
        'maximumEncodedFrameBytes',
        maximumEncodedFrameBytes,
        productMaximumEncodedFrameBytes,
      ),
      (
        'maximumParserTransitions',
        maximumParserTransitions,
        productMaximumParserTransitions,
      ),
    ]) {
      if (value <= 0 || value > maximum) {
        throw RangeError.range(value, 1, maximum, name);
      }
    }
    if (maximumInlineLeaves > maximumStructuralEntries) {
      throw RangeError(
        'Viewport inline-leaf capacity cannot exceed structural capacity.',
      );
    }
    if (maximumInlineLeafSourceBytes > maximumInlineSourceBytes) {
      throw RangeError(
        'Per-leaf source capacity cannot exceed aggregate source capacity.',
      );
    }
  }

  static const int productMaximumStructuralEntries = 256;
  static const int productMaximumStoragePages = 257;
  static const int productMaximumInlineLeaves = 128;
  static const int productMaximumInlineLeafSourceBytes = 8 * 1024;
  static const int productMaximumInlineSourceBytes = 1024 * 1024;
  static const int productMaximumFactRecords = 2048;
  static const int productMaximumEncodedFrameBytes = 4 * 1024 * 1024;
  static const int productMaximumParserTransitions = 1000000;

  static final productMaximum = FlarkV3ViewportPresentationQueryLimits(
    maximumStructuralEntries: productMaximumStructuralEntries,
    maximumStoragePages: productMaximumStoragePages,
    maximumInlineLeaves: productMaximumInlineLeaves,
    maximumInlineLeafSourceBytes: productMaximumInlineLeafSourceBytes,
    maximumInlineSourceBytes: productMaximumInlineSourceBytes,
    maximumFactRecords: productMaximumFactRecords,
    maximumEncodedFrameBytes: productMaximumEncodedFrameBytes,
    maximumParserTransitions: productMaximumParserTransitions,
  );

  final int maximumStructuralEntries;
  final int maximumStoragePages;
  final int maximumInlineLeaves;
  final int maximumInlineLeafSourceBytes;
  final int maximumInlineSourceBytes;
  final int maximumFactRecords;
  final int maximumEncodedFrameBytes;
  final int maximumParserTransitions;
}

/// VPB1 transport bounds with a wider public-wrapper frame ceiling.
///
/// This intentionally does not reuse [FlarkV3HostOfferLimits]. Structural and
/// HIO1 publication keep their engine-internal 5,140-byte frame cap, while one
/// VPB1 directory or wrapper frame may occupy the complete 64 KiB FPK3 body
/// allowance.
final class FlarkV3ViewportPresentationOfferLimits {
  FlarkV3ViewportPresentationOfferLimits({
    required this.maximumFrameCount,
    required this.maximumEncodedFrameBytes,
    required this.maximumPacketBytes,
    required this.maximumFrameBytes,
    required this.maximumProgramChildren,
  }) {
    _checkU32(maximumFrameCount, 'maximumFrameCount', positive: true);
    _checkU32(
      maximumEncodedFrameBytes,
      'maximumEncodedFrameBytes',
      positive: true,
    );
    _checkU32(maximumPacketBytes, 'maximumPacketBytes', positive: true);
    _checkU32(maximumFrameBytes, 'maximumFrameBytes', positive: true);
    _checkU32(maximumProgramChildren, 'maximumProgramChildren', positive: true);
    final minimumPacketBytes =
        FlarkV3HostPublicationPacket.wireHeaderBytes +
        FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes +
        maximumFrameBytes;
    if (maximumPacketBytes > productMaximumPacketBytes ||
        maximumFrameBytes > productMaximumFrameBytes ||
        maximumProgramChildren > productMaximumProgramChildren) {
      throw RangeError('VPB1 offer limits exceed product ceilings.');
    }
    if (maximumFrameBytes > maximumEncodedFrameBytes ||
        minimumPacketBytes > maximumPacketBytes) {
      throw RangeError('VPB1 offer limits are internally inconsistent.');
    }
  }

  static const int productMaximumPacketBytes =
      FlarkV3HostPublicationPacket.maximumRawBytes;
  static const int productMaximumFrameBytes =
      FlarkV3HostPublicationPacket.maximumAggregateFrameBytes;
  static const int productMaximumProgramChildren =
      FlarkV3HostOfferLimits.productMaximumProgramChildren;

  final int maximumFrameCount;
  final int maximumEncodedFrameBytes;
  final int maximumPacketBytes;
  final int maximumFrameBytes;
  final int maximumProgramChildren;
}

/// Exact aggregate totals committed by the VPB1 parent envelope.
final class FlarkV3ViewportPresentationEnvelopeMetrics {
  FlarkV3ViewportPresentationEnvelopeMetrics({
    required this.visitedStructuralEntries,
    required this.visitedStoragePages,
    required this.orderedLeafCount,
    required this.inlineSourceBytes,
    required this.factCount,
    required this.transferredNodeCount,
    required this.parserTransitions,
    required this.aggregateEnvelopeDigest256,
  }) {
    for (final (name, value) in [
      ('visitedStructuralEntries', visitedStructuralEntries),
      ('visitedStoragePages', visitedStoragePages),
      ('orderedLeafCount', orderedLeafCount),
      ('inlineSourceBytes', inlineSourceBytes),
      ('factCount', factCount),
      ('transferredNodeCount', transferredNodeCount),
      ('parserTransitions', parserTransitions),
    ]) {
      _checkU32(value, name);
    }
  }

  final int visitedStructuralEntries;
  final int visitedStoragePages;
  final int orderedLeafCount;
  final int inlineSourceBytes;
  final int factCount;
  final int transferredNodeCount;
  final int parserTransitions;
  final FlarkV3ProtocolDigest256 aggregateEnvelopeDigest256;

  void requireBindingAndLimits(
    FlarkV3ViewportPresentationBinding binding,
    FlarkV3ViewportPresentationQueryLimits limits,
  ) {
    if (!_ordinalSpanEqualsU32(
          binding.start.blockOrdinal,
          binding.next.blockOrdinal,
          visitedStructuralEntries,
        ) ||
        visitedStructuralEntries == 0 ||
        visitedStoragePages == 0 ||
        orderedLeafCount > visitedStructuralEntries ||
        visitedStructuralEntries > limits.maximumStructuralEntries ||
        visitedStoragePages > limits.maximumStoragePages ||
        orderedLeafCount > limits.maximumInlineLeaves ||
        inlineSourceBytes > limits.maximumInlineSourceBytes ||
        factCount > limits.maximumFactRecords ||
        parserTransitions > limits.maximumParserTransitions) {
      throw ArgumentError(
        'Viewport-presentation totals disagree with coverage or query limits.',
      );
    }
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationEnvelopeMetrics &&
      other.visitedStructuralEntries == visitedStructuralEntries &&
      other.visitedStoragePages == visitedStoragePages &&
      other.orderedLeafCount == orderedLeafCount &&
      other.inlineSourceBytes == inlineSourceBytes &&
      other.factCount == factCount &&
      other.transferredNodeCount == transferredNodeCount &&
      other.parserTransitions == parserTransitions &&
      other.aggregateEnvelopeDigest256 == aggregateEnvelopeDigest256;

  @override
  int get hashCode => Object.hash(
    visitedStructuralEntries,
    visitedStoragePages,
    orderedLeafCount,
    inlineSourceBytes,
    factCount,
    transferredNodeCount,
    parserTransitions,
    aggregateEnvelopeDigest256,
  );
}

/// Offer for one atomic aggregate viewport page tied to an exact base ACK.
final class FlarkV3ViewportPresentationOfferBegin {
  FlarkV3ViewportPresentationOfferBegin({
    this.schema = supportedSchema,
    this.mode = FlarkV3ViewportPresentationMode.aggregatePage,
    required this.offerId,
    required this.publicationSession,
    required this.baseAck,
    required this.binding,
    required this.envelope,
    required this.queryLimits,
    required this.limits,
  }) {
    if (schema != supportedSchema) {
      throw ArgumentError.value(
        schema,
        'schema',
        'Unsupported viewport-presentation schema.',
      );
    }
    _requireNonZeroId(offerId, 'offerId');
    _requireNonZeroId(publicationSession, 'publicationSession');
    if (publicationSession == baseAck.publicationSession) {
      throw ArgumentError(
        'A viewport publication identity must be fresh from its base.',
      );
    }
    binding.requireBase(baseAck);
    envelope.requireBindingAndLimits(binding, queryLimits);
    if (limits.maximumFrameCount < minimumFrameCount) {
      throw ArgumentError(
        'Viewport presentation exceeds its publication transport limits.',
      );
    }
  }

  static const int supportedSchema = 1;
  static const int minimumFrameCount = 3;

  final int schema;
  final FlarkV3ViewportPresentationMode mode;
  final FlarkV3OfferId offerId;
  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3StructuralAck baseAck;
  final FlarkV3ViewportPresentationBinding binding;
  final FlarkV3ViewportPresentationEnvelopeMetrics envelope;
  final FlarkV3ViewportPresentationQueryLimits queryLimits;
  final FlarkV3ViewportPresentationOfferLimits limits;

  bool hasExactBase(FlarkV3StructuralAck installed) => baseAck == installed;
}

/// VPB1-specific transport totals and aggregate root-stream witness.
final class FlarkV3ViewportPresentationCommitRequest {
  FlarkV3ViewportPresentationCommitRequest({
    required this.offerId,
    required this.actualFrameCount,
    required this.actualEncodedFrameBytes,
    required this.rollingTransportDigest,
    required this.aggregateRootStreamDigest,
  }) {
    _requireNonZeroId(offerId, 'offerId');
    _requireMinimumActualTransportTotals(
      actualFrameCount: actualFrameCount,
      actualEncodedFrameBytes: actualEncodedFrameBytes,
    );
  }

  final FlarkV3OfferId offerId;
  final int actualFrameCount;
  final int actualEncodedFrameBytes;
  final FlarkV3ProtocolDigest128 rollingTransportDigest;
  final FlarkV3ProtocolDigest128 aggregateRootStreamDigest;

  void requireLimits(FlarkV3ViewportPresentationOfferLimits limits) {
    _requireActualTransportTotals(
      actualFrameCount: actualFrameCount,
      actualEncodedFrameBytes: actualEncodedFrameBytes,
      limits: limits,
    );
  }
}

/// Receipt for one installed, non-structural aggregate viewport page.
final class FlarkV3ViewportPresentationAck {
  FlarkV3ViewportPresentationAck({
    required this.publicationSession,
    required this.baseAck,
    required this.binding,
    required this.envelope,
    required this.actualFrameCount,
    required this.actualEncodedFrameBytes,
    required this.aggregateRootStreamDigest,
  }) {
    _requireNonZeroId(publicationSession, 'publicationSession');
    if (publicationSession == baseAck.publicationSession) {
      throw ArgumentError('A viewport ACK cannot alias its structural base.');
    }
    binding.requireBase(baseAck);
    envelope.requireBindingAndLimits(
      binding,
      FlarkV3ViewportPresentationQueryLimits.productMaximum,
    );
    _requireMinimumActualTransportTotals(
      actualFrameCount: actualFrameCount,
      actualEncodedFrameBytes: actualEncodedFrameBytes,
    );
    final expectedFrameCount =
        envelope.orderedLeafCount * 2 +
        envelope.transferredNodeCount +
        FlarkV3ViewportPresentationOfferBegin.minimumFrameCount;
    if (expectedFrameCount > flarkV3TransportV1Maximum ||
        actualFrameCount != expectedFrameCount ||
        actualEncodedFrameBytes >
            FlarkV3ViewportPresentationQueryLimits
                .productMaximumEncodedFrameBytes) {
      throw ArgumentError(
        'Viewport ACK transport totals disagree with its aggregate envelope.',
      );
    }
  }

  final FlarkV3PublicationSessionId publicationSession;
  final FlarkV3StructuralAck baseAck;
  final FlarkV3ViewportPresentationBinding binding;
  final FlarkV3ViewportPresentationEnvelopeMetrics envelope;
  final int actualFrameCount;
  final int actualEncodedFrameBytes;
  final FlarkV3ProtocolDigest128 aggregateRootStreamDigest;

  void requireLimits(FlarkV3ViewportPresentationOfferLimits limits) {
    _requireActualTransportTotals(
      actualFrameCount: actualFrameCount,
      actualEncodedFrameBytes: actualEncodedFrameBytes,
      limits: limits,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportPresentationAck &&
      other.publicationSession == publicationSession &&
      other.baseAck == baseAck &&
      other.binding == binding &&
      other.envelope == envelope &&
      other.actualFrameCount == actualFrameCount &&
      other.actualEncodedFrameBytes == actualEncodedFrameBytes &&
      other.aggregateRootStreamDigest == aggregateRootStreamDigest;

  @override
  int get hashCode => Object.hash(
    publicationSession,
    baseAck,
    binding,
    envelope,
    actualFrameCount,
    actualEncodedFrameBytes,
    aggregateRootStreamDigest,
  );
}

sealed class FlarkV3ViewportPresentationHostPollOutcome {
  const FlarkV3ViewportPresentationHostPollOutcome();
}

final class FlarkV3ViewportPresentationHostPollPending
    extends FlarkV3ViewportPresentationHostPollOutcome {
  const FlarkV3ViewportPresentationHostPollPending();
}

final class FlarkV3ViewportPresentationHostPacketCredit
    extends FlarkV3ViewportPresentationHostPollOutcome {
  FlarkV3ViewportPresentationHostPacketCredit({
    required this.offerId,
    required this.nextFrameOrdinal,
  }) {
    _requireNonZeroId(offerId, 'offerId');
    _checkU32(nextFrameOrdinal, 'nextFrameOrdinal', positive: true);
  }

  final FlarkV3OfferId offerId;
  final int nextFrameOrdinal;
}

final class FlarkV3ViewportPresentationHostCommitted
    extends FlarkV3ViewportPresentationHostPollOutcome {
  const FlarkV3ViewportPresentationHostCommitted(this.ack);

  final FlarkV3ViewportPresentationAck ack;
}

final class FlarkV3ViewportPresentationHostAbortComplete
    extends FlarkV3ViewportPresentationHostPollOutcome {
  FlarkV3ViewportPresentationHostAbortComplete(this.offerId) {
    _requireNonZeroId(offerId, 'offerId');
  }

  final FlarkV3OfferId offerId;
}

final class FlarkV3ViewportPresentationHostClosed
    extends FlarkV3ViewportPresentationHostPollOutcome {
  const FlarkV3ViewportPresentationHostClosed();
}

/// Exact installed page identity and caller-owned copy ceiling.
final class FlarkV3ViewportPresentationQuery {
  FlarkV3ViewportPresentationQuery({
    required this.ack,
    required this.maximumEncodedBytes,
  }) {
    _checkU32(maximumEncodedBytes, 'maximumEncodedBytes', positive: true);
  }

  final FlarkV3ViewportPresentationAck ack;
  final int maximumEncodedBytes;
}

/// Public, parser-authored payload carried by one schema-8 viewport entry.
///
/// These values classify already-authoritative bytes. They never ask Dart to
/// recognize Markdown source.
enum FlarkV3ViewportPresentationPayloadKind {
  inline,
  indentedCode,
  blockQuote,
  bulletList,
  orderedListItem,
  unsupported,
}

/// Whether one schema-8 entry carries authoritative records or an exact
/// fail-closed unsupported certificate.
enum FlarkV3ViewportPresentationPayloadDisposition {
  authoritative,
  unsupported,
}

/// One random-access leaf in a decoded `FLKVP001` aggregate page.
///
/// [payload] is a read-only slice of the single page-owned byte buffer. Its
/// record width has already been checked for [payloadKind], but semantic
/// record decoding remains with the existing kind-specific Dart decoders.
final class FlarkV3ViewportPresentationAggregateEntry {
  const FlarkV3ViewportPresentationAggregateEntry._({
    required this.orderedChildIndex,
    required this.sourceVersion,
    required this.sourceRoot,
    required this.parseGeneration,
    required this.binding,
    required this.globalRowOrdinal,
    required this.recursiveGreenFrameId,
    required this.payloadKind,
    required this.disposition,
    required this.recordCount,
    required this.payloadOffset,
    required this.payload,
    required this.unsupportedReason,
  });

  final int orderedChildIndex;
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3SourceRootId sourceRoot;
  final int parseGeneration;
  final FlarkV3HotInlineSidecarBinding binding;

  /// Present only for schema-10 recursive-Green row payloads.
  final FlarkV3ProtocolU64? globalRowOrdinal;

  /// Exact parser frame key retained losslessly as two protocol words.
  final FlarkV3ProtocolU64? recursiveGreenFrameId;
  final FlarkV3ViewportPresentationPayloadKind payloadKind;
  final FlarkV3ViewportPresentationPayloadDisposition disposition;
  final int recordCount;
  final int payloadOffset;
  final Uint8List payload;

  /// Non-zero only when [disposition] is [FlarkV3ViewportPresentationPayloadDisposition.unsupported].
  final int unsupportedReason;

  int get payloadLength => payload.lengthInBytes;
  bool get isAuthoritative =>
      disposition ==
      FlarkV3ViewportPresentationPayloadDisposition.authoritative;
}

/// A corrupt or ACK-incompatible schema-8 aggregate page.
///
/// This reports a host-boundary/authority failure, never a Markdown parse
/// error.
final class FlarkV3ViewportPresentationPageDecodeException
    implements Exception {
  const FlarkV3ViewportPresentationPageDecodeException(this.message);

  final String message;

  @override
  String toString() =>
      'FlarkV3ViewportPresentationPageDecodeException($message)';
}

/// One owned and fully structurally decoded aggregate-page copy from the host.
///
/// The decoder admits only schema 8 and requires every available header and
/// directory identity to equal [ack] and its exact structural base. It also
/// validates directory order, source geometry, payload tiling, disposition,
/// and kind-specific record widths. It does not interpret Markdown.
///
/// [pageBindingDigest256] is an opaque commitment authored and checked by the
/// Rust host before query delivery. Dart deliberately does not claim to
/// recompute the Rust-private BLAKE3 commitment.
final class FlarkV3ViewportPresentationAggregatePage {
  factory FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes({
    required FlarkV3ViewportPresentationAck ack,
    required Uint8List encodedPage,
  }) {
    // Ownership transfers to this value. Keep the exact buffer so the
    // host-to-Dart boundary remains a single copy.
    final ownedPage = encodedPage;
    final decoded = _decodeViewportPresentationPage(ack, ownedPage);
    return FlarkV3ViewportPresentationAggregatePage._(
      ack: ack,
      wireSchema: decoded.wireSchema,
      encodedPage: ownedPage,
      payloadStart: decoded.payloadStart,
      pageBindingDigest256: decoded.pageBindingDigest256,
      entries: decoded.entries,
    );
  }

  const FlarkV3ViewportPresentationAggregatePage._({
    required this.ack,
    required this.wireSchema,
    required this.encodedPage,
    required this.payloadStart,
    required this.pageBindingDigest256,
    required this.entries,
  });

  static const int schema = 8;
  static const int recursiveGreenSchema = 10;
  static const int headerBytes = 160;
  static const int directoryEntryBytes = 144;
  static const int recursiveGreenDirectoryEntryBytes = 152;

  static const List<int> magicBytes = [
    0x46,
    0x4c,
    0x4b,
    0x56,
    0x50,
    0x30,
    0x30,
    0x31,
  ];

  final FlarkV3ViewportPresentationAck ack;
  final int wireSchema;
  final Uint8List encodedPage;
  final int payloadStart;
  final FlarkV3ProtocolDigest256 pageBindingDigest256;
  final List<FlarkV3ViewportPresentationAggregateEntry> entries;

  int get entryCount => entries.length;
}

final class _DecodedViewportPresentationPage {
  const _DecodedViewportPresentationPage({
    required this.wireSchema,
    required this.payloadStart,
    required this.pageBindingDigest256,
    required this.entries,
  });

  final int payloadStart;
  final int wireSchema;
  final FlarkV3ProtocolDigest256 pageBindingDigest256;
  final List<FlarkV3ViewportPresentationAggregateEntry> entries;
}

_DecodedViewportPresentationPage _decodeViewportPresentationPage(
  FlarkV3ViewportPresentationAck ack,
  Uint8List page,
) {
  const headerBytes = FlarkV3ViewportPresentationAggregatePage.headerBytes;
  if (page.lengthInBytes < headerBytes) {
    _viewportPageFailure('The FLKVP001 header is truncated.');
  }
  for (
    var index = 0;
    index < FlarkV3ViewportPresentationAggregatePage.magicBytes.length;
    index += 1
  ) {
    if (page[index] !=
        FlarkV3ViewportPresentationAggregatePage.magicBytes[index]) {
      _viewportPageFailure('Viewport aggregate page magic is not FLKVP001.');
    }
  }
  if (page.lengthInBytes >
      FlarkV3ViewportPresentationQueryLimits.productMaximumEncodedFrameBytes) {
    _viewportPageFailure('Viewport aggregate page exceeds the product bound.');
  }

  final data = ByteData.sublistView(page);
  final wireSchema = data.getUint32(8, Endian.little);
  final wireHeaderBytes = data.getUint32(12, Endian.little);
  final wireEntryBytes = data.getUint32(16, Endian.little);
  final entryCount = data.getUint32(20, Endian.little);
  final payloadStart = data.getUint32(24, Endian.little);
  final totalBytes = data.getUint32(28, Endian.little);
  final entryBytes = switch (wireSchema) {
    FlarkV3ViewportPresentationAggregatePage.schema =>
      FlarkV3ViewportPresentationAggregatePage.directoryEntryBytes,
    FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema =>
      FlarkV3ViewportPresentationAggregatePage
          .recursiveGreenDirectoryEntryBytes,
    _ => 0,
  };
  if (entryBytes == 0 ||
      wireHeaderBytes != headerBytes ||
      wireEntryBytes != entryBytes) {
    _viewportPageFailure(
      'Viewport aggregate page has an unsupported schema or fixed width.',
    );
  }
  if (entryCount != ack.envelope.orderedLeafCount) {
    _viewportPageFailure(
      'Viewport directory cardinality disagrees with the exact ACK.',
    );
  }
  if (entryCount > (page.lengthInBytes - headerBytes) ~/ entryBytes) {
    _viewportPageFailure('Viewport directory is truncated.');
  }
  final expectedPayloadStart = headerBytes + entryCount * entryBytes;
  if (payloadStart != expectedPayloadStart ||
      totalBytes != page.lengthInBytes ||
      payloadStart > totalBytes) {
    _viewportPageFailure(
      'Viewport page length or payload boundary is non-canonical.',
    );
  }

  _requirePublicationSession(
    data,
    32,
    ack.publicationSession,
    'viewport publication',
  );
  _requirePublicationSession(
    data,
    48,
    ack.baseAck.publicationSession,
    'structural-base publication',
  );
  if (data.getUint32(64, Endian.little) != ack.binding.viewportGeneration) {
    _viewportPageFailure('Viewport generation disagrees with the exact ACK.');
  }
  final completeFlags = data.getUint32(68, Endian.little);
  if (completeFlags != (ack.binding.complete ? 1 : 0)) {
    _viewportPageFailure(
      'Viewport completion flags disagree with the exact ACK.',
    );
  }
  _requireMetricRange(data, 72, ack.binding.requestedRange, 'requested');
  _requireMetricRange(data, 88, ack.binding.coveredRange, 'covered');
  if (data.getUint32(104, Endian.little) != ack.actualFrameCount ||
      data.getUint32(108, Endian.little) != ack.actualEncodedFrameBytes) {
    _viewportPageFailure(
      'Viewport transport totals disagree with the exact ACK.',
    );
  }
  _requireDigest128(
    data,
    112,
    ack.aggregateRootStreamDigest,
    'aggregate root-stream',
  );
  final pageBindingDigest256 = _readDigest256(data, 128);

  final entries = <FlarkV3ViewportPresentationAggregateEntry>[];
  var payloadCursor = payloadStart;
  FlarkV3ProtocolU64? previousBlockOrdinal;
  int? previousPhysicalEndUtf8;
  int? previousPhysicalEndUtf16;
  var inlineSourceBytes = 0;
  var factCount = 0;
  for (var index = 0; index < entryCount; index += 1) {
    final offset = headerBytes + index * entryBytes;
    final orderedChildIndex = data.getUint32(offset, Endian.little);
    if (orderedChildIndex != index) {
      _viewportPageFailure(
        'Viewport directory entries are not in canonical child order.',
      );
    }
    _requireEntryBaseIdentity(data, offset, ack.baseAck);
    _requireZeroBytes(
      page,
      offset + 60,
      offset + 64,
      'viewport entry source reserved bytes',
    );

    final parserProfile = _readU64(data, offset + 64);
    final refinementGeneration = _readU64(data, offset + 72);
    final blockOrdinal = _readU64(data, offset + 80);
    final recursiveGreenFrameId =
        wireSchema ==
            FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema
        ? _readU64(data, offset + 88)
        : null;
    if (!parserProfile.fitsU32 ||
        parserProfile.lowWord != ack.baseAck.syntaxProfile.value ||
        !refinementGeneration.fitsU32 ||
        refinementGeneration.lowWord != ack.binding.viewportGeneration) {
      _viewportPageFailure(
        'Viewport entry profile or refinement generation disagrees with its ACK.',
      );
    }
    if (blockOrdinal.compareTo(ack.binding.start.blockOrdinal) < 0 ||
        blockOrdinal.compareTo(ack.binding.next.blockOrdinal) >= 0 ||
        previousBlockOrdinal != null &&
            blockOrdinal.compareTo(previousBlockOrdinal) <= 0) {
      _viewportPageFailure(
        'Viewport entry block ordinals are outside their ordered cut.',
      );
    }

    final sourceOffset =
        wireSchema ==
            FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema
        ? offset + 96
        : offset + 88;
    final physicalStartUtf8 = data.getUint32(sourceOffset, Endian.little);
    final physicalEndUtf8 = data.getUint32(sourceOffset + 4, Endian.little);
    final visibleStartUtf8 = data.getUint32(sourceOffset + 8, Endian.little);
    final visibleEndUtf8 = data.getUint32(sourceOffset + 12, Endian.little);
    final physicalStartUtf16 = data.getUint32(sourceOffset + 16, Endian.little);
    final physicalEndUtf16 = data.getUint32(sourceOffset + 20, Endian.little);
    final visibleStartUtf16 = data.getUint32(sourceOffset + 24, Endian.little);
    final visibleEndUtf16 = data.getUint32(sourceOffset + 28, Endian.little);
    if (physicalStartUtf8 >= physicalEndUtf8 ||
        visibleStartUtf8 >= visibleEndUtf8 ||
        visibleStartUtf8 < physicalStartUtf8 ||
        visibleEndUtf8 > physicalEndUtf8 ||
        physicalStartUtf16 >= physicalEndUtf16 ||
        visibleStartUtf16 >= visibleEndUtf16 ||
        visibleStartUtf16 < physicalStartUtf16 ||
        visibleEndUtf16 > physicalEndUtf16 ||
        physicalStartUtf8 < ack.binding.coveredRange.startUtf8 ||
        physicalEndUtf8 > ack.binding.coveredRange.endUtf8 ||
        physicalStartUtf16 < ack.binding.coveredRange.startUtf16 ||
        physicalEndUtf16 > ack.binding.coveredRange.endUtf16 ||
        previousPhysicalEndUtf8 != null &&
            physicalStartUtf8 < previousPhysicalEndUtf8 ||
        previousPhysicalEndUtf16 != null &&
            physicalStartUtf16 < previousPhysicalEndUtf16) {
      _viewportPageFailure(
        'Viewport entry source geometry is outside or out of order.',
      );
    }
    final visibleBytes = visibleEndUtf8 - visibleStartUtf8;
    if (visibleBytes <= 0 ||
        visibleBytes >
            FlarkV3ViewportPresentationQueryLimits
                .productMaximumInlineLeafSourceBytes) {
      _viewportPageFailure(
        'Viewport entry visible source exceeds the bounded leaf contract.',
      );
    }

    final binding = FlarkV3HotInlineSidecarBinding(
      parserProfile: ack.baseAck.syntaxProfile,
      refinementGeneration: refinementGeneration,
      blockOrdinal: blockOrdinal,
      physicalStartUtf8: physicalStartUtf8,
      physicalEndUtf8: physicalEndUtf8,
      visibleStartUtf8: visibleStartUtf8,
      visibleEndUtf8: visibleEndUtf8,
      physicalStartUtf16: physicalStartUtf16,
      physicalEndUtf16: physicalEndUtf16,
      visibleStartUtf16: visibleStartUtf16,
      visibleEndUtf16: visibleEndUtf16,
    );
    binding.requireBase(ack.baseAck);

    final payloadMetadataOffset =
        wireSchema ==
            FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema
        ? offset + 128
        : offset + 120;
    final payloadKind = _readViewportPayloadKind(page[payloadMetadataOffset]);
    final disposition = _readViewportPayloadDisposition(
      page[payloadMetadataOffset + 1],
    );
    _requireZeroBytes(
      page,
      payloadMetadataOffset + 2,
      payloadMetadataOffset + 4,
      'viewport entry payload reserved bytes',
    );
    final recordCount = data.getUint32(
      payloadMetadataOffset + 4,
      Endian.little,
    );
    final payloadOffset = data.getUint32(
      payloadMetadataOffset + 8,
      Endian.little,
    );
    final payloadLength = data.getUint32(
      payloadMetadataOffset + 12,
      Endian.little,
    );
    final unsupportedReason = data.getUint32(
      payloadMetadataOffset + 16,
      Endian.little,
    );
    _requireZeroBytes(
      page,
      payloadMetadataOffset + 20,
      payloadMetadataOffset + 24,
      'viewport entry terminal reserved bytes',
    );
    final payloadEnd = payloadOffset + payloadLength;
    if (payloadOffset != payloadCursor ||
        payloadEnd < payloadOffset ||
        payloadEnd > totalBytes) {
      _viewportPageFailure(
        'Viewport payload ranges overlap, contain gaps, or escape the page.',
      );
    }
    final payload = Uint8List.sublistView(
      page,
      payloadOffset,
      payloadEnd,
    ).asUnmodifiableView();
    _requireViewportPayloadShape(
      payloadKind: payloadKind,
      disposition: disposition,
      recordCount: recordCount,
      payload: payload,
      unsupportedReason: unsupportedReason,
    );
    entries.add(
      FlarkV3ViewportPresentationAggregateEntry._(
        orderedChildIndex: orderedChildIndex,
        sourceVersion: ack.baseAck.sourceVersion,
        sourceRoot: ack.baseAck.sourceRoot,
        parseGeneration: ack.baseAck.parseGeneration,
        binding: binding,
        globalRowOrdinal:
            wireSchema ==
                FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema
            ? blockOrdinal
            : null,
        recursiveGreenFrameId: recursiveGreenFrameId,
        payloadKind: payloadKind,
        disposition: disposition,
        recordCount: recordCount,
        payloadOffset: payloadOffset,
        payload: payload,
        unsupportedReason: unsupportedReason,
      ),
    );

    payloadCursor = payloadEnd;
    previousBlockOrdinal = blockOrdinal;
    previousPhysicalEndUtf8 = physicalEndUtf8;
    previousPhysicalEndUtf16 = physicalEndUtf16;
    inlineSourceBytes += visibleBytes;
    factCount += recordCount;
    if (inlineSourceBytes > flarkV3TransportV1Maximum ||
        factCount > flarkV3TransportV1Maximum) {
      _viewportPageFailure('Viewport aggregate counters overflow u32.');
    }
  }
  if (payloadCursor != totalBytes) {
    _viewportPageFailure('Viewport payloads do not exactly tile the page.');
  }
  if (inlineSourceBytes != ack.envelope.inlineSourceBytes ||
      factCount != ack.envelope.factCount) {
    _viewportPageFailure(
      'Viewport entry totals disagree with the exact aggregate ACK.',
    );
  }

  return _DecodedViewportPresentationPage(
    wireSchema: wireSchema,
    payloadStart: payloadStart,
    pageBindingDigest256: pageBindingDigest256,
    entries: List.unmodifiable(entries),
  );
}

void _requireEntryBaseIdentity(
  ByteData data,
  int offset,
  FlarkV3StructuralAck baseAck,
) {
  final source = baseAck.sourceVersion;
  if (data.getUint32(offset + 4, Endian.little) != source.revision) {
    _viewportPageFailure(
      'Viewport entry source revision disagrees with its exact base.',
    );
  }
  _requireDocumentSession(
    data,
    offset + 8,
    source.documentSession,
    'entry source',
  );
  if (data.getUint32(offset + 24, Endian.little) !=
          baseAck.sourceRoot.highWord ||
      data.getUint32(offset + 28, Endian.little) !=
          baseAck.sourceRoot.lowWord ||
      data.getUint32(offset + 32, Endian.little) != source.contentHash.word0 ||
      data.getUint32(offset + 36, Endian.little) != source.contentHash.word1 ||
      data.getUint32(offset + 40, Endian.little) != source.contentHash.word2 ||
      data.getUint32(offset + 44, Endian.little) != source.contentHash.word3 ||
      data.getUint32(offset + 48, Endian.little) != source.metric.bytes ||
      data.getUint32(offset + 52, Endian.little) != source.metric.utf16 ||
      data.getUint32(offset + 56, Endian.little) != baseAck.parseGeneration) {
    _viewportPageFailure(
      'Viewport entry source identity disagrees with its exact base.',
    );
  }
}

FlarkV3ViewportPresentationPayloadKind _readViewportPayloadKind(int code) =>
    switch (code) {
      1 => FlarkV3ViewportPresentationPayloadKind.inline,
      2 => FlarkV3ViewportPresentationPayloadKind.indentedCode,
      3 => FlarkV3ViewportPresentationPayloadKind.blockQuote,
      4 => FlarkV3ViewportPresentationPayloadKind.bulletList,
      6 => FlarkV3ViewportPresentationPayloadKind.orderedListItem,
      0xff => FlarkV3ViewportPresentationPayloadKind.unsupported,
      _ => _viewportPageFailure('Viewport payload kind is unknown.'),
    };

FlarkV3ViewportPresentationPayloadDisposition _readViewportPayloadDisposition(
  int code,
) => switch (code) {
  1 => FlarkV3ViewportPresentationPayloadDisposition.authoritative,
  2 => FlarkV3ViewportPresentationPayloadDisposition.unsupported,
  _ => _viewportPageFailure('Viewport payload disposition is unknown.'),
};

void _requireViewportPayloadShape({
  required FlarkV3ViewportPresentationPayloadKind payloadKind,
  required FlarkV3ViewportPresentationPayloadDisposition disposition,
  required int recordCount,
  required Uint8List payload,
  required int unsupportedReason,
}) {
  final payloadLength = payload.lengthInBytes;
  if (payloadKind == FlarkV3ViewportPresentationPayloadKind.unsupported) {
    if (disposition !=
            FlarkV3ViewportPresentationPayloadDisposition.unsupported ||
        recordCount != 0 ||
        unsupportedReason == 0 ||
        payloadLength > 48) {
      _viewportPageFailure(
        'Viewport unsupported payload has a non-canonical certificate.',
      );
    }
    return;
  }
  if (disposition !=
          FlarkV3ViewportPresentationPayloadDisposition.authoritative ||
      unsupportedReason != 0) {
    _viewportPageFailure(
      'Viewport authoritative payload has an invalid disposition.',
    );
  }
  if (payloadKind == FlarkV3ViewportPresentationPayloadKind.inline) {
    final factBytes = recordCount * 20;
    if (payloadLength == factBytes) {
      return;
    }
    const valueHeaderBytes = 16;
    const maximumValueBytes = 64 * 1024;
    const maximumValueEntries = 2047;
    const valueMagic = <int>[0x46, 0x4c, 0x4b, 0x49, 0x56, 0x30, 0x30, 0x31];
    final valueLength = payloadLength - factBytes;
    if (valueLength < valueHeaderBytes || valueLength > maximumValueBytes) {
      _viewportPageFailure(
        'Viewport inline-value trailer is outside its bounded envelope.',
      );
    }
    for (var index = 0; index < valueMagic.length; index += 1) {
      if (payload[factBytes + index] != valueMagic[index]) {
        _viewportPageFailure(
          'Viewport inline-value trailer magic is not FLKIV001.',
        );
      }
    }
    final values = ByteData.sublistView(payload, factBytes);
    final schema = values.getUint32(8, Endian.little);
    final entryCount = values.getUint32(12, Endian.little);
    if (schema != 1 ||
        entryCount == 0 ||
        entryCount > maximumValueEntries ||
        entryCount > recordCount) {
      _viewportPageFailure(
        'Viewport inline-value trailer has an invalid schema or cardinality.',
      );
    }
    return;
  }
  final expectedLength = switch (payloadKind) {
    FlarkV3ViewportPresentationPayloadKind.inline => -1,
    FlarkV3ViewportPresentationPayloadKind.indentedCode ||
    FlarkV3ViewportPresentationPayloadKind.blockQuote => recordCount * 20,
    FlarkV3ViewportPresentationPayloadKind.bulletList => recordCount * 28,
    FlarkV3ViewportPresentationPayloadKind.orderedListItem =>
      recordCount == 1 ? 48 : -1,
    FlarkV3ViewportPresentationPayloadKind.unsupported => -1,
  };
  if (payloadLength != expectedLength) {
    _viewportPageFailure(
      'Viewport payload length disagrees with its kind-specific records.',
    );
  }
}

void _requireMetricRange(
  ByteData data,
  int offset,
  FlarkV3ViewportPresentationMetricRange expected,
  String label,
) {
  if (data.getUint32(offset, Endian.little) != expected.startUtf8 ||
      data.getUint32(offset + 4, Endian.little) != expected.startUtf16 ||
      data.getUint32(offset + 8, Endian.little) != expected.endUtf8 ||
      data.getUint32(offset + 12, Endian.little) != expected.endUtf16) {
    _viewportPageFailure('Viewport $label range disagrees with the exact ACK.');
  }
}

void _requirePublicationSession(
  ByteData data,
  int offset,
  FlarkV3PublicationSessionId expected,
  String label,
) {
  if (data.getUint32(offset, Endian.little) != expected.word0 ||
      data.getUint32(offset + 4, Endian.little) != expected.word1 ||
      data.getUint32(offset + 8, Endian.little) != expected.word2 ||
      data.getUint32(offset + 12, Endian.little) != expected.word3) {
    _viewportPageFailure(
      'Viewport $label identity disagrees with the exact ACK.',
    );
  }
}

void _requireDocumentSession(
  ByteData data,
  int offset,
  FlarkV3DocumentSessionId expected,
  String label,
) {
  if (data.getUint32(offset, Endian.little) != expected.word0 ||
      data.getUint32(offset + 4, Endian.little) != expected.word1 ||
      data.getUint32(offset + 8, Endian.little) != expected.word2 ||
      data.getUint32(offset + 12, Endian.little) != expected.word3) {
    _viewportPageFailure(
      'Viewport $label identity disagrees with the exact base.',
    );
  }
}

void _requireDigest128(
  ByteData data,
  int offset,
  FlarkV3ProtocolDigest128 expected,
  String label,
) {
  if (data.getUint32(offset, Endian.little) != expected.word0 ||
      data.getUint32(offset + 4, Endian.little) != expected.word1 ||
      data.getUint32(offset + 8, Endian.little) != expected.word2 ||
      data.getUint32(offset + 12, Endian.little) != expected.word3) {
    _viewportPageFailure(
      'Viewport $label digest disagrees with the exact ACK.',
    );
  }
}

FlarkV3ProtocolU64 _readU64(ByteData data, int offset) => FlarkV3ProtocolU64(
  lowWord: data.getUint32(offset, Endian.little),
  highWord: data.getUint32(offset + 4, Endian.little),
);

FlarkV3ProtocolDigest256 _readDigest256(ByteData data, int offset) =>
    FlarkV3ProtocolDigest256(
      data.getUint32(offset, Endian.little),
      data.getUint32(offset + 4, Endian.little),
      data.getUint32(offset + 8, Endian.little),
      data.getUint32(offset + 12, Endian.little),
      data.getUint32(offset + 16, Endian.little),
      data.getUint32(offset + 20, Endian.little),
      data.getUint32(offset + 24, Endian.little),
      data.getUint32(offset + 28, Endian.little),
    );

void _requireZeroBytes(Uint8List bytes, int start, int end, String label) {
  for (var index = start; index < end; index += 1) {
    if (bytes[index] != 0) {
      _viewportPageFailure('$label must be zero.');
    }
  }
}

Never _viewportPageFailure(String message) =>
    throw FlarkV3ViewportPresentationPageDecodeException(message);

sealed class FlarkV3ViewportPresentationQueryOutcome {
  const FlarkV3ViewportPresentationQueryOutcome();
}

final class FlarkV3ViewportPresentationQueryAvailable
    extends FlarkV3ViewportPresentationQueryOutcome {
  const FlarkV3ViewportPresentationQueryAvailable(this.page);

  final FlarkV3ViewportPresentationAggregatePage page;
}

final class FlarkV3ViewportPresentationQueryUnavailable
    extends FlarkV3ViewportPresentationQueryOutcome {
  const FlarkV3ViewportPresentationQueryUnavailable();
}

bool _ordinalSpanEqualsU32(
  FlarkV3ProtocolU64 start,
  FlarkV3ProtocolU64 end,
  int expected,
) {
  if (end.compareTo(start) <= 0) return false;
  final borrow = end.lowWord < start.lowWord ? 1 : 0;
  final lowDifference =
      (end.lowWord - start.lowWord) & flarkV3TransportV1Maximum;
  final highDifference = end.highWord - start.highWord - borrow;
  return highDifference == 0 && lowDifference == expected;
}

void _requireMinimumActualTransportTotals({
  required int actualFrameCount,
  required int actualEncodedFrameBytes,
}) {
  _checkU32(actualFrameCount, 'actualFrameCount');
  _checkU32(actualEncodedFrameBytes, 'actualEncodedFrameBytes', positive: true);
  if (actualFrameCount <
      FlarkV3ViewportPresentationOfferBegin.minimumFrameCount) {
    throw RangeError.range(
      actualFrameCount,
      FlarkV3ViewportPresentationOfferBegin.minimumFrameCount,
      flarkV3TransportV1Maximum,
      'actualFrameCount',
    );
  }
}

void _requireActualTransportTotals({
  required int actualFrameCount,
  required int actualEncodedFrameBytes,
  required FlarkV3ViewportPresentationOfferLimits limits,
}) {
  _requireMinimumActualTransportTotals(
    actualFrameCount: actualFrameCount,
    actualEncodedFrameBytes: actualEncodedFrameBytes,
  );
  if (actualFrameCount > limits.maximumFrameCount ||
      actualEncodedFrameBytes > limits.maximumEncodedFrameBytes) {
    throw RangeError(
      'Viewport-presentation transport totals exceed their admitted limits.',
    );
  }
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
