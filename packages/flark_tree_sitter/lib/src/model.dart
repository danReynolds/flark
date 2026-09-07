import 'dart:convert';
import 'dart:typed_data';

import 'backend.dart';

/// IDs are part of ABI v4. Plain text never invokes a grammar.
enum CodeLanguage {
  plain,
  dart,
  javascript,
  python,
  yaml,
  ruby,
  typescript,
  rust,
  go,
  json,
  css,
  bash,
  html,
  xml,
  sql,
}

enum CodeAnalysisStatus { highlighted, plain, limit }

/// Exact source ranges. Scopes are upstream capture names, independent of theme.
/// They are highlighting information, not yet a qualified editing classifier.
final class CodeSpan {
  CodeSpan._(this.start, this.end, this.startByte, this.endByte, this.scopes);
  final int start, end;
  final int startByte, endByte;
  final List<String> scopes;
}

final class CodeAnalysis {
  CodeAnalysis._(this.source, this.language, this.status, List<CodeSpan> spans)
    : spans = List.unmodifiable(spans);
  final String source;
  final CodeLanguage language;
  final CodeAnalysisStatus status;
  final List<CodeSpan> spans;
}

// Reject lone surrogates before UTF-8 conversion can replace them with U+FFFD.
void validateCodeSource(String source) {
  for (var i = 0; i < source.length; i++) {
    final unit = source.codeUnitAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (++i >= source.length ||
          source.codeUnitAt(i) < 0xdc00 ||
          source.codeUnitAt(i) > 0xdfff) {
        throw ArgumentError('Code source contains an unpaired surrogate.');
      }
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw ArgumentError('Code source contains an unpaired surrogate.');
    }
  }
}

CodeAnalysis plainCodeAnalysis(
  String source,
  CodeLanguage language,
  CodeAnalysisStatus status,
) => CodeAnalysis._(source, language, status, [
  if (source.isNotEmpty)
    CodeSpan._(0, source.length, 0, utf8.encode(source).length, const []),
]);

CodeAnalysis decodeCodeAnalysis(
  Uint8List bytes,
  String source,
  CodeLanguage language,
) {
  try {
    final value = jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
    if (value['version'] != 4 || value['language'] != language.index) {
      throw const FormatException('response identity');
    }
    final status = CodeAnalysisStatus.values.byName(value['status'] as String);
    if (status != CodeAnalysisStatus.highlighted) {
      throw const FormatException('unexpected backend status');
    }
    var end = 0, endByte = 0;
    final spans = <CodeSpan>[];
    final scopeSets = [
      for (final scopes in value['scope_sets'] as List)
        List<String>.unmodifiable((scopes as List).cast<String>()),
    ];
    if (scopeSets.any((scopes) => scopes.any((scope) => scope.isEmpty))) {
      throw const FormatException('empty scope');
    }
    for (final item in value['spans'] as List) {
      final span = item as List;
      if (span.length != 3) throw const FormatException('invalid range tuple');
      final start = end, startByte = endByte;
      final next = span[0] as int, nextByte = span[1] as int;
      if (next <= start || next > source.length) {
        throw const FormatException('noncontiguous source range');
      }
      if (next < source.length &&
          source.codeUnitAt(next) >= 0xdc00 &&
          source.codeUnitAt(next) <= 0xdfff) {
        throw const FormatException('range splits a surrogate pair');
      }
      // Validate both coordinate systems in one linear scan without allocating
      // a substring and UTF-8 byte array for every token.
      var byteLength = 0;
      for (var i = start; i < next; i++) {
        final unit = source.codeUnitAt(i);
        if (unit < 0x80) {
          byteLength++;
        } else if (unit < 0x800) {
          byteLength += 2;
        } else if (unit >= 0xd800 && unit <= 0xdbff) {
          byteLength += 4;
          i++;
        } else {
          byteLength += 3;
        }
      }
      if (nextByte - startByte != byteLength) {
        throw const FormatException('byte/code-unit disagreement');
      }
      final scopes = scopeSets[span[2] as int];
      spans.add(CodeSpan._(start, next, startByte, nextByte, scopes));
      end = next;
      endByte = nextByte;
    }
    if (end != source.length) {
      throw const FormatException('incomplete source coverage');
    }
    return CodeAnalysis._(source, language, status, spans);
  } catch (e) {
    throw CodeException('Invalid analysis response: $e');
  }
}
