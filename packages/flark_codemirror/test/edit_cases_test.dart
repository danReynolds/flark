// Flark's hand-authored JavaScript, TypeScript and JSON editing scenarios,
// from flark_tree_sitter/tool/edit_cases.dart, run through the port.
// The caret is marked with a broken bar; guillemets mark base and extent.
import 'package:flark/code.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:test/test.dart';

const enter = CodeEditingAction.newline,
    type = CodeEditingAction.insert,
    tab = CodeEditingAction.indent,
    backtab = CodeEditingAction.outdent;

final class EditCase {
  const EditCase(
    this.name,
    this.language,
    this.before,
    this.action,
    this.after, {
    this.text = '',
    this.unit = '  ',
  });
  final String name, language, before, after, text, unit;
  final CodeEditingAction action;
}

const cases = [
  EditCase(
    'JSON Enter before first string',
    'json',
    '{¦"answer": 42}',
    enter,
    '{\n  ¦"answer": 42}',
  ),
  EditCase(
    "typescript Enter header",
    'typescript',
    "function hello(): void {¦",
    enter,
    "function hello(): void {\n  ¦",
  ),
  EditCase(
    "typescript typed closer",
    'typescript',
    "function hello(): void {\n  ¦",
    type,
    "function hello(): void {\n}¦",
    text: "}",
  ),
  EditCase(
    "typescript complete literal",
    'typescript',
    "const text: string = \"{\";¦",
    enter,
    "const text: string = \"{\";\n¦",
  ),
  EditCase(
    "typescript preserve body indent",
    'typescript',
    "function hello(): void {\n  value¦",
    enter,
    "function hello(): void {\n  value\n  ¦",
  ),
  EditCase(
    "typescript selected lines tab",
    'typescript',
    "«first\nsecond»",
    tab,
    "  «first\n  second»",
  ),
  EditCase(
    "typescript selected lines outdent",
    'typescript',
    "  «first\n  second»",
    backtab,
    "«first\nsecond»",
  ),
  EditCase(
    "json Enter header",
    'json',
    "{\"items\": [¦",
    enter,
    "{\"items\": [\n  ¦",
  ),
  EditCase(
    "json typed closer",
    'json',
    "{\"items\": [\n  ¦",
    type,
    "{\"items\": [\n]¦",
    text: "]",
  ),
  EditCase("json complete literal", 'json', "\"{\"¦", enter, "\"{\"\n¦"),
  EditCase(
    "json preserve body indent",
    'json',
    "{\"items\": [\n  value¦",
    enter,
    "{\"items\": [\n  value\n  ¦",
  ),
  EditCase(
    "json selected lines tab",
    'json',
    "«first\nsecond»",
    tab,
    "  «first\n  second»",
  ),
  EditCase(
    "json selected lines outdent",
    'json',
    "  «first\n  second»",
    backtab,
    "«first\nsecond»",
  ),
  EditCase(
    'empty line retains exact mixed indent',
    'javascript',
    ' \t ¦',
    enter,
    ' \t \n \t ¦',
  ),
  // Changed from Flark's Tree-sitter behaviour, which left a closer without
  // an opener where it was typed: CodeMirror indents it for the block it
  // closes, here the top level.
  EditCase(
    'unmatched closer takes the enclosing indentation',
    'javascript',
    '  ¦',
    type,
    '}¦',
    text: '}',
  ),
  EditCase(
    'nested closer skips string and comment brackets',
    'javascript',
    'if (x) {\n  f("}"); // }\n  ¦',
    type,
    'if (x) {\n  f("}"); // }\n}¦',
    text: '}',
  ),
  EditCase(
    'JS Enter',
    'javascript',
    'if (ready) {¦',
    enter,
    'if (ready) {\n  ¦',
  ),
  EditCase(
    'JS brace outdent',
    'javascript',
    'if (ready) {\n  ¦',
    type,
    'if (ready) {\n}¦',
    text: '}',
  ),
  EditCase(
    'JS regex braces',
    'javascript',
    'const x = /[{}]/;¦',
    enter,
    'const x = /[{}]/;\n¦',
  ),
  EditCase(
    'JS unfinished block comment',
    'javascript',
    'if (ready) {\n  /* {¦',
    enter,
    'if (ready) {\n  /* {\n  ¦',
  ),
  EditCase(
    'JS unfinished quote',
    'javascript',
    'if (ready) {\n  const s = "{¦',
    enter,
    'if (ready) {\n  const s = "{\n  ¦',
  ),
  EditCase(
    'JS template text',
    'javascript',
    'const s = `hello\n  ¦`; ',
    type,
    'const s = `hello\n  }¦`; ',
    text: '}',
  ),
  EditCase(
    'JS template expression',
    'javascript',
    'const s = `hello \${f({¦})}`;',
    enter,
    'const s = `hello \${f({\n  ¦\n})}`;',
  ),
  EditCase(
    'JS argument continuation',
    'javascript',
    'f(one,¦',
    enter,
    'f(one,\n  ¦',
  ),
  EditCase(
    'JS mismatched closer preserves whitespace',
    'javascript',
    'f([\n  ¦',
    type,
    'f([\n  }¦',
    text: '}',
  ),
];

({String source, int base, int extent}) marked(String value) {
  final caret = value.indexOf('\u00a6');
  if (caret >= 0) {
    return (
      source: value.replaceFirst('\u00a6', ''),
      base: caret,
      extent: caret,
    );
  }
  final base = value.indexOf('\u00ab'), extent = value.indexOf('\u00bb');
  return (
    source: value.replaceAll('\u00ab', '').replaceAll('\u00bb', ''),
    base: base - (extent < base ? 1 : 0),
    extent: extent - (base < extent ? 1 : 0),
  );
}

String show(String source, int base, int extent) {
  if (base == extent) return source.replaceRange(base, base, '\u00a6');
  final lo = base < extent ? base : extent, hi = base < extent ? extent : base;
  return source.replaceRange(hi, hi, '\u00bb').replaceRange(lo, lo, '\u00ab');
}

void main() {
  final delegate = FlarkCodeMirror();
  CodeEditProposal propose(
    EditCase c,
    String source,
    int base,
    int extent,
    CodeEditingAction action,
    String text,
  ) => delegate.propose(
    source,
    language: c.language,
    base: base,
    extent: extent,
    action: action,
    text: text,
    indentUnit: c.unit,
  )!;
  for (final c in cases) {
    test(c.name, () {
      final before = marked(c.before), after = marked(c.after);
      final edit = propose(
        c,
        before.source,
        before.base,
        before.extent,
        c.action,
        c.text,
      );
      final source = before.source.replaceRange(
        edit.start,
        edit.end,
        edit.text,
      );
      expect(
        show(source, edit.base, edit.extent),
        show(after.source, after.base, after.extent),
      );
      // The next typed character lands at the promised selection.
      final next = propose(c, source, edit.base, edit.extent, type, 'z');
      final lo = edit.base < edit.extent ? edit.base : edit.extent;
      expect(
        source.replaceRange(next.start, next.end, next.text),
        source.replaceRange(
          lo,
          edit.base < edit.extent ? edit.extent : edit.base,
          'z',
        ),
      );
    });
  }
}
