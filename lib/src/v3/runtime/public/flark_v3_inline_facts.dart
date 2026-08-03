import 'dart:convert';
import 'dart:typed_data';

import '../../host/flark_v3_host_protocol.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_document_query.dart';

/// Selected inline facts exposed by the first authoritative live-render path.
///
/// This is a value taxonomy only. Markdown recognition remains entirely in
/// the parser; Dart never infers a fact from delimiters or source text.
enum FlarkV3InlineFactKind {
  emphasis,
  strong,
  code,
  strikethrough,
  autolinkUri,
  autolinkEmail,

  /// One atomic CommonMark `\X` escape where `X` is ASCII punctuation.
  ///
  /// The source is exactly two UTF-8 bytes, the backslash is the opener, the
  /// punctuation byte is the content, and the closer is collapsed at the end
  /// of the source span. Escapes are leaves and can never contain child facts.
  escapedPunctuation,

  /// One atomic CommonMark hard line break.
  ///
  /// The opener is the parser-certified marker (`\` or two or more spaces),
  /// the content is one physical LF, CR, or CRLF line ending, and the closer is
  /// collapsed at the end of the source span. Hard breaks are leaves and can
  /// never contain child facts.
  hardLineBreak,

  /// One parser-certified CommonMark character reference.
  ///
  /// The complete `&...;` source token is replaced by exactly one or two
  /// Unicode scalar values supplied by Rust. Dart does not decode entity names
  /// or numeric references. Character references are leaves and can never
  /// contain child facts.
  characterReference,

  /// One parser-certified inline link with an inline destination and title.
  ///
  /// [FlarkV3InlineFact.source] covers the complete link syntax while
  /// [FlarkV3InlineFact.content] covers only the label interior. The cooked
  /// destination/title and their exact source cuts come from the separately
  /// authenticated FLKIV001 companion payload.
  directLink,

  /// One parser-certified inline image with an inline destination and title.
  ///
  /// Its content is the exact alt-label interior. Image semantics remain
  /// distinct from actionable link semantics even when the alt label contains
  /// a nested parser-certified link.
  directImage,

  /// One parser-certified reference link resolved against the document's
  /// first winning definition.
  ///
  /// Its fact geometry remains leaf-relative, while its authenticated
  /// destination/title source cuts are document-absolute definition ranges.
  referenceLink,

  /// One parser-certified reference image resolved against the document's
  /// first winning definition.
  ///
  /// Alt-label geometry remains leaf-relative; destination/title source cuts
  /// are document-absolute definition ranges.
  referenceImage,
}

/// Parser-certified semantic kind for one actionable link.
enum FlarkV3InlineLinkKind { uri, email, direct, reference }

/// How a consumer derives the semantic link destination from certified source.
///
/// These recipes are values emitted by the decoder, not recognition rules.
enum FlarkV3InlineLinkTargetRecipe {
  exactContent,

  /// Exact markerless `www.` content with `http://` prepended.
  ///
  /// The prefix is semantic destination data only. It is never inserted into
  /// canonical Markdown or the marker-free display value.
  httpPrefixedExactContent,

  /// Exact autolink content with only its nested parser-certified character
  /// references replaced by their cooked scalar values.
  characterReferenceProjectedContent,

  mailtoExactContent,

  /// Parser-cooked bytes carried by the authenticated FLKIV001 companion.
  companionCookedValue,
}

/// One parser-certified link annotation and its exact semantic destination.
final class FlarkV3InlineLinkAnnotation {
  const FlarkV3InlineLinkAnnotation._({
    required this.kind,
    required this.targetRecipe,
    required this.destination,
    required this.source,
    required this.content,
    required this.destinationSource,
    required this.title,
    required this.titleSource,
  });

  final FlarkV3InlineLinkKind kind;
  final FlarkV3InlineLinkTargetRecipe targetRecipe;

  /// Exact parser-semantic destination.
  ///
  /// This is not an HTML-escaped `href` or a platform-normalized URI. A
  /// consumer must encode or validate it for the context in which it is used.
  final String destination;

  /// Complete parser-certified link source.
  final FlarkV3SourceSpan source;

  /// Visible parser-certified link content.
  ///
  /// For an angle autolink this is the exact source between `<` and `>`. For a
  /// markerless GFM autolink it equals [source]. For a direct or reference
  /// link it is the exact label interior.
  final FlarkV3SourceSpan content;

  /// Exact source cut from which [destination] is derived.
  ///
  /// For angle autolinks this is [content]. For a direct link this lies inside
  /// the complete closing syntax and excludes optional angle delimiters. For
  /// a reference link this is the document-absolute cut in the winning
  /// definition. The email-autolink recipe additionally prefixes `mailto:`.
  final FlarkV3SourceSpan destinationSource;

  /// Parser-cooked direct- or reference-link title.
  ///
  /// Null means no title was present. An empty string means a title was
  /// present but cooked to an empty value.
  final String? title;

  /// Exact complete title source, including its quote/parenthesis delimiters.
  ///
  /// This is null exactly when [title] is null.
  final FlarkV3SourceSpan? titleSource;
}

/// One parser-certified inline-image annotation.
///
/// This type is deliberately distinct from [FlarkV3InlineLinkAnnotation]:
/// an image is not itself an actionable link. A surrounding link may still be
/// actionable, while links nested inside an image alt label are retained as
/// semantic geometry but suppressed as actions by the projection.
final class FlarkV3InlineImageAnnotation {
  const FlarkV3InlineImageAnnotation._({
    required this.destination,
    required this.source,
    required this.content,
    required this.destinationSource,
    required this.title,
    required this.titleSource,
  });

  final String destination;

  /// Complete direct- or reference-image source.
  final FlarkV3SourceSpan source;

  /// Exact alt-label interior.
  final FlarkV3SourceSpan content;

  /// Exact destination source, excluding optional angle delimiters.
  ///
  /// This lies inside [source] for a direct image and inside the winning
  /// document-level definition for a reference image.
  final FlarkV3SourceSpan destinationSource;

  /// Null when absent; empty when a present title cooks to an empty value.
  final String? title;

  /// Complete delimited title source, or null exactly when [title] is null.
  final FlarkV3SourceSpan? titleSource;
}

/// Completeness of one bounded inline leaf.
///
/// [unsupported] is deliberately whole-leaf: consumers must source-paint the
/// complete [FlarkV3InlineFacts.source] range and ignore all semantic styling.
enum FlarkV3InlineFactsDisposition { authoritative, unsupported }

/// One parser-authored inline fact with exact absolute source coordinates.
final class FlarkV3InlineFact {
  const FlarkV3InlineFact._({
    required this.kind,
    required this.source,
    required this.content,
    required this.opener,
    required this.closer,
    required this.linkAnnotation,
    required this.imageAnnotation,
    required this.characterReferenceValue,
    required this.normalizesCodeLineEndings,
    required this.trimsOneCodeEdgeSpace,
  });

