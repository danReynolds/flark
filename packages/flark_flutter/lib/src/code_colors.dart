import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark_tree_sitter/flark_highlighting.dart';
import 'package:flutter/foundation.dart';

export 'package:flark_tree_sitter/flark_highlighting.dart' show CodeColorRequest;

/// Flutter notifications around the shared, bounded code decoration lane.
/// The surface supplies visible rows; source/selection/history stay in Flark.
final class FlarkCodeColors extends ChangeNotifier {
  FlarkCodeColors(this.editor, {Uri? workerUri, Uri? wasmUri}) {
    _colors = FlarkCodeHighlighting(editor, workerUri: workerUri, wasmUri: wasmUri);
    _colors.addListener(notifyListeners);
  }

  @visibleForTesting
  FlarkCodeColors.withWorker(this.editor, CodeColorRequest request, void Function() dispose) {
    _colors = FlarkCodeHighlighting.withWorker(editor, request, dispose);
    _colors.addListener(notifyListeners);
  }

  final FlarkEditor editor;
  late final FlarkCodeHighlighting _colors;
  bool _disposed = false;
  int get revision => _colors.revision;
  Object? get failure => _colors.failure;
  void setVisibleRows(Iterable<int> rows) => _colors.setVisibleRows(rows);
  CodeHighlight highlight(String text, String info) => _colors.highlight(text, info);

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _colors.dispose();
    super.dispose();
  }
}
