import 'dart:convert';

import 'backend.dart';
import 'backend_ffi.dart'
    if (dart.library.js_interop) 'backend_web.dart'
    as platform;
import 'model.dart';
import 'edit.dart';

/// Owns one backend. On web, pass an asynchronously loaded WasmCodeBackend.
/// Calls are synchronous after construction; dispose when the host closes.
final class CodeAnalyzer {
  CodeAnalyzer({CodeBackend? backend})
    : _backend = backend ?? platform.createCodeBackend() {
    if (_backend.version != 4) {
      _backend.dispose();
      throw const CodeException('Expected flark_tree_sitter ABI version 4.');
    }
  }

  static const maxCodeUnits = 8192;
  final CodeBackend _backend;
  bool _disposed = false;
  final _detections = <String, CodeLanguage>{};

  /// Infer from at most 128 UTF-16 units. No positive syntax evidence means
  /// plain. Cache exact samples only; manual language selection bypasses this.
  CodeLanguage detect(String source) {
    if (_disposed) throw StateError('CodeAnalyzer used after dispose.');
    validateCodeSource(source);
    if (source.length > maxCodeUnits) return CodeLanguage.plain;
    var end = source.length.clamp(0, 128);
    if (end < source.length &&
        end > 0 &&
        source.codeUnitAt(end) >= 0xdc00 &&
        source.codeUnitAt(end) <= 0xdfff) {
      end--;
    }
    final sample = source.substring(0, end);
    final cached = _detections.remove(sample);
    if (cached != null) {
      _detections[sample] = cached;
      return cached;
    }
    final Object? decoded;
    try {
      decoded = jsonDecode(utf8.decode(_backend.detect(utf8.encode(sample))));
    } on FormatException {
      throw const CodeException('Invalid language detection response.');
    }
    if (decoded is! Map ||
        decoded['version'] != 4 ||
        decoded['language'] is! int ||
        decoded['language'] < 0 ||
        decoded['language'] >= CodeLanguage.values.length) {
      throw const CodeException('Invalid language detection response.');
    }
    final language = CodeLanguage.values[decoded['language'] as int];
    _detections[sample] = language;
    if (_detections.length > 32) _detections.remove(_detections.keys.first);
    return language;
  }

  /// Proposes a code-body edit. Returns null when the source/candidate exceeds
  /// the snippet budget, so the host can perform its ordinary edit instead.
  /// Insert is a typing intent; paste/composition should use the host's literal
  /// edit path. Smart outdent only runs for a single inserted scalar.
  CodeEdit? proposeEdit(
    String source, {
    required CodeLanguage language,
    required int base,
    required int extent,
    required CodeEditAction action,
    String text = '',
    String indentUnit = '  ',
    String newline = '\n',
  }) {
    if (_disposed) throw StateError('CodeAnalyzer used after dispose.');
    validateCodeSource(source);
    validateCodeSource(text);
    if (!codePositionIsValid(source, base) ||
        !codePositionIsValid(source, extent)) {
      throw ArgumentError('Invalid code selection.');
    }
    if (indentUnit != '\t' &&
        (indentUnit.isEmpty ||
            indentUnit.length > 8 ||
            indentUnit.codeUnits.any((c) => c != 32))) {
      throw ArgumentError('Indent unit must be a tab or 1–8 spaces.');
    }
    if (newline != '\n' && newline != '\r\n') {
      throw ArgumentError('Newline must be LF or CRLF.');
    }
    if (action != CodeEditAction.insert && text.isNotEmpty) {
      throw ArgumentError('Text is only valid for insert.');
    }
    if (source.length > maxCodeUnits ||
        source.length - (base - extent).abs() + text.length > maxCodeUnits) {
      return null;
    }
    return decodeCodeEdit(
      _backend.edit(
        utf8.encode(
          jsonEncode({
            'source': source,
            'base': base,
            'extent': extent,
            'action': action.name,
            'text': text,
            'unit': indentUnit,
            'newline': newline,
          }),
        ),
        language.index,
      ),
      source,
      language,
    );
  }

  CodeAnalysis analyze(String source, {required CodeLanguage language}) {
    if (_disposed) throw StateError('CodeAnalyzer used after dispose.');
    validateCodeSource(source);
    if (source.length > maxCodeUnits || language == CodeLanguage.plain) {
      return plainCodeAnalysis(
        source,
        language,
        source.length > maxCodeUnits
            ? CodeAnalysisStatus.limit
            : CodeAnalysisStatus.plain,
      );
    }
    return decodeCodeAnalysis(
      _backend.analyze(utf8.encode(source), language.index),
      source,
      language,
    );
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _detections.clear();
    _backend.dispose();
  }
}