  final FlarkV3InlineFactKind kind;

  /// Complete source extent, including opening and closing markers.
  ///
  /// For delimiter-backed atomic facts this contains both their hidden opener
  /// and visible content; there is no closing marker. A character reference
  /// instead replaces this complete range with [characterReferenceValue].
  final FlarkV3SourceSpan source;

  /// Source extent between the opening and closing markers.
  ///
  /// For [FlarkV3InlineFactKind.escapedPunctuation], this is the one-byte `X`
  /// punctuation content. For [FlarkV3InlineFactKind.hardLineBreak], this is
  /// the exact physical line ending. For
  /// [FlarkV3InlineFactKind.characterReference], this equals [source].
  final FlarkV3SourceSpan content;

  /// Opening marker extent.
  ///
  /// For an escaped punctuation fact this is the one-byte backslash. For a
  /// hard line break this is the parser-certified backslash or space marker.
  /// It is collapsed at [source.startUtf16] for a character reference.
  final FlarkV3SourceSpan opener;

  /// Closing marker extent.
  ///
  /// For atomic facts this is empty and collapsed at
  /// [source.endUtf8] / [source.endUtf16].
  final FlarkV3SourceSpan closer;

  /// Parser-certified link semantics, or null for inline style facts.
  final FlarkV3InlineLinkAnnotation? linkAnnotation;

  /// Parser-certified image semantics, or null for non-image facts.
  final FlarkV3InlineImageAnnotation? imageAnnotation;

  /// Parser-authored display value for a CommonMark character reference.
  ///
  /// This is non-null only for [FlarkV3InlineFactKind.characterReference] and
  /// contains exactly one or two Unicode scalar values. Its source spelling is
  /// deliberately not exposed as a decoding recipe.
  final String? characterReferenceValue;

  /// Code content replaces physical line endings with spaces when rendered.
  ///
  /// Always false for non-code facts.
  final bool normalizesCodeLineEndings;

  /// Code content removes one edge space when both edges are spaces and the
  /// content contains a non-space scalar.
  ///
  /// Always false for non-code facts.
  final bool trimsOneCodeEdgeSpace;
}

/// One revision- and profile-bound, whole-leaf inline result.
final class FlarkV3InlineFacts {
  const FlarkV3InlineFacts._({
    required this.sourceVersion,
    required this.profilePartition,
    required this.source,
    required this.disposition,
    required this.facts,
  });

  /// Largest Paragraph source range admitted by the current whole-leaf
  /// presentation envelope.
  ///
  /// Larger Paragraphs remain exact source and structurally queryable, but
  /// require a future windowed inline-facts contract before they can be
  /// marker-free without materializing the whole leaf on the caller isolate.
  static const int maximumWholeLeafSourceBytes = 8 * 1024;

  /// Exact certified source authority from which every fact was derived.
  ///
  /// Retaining the complete value prevents a cached result from being rebound
  /// to another document that happens to have the same revision number.
  final FlarkV3SourceVersion sourceVersion;

  int get sourceRevision => sourceVersion.revision;

  /// Opaque parser profile/cache partition attached to this result.
  ///
  /// The current Rust prototype still calls this a profile partition rather
  /// than a parser-minted syntax-profile capability. Consumers must compare it
  /// for equality and must not infer grammar behavior from the integer.
  final int profilePartition;

  /// Exact bounded leaf for this result.
  final FlarkV3SourceSpan source;

  final FlarkV3InlineFactsDisposition disposition;

  /// Immutable parser-preorder facts. Empty for [unsupported].
  final List<FlarkV3InlineFact> facts;
}

/// Exact authority and bytes for one FLKIV001 inline-value companion.
///
/// The byte format intentionally carries no second copy of source authority.
/// This wrapper binds it to the same source version, profile partition, and
/// bounded leaf as the fixed-width inline fact payload before decoding. Bytes
/// are copied on ingress and egress so a caller cannot mutate authenticated
/// input underneath a decoded result.
final class FlarkV3InlineValuesPayload {
  FlarkV3InlineValuesPayload({
    required this.sourceVersion,
    required this.profilePartition,
    required this.source,
    required Uint8List encodedBytes,
  }) : _encodedBytes = Uint8List.fromList(encodedBytes);

  final FlarkV3SourceVersion sourceVersion;
  final int profilePartition;
  final FlarkV3SourceSpan source;
  final Uint8List _encodedBytes;

  int get encodedByteLength => _encodedBytes.lengthInBytes;

  Uint8List copyEncodedBytes() => Uint8List.fromList(_encodedBytes);
}

/// A corrupt, stale, or incompatible bounded inline result.
///
/// This is a transport/authority failure, not a Markdown parse error.
final class FlarkV3InlineFactsDecodeException implements Exception {
  const FlarkV3InlineFactsDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3InlineFactsDecodeException($message)';
}

/// Package-internal decoder for canonical schema-2 20-byte inline fact records.
///
/// The future host query should pass its already-authoritative bounded bytes
/// and metadata directly to this decoder. This type intentionally defines no
/// host query envelope and performs no Markdown parsing or delimiter
/// inference. Kinds 7 through 13 are additive uses of the existing kind byte
/// and record layout; the grammar revision partitions their semantics without
/// changing the IFO2/IFP2 or FLKIN002/FLKIP002 schema families. Kind 9 is the
/// one tagged-union case: byte 1 is a scalar count and the final two u32 words
/// carry parser-authored Unicode scalar values rather than content geometry.
/// Kinds 10 through 13 retain ordinary geometry and join by raw fact ordinal
/// to the separately authority-bound FLKIV001 value companion. The fact kind
/// is also the sole coordinate-basis authority: direct-value cuts are
/// leaf-relative, while reference-value cuts are document-absolute.
final class FlarkV3InlineFactsDecoder {
  const FlarkV3InlineFactsDecoder._();

  static const int recordBytes = 20;
  static const int maximumLeafBytes =
      FlarkV3InlineFacts.maximumWholeLeafSourceBytes;
  static const int maximumFactCount = 16 * 1024;
  static const int maximumEncodedValueBytes = 64 * 1024;
  static const int maximumValueEntryCount = 2047;

