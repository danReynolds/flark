import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_web/fleury_web.dart';
import 'package:web/web.dart' as web;

Future<void> main() => mountApp(
  () => FleuryApp(
    title: 'Flark',
    home: FlarkEditor(
      initialMarkdown: '# A Fleury note\n\nNo parser setup. **Start writing.**',
      onOpenLink: (uri) =>
          web.window.open(uri.toString(), '_blank', 'noopener,noreferrer'),
    ),
  ),
  into: web.document.getElementById('app')!,
);
