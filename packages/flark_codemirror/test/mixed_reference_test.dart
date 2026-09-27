// The composed modes, mixed HTML and PHP, against the same composition of
// CodeMirror's own modes: tokens and indentation recorded by
// tool/reference/generate_mixed.mjs.
import 'dart:convert';
import 'dart:io';

import 'package:flark_codemirror/src/languages.dart';
import 'package:flark_codemirror/src/mode.dart';
import 'package:test/test.dart';

import 'support/reference.dart';

void main() {
  // One fixture from a wider local run, or every committed one.
  final only = Platform.environment['FLARK_CODEMIRROR_MIXED_REFERENCE'];
  final files = only != null
      ? [File(only)]
      : (Directory('test/fixtures/mixed')
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
    // One mode runs every case in order, as the generator's does.
    final mode = codeMirrorMode(language, config)!;
    group(language, () {
      for (final c in (fixture['cases'] as List).cast<Map<String, dynamic>>()) {
        test(c['name'], () => checkReferenceCase(mode, c, styles));
      }
    });
  }
}
