// Each port of a CodeMirror 6 legacy mode against the mode itself: tokens
// and indentation recorded by tool/reference/generate_legacy.mjs.
import 'dart:convert';
import 'dart:io';

import 'package:flark_codemirror/src/languages.dart';
import 'package:flark_codemirror/src/mode.dart';
import 'package:flark_codemirror/src/modes/shell.dart';
import 'package:flark_codemirror/src/modes/xml.dart';
import 'package:test/test.dart';

import 'support/reference.dart';

/// Languages whose catalog mode adds to the recorded one: the catalog's html
/// is mixed HTML, checked by mixed_reference_test.dart, whose fixture here
/// records the xml mode's html export alone, and its bash adds indentation.
final _recordedAlone = {
  'html': (String language, ModeConfig config) =>
      XmlMode(config, XmlOptions.html),
  // The catalog's bash adds Flark's block indentation (modes/blocks.dart).
  'bash': (String language, ModeConfig config) => ShellMode(config),
};

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
    // One mode for every case, in order, as the generator runs one parser:
    // an export's closure can keep a value between tokens, and so between
    // cases (css.js's `type`).
    final mode = (_recordedAlone[language] ?? codeMirrorMode)(language, config);
    group(language, () {
      test('comes from the ported release', () {
        expect(
          fixture['source'],
          startsWith('@codemirror/legacy-modes 6.5.4 '),
        );
        expect(mode, isNotNull);
      });
      for (final c in (fixture['cases'] as List).cast<Map<String, dynamic>>()) {
        test(c['name'], () => checkReferenceCase(mode!, c, styles));
      }
    });
  }
}
