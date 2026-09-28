// The port against CodeMirror 5 itself: tokens and indentation recorded by
// tool/reference/generate.cjs from the upstream JavaScript.
import 'dart:convert';
import 'dart:io';

import 'package:flark_codemirror/src/mode.dart';
import 'package:flark_codemirror/src/modes/javascript.dart';
import 'package:test/test.dart';

import 'support/reference.dart';

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
      checkReferenceCase(modeFor(c['mode'] as String), c, styles);
    });
  }
}
