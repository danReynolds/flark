import 'package:highlight/highlight_core.dart';

import 'package:highlight/languages/bash.dart';
import 'package:highlight/languages/css.dart';
import 'package:highlight/languages/dart.dart';
import 'package:highlight/languages/javascript.dart';
import 'package:highlight/languages/json.dart';
import 'package:highlight/languages/markdown.dart';
import 'package:highlight/languages/python.dart';
import 'package:highlight/languages/sql.dart';
import 'package:highlight/languages/typescript.dart';
import 'package:highlight/languages/xml.dart';
import 'package:highlight/languages/yaml.dart';

class CodeHighlightRun {
  final int start; // inclusive, relative to the provided source
  final int end; // exclusive, relative to the provided source
  final String className;

  const CodeHighlightRun({
    required this.start,
    required this.end,
    required this.className,
  });
}

/// Minimal wrapper around `package:highlight` that:
/// - Registers a small set of languages for performance predictability.
/// - Normalizes markdown fence tags (```dart) into highlight language IDs.
/// - Flattens highlight's node tree into contiguous runs.
class SovereignCodeHighlighter {
  static final SovereignCodeHighlighter instance = SovereignCodeHighlighter._();

  final Highlight _hl = Highlight();

  SovereignCodeHighlighter._() {
    // Keep this list aligned with the language picker choices (when we add UI).
    _hl.registerLanguage('dart', dart);
    _hl.registerLanguage('json', json);
    _hl.registerLanguage('yaml', yaml);
    _hl.registerLanguage('bash', bash);
    _hl.registerLanguage('python', python);
    _hl.registerLanguage('javascript', javascript);
    _hl.registerLanguage('typescript', typescript);
    _hl.registerLanguage('xml', xml); // html maps to xml
    _hl.registerLanguage('css', css);
    _hl.registerLanguage('sql', sql);
    _hl.registerLanguage('markdown', markdown);
  }

  static const Map<String, String> _tagToLanguage = {
    // Canonical
    'dart': 'dart',
    'json': 'json',
    'yaml': 'yaml',
    'bash': 'bash',
    'python': 'python',
    'javascript': 'javascript',
    'typescript': 'typescript',
    'xml': 'xml',
    'css': 'css',
    'sql': 'sql',
    'markdown': 'markdown',

    // Common aliases
    'yml': 'yaml',
    'sh': 'bash',
    'shell': 'bash',
    'py': 'python',
    'js': 'javascript',
    'ts': 'typescript',
    'html': 'xml',
    'md': 'markdown',
  };

  static String? normalizeFenceTag(String? tag) {
    if (tag == null) return null;
    final normalized = tag.trim().toLowerCase();
    if (normalized.isEmpty) return null;
    return _tagToLanguage[normalized];
  }

  List<CodeHighlightRun> highlight(String source, {required String language}) {
    if (source.isEmpty) return const [];

    final result = _hl.parse(source, language: language);
    return _runsFromResult(result);
  }

  /// Best-effort language auto-detection.
  ///
  /// `package:highlight` will parse using every registered language and pick the
  /// most relevant one, so callers should guard this with budgets.
  List<CodeHighlightRun> highlightAuto(String source) {
    if (source.isEmpty) return const [];

    final heuristic = _heuristicLanguageHint(source);
    if (heuristic != null) {
      return highlight(source, language: heuristic);
    }

    final result = _hl.parse(source, autoDetection: true);
    return _runsFromResult(result);
  }

  String? _heuristicLanguageHint(String source) {
    final text = source.trimLeft();
    if (text.isEmpty) return null;

    if (_looksLikeDart(text)) return 'dart';
    if (_looksLikeJson(text)) return 'json';
    if (_looksLikeYaml(text)) return 'yaml';
    if (_looksLikeSql(text)) return 'sql';

    return null;
  }

  bool _looksLikeDart(String text) {
    if (RegExp(r'\b(import|library|part|mixin|extension)\b').hasMatch(text)) {
      return true;
    }
    if (RegExp(r'\b(class|enum|typedef)\s+[A-Z_]').hasMatch(text)) {
      return true;
    }
    if (RegExp(r'\bvoid\s+main\s*\(').hasMatch(text)) return true;
    if (RegExp(r'@\w+').hasMatch(text) &&
        RegExp(r'\b(override|pragma|deprecated)\b').hasMatch(text)) {
      return true;
    }
    if (RegExp(r'\b(final|late|required)\b').hasMatch(text) &&
        (text.contains('=>') ||
            text.contains(');') ||
            RegExp(
              r'\b(var|int|double|String|bool|List|Map|Set)\b',
            ).hasMatch(text))) {
      return true;
    }
    return false;
  }

  bool _looksLikeJson(String text) {
    if (!(text.startsWith('{') || text.startsWith('['))) return false;
    if (!text.contains(':')) return false;
    return RegExp(r'"[^"]+"\s*:').hasMatch(text);
  }

  bool _looksLikeYaml(String text) {
    if (text.startsWith('{') || text.startsWith('[')) return false;
    final lines = text.split('\n').where((l) => l.trim().isNotEmpty).take(6);
    var matched = 0;
    for (final line in lines) {
      if (RegExp(r'^\s*[\w-]+\s*:\s*').hasMatch(line)) {
        matched++;
      }
    }
    return matched >= 2;
  }

  bool _looksLikeSql(String text) {
    return RegExp(
      r'^\s*(select|insert|update|delete|create|alter|drop|with)\b',
      caseSensitive: false,
      multiLine: true,
    ).hasMatch(text);
  }

  List<CodeHighlightRun> _runsFromResult(Result result) {
    final nodes = result.nodes;
    if (nodes == null || nodes.isEmpty) return const [];

    final segments = <_Segment>[];
    final classStack = <String>[];

    void walk(Node node) {
      final cn = node.className;
      final hasClass = cn != null && cn.isNotEmpty;
      if (hasClass) classStack.add(cn);

      final value = node.value;
      if (value != null && value.isNotEmpty) {
        segments.add(
          _Segment(
            text: value,
            className: classStack.isEmpty ? null : classStack.last,
          ),
        );
      }

      final children = node.children;
      if (children != null && children.isNotEmpty) {
        for (final child in children) {
          walk(child);
        }
      }

      if (hasClass) classStack.removeLast();
    }

    for (final node in nodes) {
      walk(node);
    }

    final runs = <CodeHighlightRun>[];
    int offset = 0;
    for (final seg in segments) {
      final len = seg.text.length;
      if (len == 0) continue;

      final cn = seg.className;
      if (cn != null && cn.isNotEmpty) {
        final start = offset;
        final end = offset + len;
        if (runs.isNotEmpty &&
            runs.last.className == cn &&
            runs.last.end == start) {
          runs[runs.length - 1] = CodeHighlightRun(
            start: runs.last.start,
            end: end,
            className: cn,
          );
        } else {
          runs.add(CodeHighlightRun(start: start, end: end, className: cn));
        }
      }
      offset += len;
    }

    return runs;
  }
}

class _Segment {
  final String text;
  final String? className;

  const _Segment({required this.text, required this.className});
}
