import 'package:flark/code.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../flark_tree_sitter/tool/edit_cases.dart';

({String source, FlarkSelection selection}) marked(String text) {
  final out = StringBuffer();
  var base = -1, extent = -1;
  for (final char in text.split('')) {
    if (char == '¦') {
      base = extent = out.length;
    } else if (char == '«') {
      base = out.length;
    } else if (char == '»') {
      extent = out.length;
    } else {
      out.write(char);
    }
  }
  return (source: out.toString(), selection: FlarkSelection(base, extent));
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final backend = createParseBackend();
  late FlarkTreeSitter code;
  setUpAll(() async {
    code = await FlarkTreeSitter.load();
  });
  tearDownAll(() {
    code.dispose();
  });

  for (final info in ['ruby', 'rb', '']) {
    test('Ruby owner journey from empty fence: $info', () {
      final e = FlarkEditor(
        backend,
        text: '```$info\n\n```\n\nafter',
        caret: 4 + info.length,
        codeEditing: code,
      );
      void type(String s) {
        for (final rune in s.runes) {
          e.apply(InsertText(String.fromCharCode(rune)));
        }
      }

      type('def hello');
      e.apply(const Newline());
      expect(e.document.rowAt(e.selection.extent).text, 'def hello\n  ');
      type("print 'test'");
      e.apply(const Newline());
      type('en');
      e.history.breakCoalescing();
      final before = (e.source, e.selection);
      type('d');
      final expected =
          '```$info\ndef hello\n  print \'test\'\nend\n```\n\nafter';
      expect(e.source, expected);
      expect(e.selection.extent, expected.indexOf('\nend') + 4);
      expect(e.document.rowAt(e.selection.extent).kind, RowKind.codeBlock);
      e.history.breakCoalescing();
      type(' # done');
      expect(e.source, expected.replaceFirst('\nend', '\nend # done'));
      e.apply(const Undo());
      expect(e.source, expected);
      e.apply(const Undo());
      expect((e.source, e.selection), before);
      e.apply(const Redo());
      expect(e.source, expected);
    });
  }

  test('Automatic Bash remains Bash before the final fi character', () {
    final e = FlarkEditor(
      backend,
      text: '```\n\n```\n\nafter',
      caret: 4,
      codeEditing: code,
    );
    for (final rune in 'if true; then\necho "hello"\nfi'.runes) {
      final char = String.fromCharCode(rune);
      e.apply(char == '\n' ? const Newline() : InsertText(char));
    }
    expect(e.source, '```\nif true; then\n  echo "hello"\nfi\n```\n\nafter');
    final before = e.source;
    e.history.breakCoalescing();
    e.apply(const InsertText('x'));
    expect(e.source, before.replaceFirst('\nfi', '\nfix'));
    e.apply(const Undo());
    expect(e.source, before);
  });

  for (final c in editCases) {
    // Host indentation policy is two spaces, four for Python, or an existing
    // tab. The component corpus separately qualifies arbitrary caller units.
    if (codeIndentUnit(marked(c.before).source, c.language.name) != c.unit) {
      continue;
    }
    for (final (openerPrefix, prefix, newline) in [
      ('', '', '\n'),
      ('> - ', '>   ', '\n'),
      ('> > ', '> > ', '\r\n'),
    ]) {
      test('${c.name} in $prefix ${newline.length == 2 ? 'CRLF' : 'LF'}', () {
        String wrap(String body) =>
            '$openerPrefix```${c.language.name}$newline$prefix${body.replaceAll('\r\n', '\n').replaceAll('\n', '$newline$prefix')}$newline$prefix```$newline${newline}after';
        final before = marked(wrap(c.before)), after = marked(wrap(c.after));
        final e = FlarkEditor(
          backend,
          text: before.source,
          caret: before.selection.extent,
          codeEditing: code,
        );
        e.apply(SetSelection(before.selection.base, before.selection.extent));
        final command = switch (c.action.name) {
          'insert' => InsertText(c.text),
          'newline' => const Newline(),
          'indent' => const Indent(),
          'outdent' => const Outdent(),
          _ => throw StateError('unexpected command'),
        };
        e.apply(command);
        expect((e.source, e.selection), (after.source, after.selection));
        expect(e.document.rowAt(e.selection.extent).kind, RowKind.codeBlock);
        expect(
          e.document.rowAt(e.selection.extent).shells.length,
          e.document.rowAt(e.selection.base).shells.length,
        );
        final at = e.selection.start, end = e.selection.end;
        // Break typing coalescing so next-key and command undo are distinct.
        e.history.breakCoalescing();
        expect(e.apply(const InsertText('z')), isTrue);
        expect(e.source, after.source.replaceRange(at, end, 'z'));
        expect(e.selection, FlarkSelection.collapsed(at + 1));
        expect(e.apply(const Undo()), isTrue);
        expect((e.source, e.selection), (after.source, after.selection));
        if (after.source != before.source) {
          expect(e.apply(const Undo()), isTrue);
          expect((e.source, e.selection), (before.source, before.selection));
          expect(e.apply(const Redo()), isTrue);
          expect((e.source, e.selection), (after.source, after.selection));
        }
      });
    }
  }

  test('paste and composition do not automatically outdent', () {
    const source = '```dart\nif (ok) {\n  \n```';
    final at = source.indexOf('\n```');
    for (final composing in [false, true]) {
      final e = FlarkEditor(
        backend,
        text: source,
        caret: at,
        codeEditing: code,
      );
      if (composing) e.beginComposition();
      e.apply(composing ? const InsertText('}') : const Paste('}'));
      expect(e.source, source.replaceRange(at, at, '}'));
    }
  });

  test(
    'multiline snippet paste keeps quote/list ownership and literal indentation',
    () {
      const source = '> - ```dart\n>   here\n>   ```\n\nafter';
      final at = source.indexOf('here');
      final e = FlarkEditor(
        backend,
        text: source,
        caret: at,
        codeEditing: code,
      );
      e.apply(SetSelection(at, at + 4));
      expect(e.apply(const Paste('if (ok) {\n    }')), isTrue);
      const expected =
          '> - ```dart\n>   if (ok) {\n>       }\n>   ```\n\nafter';
      expect(e.source, expected);
      expect(e.selection.extent, expected.indexOf('}') + 1);
      expect(e.document.rowAt(e.selection.extent).kind, RowKind.codeBlock);
      e.apply(const InsertText('z'));
      expect(e.source, expected.replaceFirst('}', '}z'));
      e.apply(const Undo());
      e.apply(const Undo());
      expect((e.source, e.selection), (source, FlarkSelection(at, at + 4)));
    },
  );
}