  /// Validates and translates one complete bounded leaf result.
  ///
  /// [expectedSource] is the exact source authority currently owned by the
  /// caller. [factSource] is the authority attached to the parser result.
  /// Equality includes document session, revision, source metrics, and content
  /// hash; a merely matching revision is insufficient. [sourceDocument] is the
  /// certified Dart source substrate bound to [expectedSource], and therefore
  /// supplies authoritative UTF-8 to UTF-16 coordinate conversion.
  static FlarkV3InlineFacts decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceVersion factSource,
    required int expectedProfilePartition,
    required int profilePartition,
    required FlarkV3SourceSpan expectedLeaf,
    required FlarkV3SourceSpan factLeaf,
    required FlarkV3InlineFactsDisposition disposition,
    required int factCount,
    required Uint8List encodedFacts,
    FlarkV3InlineValuesPayload? inlineValues,
  }) {
    _validateSourceDocument(sourceDocument, expectedSource);
    if (factSource != expectedSource) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline facts do not match the current exact source authority.',
      );
    }
    if (!_sameSpan(expectedLeaf, factLeaf)) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline facts do not match the requested exact leaf.',
      );
    }
    _requireU32(expectedProfilePartition, 'expected profile partition');
    _requireU32(profilePartition, 'profile partition');
    if (profilePartition != expectedProfilePartition) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline facts do not match the configured profile partition.',
      );
    }
    final mapper = _Utf8SpanMapper(
      sourceDocument.utf8ToUtf16,
      sourceLimitUtf8: factSource.metric.bytes,
      sourceLimitUtf16: factSource.metric.utf16,
    );
    _validateLeafSource(factLeaf, factSource: factSource, mapper: mapper);
    if (factCount < 0 || factCount > maximumFactCount) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline fact count exceeds the bounded schema.',
      );
    }
    final expectedBytes = factCount * recordBytes;
    if (encodedFacts.lengthInBytes != expectedBytes) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline fact count does not match the canonical record bytes.',
      );
    }
    if (disposition == FlarkV3InlineFactsDisposition.unsupported &&
        factCount != 0) {
      throw const FlarkV3InlineFactsDecodeException(
        'A whole-leaf unsupported result cannot expose partial facts.',
      );
    }

    final data = ByteData.sublistView(encodedFacts);
    final records = <_InlineRecord>[];
    for (var index = 0; index < factCount; index += 1) {
      records.add(
        _decodeRecord(
          data,
          index * recordBytes,
          leafBytes: factLeaf.endUtf8 - factLeaf.startUtf8,
        ),
      );
    }
    _validatePreorderNesting(records);
    final inlineValuesByOrdinal = _decodeInlineValuesPayload(
      inlineValues,
      records: records,
      sourceVersion: factSource,
      profilePartition: profilePartition,
      leaf: factLeaf,
      mapper: mapper,
    );

    final facts = <FlarkV3InlineFact>[];
    for (var recordIndex = 0; recordIndex < records.length; recordIndex += 1) {
      final record = records[recordIndex];
      final factStart = factLeaf.startUtf8 + record.start;
      final factEnd = factLeaf.startUtf8 + record.end;
      final contentStart = factLeaf.startUtf8 + record.contentStart;
      final contentEnd = factLeaf.startUtf8 + record.contentEnd;
      final source = mapper.span(factStart, factEnd, 'fact source');
      final content = mapper.span(contentStart, contentEnd, 'fact content');
      final opener = mapper.span(factStart, contentStart, 'fact opener');
      final closer = mapper.span(contentEnd, factEnd, 'fact closer');
      final inlineValue = inlineValuesByOrdinal[recordIndex];
      final markerlessAutolink = _usesMarkerlessAutolinkGeometry(record);
      final linkKind = switch (record.kind) {
        FlarkV3InlineFactKind.autolinkUri => FlarkV3InlineLinkKind.uri,
        FlarkV3InlineFactKind.autolinkEmail => FlarkV3InlineLinkKind.email,
        FlarkV3InlineFactKind.directLink => FlarkV3InlineLinkKind.direct,
        FlarkV3InlineFactKind.referenceLink => FlarkV3InlineLinkKind.reference,
        _ => null,
      };
      final nestedCharacterReferences =
          record.kind == FlarkV3InlineFactKind.autolinkUri &&
              !markerlessAutolink
          ? _uriCharacterReferenceChildren(records, parentIndex: recordIndex)
          : const <_InlineRecord>[];
      final linkTargetRecipe = switch (record.kind) {
        FlarkV3InlineFactKind.autolinkUri =>
          markerlessAutolink && (record.flags & _autolinkUriWww) != 0
              ? FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent
              : nestedCharacterReferences.isEmpty
              ? FlarkV3InlineLinkTargetRecipe.exactContent
              : FlarkV3InlineLinkTargetRecipe
                    .characterReferenceProjectedContent,
        FlarkV3InlineFactKind.autolinkEmail =>
          FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
        FlarkV3InlineFactKind.directLink =>
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        FlarkV3InlineFactKind.referenceLink =>
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        _ => null,
      };
      final contentSource =
          record.kind != FlarkV3InlineFactKind.autolinkUri &&
              record.kind != FlarkV3InlineFactKind.autolinkEmail
          ? null
          : sourceDocument.readRange(content.startUtf16, content.endUtf16);
      final semanticLinkContent =
          linkTargetRecipe ==
              FlarkV3InlineLinkTargetRecipe.characterReferenceProjectedContent
          ? _projectCharacterReferences(
              contentSource: contentSource!,
              mapper: mapper,
              leafStartUtf8: factLeaf.startUtf8,
              content: content,
              records: nestedCharacterReferences,
            )
          : contentSource;
      facts.add(
        FlarkV3InlineFact._(
          kind: record.kind,
          source: source,
          content: content,
          opener: opener,
          closer: closer,
          linkAnnotation: linkKind == null
              ? null
              : FlarkV3InlineLinkAnnotation._(
                  kind: linkKind,
                  targetRecipe: linkTargetRecipe!,
                  destination:
                      inlineValue?.cookedDestination ??
                      (linkTargetRecipe ==
                              FlarkV3InlineLinkTargetRecipe.mailtoExactContent
                          ? 'mailto:${semanticLinkContent!}'
                          : linkTargetRecipe ==
                                FlarkV3InlineLinkTargetRecipe
                                    .httpPrefixedExactContent
                          ? 'http://${semanticLinkContent!}'
                          : semanticLinkContent!),
                  source: source,
                  content: content,
                  destinationSource: inlineValue?.destinationSource ?? content,
                  title: inlineValue?.cookedTitle,
                  titleSource: inlineValue?.titleSource,
                ),
          imageAnnotation:
              record.kind == FlarkV3InlineFactKind.directImage ||
                  record.kind == FlarkV3InlineFactKind.referenceImage
              ? FlarkV3InlineImageAnnotation._(
                  destination: inlineValue!.cookedDestination,
                  source: source,
                  content: content,
                  destinationSource: inlineValue.destinationSource,
                  title: inlineValue.cookedTitle,
                  titleSource: inlineValue.titleSource,
                )
              : null,
          characterReferenceValue: record.characterReferenceValue,
          normalizesCodeLineEndings:
              record.kind == FlarkV3InlineFactKind.code &&
              (record.flags & _codeNormalizeLineEndings) != 0,
          trimsOneCodeEdgeSpace:
              record.kind == FlarkV3InlineFactKind.code &&
              (record.flags & _codeTrimOneEdgeSpace) != 0,
        ),
      );
    }

    return FlarkV3InlineFacts._(
      sourceVersion: factSource,
      profilePartition: profilePartition,
      source: factLeaf,
      disposition: disposition,
      facts: List.unmodifiable(facts),
    );
  }
}

