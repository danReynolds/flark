import 'dart:convert';

import 'configurations.dart';
import 'lexical.dart';

final class IndentRules {
  IndentRules(Map<String, dynamic> json)
    : pairs = {
        for (final p in json['brackets']) p[0] as String: p[1] as String,
      },
      patterns = {
        for (final e in (json['indentationRules'] as Map).entries)
          e.key as String: RegExp(e.value as String),
      },
      enter = [
        for (final r in json['onEnterRules'])
          {
            for (final k in ['beforeText', 'afterText', 'previousLineText'])
              if (r[k] != null) k: RegExp(r[k] as String),
          },
      ];
  final Map<String, String> pairs;
  final Map<String, RegExp> patterns;
  final List<Map<String, RegExp>> enter;
}

/// An edit proposal in snippet-local UTF-16 coordinates. Applying it through
/// Flark's Markdown source mapping/history is a separate integration gate.
final class SnippetEdit {
  const SnippetEdit(this.from, this.to, this.text, this.caret);
  final int from, to, caret;
  final String text;
  String apply(String source) => source.replaceRange(from, to, text);
}

final class SnippetIndenter {
  SnippetIndenter(this.lexer);
  final TextMateProbe lexer;
  final rules = {
    for (final e in (jsonDecode(configurationsJson) as Map).entries)
      e.key as String: IndentRules((e.value as Map).cast()),
  };
  static String leading(String text) => RegExp(r'^[ \t]*').stringMatch(text)!;
  static int lineStart(String text, int at) =>
      at == 0 ? 0 : text.lastIndexOf('\n', at - 1) + 1;

  SnippetEdit newline(String language, String source, int at, String unit) {
    final start = lineStart(source, at);
    final indent = leading(source.substring(start, at));
    final newline = source.contains('\r\n') ? '\r\n' : '\n';
    final syntax = lexer.tokenize(language, source), config = rules[language];
    var increase = false;
    String? closer;
    var to = at;
    if (syntax != null && config != null) {
      final code = syntax.codeOnly;
      final before = code.substring(start, at).trimRight();
      final lineEnd = source.indexOf('\n', at);
      final after = code.substring(at, lineEnd < 0 ? source.length : lineEnd);
      final previous = start == 0
          ? ''
          : code.substring(lineStart(code, start - 1), start - 1);
      final actualBefore = source.substring(start, at).trimRight();
      // A trailing literal must not make an earlier code opener appear to be
      // the last typed character. Trailing comments may be ignored by rules.
      final last = actualBefore.isEmpty ? -1 : start + actualBefore.length - 1;
      final inLiteral =
          last >= 0 &&
          syntax.literalAt(last) &&
          !syntax.commentEndedAt(last, at);
      if (!inLiteral && before.isNotEmpty)
        closer = config.pairs[before[before.length - 1]];
      final enter = config.enter.any(
        (rule) => rule.entries.every(
          (e) => e.value.hasMatch(
            {
              'beforeText': before,
              'afterText': after,
              'previousLineText': previous,
            }[e.key]!,
          ),
        ),
      );
      increase =
          !inLiteral &&
          (closer != null ||
              enter ||
              (config.patterns['increaseIndentPattern']?.hasMatch(before) ??
                  false));
      if (closer != null && after.trimLeft().startsWith(closer)) {
        to += leading(source.substring(at)).length;
      } else {
        closer = null;
      }
    }
    final first = '$newline$indent${increase ? unit : ''}';
    return SnippetEdit(
      at,
      to,
      '$first${closer == null ? '' : '$newline$indent'}',
      at + first.length,
    );
  }

  SnippetEdit typedCloser(String language, String source, int at, String text) {
    final literal = SnippetEdit(at, at, text, at + text.length);
    final config = rules[language];
    if (config == null || !config.pairs.containsValue(text)) return literal;
    final start = lineStart(source, at),
        indent = source.substring(lineStart(source, at), at);
    final end = source.indexOf('\n', at);
    if (indent.isEmpty ||
        leading(indent) != indent ||
        source.substring(at, end < 0 ? source.length : end).trim().isNotEmpty)
      return literal;
    final candidate = literal.apply(source),
        syntax = lexer.tokenize(language, literal.apply(source));
    if (syntax == null || syntax.literalAt(at)) return literal;
    final code = syntax.codeOnly, stack = <(String, int)>[];
    for (var i = 0; i < at; i++) {
      final char = code[i];
      if (config.pairs.containsKey(char)) {
        stack.add((char, i));
      } else if (config.pairs.containsValue(char)) {
        if (stack.isEmpty || config.pairs[stack.last.$1] != char)
          return literal;
        stack.removeLast();
      }
    }
    if (stack.isEmpty || config.pairs[stack.last.$1] != text) return literal;
    final opener = stack.last.$2;
    final target = leading(
      candidate.substring(lineStart(candidate, opener), opener),
    );
    if (target.length >= indent.length || !indent.startsWith(target))
      return literal;
    return SnippetEdit(
      start,
      at,
      '$target$text',
      start + target.length + text.length,
    );
  }
}
