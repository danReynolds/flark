/// Package boundaries: pure Dart, and a public surface small enough to read.
library;

import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('the kernel never imports Flutter', () {
    for (final f in Directory('lib').listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart')) continue;
      expect(f.readAsStringSync(), isNot(contains('package:flutter')), reason: f.path);
    }
  });

  test('the facade library exports a bounded set of declarations', () {
    final exported = <String>[];
    final directives = RegExp(r"export '([^']+)'(?:\s+show\s+([^;]+))?;");
    for (final m in directives.allMatches(File('lib/flark.dart').readAsStringSync())) {
      final shown = m.group(2);
      if (shown != null) { exported.addAll(shown.split(',').map((s) => s.trim())); continue; }
      final text = File('lib/${m.group(1)}').readAsStringSync();
      final decl = RegExp(r'^(?:abstract\s+|final\s+|sealed\s+|base\s+)*(?:class|enum|typedef|extension type(?:\s+const)?|mixin)\s+([A-Za-z_]\w*)', multiLine: true);
      exported.addAll(decl.allMatches(text).map((d) => d.group(1)!).where((n) => !n.startsWith('_')));
    }
    // Commands are one concept counted once; the rest are the facade, the
    // document, projection rows and their parts, history, and the backend.
    final commands = exported.where((n) => n == 'FlarkCommand' || n == 'MoveUnit' || n == 'MoveDirection' || const {'InsertText', 'DeleteBackward', 'DeleteForward', 'Newline', 'ReplaceRange', 'SetSelection', 'PlaceCaret', 'MoveCaret', 'Undo', 'Redo', 'ToggleTask', 'Indent', 'Outdent', 'ToggleStyle', 'SetHeadingLevel', 'Paste'}.contains(n)).length;
    final concepts = exported.length - commands + 1;
    expect(concepts, lessThanOrEqualTo(24), reason: 'exported: $exported');
  });
}