/// One UTF-8/UTF-16 span in parser-certified projected text.
///
/// These coordinates are relative to a marker-free container projection and
/// are deliberately not [FlarkV3SourceSpan]s. They cannot be used as physical
/// document offsets until a consumer composes them through the container's
/// independently certified source projection.
final class FlarkV3ProjectedInlineSpan {
  const FlarkV3ProjectedInlineSpan({
    required this.startUtf8,
    required this.endUtf8,
    required this.startUtf16,
    required this.endUtf16,
  }) : assert(startUtf8 >= 0),
       assert(endUtf8 >= startUtf8),
       assert(startUtf16 >= 0),
       assert(endUtf16 >= startUtf16);

  final int startUtf8;
  final int endUtf8;
  final int startUtf16;
  final int endUtf16;

  int get lengthUtf8 => endUtf8 - startUtf8;
  int get lengthUtf16 => endUtf16 - startUtf16;
  bool get isCollapsed => startUtf8 == endUtf8;
}

/// One parser-authored inline fact in projected-container coordinates.
final class FlarkV3ProjectedInlineFact {
  const FlarkV3ProjectedInlineFact._({
    required this.kind,
    required this.source,
    required this.content,
    required this.opener,
    required this.closer,
    required this.characterReferenceValue,
    required this.normalizesCodeLineEndings,
    required this.trimsOneCodeEdgeSpace,
  });

  final FlarkV3InlineFactKind kind;
  final FlarkV3ProjectedInlineSpan source;
  final FlarkV3ProjectedInlineSpan content;
  final FlarkV3ProjectedInlineSpan opener;
  final FlarkV3ProjectedInlineSpan closer;
  final String? characterReferenceValue;
  final bool normalizesCodeLineEndings;
  final bool trimsOneCodeEdgeSpace;
}

/// Completeness of one projected-coordinate inline result.
enum FlarkV3ProjectedInlineFactsDisposition { authoritative, unsupported }

/// Exact projected-inline authority for one physical container.
///
/// [physicalSource] binds this value to the quote certificate that supplied
/// the marker-free coordinate space. Every fact range is relative to
/// [projectedSource], never to [physicalSource].
final class FlarkV3ProjectedInlineFacts {
  const FlarkV3ProjectedInlineFacts._({
    required this.sourceVersion,
    required this.profilePartition,
    required this.physicalSource,
    required this.projectedSource,
    required this.disposition,
    required this.facts,
  });

  final FlarkV3SourceVersion sourceVersion;
  int get sourceRevision => sourceVersion.revision;
  final int profilePartition;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3ProjectedInlineSpan projectedSource;
  final FlarkV3ProjectedInlineFactsDisposition disposition;
  final List<FlarkV3ProjectedInlineFact> facts;

  int get projectedUtf8Length => projectedSource.endUtf8;
  int get projectedUtf16Length => projectedSource.endUtf16;
}

/// A corrupt or authority-incompatible projected-inline result.
final class FlarkV3ProjectedInlineFactsDecodeException implements Exception {
  const FlarkV3ProjectedInlineFactsDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3ProjectedInlineFactsDecodeException($message)';
}

/// Decoder for the existing 20-byte inline record schema in projected space.
///
/// The record grammar is shared with [FlarkV3InlineFactsDecoder], but its
/// offsets are interpreted against [projectedText]. Direct/reference
/// link-value records are rejected in this first lane because their companion
/// cuts do not yet have a projected-coordinate contract; native must publish
/// the whole result as unsupported instead of exposing partial semantics.
final class FlarkV3ProjectedInlineFactsDecoder {
  const FlarkV3ProjectedInlineFactsDecoder._();

  static const int recordBytes = FlarkV3InlineFactsDecoder.recordBytes;
  static const int maximumProjectedBytes =
      FlarkV3InlineFacts.maximumWholeLeafSourceBytes;
  static const int maximumFactCount =
      FlarkV3InlineFactsDecoder.maximumFactCount;

