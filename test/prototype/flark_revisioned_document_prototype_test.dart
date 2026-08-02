@Tags(<String>['benchmark'])
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:test/test.dart';

import '../../tool/parser_research/dart/persistent_document.dart';

void main() {
  test('UTF-16, UTF-8, line, and content-hash metrics stay exact', () {
    const source = 'alpha 😀 café\nβeta 𝄞\nlast';
    final document = PrototypePersistentDocument.fromString(
      source,
      chunkSize: 7,
    );

    expect(document.toString(), source);
    expect(document.utf16Length, source.length);
    expect(document.utf8Length, utf8.encode(source).length);
    expect(document.lineCount, 3);
    expect(document.lineStartUtf16(0), 0);
    expect(document.lineStartUtf16(1), source.indexOf('\n') + 1);
    expect(document.lineStartUtf16(2), source.lastIndexOf('\n') + 1);

    for (var offset = 0; offset <= source.length; offset += 1) {
      if (!_isScalarBoundary(source, offset)) continue;
      final expectedBytes = utf8.encode(source.substring(0, offset)).length;
      expect(
        document.utf16ToUtf8(offset),
        expectedBytes,
        reason: 'UTF-16 offset $offset',
      );
      expect(
        document.utf8ToUtf16(expectedBytes),
        offset,
        reason: 'UTF-8 offset $expectedBytes',
      );
      expect(
        document.lineAtUtf16(offset),
        '\n'.allMatches(source.substring(0, offset)).length,
      );
    }

    final differentlyChunked = PrototypePersistentDocument.fromString(
      source,
      chunkSize: 32,
    );
    expect(differentlyChunked.contentHash32, document.contentHash32);
    expect(
      PrototypePersistentDocument.fromString('a😀 café β\n').contentHash32,
      0xB991EDD9,
      reason: 'shared native/WASM UTF-8 fingerprint golden',
    );
  });

  test('revisioned edit produces an exact compact UTF-8 parser delta', () {
    const source = 'before 😀 café after\nsecond';
    var document = PrototypePersistentDocument.fromString(source, chunkSize: 8);
    final start = source.indexOf('😀');
    final end = source.indexOf(' after');
    final applied = document.apply(
      PrototypeDocumentEdit(
        baseRevision: document.revision,
        startUtf16: start,
        endUtf16: end,
        replacement: 'β **new**',
      ),
    );

    const expected = 'before β **new** after\nsecond';
    expect(applied.document.toString(), expected);
    expect(applied.document.revision, 1);
    expect(
      applied.parserDelta.startUtf8,
      utf8.encode(source.substring(0, start)).length,
    );
    expect(
      applied.parserDelta.endUtf8,
      utf8.encode(source.substring(0, end)).length,
    );
    expect(utf8.decode(applied.parserDelta.replacementUtf8), 'β **new**');
    expect(applied.parserDelta.beforeHash32, document.contentHash32);
    expect(applied.parserDelta.afterHash32, applied.document.contentHash32);
    expect(applied.parserDelta.wireBytes, lessThan(64));

    document = applied.document;
    expect(
      () => document.apply(
        const PrototypeDocumentEdit(
          baseRevision: 0,
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'stale',
        ),
      ),
      throwsA(isA<PrototypeRevisionMismatch>()),
    );
  });

  test('invalid scalar boundaries and unpaired surrogates are rejected', () {
    final document = PrototypePersistentDocument.fromString('a😀b');
    expect(() => document.utf16ToUtf8(2), throwsA(isA<FormatException>()));
    expect(
      () => document.apply(
        const PrototypeDocumentEdit(
          baseRevision: 0,
          startUtf16: 2,
          endUtf16: 2,
          replacement: 'x',
        ),
      ),
      throwsA(isA<FormatException>()),
    );
    expect(
      () => PrototypePersistentDocument.fromString(String.fromCharCode(0xD800)),
      throwsA(isA<FormatException>()),
    );
  });

  test('sequential persistent edits stay equivalent to String oracle', () {
    var oracle = _largeUnicodeText(400000);
    var document = PrototypePersistentDocument.fromString(oracle);
    var seed = 0x51ced15c;

    for (var iteration = 0; iteration < 2000; iteration += 1) {
      seed = _next(seed);
      var start = 8 + seed % (oracle.length - 16);
      while (!_isScalarBoundary(oracle, start)) {
        start += 1;
      }
      seed = _next(seed);
      var end = iteration % 5 == 0 ? start : math.min(start + 1, oracle.length);
      while (!_isScalarBoundary(oracle, end)) {
        end += 1;
      }
      final replacement = switch (iteration % 7) {
        0 => 'x',
        1 => '',
        2 => '*',
        3 => '\n',
        4 => '😀',
        5 => 'β',
        _ => '`',
      };
      final applied = document.apply(
        PrototypeDocumentEdit(
          baseRevision: document.revision,
          startUtf16: start,
          endUtf16: end,
          replacement: replacement,
        ),
      );
      oracle = oracle.replaceRange(start, end, replacement);
      document = applied.document;

      expect(document.utf16Length, oracle.length);
      expect(document.utf8Length, utf8.encode(oracle).length);
      if (iteration % 100 == 0 || iteration == 1999) {
        expect(document.toString(), oracle, reason: 'iteration=$iteration');
        final rebuilt = PrototypePersistentDocument.fromString(
          oracle,
          chunkSize: 997,
        );
        expect(
          document.contentHash32,
          rebuilt.contentHash32,
          reason: 'iteration=$iteration',
        );
      }
    }
  });

  for (final size in const [1000000, 10000000]) {
    test(
      'revisioned local edit and coordinate mapping at $size UTF-16 units',
      () {
        final source = _largeUnicodeText(size);
        final document = PrototypePersistentDocument.fromString(source);
        var offset = source.length ~/ 2;
        while (!_isScalarBoundary(source, offset)) {
          offset += 1;
        }

        final samples = <int>[];
        for (var iteration = 0; iteration < 100; iteration += 1) {
          final stopwatch = Stopwatch()..start();
          final applied = document.apply(
            PrototypeDocumentEdit(
              baseRevision: document.revision,
              startUtf16: offset,
              endUtf16: offset,
              replacement: 'x',
            ),
          );
          final byteOffset = applied.parserDelta.startUtf8;
          expect(document.utf8ToUtf16(byteOffset), offset);
          stopwatch.stop();
          samples.add(stopwatch.elapsedMicroseconds);
        }
        samples.sort();
        stdout.writeln(
          'flark_revisioned_document size=$size iterations=${samples.length} '
          'median_us=${samples[samples.length ~/ 2]} '
          'p95_us=${samples[((samples.length - 1) * 0.95).ceil()]} '
          'wire_bytes=${document.apply(PrototypeDocumentEdit(baseRevision: 0, startUtf16: offset, endUtf16: offset, replacement: 'x')).parserDelta.wireBytes}',
        );
      },
    );
  }
}

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

bool _isScalarBoundary(String source, int offset) {
  if (offset <= 0 || offset >= source.length) return true;
  final previous = source.codeUnitAt(offset - 1);
  final next = source.codeUnitAt(offset);
  return !(previous >= 0xD800 &&
      previous <= 0xDBFF &&
      next >= 0xDC00 &&
      next <= 0xDFFF);
}

String _largeUnicodeText(int targetLength) {
  final output = StringBuffer();
  var index = 0;
  while (output.length < targetLength) {
    output.writeln(
      'Paragraph $index has 😀, café, βeta, **markdown**, and [link][shared].',
    );
    index += 1;
  }
  output.writeln('[shared]: https://example.com');
  return output.toString();
}
