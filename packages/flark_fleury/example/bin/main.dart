import 'dart:io';
import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:fleury/fleury.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart' as widgets;
import 'package:flark_fleury_example/playground.dart';

Future<void> main(List<String> args) async {
  final controller = FlarkFleuryController(
    FlarkEditor(
      createParseBackend(),
      text: args.contains('--headings') ? headingSample : sample,
      codeEditing: FlarkCodeMirror(),
    ),
  );
  try {
    await runApp(
      Playground(
        controller: controller,
        imagePreviewBuilder: (context, resource, uri) =>
            resource.destination == 'demo.png'
            ? widgets.Image.file(
                'assets/demo.png',
                semanticLabel: resource.text,
              )
            : FlarkImagePreview(uri: uri, label: resource.text),
        onOpenLink: !(Platform.isMacOS || Platform.isLinux)
            ? null
            : (uri) {
                if (Platform.isMacOS) {
                  Process.run('open', [uri.toString()]);
                } else if (Platform.isLinux) {
                  Process.run('xdg-open', [uri.toString()]);
                }
              },
      ),
      enableHotReload: false,
      onEvent: (event) =>
          event is KeyEvent && event.hasCtrl && event.code == KeyCode.q
          ? const ExitRequested()
          : null,
    );
  } finally {
    controller.dispose();
  }
}
