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
import 'src/modes/brackets.dart';

export 'src/detect.dart' show detectCodeMirrorLanguage;
export 'src/languages.dart'
    show CodeMirrorLanguage, CodeMirrorLanguages, codeMirrorLanguageName;

/// A [CodeEditingDelegate] backed by the ported modes. A fence without a
/// language is detected among the ported ones ([detectCodeMirrorLanguage]),
/// or plain when none is recognized. A fence in a language without a ported
/// mode is plain but indents by its brackets ([BracketsMode]); plain text
/// and undetected fences take the kernel's editing defaults.
final class FlarkCodeMirror implements CodeEditingDelegate {
  /// Every ported language.
  FlarkCodeMirror() : this.only(CodeMirrorLanguages.all);

  /// Only [languages]: a fence in another is plain but indents by its
  /// brackets, detection chooses among these alone, and the other languages'
  /// modes stay out of the build.
  FlarkCodeMirror.only(Iterable<CodeMirrorLanguage> languages)
    : languages = List.unmodifiable(languages);

  /// The languages this delegate highlights, in detection priority.
  final List<CodeMirrorLanguage> languages;
  late final _byName = {for (final l in languages) l.name: l};

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
    final language = cached ?? detectCodeMirrorLanguage(source, languages);
    _detected[sample] = language;
    if (_detected.length > 64) _detected.remove(_detected.keys.first);
    return language;
  }

  /// Both hosts call this while laying out a frame, so it never throws: a
  /// mode that fails leaves the rest of its snippet plain
  /// ([codeMirrorTokens]).
  @override
  CodeHighlight highlight(String source, String info) {
    final name = resolveLanguage(source, info), key = (source, name);
    final cached = _colors.remove(key);
    if (cached != null) {
      _colors[key] = cached;
      return cached;
    }
    final mode = source.length > maxCodeUnits
        ? null
        : _byName[name]?.mode(const ModeConfig());
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
    if (source.length > maxCodeUnits ||
        language.isEmpty ||
        language == 'text') {
      return null;
    }
    try {
      return proposeCodeEdit(
        (columns) {
          final config = ModeConfig(indentUnit: columns, tabSize: codeTabSize);
          return _byName[language]?.mode(config) ?? BracketsMode(config);
        },
        source,
        base: base,
        extent: extent,
        action: action,
        text: text,
        indentUnit: indentUnit,
      );
    } on Object {
      // A mode that fails declines the edit: the kernel's own editing then
      // applies, where an exception would lose the keystroke.
      return null;
    }
  }
}
