import 'dart:convert';

import 'package:flark_codemirror/src/mode.dart';
import 'package:test/test.dart';

final _leading = RegExp(r'^\s*');

/// Runs [mode] over a recorded case and fails at the first line whose
/// indentation queries or tokens differ from the reference's.
void checkReferenceCase(
  Mode<Object?> mode,
  Map<String, dynamic> recorded,
  List<String?> styles,
) {
  final text = recorded['text'] as String;
  if (recorded['error'] != null) {
    // Upstream throws where a mode stops advancing. readToken steps over the
    // character instead, so the port reads such a case to its end.
    expect(() => runMode(mode, text), returnsNormally);
    return;
  }
  final expected = (recorded['lines'] as List).cast<List>();
  final actual = <List<Object?>>[];
  runMode(
    mode,
    text,
    onLine: (line, lineText, state) {
      final lead = _leading.firstMatch(lineText)![0]!.length;
      actual.add([
        mode.indent(mode.copyState(state), lineText.substring(lead), lineText),
        mode.indent(mode.copyState(state), '', lineText),
      ]);
    },
    onToken: (line, start, end, style) =>
        actual[line].addAll([start, end, style]),
  );
  final lines = splitLines(text).lines;
  expect(actual.length, expected.length, reason: 'line count');
  for (var i = 0; i < expected.length; i++) {
    final want = [
      expected[i][0],
      expected[i][1],
      for (var t = 2; t < expected[i].length; t += 3) ...[
        expected[i][t],
        expected[i][t + 1],
        styles[expected[i][t + 2] as int],
      ],
    ];
    if (!_same(want, actual[i])) {
      fail(
        'line $i: ${jsonEncode(lines[i])}\n'
        'reference: ${_describe(want, lines[i])}\n'
        'port:      ${_describe(actual[i], lines[i])}',
      );
    }
  }
}

bool _same(List<Object?> a, List<Object?> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

String _describe(List<Object?> row, String line) {
  final tokens = [
    for (var t = 2; t + 2 < row.length; t += 3)
      '${jsonEncode(line.substring(row[t] as int, row[t + 1] as int))}:${row[t + 2]}',
  ];
  return 'indent ${row[0]}/${row[1]} ${tokens.join(' ')}';
}
