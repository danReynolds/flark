import 'dart:convert';

import 'package:flark_peer_benchmark/profile_fixture.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('ordinary-prose generator is exact at all competitor tiers', () {
    for (final bytes in const [0, 1, 1024, 1048576, 5242880, 10485760]) {
      final source = generateOrdinaryProseExact(bytes);
      expect(source.length, bytes);
      expect(utf8.encode(source), hasLength(bytes));
      expect(source, _reference(bytes));
    }
  });

  test(
    'fidelity comparison identifies Quill terminal-newline normalization',
    () {
      final comparison = compareSource(expected: 'source', actual: 'source\n');
      expect(comparison['exact'], isFalse);
      expect(comparison['classification'], 'peer-appended-terminal-newline');
      expect(comparison['firstDifferenceUtf16Offset'], 6);
      expect(comparison['lengthDeltaUtf16'], 1);
    },
  );
}

String _reference(int targetBytes) {
  final buffer = StringBuffer();
  while (buffer.length < targetBytes) {
    buffer.write(ordinaryProseCycle);
  }
  return buffer.toString().substring(0, targetBytes);
}