  static FlarkV3ProjectedInlineFacts decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceVersion factSource,
    required int expectedProfilePartition,
    required int profilePartition,
    required FlarkV3SourceSpan expectedPhysicalSource,
    required FlarkV3SourceSpan factPhysicalSource,
    required int expectedProjectedUtf8Length,
    required int expectedProjectedUtf16Length,
    required String projectedText,
    required FlarkV3ProjectedInlineFactsDisposition disposition,
    required int factCount,
    required Uint8List encodedFacts,
  }) {
    if (!sourceDocument.hasCertifiedFacts ||
        sourceDocument.revision != expectedSource.revision ||
        sourceDocument.utf8Length != expectedSource.metric.bytes ||
        sourceDocument.utf16Length != expectedSource.metric.utf16 ||
        sourceDocument.contentHash128 != expectedSource.contentHash ||
        factSource != expectedSource) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline facts do not match exact source authority.',
      );
    }
    if (!_sameSpan(expectedPhysicalSource, factPhysicalSource) ||
        factPhysicalSource.startUtf8 < 0 ||
        factPhysicalSource.endUtf8 <= factPhysicalSource.startUtf8 ||
        factPhysicalSource.endUtf8 > factSource.metric.bytes ||
        factPhysicalSource.startUtf16 < 0 ||
        factPhysicalSource.endUtf16 <= factPhysicalSource.startUtf16 ||
        factPhysicalSource.endUtf16 > factSource.metric.utf16) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline facts do not match their physical container.',
      );
    }
    try {
      if (sourceDocument.utf8ToUtf16(factPhysicalSource.startUtf8) !=
              factPhysicalSource.startUtf16 ||
          sourceDocument.utf8ToUtf16(factPhysicalSource.endUtf8) !=
              factPhysicalSource.endUtf16) {
        throw const FlarkV3ProjectedInlineFactsDecodeException(
          'Projected inline container coordinates are not exact.',
        );
      }
    } on FlarkV3ProjectedInlineFactsDecodeException {
      rethrow;
    } on Object {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline container is not on exact UTF-8 boundaries.',
      );
    }
    _requireProjectedU32(
      expectedProfilePartition,
      'expected profile partition',
    );
    _requireProjectedU32(profilePartition, 'profile partition');
    if (profilePartition != expectedProfilePartition) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline facts do not match the parser profile.',
      );
    }
    if (expectedProjectedUtf8Length <= 0 ||
        expectedProjectedUtf8Length > maximumProjectedBytes ||
        expectedProjectedUtf16Length <= 0 ||
        expectedProjectedUtf16Length > maximumProjectedBytes) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline metrics exceed the bounded lane.',
      );
    }
    final mapper = _ProjectedUtf8SpanMapper(
      projectedText,
      expectedUtf8Length: expectedProjectedUtf8Length,
      expectedUtf16Length: expectedProjectedUtf16Length,
    );
    if (factCount < 0 || factCount > maximumFactCount) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline fact count exceeds the bounded schema.',
      );
    }
    if (encodedFacts.lengthInBytes != factCount * recordBytes) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected inline fact count does not match its record bytes.',
      );
    }
    if (disposition == FlarkV3ProjectedInlineFactsDisposition.unsupported &&
        factCount != 0) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'An unsupported projected leaf cannot expose partial facts.',
      );
    }

    try {
      final data = ByteData.sublistView(encodedFacts);
      final records = <_InlineRecord>[
        for (var index = 0; index < factCount; index += 1)
          _decodeRecord(
            data,
            index * recordBytes,
            leafBytes: expectedProjectedUtf8Length,
          ),
      ];
      _validatePreorderNesting(records);
      if (records.any((record) => _hasLinkValueCompanion(record.kind))) {
        throw const FlarkV3ProjectedInlineFactsDecodeException(
          'Projected link/image facts require an unsupported value companion.',
        );
      }

      final facts = <FlarkV3ProjectedInlineFact>[];
      for (final record in records) {
        final source = mapper.span(record.start, record.end, 'fact source');
        final content = mapper.span(
          record.contentStart,
          record.contentEnd,
          'fact content',
        );
        facts.add(
          FlarkV3ProjectedInlineFact._(
            kind: record.kind,
            source: source,
            content: content,
            opener: mapper.span(
              record.start,
              record.contentStart,
              'fact opener',
            ),
            closer: mapper.span(record.contentEnd, record.end, 'fact closer'),
            characterReferenceValue: record.characterReferenceValue,
            normalizesCodeLineEndings:
                record.kind == FlarkV3InlineFactKind.code &&
                (record.flags & _codeNormalizeLineEndings) != 0,
            trimsOneCodeEdgeSpace:
                record.kind == FlarkV3InlineFactKind.code &&
                (record.flags & _codeTrimOneEdgeSpace) != 0,
          ),
        );
      }
      return FlarkV3ProjectedInlineFacts._(
        sourceVersion: factSource,
        profilePartition: profilePartition,
        physicalSource: factPhysicalSource,
        projectedSource: FlarkV3ProjectedInlineSpan(
          startUtf8: 0,
          endUtf8: expectedProjectedUtf8Length,
          startUtf16: 0,
          endUtf16: expectedProjectedUtf16Length,
        ),
        disposition: disposition,
        facts: List.unmodifiable(facts),
      );
    } on FlarkV3ProjectedInlineFactsDecodeException {
      rethrow;
    } on FlarkV3InlineFactsDecodeException catch (error) {
      throw FlarkV3ProjectedInlineFactsDecodeException(error.message);
    }
  }
}

void _requireProjectedU32(int value, String name) {
  if (value < 0 || value > _u32Maximum) {
    throw FlarkV3ProjectedInlineFactsDecodeException('$name is outside u32.');
  }
}

final class _ProjectedUtf8SpanMapper {
  _ProjectedUtf8SpanMapper(
    String text, {
    required int expectedUtf8Length,
    required int expectedUtf16Length,
  }) : _utf8Length = expectedUtf8Length {
    var utf8Offset = 0;
    var utf16Offset = 0;
    _boundaries[0] = 0;
    for (final scalar in text.runes) {
      utf8Offset += switch (scalar) {
        <= 0x7F => 1,
        <= 0x7FF => 2,
        <= 0xFFFF => 3,
        _ => 4,
      };
      utf16Offset += scalar <= 0xFFFF ? 1 : 2;
      _boundaries[utf8Offset] = utf16Offset;
    }
    if (utf8Offset != expectedUtf8Length ||
        utf16Offset != expectedUtf16Length ||
        text.length != expectedUtf16Length) {
      throw const FlarkV3ProjectedInlineFactsDecodeException(
        'Projected text disagrees with parser-certified metrics.',
      );
    }
  }

  final int _utf8Length;
  final Map<int, int> _boundaries = <int, int>{};

  FlarkV3ProjectedInlineSpan span(int startUtf8, int endUtf8, String name) {
    if (startUtf8 < 0 || endUtf8 < startUtf8 || endUtf8 > _utf8Length) {
      throw FlarkV3ProjectedInlineFactsDecodeException(
        '$name is outside projected text.',
      );
    }
    final startUtf16 = _boundaries[startUtf8];
    final endUtf16 = _boundaries[endUtf8];
    if (startUtf16 == null || endUtf16 == null) {
      throw FlarkV3ProjectedInlineFactsDecodeException(
        '$name is not on projected UTF-8 scalar boundaries.',
      );
    }
    return FlarkV3ProjectedInlineSpan(
      startUtf8: startUtf8,
      endUtf8: endUtf8,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
    );
  }
}

List<_InlineRecord> _uriCharacterReferenceChildren(
  List<_InlineRecord> records, {
  required int parentIndex,
}) {
  final parent = records[parentIndex];
  List<_InlineRecord>? children;
  for (
    var index = parentIndex + 1;
    index < records.length && records[index].start < parent.end;
    index += 1
  ) {
    final candidate = records[index];
    if (candidate.kind == FlarkV3InlineFactKind.characterReference &&
        parent.contentStart <= candidate.start &&
        candidate.end <= parent.contentEnd) {
      (children ??= <_InlineRecord>[]).add(candidate);
    }
  }
  return children ?? const <_InlineRecord>[];
}

String _projectCharacterReferences({
  required String contentSource,
  required _Utf8SpanMapper mapper,
  required int leafStartUtf8,
  required FlarkV3SourceSpan content,
  required List<_InlineRecord> records,
}) {
  final output = StringBuffer();
  var sourceCursorUtf16 = 0;
  for (final record in records) {
    final replacement = record.characterReferenceValue;
    if (replacement == null) {
      throw const FlarkV3InlineFactsDecodeException(
        'Autolink replacement is not a character reference.',
      );
    }
    final span = mapper.span(
      leafStartUtf8 + record.start,
      leafStartUtf8 + record.end,
      'autolink character reference',
    );
    final relativeStartUtf16 = span.startUtf16 - content.startUtf16;
    final relativeEndUtf16 = span.endUtf16 - content.startUtf16;
    if (relativeStartUtf16 < sourceCursorUtf16 ||
        span.endUtf16 > content.endUtf16) {
      throw const FlarkV3InlineFactsDecodeException(
        'Autolink character references are not source ordered.',
      );
    }
    output
      ..write(contentSource.substring(sourceCursorUtf16, relativeStartUtf16))
      ..write(replacement);
    sourceCursorUtf16 = relativeEndUtf16;
  }
  output.write(contentSource.substring(sourceCursorUtf16));
  return output.toString();
}

