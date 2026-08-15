import 'package:flutter/widgets.dart';
import 'package:highlight/highlight_core.dart' as syntax;

import 'flark_markdown_theme.dart';
import 'package:highlight/languages/dart.dart' as highlight_dart;
import 'package:highlight/languages/javascript.dart' as highlight_javascript;
import 'package:highlight/languages/json.dart' as highlight_json;
import 'package:highlight/languages/kotlin.dart' as highlight_kotlin;
import 'package:highlight/languages/markdown.dart' as highlight_markdown;
import 'package:highlight/languages/python.dart' as highlight_python;
import 'package:highlight/languages/rust.dart' as highlight_rust;
import 'package:highlight/languages/shell.dart' as highlight_shell;
import 'package:highlight/languages/sql.dart' as highlight_sql;
import 'package:highlight/languages/swift.dart' as highlight_swift;
import 'package:highlight/languages/typescript.dart' as highlight_typescript;
import 'package:highlight/languages/yaml.dart' as highlight_yaml;

bool _registeredSyntaxLanguages = false;
const _autoHighlightMinimumSourceLength = 8;
const _autoHighlightMinimumRelevance = 4;
const _autoHighlightMinimumRelevanceMargin = 2;

TextSpan? buildFlarkHighlightedCodeSpan({
  required String source,
  required String? language,
  required TextStyle baseStyle,
  TextRange? composingRange,
  FlarkCodeSyntaxThemeData syntaxTheme = FlarkCodeSyntaxThemeData.light,
}) {
  if (source.isEmpty) return null;
  final requestedLanguage = language?.trim();
  final normalizedLanguage = _normalizeCodeLanguage(language);
  if (normalizedLanguage == null && requestedLanguage?.isNotEmpty == true) {
    return null;
  }
  _registerSyntaxLanguages();

  final result = normalizedLanguage == null
      ? _autoDetectedSyntaxResult(source)
      : _syntaxResultForLanguage(source, normalizedLanguage);
  if (result == null) return null;

  final nodes = result.nodes;
  if (nodes == null || nodes.isEmpty) return null;

  final children = <TextSpan>[];
  var offset = 0;
  for (final node in nodes) {
    offset = _appendSyntaxNode(
      children: children,
      source: source,
      node: node,
      offset: offset,
      style: baseStyle,
      composingRange: composingRange,
      syntaxTheme: syntaxTheme,
    );
  }
  if (offset < source.length) {
    _appendStyledText(
      children,
      source: source,
      start: offset,
      end: source.length,
      style: baseStyle,
      composingRange: composingRange,
    );
  }
  if (children.isEmpty || !_containsHighlightedStyle(children, baseStyle)) {
    return null;
  }
  return TextSpan(style: baseStyle, children: children);
}

syntax.Result? _syntaxResultForLanguage(String source, String language) {
  try {
    return syntax.highlight.parse(source, language: language);
  } catch (_) {
    return null;
  }
}

syntax.Result? _autoDetectedSyntaxResult(String source) {
  if (source.trim().length < _autoHighlightMinimumSourceLength) return null;
  final syntax.Result result;
  try {
    result = syntax.highlight.parse(source, autoDetection: true);
  } catch (_) {
    return null;
  }
  if (!_autoDetectionIsConfident(result)) return null;
  return result;
}

bool _autoDetectionIsConfident(syntax.Result result) {
  final language = result.language;
  final relevance = result.relevance ?? 0;
  if (language == null || language.isEmpty) return false;
  if (relevance < _autoHighlightMinimumRelevance) return false;

  final secondRelevance = result.secondBest?.relevance;
  if (secondRelevance == null || result.secondBest?.language == null) {
    return true;
  }
  return relevance - secondRelevance >= _autoHighlightMinimumRelevanceMargin;
}

void _registerSyntaxLanguages() {
  if (_registeredSyntaxLanguages) return;
  syntax.highlight
    ..registerLanguage('dart', highlight_dart.dart)
    ..registerLanguage('markdown', highlight_markdown.markdown)
    ..registerLanguage('json', highlight_json.json)
    ..registerLanguage('yaml', highlight_yaml.yaml)
    ..registerLanguage('sql', highlight_sql.sql)
    ..registerLanguage('javascript', highlight_javascript.javascript)
    ..registerLanguage('typescript', highlight_typescript.typescript)
    ..registerLanguage('python', highlight_python.python)
    ..registerLanguage('rust', highlight_rust.rust)
    ..registerLanguage('swift', highlight_swift.swift)
    ..registerLanguage('kotlin', highlight_kotlin.kotlin)
    ..registerLanguage('shell', highlight_shell.shell);
  _registeredSyntaxLanguages = true;
}

