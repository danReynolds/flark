@Tags(<String>['benchmark'])
library;

import 'dart:io';

import 'package:flark/src/v2/core/document/flark_utf8_utf16_mapper.dart';
import 'package:test/test.dart';

int _blackHole = 0;

void main() {
  for (final size in const [1000000, 5000000]) {
    test('current whole-document UTF offset index at $size ASCII units', () {
      final text = 'a' * size;
      final samples = <Duration>[];
      var slots = 0;
      for (var iteration = 0; iteration < 3; iteration += 1) {
        final stopwatch = Stopwatch()..start();
        final mapper = FlarkUtf8Utf16Mapper(text);
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
        slots = mapper.utf16Length + 1 + mapper.utf8Length + 1;
        _consume(
          mapper.utf8OffsetForUtf16Offset(size ~/ 2) +
              mapper.utf16OffsetForUtf8Offset(size - 1),
        );
      }
      samples.sort();
      stdout.writeln(
        'flark_prototype utf_offset_index_ascii '
        'utf16=$size utf8=$size mapping_slots=$slots '
        'minimum_slot_bytes=${slots * 8} '
        'median=${_fmt(samples[samples.length ~/ 2])}',
      );
    });
  }

  test('current whole-document UTF offset index at 1MB mixed units', () {
    final text = 'a😀' * 333334;
    final samples = <Duration>[];
    var utf8Length = 0;
    for (var iteration = 0; iteration < 3; iteration += 1) {
      final stopwatch = Stopwatch()..start();
      final mapper = FlarkUtf8Utf16Mapper(text);
      stopwatch.stop();
      samples.add(stopwatch.elapsed);
      utf8Length = mapper.utf8Length;
      _consume(mapper.utf8OffsetForUtf16Offset(text.length));
    }
    samples.sort();
    final slots = text.length + 1 + utf8Length + 1;
    stdout.writeln(
      'flark_prototype utf_offset_index_mixed '
      'utf16=${text.length} utf8=$utf8Length mapping_slots=$slots '
      'minimum_slot_bytes=${slots * 8} '
      'median=${_fmt(samples[samples.length ~/ 2])}',
    );
  });
}

void _consume(int value) {
  _blackHole = (_blackHole + value) & 0x3fffffff;
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
