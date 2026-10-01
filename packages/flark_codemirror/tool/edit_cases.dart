// Flark's hand-authored JavaScript, TypeScript and JSON editing scenarios,
// first written for flark_tree_sitter, as CodeMirror's modes answer them.
// test/edit_cases_test.dart runs them through the delegate and the Flutter
// host's code editing test through its containers. The caret is marked with
// a broken bar; guillemets mark base and extent.
import 'package:flark/code.dart';

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
    this.next,
  });
  final String name, language, before, after, text, unit;
  final CodeEditingAction action;

  /// [after] once `z` is typed at its selection, where that also re-indents:
  /// a closing word electric input outdented that `z` makes a longer word.
  final String? next;
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
  // Bash, as Flark indents its blocks over CodeMirror's shell mode.
  EditCase(
    'Bash typed else aligns with then header',
    'bash',
    'if ready; then\n  echo ok\n  els¦',
    type,
    'if ready; then\n  echo ok\nelse¦',
    text: 'e',
    // `elsez` is a command in the then-branch, not the else that ends it.
    next: 'if ready; then\n  echo ok\n  elsez¦',
  ),
  EditCase(
    'Bash elif body Enter',
    'bash',
    'if ready; then\n  echo ok\nelif other; then¦',
    enter,
    'if ready; then\n  echo ok\nelif other; then\n  ¦',
  ),
  EditCase(
    'Bash fi after branches',
    'bash',
    'if ready; then\n  echo ok\nelif other; then\n  echo other\nelse\n  echo no\n  f¦',
    type,
    'if ready; then\n  echo ok\nelif other; then\n  echo other\nelse\n  echo no\nfi¦',
    text: 'i',
    // Only `fi` closes the if; `fiz` is one more command in the else-branch.
    next:
        'if ready; then\n  echo ok\nelif other; then\n  echo other\nelse\n  echo no\n  fiz¦',
  ),
  EditCase(
    "bash Enter header",
    'bash',
    "if ready; then¦",
    enter,
    "if ready; then\n  ¦",
  ),
  EditCase(
    "bash typed closer",
    'bash',
    "if ready; then\n  f¦",
    type,
    "if ready; then\nfi¦",
    text: "i",
    // `fiz` closes nothing, so it returns to the body like any command.
    next: "if ready; then\n  fiz¦",
  ),
  EditCase(
    'Bash command that starts like do stays in its block',
    'bash',
    'for f in *; do\ndo¦',
    type,
    'for f in *; do\n  doc¦',
    text: 'c',
  ),
  EditCase(
    'Ruby name that starts like end stays in its method',
    'ruby',
    'def area\nend¦',
    type,
    'def area\n  endp¦',
    text: 'p',
  ),
  EditCase(
    "bash complete literal",
    'bash',
    "echo \"{\"¦",
    enter,
    "echo \"{\"\n¦",
  ),
  EditCase(
    "bash preserve body indent",
    'bash',
    "if ready; then\n  value¦",
    enter,
    "if ready; then\n  value\n  ¦",
  ),
  EditCase(
    "bash selected lines tab",
    'bash',
    "«first\nsecond»",
    tab,
    "  «first\n  second»",
  ),
  EditCase(
    "bash selected lines outdent",
    'bash',
    "  «first\n  second»",
    backtab,
    "«first\nsecond»",
  ),
  EditCase(
    "bash for x in items; do¦",
    'bash',
    "for x in items; do¦",
    enter,
    "for x in items; do\n  ¦",
  ),
  EditCase(
    "bash case \"\$x\" in¦",
    'bash',
    "case \"\$x\" in¦",
    enter,
    "case \"\$x\" in\n  ¦",
  ),
  EditCase(
    "bash if ready; then\nelse¦",
    'bash',
    "if ready; then\nelse¦",
    enter,
    "if ready; then\nelse\n  ¦",
  ),
];
