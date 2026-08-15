import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_indented_code_projection.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3IndentedCodeProjectionDecoder', () {
    test('decodes exact Unicode, NUL, blank, and mixed-EOL line geometry', () {
      const documentText = 'lead\n    α\u0000\r\n \t\r\n\tβ\ntrail';
      const blockText = '    α\u0000\r\n \t\r\n\tβ\n';
      final document = FlarkV3SourceDocument.fromString(documentText);
      final blockStartUtf16 = documentText.indexOf(blockText);
      final block = _span(
        document,
        blockStartUtf16,
        blockStartUtf16 + blockText.length,
      );
      final facts = _facts(
        lineCount: 3,
        projectedUtf8Length: 10,
        projectedUtf16Length: 8,
        terminalLineEndingBytes: 1,
      );
      final sourceVersion = _sourceVersion(document);

      final decoded = FlarkV3IndentedCodeProjectionDecoder.decode(
        sourceDocument: document,
        expectedSource: sourceVersion,
        source: block,
        facts: facts,
        encodedRecords: _records([
          (start: 0, physical: 9, hidden: 4, content: 3, flags: 0),
          (
            start: 9,
            physical: 4,
            hidden: 2,
            content: 0,
            flags: FlarkV3IndentedCodeProjectionDecoder.internalBlankFlag,
          ),
          (start: 13, physical: 4, hidden: 1, content: 2, flags: 0),
        ]),
      );

      expect(decoded.source, same(block));
      expect(decoded.facts, same(facts));
      expect(decoded.records, hasLength(3));
      expect(decoded.records[0].relativeLineStartUtf8, 0);
      expect(decoded.records[0].physicalSourceLengthUtf8, 9);
      expect(decoded.records[0].hiddenPrefixLengthUtf8, 4);
      expect(decoded.records[0].contentLengthUtf8, 3);
      expect(decoded.records[0].lineEndingLengthUtf8, 2);
      expect(decoded.records[0].isInternalBlank, isFalse);
      expect(decoded.records[1].isInternalBlank, isTrue);
      expect(decoded.records[1].contentLengthUtf8, 0);
      expect(decoded.records[2].lineEndingLengthUtf8, 1);
      expect(decoded.projectionPieces, hasLength(6));

      expect(decoded.sourceVersion, same(sourceVersion));
      final projection = decoded.toSourceProjection();
      expect(projection.isCertified, isTrue);
      expect(projection.sourceStartUtf16, block.startUtf16);
      expect(projection.sourceText, blockText);
      expect(projection.displayText, 'α\u0000\r\n\r\nβ\n');
      expect(projection.displayText.codeUnits, contains(0));
      expect(projection.displayLengthUtf16, facts.projectedUtf16Length);
    });

    test('accepts zero-prefix internal blanks and physical CR endings', () {
      const source = '    a\r\n\r    b';
      final document = FlarkV3SourceDocument.fromString(source);
      final decoded = _decodeWhole(
        document,
        facts: _facts(
          lineCount: 3,
          projectedUtf8Length: 5,
          projectedUtf16Length: 5,
          terminalLineEndingBytes: 0,
        ),
        records: [
          (start: 0, physical: 7, hidden: 4, content: 1, flags: 0),
          (
            start: 7,
            physical: 1,
            hidden: 0,
            content: 0,
            flags: FlarkV3IndentedCodeProjectionDecoder.internalBlankFlag,
          ),
          (start: 8, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
      );

      expect(decoded.records[1].hiddenPrefixLengthUtf8, 0);
      expect(decoded.records[1].lineEndingLengthUtf8, 1);
      expect(decoded.toSourceProjection().displayText, 'a\r\n\rb');
    });

    test('accepts and verifies BOF BOM ownership', () {
      const source = '\uFEFF\tz\n    q';
      final document = FlarkV3SourceDocument.fromString(source);
      final decoded = _decodeWhole(
        document,
        facts: _facts(
          hasBofBom: true,
          lineCount: 2,
          projectedUtf8Length: 3,
          projectedUtf16Length: 3,
          terminalLineEndingBytes: 0,
        ),
        records: [
          (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
      );

      expect(decoded.records.first.hiddenPrefixLengthUtf8, 4);
      expect(decoded.records.first.hiddenPrefix.endUtf16, 2);
      expect(decoded.toSourceProjection().displayText, 'z\nq');

      expect(
        () => _decodeWhole(
          document,
          facts: _facts(
            lineCount: 2,
            projectedUtf8Length: 3,
            projectedUtf16Length: 3,
            terminalLineEndingBytes: 0,
          ),
          records: [
            (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
            (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
          ],
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );
    });

    test('rejects nonrecords, count mismatch, gaps, and incomplete tiling', () {
      final document = FlarkV3SourceDocument.fromString('    a\n    b');
      final facts = _facts(
        lineCount: 2,
        projectedUtf8Length: 3,
        projectedUtf16Length: 3,
        terminalLineEndingBytes: 0,
      );

      final corrupt = <Uint8List>[
        Uint8List(1),
        _records([(start: 0, physical: 6, hidden: 4, content: 1, flags: 0)]),
        _records([
          (start: 1, physical: 5, hidden: 4, content: 1, flags: 0),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ]),
        _records([
          (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
          (start: 7, physical: 4, hidden: 3, content: 1, flags: 0),
        ]),
        _records([
          (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
          (start: 6, physical: 4, hidden: 3, content: 1, flags: 0),
        ]),
      ];
      for (final encoded in corrupt) {
        expect(
          () => FlarkV3IndentedCodeProjectionDecoder.decode(
            sourceDocument: document,
            expectedSource: _sourceVersion(document),
            source: _wholeSpan(document),
            facts: facts,
            encodedRecords: encoded,
          ),
          throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
        );
      }
    });

    test('rejects invalid ranges, flags, and blank-line placement', () {
      final twoLine = FlarkV3SourceDocument.fromString('    a\n    b');
      final twoLineFacts = _facts(
        lineCount: 2,
        projectedUtf8Length: 3,
        projectedUtf16Length: 3,
        terminalLineEndingBytes: 0,
      );
      final corrupt = <List<_LineRecord>>[
        [
          (start: 0, physical: 0, hidden: 0, content: 0, flags: 0),
          (start: 0, physical: 11, hidden: 4, content: 7, flags: 0),
        ],
        [
          (start: 0, physical: 6, hidden: 0, content: 5, flags: 0),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
        [
          (start: 0, physical: 6, hidden: 4, content: 3, flags: 0),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
        [
          (start: 0, physical: 6, hidden: 4, content: 1, flags: 2),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
        [
          (
            start: 0,
            physical: 6,
            hidden: 5,
            content: 0,
            flags: FlarkV3IndentedCodeProjectionDecoder.internalBlankFlag,
          ),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
        [
          (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
          (
            start: 6,
            physical: 5,
            hidden: 4,
            content: 0,
            flags: FlarkV3IndentedCodeProjectionDecoder.internalBlankFlag,
          ),
        ],
        [
          (start: 0, physical: 6, hidden: 0xFFFFFFFF, content: 1, flags: 0),
          (start: 6, physical: 5, hidden: 4, content: 1, flags: 0),
        ],
      ];
      for (final records in corrupt) {
        expect(
          () => _decodeWhole(twoLine, facts: twoLineFacts, records: records),
          throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
        );
      }

      final internalBlankWithoutEol = FlarkV3SourceDocument.fromString(
        '    a\n ',
      );
      expect(
        () => _decodeWhole(
          internalBlankWithoutEol,
          facts: _facts(
            lineCount: 2,
            projectedUtf8Length: 2,
            projectedUtf16Length: 2,
            terminalLineEndingBytes: 0,
          ),
          records: [
            (start: 0, physical: 6, hidden: 4, content: 1, flags: 0),
            (
              start: 6,
              physical: 1,
              hidden: 1,
              content: 0,
              flags: FlarkV3IndentedCodeProjectionDecoder.internalBlankFlag,
            ),
          ],
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );
    });

    test('rejects false physical line endings and scalar splits', () {
      for (final fixture in const [
        (source: '    ax', content: 1),
        (source: '    axy', content: 1),
        (source: '    a\rx', content: 2),
      ]) {
        final document = FlarkV3SourceDocument.fromString(fixture.source);
        final physicalLength = utf8.encode(fixture.source).length;
        final lineEndingLength = physicalLength - 4 - fixture.content;
        expect(
          () => _decodeWhole(
            document,
            facts: _facts(
              lineCount: 1,
              projectedUtf8Length: physicalLength - 4,
              projectedUtf16Length: fixture.source.length - 4,
              terminalLineEndingBytes: lineEndingLength,
            ),
            records: [
              (
                start: 0,
                physical: physicalLength,
                hidden: 4,
                content: fixture.content,
                flags: 0,
              ),
            ],
          ),
          throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
        );
      }

      final unicode = FlarkV3SourceDocument.fromString('    α');
      expect(
        () => _decodeWhole(
          unicode,
          facts: _facts(
            lineCount: 1,
            projectedUtf8Length: 1,
            projectedUtf16Length: 1,
            terminalLineEndingBytes: 0,
          ),
          records: [(start: 0, physical: 6, hidden: 5, content: 1, flags: 0)],
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );

      final nonWhitespacePrefix = FlarkV3SourceDocument.fromString('xabc');
      expect(
        () => _decodeWhole(
          nonWhitespacePrefix,
          facts: _facts(
            lineCount: 1,
            projectedUtf8Length: 3,
            projectedUtf16Length: 3,
            terminalLineEndingBytes: 0,
          ),
          records: [(start: 0, physical: 4, hidden: 1, content: 3, flags: 0)],
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );
    });

    test('rejects structural summary mismatches independently', () {
      final document = FlarkV3SourceDocument.fromString('    α\r\n');
      final records = <_LineRecord>[
        (start: 0, physical: 8, hidden: 4, content: 2, flags: 0),
      ];
      for (final facts in [
        _facts(
          lineCount: 1,
          projectedUtf8Length: 5,
          projectedUtf16Length: 3,
          terminalLineEndingBytes: 2,
        ),
        _facts(
          lineCount: 1,
          projectedUtf8Length: 4,
          projectedUtf16Length: 4,
          terminalLineEndingBytes: 2,
        ),
        _facts(
          lineCount: 1,
          projectedUtf8Length: 4,
          projectedUtf16Length: 3,
          terminalLineEndingBytes: 1,
        ),
      ]) {
        expect(
          () => _decodeWhole(document, facts: facts, records: records),
          throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
        );
      }
    });

    test('rejects uncertified coordinates and authority rebinding', () {
      final provisional = FlarkV3SourceDocument.fromProvisionalString('    a');
      expect(
        () => FlarkV3IndentedCodeProjectionDecoder.decode(
          sourceDocument: provisional,
          expectedSource: FlarkV3SourceVersion(
            documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
            revision: provisional.revision,
            metric: FlarkV3SourceMetric(bytes: 5, utf16: 5),
            contentHash: FlarkV3ContentHash128.zero,
          ),
          source: const FlarkV3SourceSpan(
            startUtf8: 0,
            endUtf8: 5,
            startUtf16: 0,
            endUtf16: 5,
          ),
          facts: _facts(
            lineCount: 1,
            projectedUtf8Length: 1,
            projectedUtf16Length: 1,
            terminalLineEndingBytes: 0,
          ),
          encodedRecords: _records([
            (start: 0, physical: 5, hidden: 4, content: 1, flags: 0),
          ]),
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );

      final document = FlarkV3SourceDocument.fromString('    a');
      final decoded = _decodeWhole(
        document,
        facts: _facts(
          lineCount: 1,
          projectedUtf8Length: 1,
          projectedUtf16Length: 1,
          terminalLineEndingBytes: 0,
        ),
        records: [(start: 0, physical: 5, hidden: 4, content: 1, flags: 0)],
      );
      final other = FlarkV3SourceDocument.fromString('    b');
      final wrongVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
        document: other,
      );
      expect(
        () => FlarkV3IndentedCodeProjectionDecoder.decode(
          sourceDocument: document,
          expectedSource: wrongVersion,
          source: _wholeSpan(document),
          facts: decoded.facts,
          encodedRecords: _records([
            (start: 0, physical: 5, hidden: 4, content: 1, flags: 0),
          ]),
        ),
        throwsA(isA<FlarkV3IndentedCodeProjectionDecodeException>()),
      );
    });
  });
}

typedef _LineRecord = ({
  int start,
  int physical,
  int hidden,
  int content,
  int flags,
});

FlarkV3IndentedCodeProjectionPayload _decodeWhole(
  FlarkV3SourceDocument document, {
  required FlarkV3IndentedCodeFacts facts,
  required List<_LineRecord> records,
}) => FlarkV3IndentedCodeProjectionDecoder.decode(
  sourceDocument: document,
  expectedSource: _sourceVersion(document),
  source: _wholeSpan(document),
  facts: facts,
  encodedRecords: _records(records),
);

FlarkV3SourceVersion _sourceVersion(FlarkV3SourceDocument document) =>
    FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    );

FlarkV3IndentedCodeFacts _facts({
  bool hasBofBom = false,
  required int lineCount,
  required int projectedUtf8Length,
  required int projectedUtf16Length,
  required int terminalLineEndingBytes,
}) => FlarkV3IndentedCodeFacts(
  deindentColumns: 4,
  hasBofBom: hasBofBom,
  lineCount: lineCount,
  projectedUtf8Length: projectedUtf8Length,
  projectedUtf16Length: projectedUtf16Length,
  terminalLineEndingBytes: terminalLineEndingBytes,
);

FlarkV3SourceSpan _wholeSpan(FlarkV3SourceDocument document) =>
    FlarkV3SourceSpan(
      startUtf8: 0,
      endUtf8: document.utf8Length,
      startUtf16: 0,
      endUtf16: document.utf16Length,
    );

FlarkV3SourceSpan _span(
  FlarkV3SourceDocument document,
  int startUtf16,
  int endUtf16,
) => FlarkV3SourceSpan(
  startUtf8: document.utf16ToUtf8(startUtf16),
  endUtf8: document.utf16ToUtf8(endUtf16),
  startUtf16: startUtf16,
  endUtf16: endUtf16,
);

Uint8List _records(List<_LineRecord> records) {
  final encoded = Uint8List(
    records.length * FlarkV3IndentedCodeProjectionDecoder.recordBytes,
  );
  final data = ByteData.sublistView(encoded);
  for (var index = 0; index < records.length; index += 1) {
    final record = records[index];
    final offset = index * FlarkV3IndentedCodeProjectionDecoder.recordBytes;
    data
      ..setUint32(
        offset + FlarkV3IndentedCodeProjectionDecoder.relativeLineStartOffset,
        record.start,
        Endian.little,
      )
      ..setUint32(
        offset +
            FlarkV3IndentedCodeProjectionDecoder.physicalSourceLengthOffset,
        record.physical,
        Endian.little,
      )
      ..setUint32(
        offset + FlarkV3IndentedCodeProjectionDecoder.hiddenPrefixLengthOffset,
        record.hidden,
        Endian.little,
      )
      ..setUint32(
        offset + FlarkV3IndentedCodeProjectionDecoder.contentLengthOffset,
        record.content,
        Endian.little,
      )
      ..setUint32(
        offset + FlarkV3IndentedCodeProjectionDecoder.flagsOffset,
        record.flags,
        Endian.little,
      );
  }
  return encoded;
}
