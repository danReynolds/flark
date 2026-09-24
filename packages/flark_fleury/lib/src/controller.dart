import 'package:flark/code.dart' show CodeHighlight;
import 'package:flark/rendering.dart';
import 'package:flark/flark.dart';

import 'package:flark_tree_sitter/flark_highlighting.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:fleury/fleury_core.dart';

/// Host notifications and optional asynchronous code decorations.
/// [editor] remains the only source/selection/history authority and is borrowed.
/// A supplied [highlightWorker] or [codeColors] is owned and disposed here.
abstract interface class FlarkCellController {
  FlarkDocumentState get editor;
  int get colorRevision;
  String languageInfo(ProjectedRow row);
  CodeHighlight? colorsFor(ProjectedRow row);
  void setVisibleRows(Iterable<int> rows);
}

final class FlarkFleuryController extends ChangeNotifier
    implements FlarkCellController {
  FlarkFleuryController(
    this.editor, {
    CodeHighlightWorker? highlightWorker,
    FlarkCodeHighlighting? codeColors,
  }) : _colors = _ownedColors(editor, highlightWorker, codeColors) {
    editor.addListener(_changed);
    _colors?.addListener(_changed);
  }

  static FlarkCodeHighlighting? _ownedColors(
    FlarkEditor editor,
    CodeHighlightWorker? worker,
    FlarkCodeHighlighting? colors,
  ) {
    if (worker != null && colors != null) {
      throw ArgumentError('Supply highlightWorker or codeColors, not both.');
    }
    if (colors != null && !identical(colors.editor, editor)) {
      throw ArgumentError('codeColors must belong to this editor.');
    }
    return colors ??
        (worker == null
            ? null
            : FlarkCodeHighlighting.fromWorker(editor, worker));
  }

  @override
  final FlarkEditor editor;

  /// Current selection/typing style. Re-read when this controller notifies.
  FlarkStyleState styleState(int style) => editor.styleState(style);

  /// Idempotent formatting, using the shared kernel's history and rules.
  bool setStyle(int style, {required bool enabled}) =>
      editor.apply(SetStyle(style, enabled: enabled));
  final FlarkCodeHighlighting? _colors;
  bool _closed = false;
  @override
  int get colorRevision => _colors?.revision ?? 0;

  /// Coloring failure is observable; it never prevents editing current text.
  Object? get highlightError => _colors?.failure;

  @override
  String languageInfo(ProjectedRow row) => row.codeInfoStart < 0
      ? ''
      : editor.source.substring(row.codeInfoStart, row.codeInfoEnd);

  /// Colors to paint for [row]: exact, or shifted through a pending edit.
  @override
  CodeHighlight? colorsFor(ProjectedRow row) =>
      _colors?.colors(row.text, languageInfo(row));

  /// The host reports its painted rows; the shared lane prioritizes these and
  /// the active fence, then applies its snippet/count/size bounds.
  @override
  void setVisibleRows(Iterable<int> rows) => _colors?.setVisibleRows(rows);

  void _changed() {
    if (!_closed) notifyListeners();
  }

  @override
  void dispose() {
    if (_closed) return;
    _closed = true;
    editor.removeListener(_changed);
    _colors?.dispose();
    super.dispose();
  }
}
