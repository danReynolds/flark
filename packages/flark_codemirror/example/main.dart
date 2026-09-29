// Highlights a snippet, names the language of an unlabeled one, and proposes
// the edit for Return in a Python block: the calls Flark's kernel makes on a
// code fence, here without an editor.
import 'dart:convert';

import 'package:flark/code.dart';
import 'package:flark_codemirror/flark_codemirror.dart';

void main() {
  final code = FlarkCodeMirror();

  const python = 'def greet(name):\n    return f"hello {name}"  # greet\n';
  for (final token in code.highlight(python, 'python').tokens) {
    final role = codeSyntaxRole(token.kind);
    if (role == null) continue;
    print(
      '${role.name.padRight(8)} ${python.substring(token.start, token.end)}',
    );
  }

  // A fence without an info string takes the language it looks like.
  print(code.resolveLanguage('if err != nil {\n\treturn err\n}\n', ''));

  // Return after the colon continues the block one indent deeper.
  const line = 'def greet(name):';
  final proposal = code.propose(
    line,
    language: 'python',
    base: line.length,
    extent: line.length,
    action: CodeEditingAction.newline,
    text: '',
    indentUnit: '    ',
  );
  print(jsonEncode(proposal?.text)); // "\n    "
}
