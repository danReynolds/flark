import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_current_revision_inline_cache.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3CurrentRevisionInlineCache', () {
    test('retains several leaves and evicts by bounded LRU order', () {
      const sourceText = '*a*\n\n*b*\n\n*c*';
      final source = FlarkV3SourceDocument.fromString(sourceText);
      final sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: source,
      );
      final authority = _ack(sourceVersion, generation: 1);
      final cache = FlarkV3CurrentRevisionInlineCache(
        maximumEntries: 2,
        maximumFactRecords: 2,
      );
      final a = _query(source, sourceVersion, 0, 3, withFact: true);
      final b = _query(source, sourceVersion, 5, 8, withFact: true);
      final c = _query(source, sourceVersion, 10, 13, withFact: true);

      cache.resolve(authority: authority, query: a);
      cache.resolve(authority: authority, query: b);
      expect(cache.entryCount, 2);
      expect(cache.retainedFactRecords, 2);

      // Touch A so B becomes the deterministic least-recently-used entry.
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 0, 3),
            )
            .inlineFacts,
        same(a.inlineFacts),
      );
      cache.resolve(authority: authority, query: c);

      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 5, 8),
            )
            .inlineFacts,
        isNull,
      );
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 0, 3),
            )
            .inlineFacts,
        same(a.inlineFacts),
      );
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 10, 13),
            )
            .inlineFacts,
        same(c.inlineFacts),
      );
      expect(cache.entryCount, 2);
      expect(cache.retainedFactRecords, 2);
    });

    test('a different exact structural ACK invalidates the whole epoch', () {
      final source = FlarkV3SourceDocument.fromString('*a*');
      final sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: source,
      );
      final cache = FlarkV3CurrentRevisionInlineCache(
        maximumEntries: 4,
        maximumFactRecords: 4,
      );
      final facts = _query(source, sourceVersion, 0, 3, withFact: true);
      cache.resolve(
        authority: _ack(sourceVersion, generation: 1),
        query: facts,
      );
      expect(cache.entryCount, 1);

      final afterReplacement = cache.resolve(
        authority: _ack(sourceVersion, generation: 2),
        query: _query(source, sourceVersion, 0, 3),
      );

      expect(afterReplacement.inlineFacts, isNull);
      expect(cache.entryCount, 0);
      expect(cache.retainedFactRecords, 0);
    });

    test(
      'reuses escaped-punctuation facts only within the exact ACK epoch',
      () {
        final source = FlarkV3SourceDocument.fromString(r'\*');
        final sourceVersion = FlarkV3SourceVersion.fromDocument(
          documentSession: _documentSession,
          document: source,
        );
        final cache = FlarkV3CurrentRevisionInlineCache(
          maximumEntries: 1,
          maximumFactRecords: 1,
        );
        final authority = _ack(sourceVersion, generation: 1);
        final certified = _query(
          source,
          sourceVersion,
          0,
          2,
          factRecord: _escapedPunctuationRecord(),
        );

        cache.resolve(authority: authority, query: certified);
        final reused = cache.resolve(
          authority: authority,
          query: _query(source, sourceVersion, 0, 2),
        );

        expect(reused.inlineFacts, same(certified.inlineFacts));
        expect(
          reused.inlineFacts!.facts.single.kind,
          FlarkV3InlineFactKind.escapedPunctuation,
        );
        expect(cache.retainedFactRecords, 1);

        final afterReplacement = cache.resolve(
          authority: _ack(sourceVersion, generation: 2),
          query: _query(source, sourceVersion, 0, 2),
        );

        expect(afterReplacement.inlineFacts, isNull);
        expect(cache.entryCount, 0);
        expect(cache.retainedFactRecords, 0);
      },
    );

    test('terminal empty facts cache while over-budget facts do not', () {
      const sourceText = 'plain\n\n@bad\n\n*b*';
      final source = FlarkV3SourceDocument.fromString(sourceText);
      final sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: source,
      );
      final authority = _ack(sourceVersion, generation: 1);
      final cache = FlarkV3CurrentRevisionInlineCache(
        maximumEntries: 2,
        maximumFactRecords: 0,
      );
      final plain = _query(source, sourceVersion, 0, 5, withEmptyFacts: true);
      final unsupported = _query(
        source,
        sourceVersion,
        7,
        11,
        withUnsupportedFacts: true,
      );
      final formatted = _query(source, sourceVersion, 13, 16, withFact: true);

      cache.resolve(authority: authority, query: plain);
      cache.resolve(authority: authority, query: unsupported);
      cache.resolve(authority: authority, query: formatted);

      expect(cache.entryCount, 2);
      expect(cache.retainedFactRecords, 0);
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 0, 5),
            )
            .inlineFacts,
        same(plain.inlineFacts),
      );
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 7, 11),
            )
            .inlineFacts,
        same(unsupported.inlineFacts),
      );
      expect(
        cache
            .resolve(
              authority: authority,
              query: _query(source, sourceVersion, 13, 16),
            )
            .inlineFacts,
        isNull,
      );
    });

    test('reuses parser-certified facts for ATX heading content', () {
      final source = FlarkV3SourceDocument.fromString('# *a*\n');
      final sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: source,
      );
      final authority = _ack(sourceVersion, generation: 1);
      final cache = FlarkV3CurrentRevisionInlineCache(
        maximumEntries: 2,
        maximumFactRecords: 2,
      );
      final withFacts = _headingQuery(source, sourceVersion, withFact: true);

      cache.resolve(authority: authority, query: withFacts);
      final resolved = cache.resolve(
        authority: authority,
        query: _headingQuery(source, sourceVersion),
      );

      expect(resolved.inlineFacts, same(withFacts.inlineFacts));
      expect(resolved.structure.kind, FlarkV3DocumentStructureKind.heading);
      expect(cache.entryCount, 1);
    });
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);

