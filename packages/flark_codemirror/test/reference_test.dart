// The port against CodeMirror 5 itself: tokens and indentation recorded by
// tool/reference/generate.cjs from the upstream JavaScript.
import 'dart:convert';
import 'dart:io';

import 'package:flark_codemirror/src/mode.dart';
import 'package:flark_codemirror/src/modes/javascript.dart';
import 'package:test/test.dart';

final _leading = RegExp(r'^\s*');

void main() {
  final path =
      Platform.environment['FLARK_CODEMIRROR_REFERENCE'] ??
      'test/fixtures/reference.json';
  final fixture =
      jsonDecode(File(path).readAsStringSync()) as Map<String, dynamic>;
  final styles = (fixture['styles'] as List).cast<String?>();
  final config = ModeConfig(
    indentUnit: fixture['indentUnit'] as int,
    tabSize: fixture['tabSize'] as int,
  );
  JavaScriptMode modeFor(String name) => JavaScriptMode(config, switch (name) {
    'javascript' => const JavaScriptOptions(),
    'typescript' => const JavaScriptOptions(typescript: true),
    'json' => const JavaScriptOptions(json: true),
    'jsonld' => const JavaScriptOptions(jsonld: true),
    _ => throw ArgumentError(name),
  });

  test('the fixture comes from the ported release', () {
    expect(fixture['codemirror'], '5.65.21');
  });

  for (final c in (fixture['cases'] as List).cast<Map<String, dynamic>>()) {
    test(c['name'], () {
      final mode = modeFor(c['mode'] as String);
      final text = c['text'] as String;
      if (c['error'] != null) {
        expect(() => runMode(mode, text), throwsA(anything));
        return;
      }
      final expected = (c['lines'] as List).cast<List>();
      final actual = <List<Object?>>[];
      runMode(
        mode,
        text,
        tabSize: config.tabSize,
        onLine: (line, lineText, state) {
          final lead = _leading.firstMatch(lineText)![0]!.length;
          actual.add([
            mode.indent(
              mode.copyState(state),
              lineText.substring(lead),
              lineText,
            ),
            mode.indent(mode.copyState(state), '', lineText),
          ]);
        },
        onToken: (line, start, end, style) =>
            actual[line].addAll([start, end, style]),
      );
      final lines = text.split(lineBreak);
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
            'CodeMirror: ${_describe(want, lines[i])}\n'
            'port:       ${_describe(actual[i], lines[i])}',
          );
        }
      }
    });
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
    for (var t = 2; t + 2 < row.length + 1 && t + 2 < row.length + 3; t += 3)
      if (t + 2 < row.length)
        '${jsonEncode(line.substring(row[t] as int, row[t + 1] as int))}:${row[t + 2]}',
  ];
  return 'indent ${row[0]}/${row[1]} ${tokens.join(' ')}';
}
