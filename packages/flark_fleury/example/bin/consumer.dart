import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury.dart';

Future<void> main() => runApp(
  FleuryApp(
    title: 'Flark',
    home: const FlarkEditor(
      initialMarkdown:
          '# A terminal note\n\nNo parser setup. **Start writing.**',
    ),
  ),
  enableHotReload: false,
  onEvent: (event) =>
      event is KeyEvent && event.hasCtrl && event.code == KeyCode.q
      ? const ExitRequested()
      : null,
);
