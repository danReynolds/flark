import 'dart:convert';

import 'package:flark_peer_supereditor/src/competitor_fixture.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('ordinary-prose generation is deterministic and byte exact', () {
    for (final bytes in [1, 7, 1024, 1048576]) {
      final fixture = generateOrdinaryProseFixture(bytes);
      expect(utf8.encode(fixture), hasLength(bytes));
      expect(generateOrdinaryProseFixture(bytes), fixture);
    }
  });

  test('SuperEditor mapping round trips every newline edge', () {
    for (final source in [
      'a',
      'a\n',
      'a\n\n',
      '\n',
      '\n\n',
      generateOrdinaryProseFixture(1024),
    ]) {
      expect(exportExactSource(documentFromExactSource(source)), source);
    }
  });

  test('source caret mapping covers start, middle, newline, and end', () {
    final document = documentFromExactSource('abc\ndef\n');
    final start = sourceCaretAt(document, 0);
    final newline = sourceCaretAt(document, 4);
    final end = sourceCaretAt(document, 8);

    expect(start.nodeId, 'source-line-0');
    expect(start.nodeOffset, 0);
    expect(newline.nodeId, 'source-line-1');
    expect(newline.nodeOffset, 0);
    expect(end.nodeId, 'source-line-2');
    expect(end.nodeOffset, 0);
  });
}
