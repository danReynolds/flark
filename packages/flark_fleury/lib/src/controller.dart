import 'package:flark/code.dart' show CodeHighlight;
import 'package:flark/rendering.dart';
import 'package:flark/flark.dart';

import 'package:fleury/fleury_core.dart';

/// Host notifications and code colors for a borrowed [editor], which remains
/// the only source/selection/history authority.
abstract interface class FlarkCellController {
  FlarkDocumentState get editor;
  String languageInfo(ProjectedRow row);
  CodeHighlight? colorsFor(ProjectedRow row);
}

final class FlarkFleuryController extends ChangeNotifier
    implements FlarkCellController {
  FlarkFleuryController(this.editor) {
    editor.addListener(_changed);
  }

  @override
  final FlarkEditor editor;

  /// Current selection/typing style. Re-read when this controller notifies.
  FlarkStyleState styleState(int style) => editor.styleState(style);

  /// Idempotent formatting, using the shared kernel's history and rules.
  bool setStyle(int style, {required bool enabled}) =>
      editor.apply(SetStyle(style, enabled: enabled));
  bool _closed = false;

  @override
  String languageInfo(ProjectedRow row) => row.codeInfoStart < 0
      ? ''
      : editor.source.substring(row.codeInfoStart, row.codeInfoEnd);

  /// Colors to paint for [row] from the editor's code delegate, or null
  /// without one. Layout keeps a fence's colors until its text or info string
  /// changes.
  @override
  CodeHighlight? colorsFor(ProjectedRow row) =>
      editor.codeEditing?.highlight(row.text, languageInfo(row));

  void _changed() {
    if (!_closed) notifyListeners();
  }

  @override
  void dispose() {
    if (_closed) return;
    _closed = true;
    editor.removeListener(_changed);
    super.dispose();
  }
}
