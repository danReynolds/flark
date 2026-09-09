import 'dart:io';
import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:fleury/fleury.dart';
import 'package:flark_fleury_example/playground.dart';

Future<void> main() async {
  final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  final controller = FlarkFleuryController(
    FlarkEditor(createParseBackend(), text: sample, codeEditing: code),
    highlightWorker: await CodeHighlightWorker.start(),
  );
  try {
    await runApp(
      Playground(
        controller: controller,
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
    code.dispose();
  }
}
