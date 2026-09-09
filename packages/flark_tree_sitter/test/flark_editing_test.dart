import 'package:flark/flark.dart';
import 'package:test/test.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';

void main() {
  final service = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(service.dispose);
  final backend = createParseBackend();
  for (final newline in ['\n', '\r\n']) {
    test(
      'multiline code paste retains container prefixes with ${newline.length}-unit newline',
      () {
        final source =
            '> - ```text$newline>   old$newline>   ```$newline${newline}after';
        final at = source.indexOf('old');
        final e = FlarkEditor(
          backend,
          codeEditing: service,
          text: source,
          caret: at,
        );
        e.apply(SetSelection(at + 3, at));
        expect(e.apply(const Paste('x\r\n  }\r\ny')), isTrue);
        final expected =
            '> - ```text$newline>   x$newline>     }$newline>   y$newline>   ```$newline${newline}after';
        expect(e.source, expected);
        expect(e.selection.extent, expected.indexOf('$newline>   ```'));
        expect(e.document.rowAt(e.selection.extent).text, 'x\n  }\ny');
        e.apply(const InsertText('z'));
        expect(
          e.source,
          expected.replaceFirst('$newline>   y', '$newline>   yz'),
        );
        e.apply(const Undo());
        e.apply(const Undo());
        expect((e.source, e.selection), (source, FlarkSelection(at + 3, at)));
      },
    );
  }
  test(
    'language override changes only the info token and preserves selection/history',
    () {
      const source = '> ```js title="example"\n> const x = 1;\n> ```\n\nafter';
      final start = source.indexOf('const'), end = start + 5;
      final e = FlarkEditor(
        backend,
        codeEditing: service,
        text: source,
        caret: start,
      );
      e.apply(SetSelection(end, start));
      expect(e.apply(const SetCodeLanguage('python')), isTrue);
      expect(e.source, source.replaceFirst('js', 'python'));
      expect(e.selection, FlarkSelection(end + 4, start + 4));
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection), (source, FlarkSelection(end, start)));
      expect(e.apply(const Redo()), isTrue);
      expect(e.apply(const SetCodeLanguage('')), isTrue);
      expect(e.source, source.replaceFirst('js', 'auto'));
      final before = e.snapshot;
      expect(e.apply(const SetCodeLanguage('dart\n```')), isFalse);
      expect(e.snapshot, same(before));
    },
  );
  test(
    'empty tag can be selected and cleared without moving the body caret',
    () {
      final e = FlarkEditor(
        backend,
        codeEditing: service,
        text: '```\nx\n```',
        caret: 5,
      );
      expect(e.apply(const SetCodeLanguage('dart')), isTrue);
      expect((e.source, e.selection.extent), ('```dart\nx\n```', 9));
      expect(e.apply(const SetCodeLanguage('')), isTrue);
      expect((e.source, e.selection.extent), ('```\nx\n```', 5));
    },
  );
  for (final (prefix, body, language, expected, caretBody) in [
    ('', '  x', 'dart', '  x\n  ', 6),
    ('', 'if (x) {}', 'dart', 'if (x) {\n  \n}', 11),
    ('', 'def f():', 'python', 'def f():\n    ', 13),
    ('', '\tvalue', 'go', '\tvalue\n\t', 8),
    ('> ', '  if (x) {', 'dart', '  if (x) {\n>     ', 17),
    ('', '// {', 'dart', '// {\n', 5),
    ('', 'print("{");', 'dart', 'print("{");\n', 12),
  ]) {
    test('Return indents code with $language $prefix $body', () {
      final before = '$prefix```$language\n$prefix';
      final source = '$before$body\n$prefix```\n\nafter';
      final caret =
          before.length + (body.endsWith('{}') ? body.length - 1 : body.length);
      final e = FlarkEditor(
        backend,
        codeEditing: service,
        text: source,
        caret: caret,
      );
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, '$before$expected\n$prefix```\n\nafter');
      expect(e.selection.extent, before.length + caretBody);
      expect(e.document.rowAt(e.selection.extent).kind, RowKind.codeBlock);
      expect(e.apply(const InsertText('z')), isTrue);
      expect(e.apply(const Undo()), isTrue);
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (source, caret));
    });
  }
  test(
    'Tab and Shift-Tab operate on code lines and retain backward selection',
    () {
      const source = '> ```dart\n> one\n> two\n> ```\n\nafter';
      final first = source.indexOf('one'), end = source.indexOf('two') + 3;
      final e = FlarkEditor(
        backend,
        codeEditing: service,
        text: source,
        caret: first,
      );
      e.apply(SetSelection(end, first));
      expect(e.apply(const Indent()), isTrue);
      expect(e.source, '> ```dart\n>   one\n>   two\n> ```\n\nafter');
      expect(e.selection, FlarkSelection(end + 4, first + 2));
      expect(e.apply(const Outdent()), isTrue);
      expect((e.source, e.selection), (source, FlarkSelection(end, first)));
      e.apply(SetSelection.caret(first + 1));
      expect(e.apply(const Indent()), isTrue);
      expect(e.source, source.replaceFirst('one', 'o  ne'));
      expect(e.selection.extent, first + 3);
    },
  );
  test('indentation rejects selections crossing a code-block boundary', () {
    for (final source in ['    code\n\nafter', '```dart\ncode\n```\n\nafter']) {
      final code = source.indexOf('code'), after = source.indexOf('after');
      for (final selection in [
        FlarkSelection(code, after + 2),
        FlarkSelection(after + 2, code),
      ]) {
        final e = FlarkEditor(
          backend,
          codeEditing: service,
          text: source,
          caret: code,
        );
        e.apply(SetSelection(selection.base, selection.extent));
        final before = e.snapshot;
        expect(e.apply(const Indent()), isFalse);
        expect(e.snapshot, same(before));
        expect(e.apply(const Outdent()), isFalse);
        expect(e.snapshot, same(before));
      }
    }
  });
  test('Tab then Shift-Tab restores an already indented selected line', () {
    for (final (language, indentation) in [
      ('dart', '  '),
      ('python', '    '),
    ]) {
      final source = '```$language\n${indentation}print(42);\n```';
      final first = source.indexOf('\n') + 1, end = source.indexOf(';') + 1;
      final e = FlarkEditor(
        backend,
        codeEditing: service,
        text: source,
        caret: first,
      );
      e.apply(SetSelection(end, first));
      expect(e.apply(const Indent()), isTrue);
      expect(e.apply(const Outdent()), isTrue);
      expect((e.source, e.selection), (source, FlarkSelection(end, first)));
    }
  });
}