Map<int, _InlineValue> _decodeInlineValuesPayload(
  FlarkV3InlineValuesPayload? payload, {
  required List<_InlineRecord> records,
  required FlarkV3SourceVersion sourceVersion,
  required int profilePartition,
  required FlarkV3SourceSpan leaf,
  required _Utf8SpanMapper mapper,
}) {
  final expectedParentOrdinals = <int>[
    for (var ordinal = 0; ordinal < records.length; ordinal += 1)
      if (_hasLinkValueCompanion(records[ordinal].kind)) ordinal,
  ];
  if (payload == null) {
    if (expectedParentOrdinals.isNotEmpty) {
      throw const FlarkV3InlineFactsDecodeException(
        'Link/image inline facts are missing their value companion.',
      );
    }
    return const <int, _InlineValue>{};
  }
  if (payload.sourceVersion != sourceVersion ||
      payload.profilePartition != profilePartition ||
      !_sameSpan(payload.source, leaf)) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline values do not match the inline-fact source authority.',
    );
  }

  final encoded = payload._encodedBytes;
  if (encoded.lengthInBytes < _inlineValuesHeaderBytes ||
      encoded.lengthInBytes >
          FlarkV3InlineFactsDecoder.maximumEncodedValueBytes) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline-value payload is outside its bounded byte envelope.',
    );
  }
  for (var index = 0; index < _inlineValuesMagic.length; index += 1) {
    if (encoded[index] != _inlineValuesMagic[index]) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline-value payload magic is invalid.',
      );
    }
  }
  final data = ByteData.sublistView(encoded);
  if (data.getUint32(8, Endian.little) != _inlineValuesSchema) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline-value payload schema is unsupported.',
    );
  }
  final entryCount = data.getUint32(12, Endian.little);
  if (entryCount > FlarkV3InlineFactsDecoder.maximumValueEntryCount ||
      entryCount > records.length) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline-value entry count exceeds the bounded query.',
    );
  }
  if (entryCount != expectedParentOrdinals.length) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline values do not join one-to-one with link/image facts.',
    );
  }

  final values = <int, _InlineValue>{};
  var offset = _inlineValuesHeaderBytes;
  var previousParentOrdinal = -1;
  for (var entryIndex = 0; entryIndex < entryCount; entryIndex += 1) {
    if (encoded.lengthInBytes - offset < _inlineValueEntryHeaderBytes) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline-value entry header is truncated.',
      );
    }
    final parentFactOrdinal = data.getUint32(offset, Endian.little);
    final flags = data.getUint32(offset + 4, Endian.little);
    final destinationStart = data.getUint32(offset + 8, Endian.little);
    final destinationLength = data.getUint32(offset + 12, Endian.little);
    final titleStart = data.getUint32(offset + 16, Endian.little);
    final titleLength = data.getUint32(offset + 20, Endian.little);
    final cookedDestinationLength = data.getUint32(offset + 24, Endian.little);
    final cookedTitleLength = data.getUint32(offset + 28, Endian.little);
    offset += _inlineValueEntryHeaderBytes;

    if (parentFactOrdinal <= previousParentOrdinal ||
        parentFactOrdinal != expectedParentOrdinals[entryIndex] ||
        parentFactOrdinal >= records.length ||
        !_hasLinkValueCompanion(records[parentFactOrdinal].kind)) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline-value parent ordinals are not a canonical exact join.',
      );
    }
    previousParentOrdinal = parentFactOrdinal;
    if ((flags & ~_inlineValueTitlePresent) != 0) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline-value flags are not canonical.',
      );
    }

    final titlePresent = (flags & _inlineValueTitlePresent) != 0;
    if (!titlePresent &&
        (titleStart != 0 || titleLength != 0 || cookedTitleLength != 0)) {
      throw const FlarkV3InlineFactsDecodeException(
        'An absent inline title carries nonzero metadata.',
      );
    }
    if (titlePresent && titleLength == 0) {
      throw const FlarkV3InlineFactsDecodeException(
        'A present inline title has no delimited source.',
      );
    }

    final parent = records[parentFactOrdinal];
    final usesDocumentAbsoluteCuts = _isReferenceValueFactKind(parent.kind);
    final destinationEnd = _checkedEnd(
      destinationStart,
      destinationLength,
      'inline destination',
    );
    final titleEnd = titlePresent
        ? _checkedEnd(titleStart, titleLength, 'inline title')
        : 0;
    if (usesDocumentAbsoluteCuts) {
      if (destinationEnd > sourceVersion.metric.bytes ||
          (titlePresent &&
              (titleEnd > sourceVersion.metric.bytes ||
                  destinationEnd > titleStart))) {
        throw const FlarkV3InlineFactsDecodeException(
          'Reference inline-value source cuts escape exact document source.',
        );
      }
    } else if (destinationStart < parent.contentEnd ||
        destinationEnd > parent.end ||
        (titlePresent &&
            (titleStart < parent.contentEnd ||
                titleEnd > parent.end ||
                destinationEnd > titleStart))) {
      throw const FlarkV3InlineFactsDecodeException(
        'Direct inline-value source cuts escape their parent closing syntax.',
      );
    }

    final cookedEnd = offset + cookedDestinationLength + cookedTitleLength;
    if (cookedEnd < offset || cookedEnd > encoded.lengthInBytes) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline-value cooked bytes are truncated.',
      );
    }
    final cookedDestination = _decodeStrictUtf8(
      encoded,
      offset,
      offset + cookedDestinationLength,
      'inline destination',
    );
    offset += cookedDestinationLength;
    final cookedTitle = titlePresent
        ? _decodeStrictUtf8(
            encoded,
            offset,
            offset + cookedTitleLength,
            'inline title',
          )
        : null;
    offset += cookedTitleLength;

    values[parentFactOrdinal] = _InlineValue(
      destinationSource: mapper.span(
        (usesDocumentAbsoluteCuts ? 0 : leaf.startUtf8) + destinationStart,
        (usesDocumentAbsoluteCuts ? 0 : leaf.startUtf8) + destinationEnd,
        'inline destination',
      ),
      titleSource: titlePresent
          ? mapper.span(
              (usesDocumentAbsoluteCuts ? 0 : leaf.startUtf8) + titleStart,
              (usesDocumentAbsoluteCuts ? 0 : leaf.startUtf8) + titleEnd,
              'inline title',
            )
          : null,
      cookedDestination: cookedDestination,
      cookedTitle: cookedTitle,
    );
  }
  if (offset != encoded.lengthInBytes) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline-value payload has trailing bytes.',
    );
  }
  return Map<int, _InlineValue>.unmodifiable(values);
}

String _decodeStrictUtf8(Uint8List encoded, int start, int end, String name) {
  try {
    return utf8.decode(
      Uint8List.sublistView(encoded, start, end),
      allowMalformed: false,
    );
  } on FormatException {
    throw FlarkV3InlineFactsDecodeException(
      '$name cooked bytes are not valid UTF-8.',
    );
  }
}

