import 'package:flark_dogfood/main.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  testWidgets('Copy repro puts a replayable session on the clipboard', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    final preferences = await SharedPreferences.getInstance();
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied = (call.arguments as Map)['text'] as String?;
        }
        return null;
      },
    );
    await tester.pumpWidget(
      DogfoodApp(backend: createParseBackend(), preferences: preferences),
    );
    await tester.pump();
    final c = tester
        .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
        .controller;
    c.command(const InsertText('x'));
    c.command(const DeleteBackward());
    await tester.tap(find.byTooltip('Copy'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy repro'));
    await tester.pumpAndSettle();
    // The document's session since it was loaded, as Dart a test can paste.
    expect(copied, startsWith('// Flark repro: 2 calls.'));
    // Host commands close any composition first, so they are recorded as
    // applyAfterComposition calls.
    expect(
      copied,
      contains('editor.applyAfterComposition(const InsertText("x")'),
    );
    expect(
      copied,
      contains('editor.applyAfterComposition(const DeleteBackward()'),
    );
    expect(copied, contains('expect(editor.source, '));
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    );
  });
}
