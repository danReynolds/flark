import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:characters/characters.dart';
import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  final matrix = _object('test/fixtures/v4/unicode_matrix_v1.json');

  test('public String opening boundaries reject malformed UTF-16 eagerly', () {
    for (final source in [
      String.fromCharCode(0xd800),
      String.fromCharCode(0xdc00),
    ]) {
      expect(
        () => FlarkCoreDocument.open(source),
        throwsA(_invalidHostUtf16<FlarkCoreNativeException>()),
      );
      expect(
        () => FlarkCoreDocument.openStreaming(source),
        throwsA(_invalidHostUtf16<FlarkCoreNativeException>()),
      );
      expect(
        () => FlarkNativeDocument.open(source),
        throwsA(_invalidHostUtf16<FlarkNativeException>()),
      );
    }
  });

  test('pins exact bytes, UTF-16 units, and Unicode 16 graphemes', () {
    final grapheme = _map(matrix['grapheme']);
    expect(grapheme['dartPackageVersion'], '1.4.1');
    expect(grapheme['unicodeVersion'], '16.0.0');

    for (final rawCase in _list(matrix['validCases'])) {
      final entry = _map(rawCase);
      final source = entry['source'] as String;
      expect(
        _hex(utf8.encode(source)),
        entry['utf8Hex'],
        reason: '${entry['id']} UTF-8 drifted',
      );
      expect(source.codeUnits, entry['utf16']);
      expect(
        _isWellFormedUtf16(source.codeUnits),
        isTrue,
        reason: '${entry['id']} valid source must contain paired surrogates',
      );
      expect(_graphemeBoundaries(source), entry['graphemeBoundariesUtf16']);
      if (entry['lineStartsUtf8'] != null || entry['lineStartsUtf16'] != null) {
        expect(
          _lineStartsUtf8(source),
          entry['lineStartsUtf8'],
          reason: '${entry['id']} UTF-8 line starts drifted',
        );
        expect(
          _lineStartsUtf16(source),
          entry['lineStartsUtf16'],
          reason: '${entry['id']} UTF-16 line starts drifted',
        );
      }
    }

    final nonAsciiLines = _list(
      matrix['validCases'],
    ).map(_map).singleWhere((entry) => entry['id'] == 'U-NONASCII-LINE-STARTS');
    expect(
      nonAsciiLines['lineStartsUtf8'],
      isNot(nonAsciiLines['lineStartsUtf16']),
      reason: 'byte and UTF-16 line coordinates must be distinct authorities',
    );
  });

  test('keeps normalization-distinct source byte sequences distinct', () {
    final cases = {
      for (final entry in _list(matrix['validCases']).map(_map))
        entry['id'] as String: entry,
    };
    expect(
      cases['U-NFD-E-ACUTE']!['source'],
      isNot(cases['U-NFC-E-ACUTE']!['source']),
    );
    expect(
      cases['U-NFD-E-ACUTE']!['utf8Hex'],
      isNot(cases['U-NFC-E-ACUTE']!['utf8Hex']),
    );
  });

  test('pins bounded oversized-cluster and invalid-input outcomes', () {
    final generated = _map(_list(matrix['generatedCases']).single);
    final recipe = _map(generated['generator']);
    final source =
        '${recipe['prefix']}${String.fromCharCode(recipe['repeatScalar'] as int) * (recipe['repeat'] as int)}';
    expect(source.codeUnits, hasLength(generated['utf16Length'] as int));
    expect(_graphemeBoundaries(source), generated['graphemeBoundariesUtf16']);
    expect(generated['boundedLookupResult'], 'needs_more_context');

    final invalid = _list(matrix['invalidHostInputs']).map(_map).toList();
    expect(
      invalid.map((entry) => entry['id']).toSet(),
      hasLength(invalid.length),
    );
    expect(invalid.map((entry) => entry['result']).toSet(), {
      'invalid_utf8',
      'invalid_utf16',
      'invalid_coordinate',
    });
    var executedInvalidCases = 0;
    for (final entry in invalid) {
      if (entry['utf8Hex'] != null) {
        expect(entry['result'], 'invalid_utf8');
        expect(
          () => utf8.decode(
            _bytes(entry['utf8Hex'] as String),
            allowMalformed: false,
          ),
          throwsFormatException,
          reason: '${entry['id']} must fail strict UTF-8 decoding',
        );
        executedInvalidCases += 1;
        continue;
      }
      if (entry['utf16'] != null) {
        expect(entry['result'], 'invalid_utf16');
        expect(
          _isWellFormedUtf16(_ints(entry['utf16'])),
          isFalse,
          reason: '${entry['id']} must fail host UTF-16 validation',
        );
        executedInvalidCases += 1;
        continue;
      }
      if (entry['editRangeUtf16'] != null) {
        expect(entry['result'], 'invalid_coordinate');
        final units = (entry['source']! as String).codeUnits;
        expect(_isWellFormedUtf16(units), isTrue);
        final range = _ints(entry['editRangeUtf16']);
        expect(range, hasLength(2));
        expect(
          range.every((offset) => _isUtf16ScalarBoundary(units, offset)),
          isFalse,
          reason: '${entry['id']} must execute a split-surrogate rejection',
        );
        executedInvalidCases += 1;
        continue;
      }
      fail('${entry['id']} has no executable invalid-input representation');
    }
    expect(executedInvalidCases, invalid.length);

    final emojiUnits = '😀'.codeUnits;
    expect(_isUtf16ScalarBoundary(emojiUnits, 0), isTrue);
    expect(_isUtf16ScalarBoundary(emojiUnits, 1), isFalse);
    expect(_isUtf16ScalarBoundary(emojiUnits, 2), isTrue);
  });
}

