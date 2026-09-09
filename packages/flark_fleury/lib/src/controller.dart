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
  /// Least-recently wanted first, so the bound drops the coldest snippet.
  final _colors = <(String, String), CodeAnalysis>{};
  var _wanted = <(String, String)>{};
  final _failed = <(String, String)>{};
  int _colorUnits = 0;
  int _colorRevision = 0;
  static const _maxSnippets = 32, _maxUnits = 65536;

  /// Changes only when a snippet's colours do. Cell geometry bakes them in, so
  /// this is what tells a cached layout it is out of date — the editor's own
  /// revision also counts selection moves, which geometry does not depend on.
  int get colorRevision => _colorRevision;
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
          ? <(String, String)>{}
          : {
              for (final row in editor.projection.rows)
                if (row.kind == RowKind.codeBlock)
                  (row.text, languageInfo(row)),
            };
      // Retain only current snippets: no old-revision ranges can reach paint.
      // A Set keeps this a hash lookup per entry; a List compared whole code
      // bodies against every wanted key on every keystroke.
      final dropped = _colors.length;
      _colors.removeWhere((key, _) {
        if (_wanted.contains(key)) return false;
        _colorUnits -= key.$1.length;
        return true;
      });
      if (_colors.length != dropped) _colorRevision++;
      _failed.removeWhere((key) => !_wanted.contains(key));
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
        final next = _wanted.where(
          (key) => !_colors.containsKey(key) && !_failed.contains(key),
        );
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
        // A superseded request resolves to null. Retrying it immediately would
        // pick the same key forever, so the snippet waits for its next edit.
        if (result == null) {
          _failed.add(key);
          continue;
        }
        if (_wanted.contains(key)) {
          _colors[key] = result;
          _colorUnits += key.$1.length;
          _colorRevision++;
          while (_colors.length > _maxSnippets || _colorUnits > _maxUnits) {
            final coldest = _colors.keys.first;
            _colorUnits -= coldest.$1.length;
            _colors.remove(coldest);
          }
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
    _failed.clear();
    super.dispose();
  }
}
