import 'package:flark/flark.dart';
import 'package:flark/wasm.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:fleury_web/fleury_web.dart';
import 'package:web/web.dart' as web;
import 'package:flark_fleury_example/playground.dart';

Future<void> main() async {
  final into = web.document.getElementById('app')!;
  try {
    final parser = await WasmParseBackend.load();
    final controller = FlarkFleuryController(
      FlarkEditor(
        parser,
        text: Uri.base.queryParameters['sample'] == 'headings'
            ? headingSample
            : sample,
        codeEditing: FlarkCodeMirror(),
      ),
    );
    into.textContent = '';
    await mountApp(
      () => Playground(
        controller: controller,
        baseUri: Uri.base,
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
