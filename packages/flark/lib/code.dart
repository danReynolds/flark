/// Optional snippet language service and theme-free decoration types.
/// Markdown recognition and document authority remain in the Flark kernel.
library;

/// Optional synchronous snippet editing. Coordinates refer to the projected
/// code body (UTF-16, LF), without Markdown container prefixes. The caller owns
/// the delegate; Flark owns validation, source translation and history.
abstract interface class CodeEditingDelegate {
  String resolveLanguage(String source, String info);
  CodeHighlight highlight(String source, String info);

  CodeEditProposal? propose(
    String source, {
    required String language,
    required int base,
    required int extent,
    required CodeEditingAction action,
    required String text,
    required String indentUnit,
  });
}

enum CodeEditingAction { insert, newline, indent, outdent }

final class CodeEditProposal {
  const CodeEditProposal(
    this.start,
    this.end,
    this.text,
    this.base,
    this.extent,
  );
  final int start, end, base, extent;
  final String text;
}

final class CodeToken {
  const CodeToken(this.start, this.end, this.kind);
  final int start, end;
  final String? kind;
}

/// The presentation roles a highlighter's scope names collapse to. Hosts theme
/// these, not the scope vocabulary: the analyzer emits well over twenty names,
/// and a host that keys its theme on raw scopes silently renders the ones it
/// has not enumerated as ordinary body text.
enum CodeSyntaxRole { keyword, string, number, comment, function, variable }

/// The role a scope name carries, or null when it has no distinct one.
CodeSyntaxRole? codeSyntaxRole(String? kind) => switch (kind) {
  'keyword' || 'selector-tag' || 'meta' => CodeSyntaxRole.keyword,
  'string' || 'regexp' || 'attr' || 'selector-attr' => CodeSyntaxRole.string,
  'number' ||
  'constant' ||
  'boolean' ||
  'literal' ||
  'built_in' => CodeSyntaxRole.number,
  'comment' || 'doctag' => CodeSyntaxRole.comment,
  'title' ||
  'function' ||
  'constructor' ||
  'type' ||
  'class' => CodeSyntaxRole.function,
  'variable' ||
  'property' ||
  'tag' ||
  'params' ||
  'attribute' ||
  'selector-class' => CodeSyntaxRole.variable,
  _ => null,
};

final class CodeHighlight {
  CodeHighlight(this.language, List<CodeToken> tokens)
    : tokens = List.unmodifiable(tokens);
  final String? language;
  final List<CodeToken> tokens;
  String? kindAt(int offset) {
    for (final token in tokens) {
      if (token.start <= offset && offset < token.end) return token.kind;
    }
    return null;
  }
}

String codeLeadingWhitespace(String text) {
  var end = 0;
  while (end < text.length &&
      (text.codeUnitAt(end) == 32 || text.codeUnitAt(end) == 9)) {
    end++;
  }
  return text.substring(0, end);
}

/// A stable editing step: two spaces, four for Python, or an existing tab.
/// Re-inferring a space width after indenting makes Tab/Shift-Tab asymmetric.
String codeIndentUnit(String text, String language) {
  for (final line in text.split('\n')) {
    final leading = codeLeadingWhitespace(line);
    if (leading.contains('\t')) return '\t';
  }
  return language == 'python' ? '    ' : '  ';
}