void _validateSourceDocument(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3SourceVersion expectedSource,
) {
  if (!sourceDocument.hasCertifiedFacts ||
      sourceDocument.revision != expectedSource.revision ||
      sourceDocument.utf8Length != expectedSource.metric.bytes ||
      sourceDocument.utf16Length != expectedSource.metric.utf16 ||
      sourceDocument.contentHash128 != expectedSource.contentHash) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline coordinate authority does not match the current exact source.',
    );
  }
}

_InlineRecord _decodeRecord(
  ByteData data,
  int offset, {
  required int leafBytes,
}) {
  final kind = switch (data.getUint8(offset)) {
    1 => FlarkV3InlineFactKind.emphasis,
    2 => FlarkV3InlineFactKind.strong,
    3 => FlarkV3InlineFactKind.code,
    4 => FlarkV3InlineFactKind.strikethrough,
    5 => FlarkV3InlineFactKind.autolinkUri,
    6 => FlarkV3InlineFactKind.autolinkEmail,
    7 => FlarkV3InlineFactKind.escapedPunctuation,
    8 => FlarkV3InlineFactKind.hardLineBreak,
    9 => FlarkV3InlineFactKind.characterReference,
    10 => FlarkV3InlineFactKind.directLink,
    11 => FlarkV3InlineFactKind.directImage,
    12 => FlarkV3InlineFactKind.referenceLink,
    13 => FlarkV3InlineFactKind.referenceImage,
    _ => throw const FlarkV3InlineFactsDecodeException(
      'Inline fact kind is unknown.',
    ),
  };
  final flags = data.getUint8(offset + 1);
  if (data.getUint16(offset + 2, Endian.little) != 0) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline fact reserved bits are nonzero.',
    );
  }
  final flagsAreCanonical = switch (kind) {
    FlarkV3InlineFactKind.code => (flags & ~_knownCodeFlags) == 0,
    FlarkV3InlineFactKind.autolinkUri => (flags & ~_knownAutolinkUriFlags) == 0,
    FlarkV3InlineFactKind.autolinkEmail => flags == 0,
    FlarkV3InlineFactKind.characterReference => flags == 1 || flags == 2,
    _ => flags == 0,
  };
  if (!flagsAreCanonical) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline fact flags are invalid for its kind.',
    );
  }

  final start = data.getUint32(offset + 4, Endian.little);
  final length = data.getUint32(offset + 8, Endian.little);
  final end = _checkedEnd(start, length, 'fact');
  if (length == 0 || end > leafBytes) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline fact range is outside its exact leaf.',
    );
  }

  if (kind == FlarkV3InlineFactKind.characterReference) {
    final first = data.getUint32(offset + 12, Endian.little);
    final second = data.getUint32(offset + 16, Endian.little);
    if (length < _minimumCharacterReferenceSourceBytes ||
        length > _maximumCharacterReferenceSourceBytes ||
        !_isUnicodeScalar(first) ||
        (flags == 1 ? second != 0 : second == 0 || !_isUnicodeScalar(second))) {
      throw const FlarkV3InlineFactsDecodeException(
        'Character-reference scalar payload is not canonical.',
      );
    }
    return _InlineRecord(
      kind: kind,
      flags: flags,
      start: start,
      end: end,
      contentStart: start,
      contentEnd: end,
      characterReferenceValue: String.fromCharCodes([
        first,
        if (flags == 2) second,
      ]),
    );
  }

  final contentStart = data.getUint32(offset + 12, Endian.little);
  final contentLength = data.getUint32(offset + 16, Endian.little);
  final contentEnd = _checkedEnd(contentStart, contentLength, 'fact content');
  if (contentStart < start || contentEnd > end) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline fact range is outside its exact leaf.',
    );
  }
  final openerBytes = contentStart - start;
  final closerBytes = end - contentEnd;
  final markerlessAutolink =
      openerBytes == 0 &&
      closerBytes == 0 &&
      contentStart == start &&
      contentEnd == end;
  final angleAutolink = openerBytes == 1 && closerBytes == 1;
  final canonicalMarkers = switch (kind) {
    FlarkV3InlineFactKind.emphasis => openerBytes == 1 && closerBytes == 1,
    FlarkV3InlineFactKind.strong => openerBytes == 2 && closerBytes == 2,
    FlarkV3InlineFactKind.code => openerBytes > 0 && openerBytes == closerBytes,
    FlarkV3InlineFactKind.strikethrough =>
      (openerBytes == 1 || openerBytes == 2) && openerBytes == closerBytes,
    FlarkV3InlineFactKind.autolinkUri || FlarkV3InlineFactKind.autolinkEmail =>
      (angleAutolink && flags == 0) || markerlessAutolink,
    FlarkV3InlineFactKind.escapedPunctuation =>
      openerBytes == 1 && contentLength == 1 && closerBytes == 0,
    FlarkV3InlineFactKind.hardLineBreak =>
      openerBytes >= 1 &&
          (contentLength == 1 || contentLength == 2) &&
          closerBytes == 0,
    FlarkV3InlineFactKind.directLink => openerBytes == 1 && closerBytes >= 3,
    FlarkV3InlineFactKind.directImage => openerBytes == 2 && closerBytes >= 3,
    FlarkV3InlineFactKind.referenceLink => openerBytes == 1 && closerBytes >= 1,
    FlarkV3InlineFactKind.referenceImage =>
      openerBytes == 2 && closerBytes >= 1,
    FlarkV3InlineFactKind.characterReference => false,
  };
  if (!canonicalMarkers) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline fact marker ranges are not canonical for its kind.',
    );
  }

  return _InlineRecord(
    kind: kind,
    flags: flags,
    start: start,
    end: end,
    contentStart: contentStart,
    contentEnd: contentEnd,
    characterReferenceValue: null,
  );
}

