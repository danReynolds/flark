import 'package:flark/flark.dart';
import 'package:flark_example/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('v4 opens, edits, projects, and replays on Android', (
    tester,
  ) async {
    debugPrint('FLARK_ANDROID_STEP open');
    final controller = (await tester.runAsync(() async {
      final opened = await FlarkEditorController.open(
        '**sentinel**\n\nalpha\n',
      ).timeout(const Duration(seconds: 10));
      debugPrint('FLARK_ANDROID_STEP parse');
      await opened.continueParsing().timeout(const Duration(seconds: 10));
      return opened;
    }))!;
    debugPrint('FLARK_ANDROID_STEP mounted');
    addTearDown(controller.close);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditor(controller: controller, autofocus: true),
        ),
      ),
    );
    await tester.pump();

    expect(controller.lastError, isNull);
    expect(controller.rows, hasLength(2));
    expect(controller.surfaceRow(controller.rows.first).text, 'sentinel');

    final alpha = controller.rows.last;
    controller.activateRow(alpha, 19);
    await tester.pump();
    expect(controller.inputValue.text, 'alpha\n');
    final dynamic editorState = tester.state(find.byType(FlarkEditor));
    editorState.updateEditingValue(
      const TextEditingValue(
        text: 'alpha*\n',
        selection: TextSelection.collapsed(offset: 6),
      ),
    );
    debugPrint('FLARK_ANDROID_STEP asterisk');
    await tester.runAsync(
      () => controller.debugWaitForPresentationSettled().timeout(
        const Duration(seconds: 10),
      ),
    );
    await tester.pump();
    expect(
      await tester.runAsync(controller.readSource),
      '**sentinel**\n\nalpha*\n',
    );
    expect(controller.lastError, isNull);
    expect(controller.surfaceRow(controller.rows.first).text, 'sentinel');

    controller.deleteBackward();
    debugPrint('FLARK_ANDROID_STEP backspace');
    await tester.runAsync(
      () => controller.debugWaitForPresentationSettled().timeout(
        const Duration(seconds: 10),
      ),
    );
    expect(
      await tester.runAsync(controller.readSource),
      '**sentinel**\n\nalpha\n',
    );

    controller.insertNewline();
    controller.replaceSelection('Next');
    debugPrint('FLARK_ANDROID_STEP return-successor');
    await tester.runAsync(
      () => controller.debugWaitForPresentationSettled().timeout(
        const Duration(seconds: 10),
      ),
    );
    await tester.pump();
    expect(
      await tester.runAsync(controller.readSource),
      '**sentinel**\n\nalpha\n\nNext\n',
    );
    expect(controller.lastError, isNull);

    debugPrint('FLARK_ANDROID_STEP undo');
    expect(
      await tester.runAsync(
        () => controller.undo().timeout(const Duration(seconds: 10)),
      ),
      isTrue,
    );
    expect(
      await tester.runAsync(controller.readSource),
      '**sentinel**\n\nalpha\n\n\n',
    );
    expect(
      await tester.runAsync(
        () => controller.undo().timeout(const Duration(seconds: 10)),
      ),
      isTrue,
    );
    expect(
      await tester.runAsync(controller.readSource),
      '**sentinel**\n\nalpha\n',
    );
  });

  testWidgets('dogfood shell fits the physical Android viewport', (
    tester,
  ) async {
    final logicalWidth =
        tester.view.physicalSize.width / tester.view.devicePixelRatio;
    expect(logicalWidth, lessThan(700));

    await tester.pumpWidget(const FlarkDogfoodApp());
    Object? frameworkError;
    for (var attempt = 0; attempt < 100; attempt += 1) {
      await tester.pump(const Duration(milliseconds: 50));
      frameworkError ??= tester.takeException();
      if (find.byType(FlarkEditor).evaluate().isNotEmpty) break;
    }

    expect(find.text('FLARK'), findsOneWidget);
    expect(find.byType(FlarkEditor), findsOneWidget);
    expect(frameworkError, isNull);

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump(const Duration(milliseconds: 100));
  });
}
