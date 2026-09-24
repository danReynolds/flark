/// Code snippet highlighting and indentation from CodeMirror 5's language
/// modes, ported to Dart. Synchronous and pure Dart: no native library, Wasm
/// module or worker.
library;

import 'package:flark/code.dart';

import 'src/detect.dart';
import 'src/edit.dart';
import 'src/highlight.dart';
import 'src/languages.dart';
import 'src/mode.dart';

export 'src/detect.dart' show detectCodeMirrorLanguage;
export 'src/languages.dart'
    show codeMirrorCatalog, codeMirrorLanguages, codeMirrorLanguageName;

/// A [CodeEditingDelegate] backed by the ported modes. A fence without a
/// language is detected among the ported ones ([detectCodeMirrorLanguage]),
/// or plain when none is recognized. Languages without a ported mode are
/// plain, and their edits take the kernel's defaults.
final class FlarkCodeMirror implements CodeEditingDelegate {
  /// Longer snippets are plain and edit without proposals, like the previous
  /// highlighter's bound.
  static const maxCodeUnits = 8192;

  final _colors = <(String, String), CodeHighlight>{};
  int _colorUnits = 0;

  final _detected = <String, String>{};

  @override
  String resolveLanguage(String source, String info) {
    final selected = codeMirrorLanguageName(info);
    if (selected.isNotEmpty) return selected;
    // Detection reads a prefix, so typing further down reuses its answer.
    final sample = detectionSampleOf(source);
    final cached = _detected.remove(sample);
    final language = cached ?? detectCodeMirrorLanguage(source);
    _detected[sample] = language;
    if (_detected.length > 64) _detected.remove(_detected.keys.first);
    return language;
  }

  @override
  CodeHighlight highlight(String source, String info) {
    final name = resolveLanguage(source, info), key = (source, name);
    final cached = _colors.remove(key);
    if (cached != null) {
      _colors[key] = cached;
      return cached;
    }
    final mode = source.length > maxCodeUnits ? null : codeMirrorMode(name);
    final colors = mode == null
        ? CodeHighlight(null, [CodeToken(0, source.length, null)])
        : CodeHighlight(name, codeMirrorTokens(mode, source));
    _colors[key] = colors;
    _colorUnits += source.length;
    while (_colors.length > 32 || _colorUnits > 65536) {
      final oldest = _colors.keys.first;
      _colorUnits -= oldest.$1.length;
      _colors.remove(oldest);
    }
    return colors;
  }

  @override
  CodeEditProposal? propose(
    String source, {
    required String language,
    required int base,
    required int extent,
    required CodeEditingAction action,
    required String text,
    required String indentUnit,
  }) {
    if (source.length > maxCodeUnits || codeMirrorMode(language) == null) {
      return null;
    }
    return proposeCodeEdit(
      (columns) => codeMirrorMode(
        language,
        ModeConfig(indentUnit: columns, tabSize: codeTabSize),
      )!,
      source,
      base: base,
      extent: extent,
      action: action,
      text: text,
      indentUnit: indentUnit,
    );
  }
}