Matcher _invalidHostUtf16<T>() => isA<T>()
    .having(
      (error) => switch (error) {
        FlarkCoreNativeException() => error.status,
        FlarkNativeException() => error.status,
        _ => -1,
      },
      'status',
      0x020b,
    )
    .having(
      (error) => switch (error) {
        FlarkCoreNativeException() => error.detail,
        FlarkNativeException() => error.detail,
        _ => -1,
      },
      'invalid UTF-16 offset',
      0,
    );

List<int> _lineStartsUtf8(String source) {
  final bytes = utf8.encode(source);
  final starts = <int>[0];
  var offset = 0;
  while (offset < bytes.length) {
    if (bytes[offset] == 0x0d) {
      offset += offset + 1 < bytes.length && bytes[offset + 1] == 0x0a ? 2 : 1;
      starts.add(offset);
    } else if (bytes[offset] == 0x0a) {
      offset += 1;
      starts.add(offset);
    } else {
      offset += 1;
    }
  }
  return starts;
}

List<int> _lineStartsUtf16(String source) {
  final units = source.codeUnits;
  final starts = <int>[0];
  var offset = 0;
  while (offset < units.length) {
    if (units[offset] == 0x0d) {
      offset += offset + 1 < units.length && units[offset + 1] == 0x0a ? 2 : 1;
      starts.add(offset);
    } else if (units[offset] == 0x0a) {
      offset += 1;
      starts.add(offset);
    } else {
      offset += 1;
    }
  }
  return starts;
}

bool _isWellFormedUtf16(List<int> units) {
  var offset = 0;
  while (offset < units.length) {
    final unit = units[offset];
    if (unit < 0 || unit > 0xffff || _isLowSurrogate(unit)) return false;
    if (_isHighSurrogate(unit)) {
      if (offset + 1 >= units.length || !_isLowSurrogate(units[offset + 1])) {
        return false;
      }
      offset += 2;
    } else {
      offset += 1;
    }
  }
  return true;
}

bool _isUtf16ScalarBoundary(List<int> units, int offset) {
  if (offset < 0 || offset > units.length) return false;
  if (offset == 0 || offset == units.length) return true;
  return !(_isHighSurrogate(units[offset - 1]) &&
      _isLowSurrogate(units[offset]));
}

bool _isHighSurrogate(int unit) => unit >= 0xd800 && unit <= 0xdbff;

bool _isLowSurrogate(int unit) => unit >= 0xdc00 && unit <= 0xdfff;

List<int> _graphemeBoundaries(String source) {
  final result = <int>[0];
  var utf16 = 0;
  for (final character in source.characters) {
    utf16 += character.length;
    result.add(utf16);
  }
  return result;
}

String _hex(List<int> bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();

Uint8List _bytes(String value) => Uint8List.fromList([
  for (var index = 0; index < value.length; index += 2)
    int.parse(value.substring(index, index + 2), radix: 16),
]);

List<int> _ints(Object? value) => _list(value).cast<int>();

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();
