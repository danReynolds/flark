// Each port of a CodeMirror 6 legacy mode against the mode itself: tokens
// and indentation recorded by tool/reference/generate_legacy.mjs.
import 'dart:convert';
import 'dart:io';

import 'package:flark_codemirror/src/languages.dart';
import 'package:flark_codemirror/src/mode.dart';
import 'package:test/test.dart';

import 'support/reference.dart';

void main() {
  // One fixture from a wider local run, or every committed one.
  final only = Platform.environment['FLARK_CODEMIRROR_LEGACY_REFERENCE'];
  final files = only != null
      ? [File(only)]
      : (Directory('test/fixtures/legacy')
            .listSync()
            .whereType<File>()
            .where((f) => f.path.endsWith('.json'))
            .toList()
          ..sort((a, b) => a.path.compareTo(b.path)));
  for (final file in files) {
    final fixture = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
    final language = fixture['language'] as String;
    final styles = (fixture['styles'] as List).cast<String?>();
    final config = ModeConfig(
      indentUnit: fixture['indentUnit'] as int,
      tabSize: fixture['tabSize'] as int,
    );
    group(language, () {
      test('comes from the ported release', () {
        expect(
          fixture['source'],
          startsWith('@codemirror/legacy-modes 6.5.4 '),
        );
        expect(codeMirrorMode(language, config), isNotNull);
      });
      for (final c in (fixture['cases'] as List).cast<Map<String, dynamic>>()) {
        test(c['name'], () {
          checkReferenceCase(codeMirrorMode(language, config)!, c, styles);
        });
      }
    });
  }
}
