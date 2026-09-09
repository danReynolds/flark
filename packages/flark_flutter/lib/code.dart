/// Optional Tree-sitter code regions. Initialize before mounting an editor.
library;

import 'package:flark_tree_sitter/flark.dart' as shared;
import 'src/code_backend.dart';

export 'src/code_colors.dart' show FlarkCodeColors;

/// Flutter asset loading around the shared, pure Dart editing adapter.
/// The application disposes it after its editors close.
final class FlarkTreeSitter extends shared.FlarkTreeSitter {
  FlarkTreeSitter.fromAnalyzer(super.analyzer) : super.fromAnalyzer();

  static Future<FlarkTreeSitter> load() async =>
      FlarkTreeSitter.fromAnalyzer(await loadCodeAnalyzer());
}
