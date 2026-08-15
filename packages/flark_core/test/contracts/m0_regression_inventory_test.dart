import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

void main() {
  final inventory = _object('test/fixtures/v4/m0_regressions_v1.json');
  final cases = _list(inventory['cases']).map(_map).toList();

  test('keeps every M0 blocker named and non-passing', () {
    expect(cases.map((entry) => entry['id']).toSet(), {
      'M0-STALL-32K-PASTE',
      'M0-CERT-EMPHASIS-CLOSER-DELETE',
      'M0-CERT-REFERENCE-RETARGET',
      'M0-GIANT-PARAGRAPH',
      'M0-GIANT-LINE-MARKER-PREFIX',
      'M0-LAZY-LIST-CONTINUATION',
      'M0-VIEWPORT-OUT-OF-AUTHORITY',
      'M0-STATUS-FAULT-NO-REASON',
      'M0-STATUS-CLOSED-ONLY',
      'M0-STATUS-AWAITING-PRESENTATION',
      'M0-STATUS-QUIESCENT-NONCURRENT',
    });
    for (final entry in cases) {
      expect(
        entry['currentStatus'],
        anyOf('known_failure', 'coverage_gap'),
        reason: '${entry['id']} cannot become green without a v4 receipt',
      );
      expect(entry['currentEvidence'], isNotEmpty);
      expect(entry['reproduction'], isNotEmpty);
      expect(entry['targetOutcome'], isNotEmpty);
      expect(entry['targetMilestone'], matches(RegExp(r'^M[2-7]$')));
    }
  });

  test('pins deterministic large-input recipes', () {
    final paste = _paragraphFixture(32 * 1024);
    _expectGenerated('M0-STALL-32K-PASTE', paste, expectedLength: 32789);

    final giantParagraph = '${'word ' * 1048576}\n';
    _expectGenerated(
      'M0-GIANT-PARAGRAPH',
      giantParagraph,
      expectedLength: 5242881,
    );

    final giantLine = '# ${'a' * 5242877}\n';
    _expectGenerated(
      'M0-GIANT-LINE-MARKER-PREFIX',
      giantLine,
      expectedLength: 5242880,
    );
  });
}

void _expectGenerated(String id, String source, {required int expectedLength}) {
  final entry = casesById[id]!;
  final generator = _map(entry['generator']);
  expect(utf8.encode(source), hasLength(expectedLength));
  expect(
    sha256.convert(utf8.encode(source)).toString(),
    generator['sha256Utf8'],
  );
}

final Map<String, Map<String, Object?>> casesById = {
  for (final entry in _list(
    _object('test/fixtures/v4/m0_regressions_v1.json')['cases'],
  ).map(_map))
    entry['id'] as String: entry,
};

String _paragraphFixture(int targetBytes) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetBytes) {
    buffer
      ..writeln(
        'Paragraph $index opens with ordinary prose and a **bold** run here.',
      )
      ..writeln(
        'It continues with _emphasis_, some `inline code`, and plain words.',
      )
      ..writeln(
        'Then a third physical line closes the paragraph with more text.',
      )
      ..writeln();
    index += 1;
  }
  return buffer.toString();
}

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();
