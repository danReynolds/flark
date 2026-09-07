/// Package boundaries: pure Dart, and a public surface small enough to read.
library;

import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('the kernel never imports Flutter', () {
    for (final f in Directory(
      'lib',
    ).listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart')) continue;
      expect(
        f.readAsStringSync(),
        isNot(contains('package:flutter')),
        reason: f.path,
      );
    }
  });

  test('the facade library exports a bounded set of declarations', () {
    final exported = _publicExports(File('lib/flark.dart'));
    // Commands are one concept counted once; the rest are the facade, the
    // document, projection rows and their parts, history, and the backend.
    final commands = exported
        .where(
          (n) =>
              n == 'FlarkCommand' ||
              n == 'MoveUnit' ||
              n == 'MoveDirection' ||
              const {
                'InsertText',
                'DeleteBackward',
                'DeleteForward',
                'Newline',
                'ReplaceRange',
                'SetSelection',
                'SelectAll',
                'PlaceCaret',
                'MoveCaret',
                'Undo',
                'Redo',
                'ToggleTask',
                'Indent',
                'Outdent',
                'ToggleStyle',
                'SetHeadingLevel',
                'SetCodeLanguage',
                'Paste',
              }.contains(n),
        )
        .length;
    final concepts = exported.length - commands + 1;
    // The union includes both conditional parse transports; one platform sees
    // only one of FfiParseBackend and WasmParseBackend at a time.
    // The Flutter host names two more concepts: shared admission limits and
    // rejection reasons. Revision and composition stay on the existing facade.
    expect(concepts, lessThanOrEqualTo(29), reason: 'exported: $exported');
  });
}

Set<String> _publicExports(File library, [Set<String>? visited]) {
  visited ??= <String>{};
  if (!visited.add(library.absolute.path)) return const {};
  final text = library.readAsStringSync();
  final names = <String>{};
  final exports = RegExp(r"export\s+'[^']+'[^;]*;", multiLine: true);
  for (final match in exports.allMatches(text)) {
    final directive = match.group(0)!;
    final shown = RegExp(r'\bshow\s+([^;]+)').firstMatch(directive)?.group(1);
    if (shown != null) {
      names.addAll(shown.split(',').map((name) => name.trim()));
      continue;
    }
    for (final uri in RegExp(r"'([^']+)'").allMatches(directive)) {
      names.addAll(
        _publicExports(
          File.fromUri(library.uri.resolve(uri.group(1)!)),
          visited,
        ),
      );
    }
  }
  final types = RegExp(
    r'^(?:abstract\s+|final\s+|sealed\s+|base\s+)*(?:class|enum|typedef|extension type(?:\s+const)?|mixin)\s+([A-Za-z_]\w*)',
    multiLine: true,
  );
  names.addAll(
    types
        .allMatches(text)
        .map((match) => match.group(1)!)
        .where((name) => !name.startsWith('_')),
  );
  final functions = RegExp(
    r'^(?:[A-Za-z_]\w*(?:<[^>{};\n]+>)?\??\s+)+([A-Za-z_]\w*)\s*\(',
    multiLine: true,
  );
  names.addAll(
    functions
        .allMatches(text)
        .map((match) => match.group(1)!)
        .where((name) => !name.startsWith('_')),
  );
  return names;
}
