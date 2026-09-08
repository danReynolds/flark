// Attended diagnostic: run with `flutter run --profile -d macos
// --target integration_test/input_comparison.dart`, then type the same sequence
// in both controls. Inputs must come through the OS; this is not a widget test.
// The ordinary workbench remains the native acceptance target.
import 'dart:convert';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flark_dogfood/backend.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final code = await FlarkTreeSitter.load();
  final editor = FlarkEditor(await loadBackend(), codeEditing: code);
  runApp(MaterialApp(home: _Comparison(editor)));
}

class _Comparison extends StatefulWidget {
  const _Comparison(this.editor);
  final FlarkEditor editor;
  @override
  State<_Comparison> createState() => _ComparisonState();
}

class _ComparisonState extends State<_Comparison> {
  late final flark = FlarkController(
    widget.editor,
    codeColors: FlarkCodeColors(widget.editor),
  );
  final standard = TextEditingController();
  @override
  void initState() {
    super.initState();
    flark.addListener(_changed);
    standard.addListener(_changed);
  }

  void _changed() => setState(() {});
  @override
  void dispose() {
    flark.dispose();
    standard.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    body: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Flark source: ${jsonEncode(flark.text)}; '
            'caret ${flark.editor.selection.extent}',
          ),
          Expanded(
            child: FlarkEditorWidget(controller: flark, autofocus: true),
          ),
          const Divider(),
          Text(
            'Flutter TextField: ${jsonEncode(standard.text)}; '
            'caret ${standard.selection.extentOffset}',
          ),
          Expanded(
            child: TextField(
              controller: standard,
              maxLines: null,
              decoration: const InputDecoration(
                labelText: 'Standard Flutter input',
              ),
            ),
          ),
        ],
      ),
    ),
  );
}
