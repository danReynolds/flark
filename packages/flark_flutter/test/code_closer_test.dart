import 'package:flark_flutter/code.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final service = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(service.dispose);
  testWidgets('typed closer outdents on every paint and accepts the next key', (
    tester,
  ) async {
    const source = '```\nfor (final x in [1,2,3]) {\n  \n```\n\nafter';
    const result = '```\nfor (final x in [1,2,3]) {\n}\n```\n\nafter';
    final at = source.indexOf('\n```'), caret = result.indexOf('\n```');
    final c = FlarkController(
      FlarkEditor(
        createParseBackend(),
        codeEditing: service,
        text: source,
        caret: at,
      ),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pump();
    paints.clear();
    final delivered = TextEditingValue(
      text: source.replaceRange(at, at, '}'),
      selection: TextSelection.collapsed(offset: at + 1),
    );
    tester.testTextInput.updateEditingValue(delivered);
    await tester.pump();
    expect((c.text, c.editor.selection.extent), (result, caret));
    expect(paints, isNotEmpty);
    for (final paint in paints) {
      expect(paint.rows.first, 'for (final x in [1,2,3]) {\n}');
      expect(paint.caretSource, caret);
    }
    // Duplicate delivery of the pre-normalized platform value must not put
    // the removed spaces back or insert another closer.
    tester.testTextInput.updateEditingValue(delivered);
    await tester.pump();
    expect(c.text, result);
    tester.testTextInput.updateEditingValue(
      TextEditingValue(
        text: result.replaceRange(caret, caret, ';'),
        selection: TextSelection.collapsed(offset: caret + 1),
      ),
    );
    await tester.pump();
    expect(c.text, result.replaceRange(caret, caret, ';'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    await tester.pump();
    expect((c.text, c.editor.selection.extent), (result, caret));
    c.command(const Undo());
    await tester.pump();
    expect((c.text, c.editor.selection.extent), (source, at));
    c.command(const Redo());
    await tester.pump();
    expect((c.text, c.editor.selection.extent), (result, caret));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
