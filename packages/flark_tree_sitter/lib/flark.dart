/// Tree-sitter adapter for the pure Dart Flark command contract.
library;

import 'package:flark/code.dart';
import 'flark_tree_sitter.dart';

/// Owns synchronous snippet parsing; can be shared by sequential documents.
/// The application disposes it after its editors close. Coloring workers belong
/// to individual controllers and are disposed with them.
class FlarkTreeSitter implements CodeEditingDelegate {
  /// Takes ownership of an already loaded native or Wasm analyzer and warms
  /// editing queries. Useful for hosts with custom asset loading.
  FlarkTreeSitter.fromAnalyzer(this._analyzer) {
    try {
      for (final language in CodeLanguage.values.skip(1)) {
        _analyzer.proposeEdit(
          '',
          language: language,
          base: 0,
          extent: 0,
          action: CodeEditAction.newline,
        );
      }
      _analyzer.detect("x");
    } catch (_) {
      _analyzer.dispose();
      rethrow;
    }
  }
  final CodeAnalyzer _analyzer;
  final _colors = <(String, String), CodeHighlight>{};
  int _colorUnits = 0;

  @override
  String resolveLanguage(String source, String info) {
    final selected = codeLanguageName(info);
    if (selected.isNotEmpty) return selected;
    final inferred = _analyzer.detect(source);
    return inferred == CodeLanguage.plain ? '' : inferred.name;
  }

  @override
  CodeHighlight highlight(String source, String info) {
    final name = resolveLanguage(source, info), key = (source, name);
    final cached = _colors.remove(key);
    if (cached != null) {
      _colors[key] = cached;
      return cached;
    }
    final selected = codeLanguage(name);
    final result = _analyzer.analyze(source, language: selected);
    final colors = CodeHighlight(selected == CodeLanguage.plain ? null : name, [
      for (final span in result.spans)
        CodeToken(
          span.start,
          span.end,
          span.scopes.lastOrNull?.split('.').first,
        ),
    ]);
    if (source.length <= CodeAnalyzer.maxCodeUnits) {
      _colors[key] = colors;
      _colorUnits += source.length;
      while (_colors.length > 32 || _colorUnits > 65536) {
        final oldest = _colors.keys.first;
        _colorUnits -= oldest.$1.length;
        _colors.remove(oldest);
      }
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
    final selected = codeLanguage(language);
    final edit = _analyzer.proposeEdit(
      source,
      language: selected,
      base: base,
      extent: extent,
      action: CodeEditAction.values.byName(action.name),
      text: text,
      indentUnit: indentUnit,
    );
    return edit == null
        ? null
        : CodeEditProposal(
            edit.start,
            edit.end,
            edit.text,
            edit.base,
            edit.extent,
          );
  }

  void dispose() {
    _colors.clear();
    _analyzer.dispose();
  }
}
