import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:fleury/fleury_core.dart';

/// Host notifications and optional asynchronous code decorations.
/// [editor] remains the only source/selection/history authority and is borrowed.
/// A supplied [highlightWorker] is owned and disposed by this controller.
final class FlarkFleuryController extends ChangeNotifier {
  FlarkFleuryController(this.editor, {CodeHighlightWorker? highlightWorker})
    : _worker = highlightWorker {
    editor.addListener(_changed);
    _changed();
  }

  final FlarkEditor editor;
  final CodeHighlightWorker? _worker;
  final _colors = <(String, String), CodeAnalysis>{};
  List<(String, String)> _wanted = [];
  String? _source;
  bool _sourceMode = false, _working = false, _closed = false;
  Object? _highlightError;

  /// Coloring failure is observable; it never prevents editing current text.
  Object? get highlightError => _highlightError;

  String languageInfo(ProjectedRow row) => row.codeInfoStart < 0
      ? ''
      : editor.source.substring(row.codeInfoStart, row.codeInfoEnd);

  CodeAnalysis? colorsFor(ProjectedRow row) =>
      _colors[(row.text, languageInfo(row))];

  void _changed() {
    if (_source != editor.source || _sourceMode != editor.sourceMode) {
      _source = editor.source;
      _sourceMode = editor.sourceMode;
      _wanted = editor.sourceMode
          ? []
          : [
              for (final row in editor.projection.rows)
                if (row.kind == RowKind.codeBlock)
                  (row.text, languageInfo(row)),
            ];
      // Retain only current snippets: no old-revision ranges can reach paint.
      _colors.removeWhere((key, _) => !_wanted.contains(key));
      if (!_working && _worker != null && _highlightError == null) {
        unawaited(_color());
      }
    }
    notifyListeners();
  }

  Future<void> _color() async {
    _working = true;
    try {
      while (!_closed) {
        final next = _wanted.where((key) => !_colors.containsKey(key));
        if (next.isEmpty) break;
        final key = next.first;
        final name =
            editor.codeEditing?.resolveLanguage(key.$1, key.$2) ??
            codeLanguageName(key.$2);
        final result = await _worker!.analyze(
          key.$1,
          language: codeLanguage(name),
        );
        if (_closed) break;
        if (result != null && _wanted.contains(key)) {
          _colors[key] = result;
          notifyListeners();
        }
      }
    } catch (error) {
      if (!_closed) {
        _highlightError = error;
        notifyListeners();
      }
    } finally {
      _working = false;
    }
  }

  @override
  void dispose() {
    if (_closed) return;
    _closed = true;
    editor.removeListener(_changed);
    _worker?.dispose();
    _colors.clear();
    super.dispose();
  }
}