FlarkV3StructuralAck _ack(
  FlarkV3SourceVersion sourceVersion, {
  required int generation,
}) => FlarkV3StructuralAck(
  publicationSession: FlarkV3PublicationSessionId(generation, 20, 30, 40),
  hostRevision: FlarkV3HostRevisionId(generation),
  sourceVersion: sourceVersion,
  sourceRoot: FlarkV3SourceRootId(generation, 1),
  parseGeneration: generation,
  grammarRevision: flarkV3CurrentGrammarRevision,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 1,
  sequenceDigest: FlarkV3ProtocolDigest128(generation, 2, 3, 4),
  manifestDigest: FlarkV3ProtocolDigest128(generation, 5, 6, 7),
);

FlarkV3DocumentStructuralQuery _query(
  FlarkV3SourceDocument source,
  FlarkV3SourceVersion sourceVersion,
  int startUtf16,
  int endUtf16, {
  bool withFact = false,
  Uint8List? factRecord,
  bool withEmptyFacts = false,
  bool withUnsupportedFacts = false,
}) {
  if (withFact && factRecord != null) {
    throw ArgumentError('Specify either withFact or factRecord, not both.');
  }
  final leaf = FlarkV3SourceSpan(
    startUtf8: source.utf16ToUtf8(startUtf16),
    endUtf8: source.utf16ToUtf8(endUtf16),
    startUtf16: startUtf16,
    endUtf16: endUtf16,
  );
  FlarkV3InlineFacts? inlineFacts;
  final hasFact = withFact || factRecord != null;
  if (hasFact || withEmptyFacts || withUnsupportedFacts) {
    final encoded =
        factRecord ??
        (withFact ? _emphasisRecord(endUtf16 - startUtf16) : Uint8List(0));
    inlineFacts = FlarkV3InlineFactsDecoder.decode(
      sourceDocument: source,
      expectedSource: sourceVersion,
      factSource: sourceVersion,
      expectedProfilePartition: 1,
      profilePartition: 1,
      expectedLeaf: leaf,
      factLeaf: leaf,
      disposition: withUnsupportedFacts
          ? FlarkV3InlineFactsDisposition.unsupported
          : FlarkV3InlineFactsDisposition.authoritative,
      factCount: hasFact ? 1 : 0,
      encodedFacts: encoded,
    );
  }
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: sourceVersion.revision,
    structureRevision: sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      visibleSource: leaf,
      referenceDefinitionCount: 0,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      projectedSource: leaf,
      runCount: 1,
    ),
    inlineFacts: inlineFacts,
  );
}

FlarkV3DocumentStructuralQuery _headingQuery(
  FlarkV3SourceDocument source,
  FlarkV3SourceVersion sourceVersion, {
  bool withFact = false,
}) {
  const physical = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 6,
    startUtf16: 0,
    endUtf16: 6,
  );
  const content = FlarkV3SourceSpan(
    startUtf8: 2,
    endUtf8: 5,
    startUtf16: 2,
    endUtf16: 5,
  );
  final inlineFacts = withFact
      ? FlarkV3InlineFactsDecoder.decode(
          sourceDocument: source,
          expectedSource: sourceVersion,
          factSource: sourceVersion,
          expectedProfilePartition: 1,
          profilePartition: 1,
          expectedLeaf: content,
          factLeaf: content,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 1,
          encodedFacts: _emphasisRecord(3),
        )
      : null;
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: sourceVersion.revision,
    structureRevision: sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.heading,
      source: physical,
      visibleSource: content,
      referenceDefinitionCount: 0,
      heading: FlarkV3AtxHeadingFacts(
        level: 1,
        openingMarker: FlarkV3SourceSpan(
          startUtf8: 0,
          endUtf8: 1,
          startUtf16: 0,
          endUtf16: 1,
        ),
        contentSource: content,
        closingMarker: null,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.heading,
      source: physical,
      projectedSource: content,
      runCount: 1,
    ),
    inlineFacts: inlineFacts,
  );
}

Uint8List _emphasisRecord(int leafLength) {
  final record = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  ByteData.sublistView(record)
    ..setUint8(0, 1)
    ..setUint8(1, 0)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, leafLength, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, leafLength - 2, Endian.little);
  return record;
}

Uint8List _escapedPunctuationRecord() {
  final record = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  ByteData.sublistView(record)
    ..setUint8(0, 7)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 2, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return record;
}
