import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/editor/flark_v3_inline_projection.dart';
import 'package:flark/src/v3/editor/flark_v3_projected_inline_projection.dart';
import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('projected-coordinate inline facts', () {
    test('decode and project disjoint multiline quote text marker-free', () {
      const projected = '**first\nsecond** and `code`';
      final facts = _decodeProjected(
        projected,
        records: [
          _record(
            kind: 2,
            start: 0,
            length: 16,
            contentStart: 2,
            contentLength: 12,
          ),
          _record(
            kind: 3,
            start: 21,
            length: 6,
            contentStart: 22,
            contentLength: 4,
          ),
        ],
      );

      expect(facts.physicalSource.endUtf16, 31);
      expect(facts.projectedUtf16Length, 27);
      expect(facts.facts.first.source.startUtf16, 0);
      expect(facts.facts.first.content.endUtf16, 14);
      expect(facts.facts.last.kind, FlarkV3InlineFactKind.code);

      final projection = FlarkV3ProjectedInlineProjection.fromValidatedFacts(
        projectedText: projected,
        facts: facts,
      );
      expect(projection.displayText, 'first\nsecond and code');
      expect(
        projection.pieces.map((piece) => piece.kind),
        containsAll([
          FlarkV3ProjectedInlineProjectionPieceKind.copy,
          FlarkV3ProjectedInlineProjectionPieceKind.hide,
        ]),
      );
      expect(
        projection.runs
            .where(
              (run) =>
                  run.semanticStyles.contains(FlarkV3InlineFactKind.strong),
            )
            .map((run) => run.text)
            .join(),
        'first\nsecond',
      );
      expect(
        projection.runs
            .singleWhere(
              (run) => run.semanticStyles.contains(FlarkV3InlineFactKind.code),
            )
            .text,
        'code',
      );
      expect(
        projection.pieces.first.projectedStartUtf16,
        0,
        reason: 'piece coordinates remain projected, never physical',
      );
      expect(projection.pieces.last.projectedEndUtf16, projected.length);
    });

    test('rejects metric, physical-range, and value-companion confusion', () {
      const projected = '**x**';
      expect(
        () => _decodeProjected(projected, projectedUtf16Length: 4),
        throwsA(isA<FlarkV3ProjectedInlineFactsDecodeException>()),
      );
      expect(
        () => _decodeProjected(projected, physicalEndUtf16: 6),
        throwsA(isA<FlarkV3ProjectedInlineFactsDecodeException>()),
      );
      expect(
        () => _decodeProjected(
          '[x](u)',
          records: [
            _record(
              kind: 10,
              start: 0,
              length: 6,
              contentStart: 1,
              contentLength: 1,
            ),
          ],
        ),
        throwsA(isA<FlarkV3ProjectedInlineFactsDecodeException>()),
        reason: 'the first projected lane fails whole-leaf closed on links',
      );
    });

    test('unsupported is an exact identity projection with no semantics', () {
      const projected = '[x](u)';
      final facts = _decodeProjected(
        projected,
        disposition: FlarkV3ProjectedInlineFactsDisposition.unsupported,
      );
      final projection = FlarkV3ProjectedInlineProjection.fromValidatedFacts(
        projectedText: projected,
        facts: facts,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
      );

      expect(facts.facts, isEmpty);
      expect(projection.displayText, projected);
      expect(projection.pieces, hasLength(1));
      expect(
        projection.pieces.single.kind,
        FlarkV3ProjectedInlineProjectionPieceKind.copy,
      );
      expect(projection.runs.single.semanticStyles, isEmpty);
    });
  });
}

FlarkV3ProjectedInlineFacts _decodeProjected(
  String projected, {
  FlarkV3ProjectedInlineFactsDisposition disposition =
      FlarkV3ProjectedInlineFactsDisposition.authoritative,
  List<Uint8List> records = const [],
  int? projectedUtf16Length,
  int? physicalEndUtf16,
}) {
  const physical = '> **first\n> second** and `code`';
  final document = FlarkV3SourceDocument.fromString(physical);
  final version = FlarkV3SourceVersion.fromDocument(
    documentSession: FlarkV3DocumentSessionId(11, 12, 13, 14),
    document: document,
  );
  final physicalSource = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: utf8.encode(physical).length,
    startUtf16: 0,
    endUtf16: physicalEndUtf16 ?? physical.length,
  );
  return FlarkV3ProjectedInlineFactsDecoder.decode(
    sourceDocument: document,
    expectedSource: version,
    factSource: version,
    expectedProfilePartition: 3,
    profilePartition: 3,
    expectedPhysicalSource: FlarkV3SourceSpan(
      startUtf8: 0,
      endUtf8: utf8.encode(physical).length,
      startUtf16: 0,
      endUtf16: physical.length,
    ),
    factPhysicalSource: physicalSource,
    expectedProjectedUtf8Length: utf8.encode(projected).length,
    expectedProjectedUtf16Length: projectedUtf16Length ?? projected.length,
    projectedText: projected,
    disposition: disposition,
    factCount: records.length,
    encodedFacts: Uint8List.fromList([for (final record in records) ...record]),
  );
}

Uint8List _record({
  required int kind,
  int flags = 0,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(FlarkV3ProjectedInlineFactsDecoder.recordBytes);
  ByteData.sublistView(bytes)
    ..setUint8(0, kind)
    ..setUint8(1, flags)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}
