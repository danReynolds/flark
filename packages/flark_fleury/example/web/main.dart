import 'package:flark/flark.dart';
import 'package:flark/wasm.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:flark_tree_sitter/wasm.dart';
import 'package:fleury_web/fleury_web.dart';
import 'package:web/web.dart' as web;
import 'package:flark_fleury_example/playground.dart';

Future<void> main() async {
  final into = web.document.getElementById('app')!;
  try {
    final parser = await WasmParseBackend.load();
    final code = FlarkTreeSitter.fromAnalyzer(
      CodeAnalyzer(
        backend: await WasmCodeBackend.load(
          uri: Uri.base.resolve('flark_tree_sitter.wasm'),
        ),
      ),
    );
    final controller = FlarkFleuryController(
      FlarkEditor(parser, text: sample, codeEditing: code),
      highlightWorker: await CodeHighlightWorker.start(
        workerUri: Uri.base.resolve('highlight_worker.mjs'),
        wasmUri: Uri.base.resolve('flark_tree_sitter.wasm'),
      ),
    );
    into.textContent = '';
    await mountApp(
      () => Playground(
        controller: controller,
        onOpenLink: (uri) {
          web.window.open(uri.toString(), '_blank', 'noopener,noreferrer');
        },
      ),
      into: into,
    );
  } catch (error) {
    into.textContent = 'Could not start the Fleury playground: $error';
    rethrow;
  }
}