String? _normalizeCodeLanguage(String? language) {
  final normalized = language?.trim().toLowerCase();
  if (normalized == null || normalized.isEmpty) return null;
  return switch (normalized) {
    'plain' || 'plaintext' || 'text' => null,
    'md' => 'markdown',
    'js' => 'javascript',
    'ts' => 'typescript',
    'py' => 'python',
    'rs' => 'rust',
    'kt' => 'kotlin',
    'yml' => 'yaml',
    'sh' || 'bash' || 'zsh' => 'shell',
    _ => normalized,
  };
}

int _appendSyntaxNode({
  required List<TextSpan> children,
  required String source,
  required syntax.Node node,
  required int offset,
  required TextStyle style,
  required TextRange? composingRange,
  required FlarkCodeSyntaxThemeData syntaxTheme,
}) {
  final nodeStyle = _syntaxStyle(style, node.className, syntaxTheme);
  final value = node.value;
  if (value != null) {
    final end = (offset + value.length).clamp(offset, source.length);
    _appendStyledText(
      children,
      source: source,
      start: offset,
      end: end,
      style: nodeStyle,
      composingRange: composingRange,
    );
    return end;
  }

  var childOffset = offset;
  final nodeChildren = node.children;
  if (nodeChildren != null) {
    for (final child in nodeChildren) {
      childOffset = _appendSyntaxNode(
        children: children,
        source: source,
        node: child,
        offset: childOffset,
        style: nodeStyle,
        composingRange: composingRange,
        syntaxTheme: syntaxTheme,
      );
    }
  }
  return childOffset;
}

TextStyle _syntaxStyle(
  TextStyle baseStyle,
  String? className,
  FlarkCodeSyntaxThemeData syntaxTheme,
) {
  if (className == null || className.isEmpty) return baseStyle;
  final classes = className.split(RegExp(r'[\s.]+')).toSet();
  bool has(String value) => classes.contains(value);

  if (has('comment')) {
    return baseStyle.copyWith(
      color: syntaxTheme.commentColor,
      fontStyle: FontStyle.italic,
    );
  }
  if (has('string') || has('quote')) {
    return baseStyle.copyWith(color: syntaxTheme.stringColor);
  }
  if (has('number') || has('literal')) {
    return baseStyle.copyWith(color: syntaxTheme.numberColor);
  }
  if (has('keyword') || has('selector-tag')) {
    return baseStyle.copyWith(
      color: syntaxTheme.keywordColor,
      fontWeight: FontWeight.w700,
    );
  }
  if (has('title') || has('function') || has('section')) {
    return baseStyle.copyWith(color: syntaxTheme.functionColor);
  }
  if (has('type') || has('class') || has('built_in') || has('built-in')) {
    return baseStyle.copyWith(color: syntaxTheme.typeColor);
  }
  if (has('attr') || has('attribute') || has('property')) {
    return baseStyle.copyWith(color: syntaxTheme.attributeColor);
  }
  if (has('variable') || has('template-variable') || has('symbol')) {
    return baseStyle.copyWith(color: syntaxTheme.variableColor);
  }
  if (has('meta') || has('doctag')) {
    return baseStyle.copyWith(color: syntaxTheme.metaColor);
  }
  if (has('deletion')) {
    return baseStyle.copyWith(color: syntaxTheme.deletionColor);
  }
  if (has('addition')) {
    return baseStyle.copyWith(color: syntaxTheme.additionColor);
  }
  return baseStyle;
}

void _appendStyledText(
  List<TextSpan> spans, {
  required String source,
  required int start,
  required int end,
  required TextStyle style,
  required TextRange? composingRange,
}) {
  if (start >= end) return;
  if (composingRange == null ||
      end <= composingRange.start ||
      start >= composingRange.end) {
    spans.add(TextSpan(text: source.substring(start, end), style: style));
    return;
  }

  final composingStart = composingRange.start.clamp(start, end);
  final composingEnd = composingRange.end.clamp(start, end);
  if (start < composingStart) {
    spans.add(
      TextSpan(text: source.substring(start, composingStart), style: style),
    );
  }
  spans.add(
    TextSpan(
      text: source.substring(composingStart, composingEnd),
      style: style.merge(const TextStyle(decoration: TextDecoration.underline)),
    ),
  );
  if (composingEnd < end) {
    spans.add(
      TextSpan(text: source.substring(composingEnd, end), style: style),
    );
  }
}

bool _containsHighlightedStyle(List<TextSpan> spans, TextStyle baseStyle) {
  for (final span in spans) {
    final style = span.style;
    if (style == null) continue;
    if (style.color != baseStyle.color ||
        style.fontWeight != baseStyle.fontWeight ||
        style.fontStyle != baseStyle.fontStyle) {
      return true;
    }
  }
  return false;
}