void _validatePreorderNesting(List<_InlineRecord> records) {
  final open = <_InlineRecord>[];
  var openActionableLinkCount = 0;
  var previousStart = -1;
  for (final record in records) {
    if (record.start < previousStart) {
      throw const FlarkV3InlineFactsDecodeException(
        'Inline facts are not in canonical parser preorder.',
      );
    }
    previousStart = record.start;

    while (open.isNotEmpty && record.start >= open.last.end) {
      final closed = open.removeLast();
      if (_isActionableContainerLinkKind(closed.kind)) {
        openActionableLinkCount -= 1;
      }
    }
    if (open.isNotEmpty) {
      final parent = open.last;
      final nestedInsideActionableLink =
          _isLinkFactKind(record.kind) && openActionableLinkCount > 0;
      final isCertifiedUriCharacterReference =
          parent.kind == FlarkV3InlineFactKind.autolinkUri &&
          !_usesMarkerlessAutolinkGeometry(parent) &&
          record.kind == FlarkV3InlineFactKind.characterReference;
      if (parent.kind == FlarkV3InlineFactKind.code ||
          (parent.kind == FlarkV3InlineFactKind.autolinkUri &&
              !isCertifiedUriCharacterReference) ||
          parent.kind == FlarkV3InlineFactKind.autolinkEmail ||
          parent.kind == FlarkV3InlineFactKind.escapedPunctuation ||
          parent.kind == FlarkV3InlineFactKind.hardLineBreak ||
          parent.kind == FlarkV3InlineFactKind.characterReference ||
          nestedInsideActionableLink ||
          record.start < parent.contentStart ||
          record.end > parent.contentEnd) {
        throw const FlarkV3InlineFactsDecodeException(
          'Inline fact ranges cross or overlap non-canonically.',
        );
      }
    }
    open.add(record);
    if (_isActionableContainerLinkKind(record.kind)) {
      openActionableLinkCount += 1;
    }
  }
}

bool _isLinkFactKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.autolinkUri ||
    kind == FlarkV3InlineFactKind.autolinkEmail ||
    kind == FlarkV3InlineFactKind.directLink ||
    kind == FlarkV3InlineFactKind.referenceLink;

bool _isActionableContainerLinkKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.directLink ||
    kind == FlarkV3InlineFactKind.referenceLink;

bool _hasLinkValueCompanion(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.directLink ||
    kind == FlarkV3InlineFactKind.directImage ||
    _isReferenceValueFactKind(kind);

bool _isReferenceValueFactKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.referenceLink ||
    kind == FlarkV3InlineFactKind.referenceImage;

bool _usesMarkerlessAutolinkGeometry(_InlineRecord record) =>
    (record.kind == FlarkV3InlineFactKind.autolinkUri ||
        record.kind == FlarkV3InlineFactKind.autolinkEmail) &&
    record.start == record.contentStart &&
    record.end == record.contentEnd;

void _validateLeafSource(
  FlarkV3SourceSpan source, {
  required FlarkV3SourceVersion factSource,
  required _Utf8SpanMapper mapper,
}) {
  if (source.startUtf8 < 0 ||
      source.endUtf8 <= source.startUtf8 ||
      source.endUtf8 > factSource.metric.bytes ||
      source.startUtf16 < 0 ||
      source.endUtf16 < source.startUtf16 ||
      source.endUtf16 > factSource.metric.utf16 ||
      source.endUtf8 - source.startUtf8 >
          FlarkV3InlineFactsDecoder.maximumLeafBytes) {
    throw const FlarkV3InlineFactsDecodeException(
      'Inline leaf is outside its exact bounded source.',
    );
  }
  mapper.expect(source.startUtf8, source.startUtf16, 'leaf start');
  mapper.expect(source.endUtf8, source.endUtf16, 'leaf end');
}

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

int _checkedEnd(int start, int length, String name) {
  if (length > _u32Maximum - start) {
    throw FlarkV3InlineFactsDecodeException('$name range overflows u32.');
  }
  return start + length;
}

void _requireU32(int value, String name) {
  if (value < 0 || value > _u32Maximum) {
    throw FlarkV3InlineFactsDecodeException('$name is outside u32.');
  }
}

final class _InlineRecord {
  const _InlineRecord({
    required this.kind,
    required this.flags,
    required this.start,
    required this.end,
    required this.contentStart,
    required this.contentEnd,
    required this.characterReferenceValue,
  });

  final FlarkV3InlineFactKind kind;
  final int flags;
  final int start;
  final int end;
  final int contentStart;
  final int contentEnd;
  final String? characterReferenceValue;
}

final class _InlineValue {
  const _InlineValue({
    required this.destinationSource,
    required this.titleSource,
    required this.cookedDestination,
    required this.cookedTitle,
  });

  final FlarkV3SourceSpan destinationSource;
  final FlarkV3SourceSpan? titleSource;
  final String cookedDestination;
  final String? cookedTitle;
}

final class _Utf8SpanMapper {
  _Utf8SpanMapper(
    this._utf8ToUtf16, {
    required this.sourceLimitUtf8,
    required this.sourceLimitUtf16,
  });

  final int Function(int utf8Offset) _utf8ToUtf16;
  final int sourceLimitUtf8;
  final int sourceLimitUtf16;
  final Map<int, int> _cache = <int, int>{};

  void expect(int utf8, int utf16, String name) {
    if (_map(utf8, name) != utf16) {
      throw FlarkV3InlineFactsDecodeException(
        '$name does not match exact source coordinates.',
      );
    }
  }

  FlarkV3SourceSpan span(int startUtf8, int endUtf8, String name) {
    final startUtf16 = _map(startUtf8, '$name start');
    final endUtf16 = _map(endUtf8, '$name end');
    if (endUtf16 < startUtf16) {
      throw FlarkV3InlineFactsDecodeException(
        '$name has non-monotonic source coordinates.',
      );
    }
    return FlarkV3SourceSpan(
      startUtf8: startUtf8,
      endUtf8: endUtf8,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
    );
  }

  int _map(int utf8, String name) {
    if (utf8 < 0 || utf8 > sourceLimitUtf8) {
      throw FlarkV3InlineFactsDecodeException('$name is outside exact source.');
    }
    final cached = _cache[utf8];
    if (cached != null) return cached;

    late final int utf16;
    try {
      utf16 = _utf8ToUtf16(utf8);
    } on Object {
      throw FlarkV3InlineFactsDecodeException(
        '$name is not an exact UTF-8 boundary.',
      );
    }
    if (utf16 < 0 || utf16 > sourceLimitUtf16) {
      throw FlarkV3InlineFactsDecodeException(
        '$name maps outside exact UTF-16 source.',
      );
    }
    _cache[utf8] = utf16;
    return utf16;
  }
}

const int _u32Maximum = 0xFFFFFFFF;
const int _codeNormalizeLineEndings = 1;
const int _codeTrimOneEdgeSpace = 2;
const int _knownCodeFlags = _codeNormalizeLineEndings | _codeTrimOneEdgeSpace;
const int _autolinkUriWww = 1;
const int _knownAutolinkUriFlags = _autolinkUriWww;
const int _minimumCharacterReferenceSourceBytes = 4;
const int _maximumCharacterReferenceSourceBytes = 33;
const List<int> _inlineValuesMagic = <int>[
  0x46, // F
  0x4C, // L
  0x4B, // K
  0x49, // I
  0x56, // V
  0x30, // 0
  0x30, // 0
  0x31, // 1
];
const int _inlineValuesSchema = 1;
const int _inlineValuesHeaderBytes = 16;
const int _inlineValueEntryHeaderBytes = 32;
const int _inlineValueTitlePresent = 1;

bool _isUnicodeScalar(int value) =>
    value <= 0x10FFFF && (value < 0xD800 || value > 0xDFFF);
