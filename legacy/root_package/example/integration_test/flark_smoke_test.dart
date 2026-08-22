// Cross-platform launch smoke for the flark package.
//
// Unlike the package unit suites (which load the native bridge through an
// explicit `overrideLibraryPath`) and the host `flutter test` run, this suite
// drives the real example app through `integration_test`, so on a device or
// desktop it exercises the *shipped* parser-loading path end to end:
//
//   * mobile / desktop  → the native Comrak bridge resolved by Flutter's
//                          native-assets pipeline (the build hook's output),
//   * web               → the prebundled Comrak WASM module.
//
// It is deliberately small and interaction-light (toolbar commands and the
// controller API, not raw IME typing) so the same assertions hold on macOS,
// iOS, Android, Linux, and web. The goal is "does the parser load and the
// edit→parse→project→render pipeline work at all on this platform", not to
// re-cover the exhaustive editing matrix — that lives in
// `test/markdown_flow_test.dart`.
//
// Run it with, e.g.:
//   flutter test integration_test/flark_smoke_test.dart -d macos
//   flutter test integration_test/flark_smoke_test.dart -d <ios-sim-id>
//   flutter test integration_test/flark_smoke_test.dart -d <android-emu-id>
//   flutter test integration_test/flark_smoke_test.dart -d linux
//   flutter test integration_test/flark_smoke_test.dart -d chrome
import 'package:example/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the shipped parser loads and renders the seeded document', (
    tester,
  ) async {
    await tester.pumpWidget(const FlarkExampleApp());
    await _settleParsing(tester);

    // If the platform's parser backend failed to load, the controller never
    // reaches an authoritative render plan and this is where it shows.
    expect(
      _controller(tester).hasAuthoritativeRenderPlan,
      isTrue,
      reason:
          'the native/WASM Comrak bridge must load and parse on this '
          'platform',
    );
    expect(find.text('Flark Markdown Editor'), findsOneWidget);
    expect(_controller(tester).markdown, contains('# Flark Markdown'));
    expect(find.byKey(const Key('FlarkLiveBlockEditor')), findsOneWidget);
  });

  testWidgets('a bold command round-trips through the real parser', (
    tester,
  ) async {
    await tester.pumpWidget(const FlarkExampleApp());
    await _settleParsing(tester);
    await _loadScratch(tester);

    final editable = find.byType(EditableText);
    expect(editable, findsOneWidget);
    await tester.enterText(editable, 'bold text');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, 'bold text');

    // Select "bold" and toggle strong through the toolbar (deterministic on
    // every platform, unlike synthesizing a key chord).
    expect(
      _controller(
        tester,
      ).applySelection(const FlarkSelection(baseOffset: 0, extentOffset: 4)),
      isTrue,
    );
    await tester.pump();
    await _tapKey(tester, 'flark-example-command-bold');
    await _settleParsing(tester);

    // Source carries the markers; the rendered slice hides them — proof the
    // parse → projection → render path ran, not just a string edit.
    expect(_controller(tester).markdown, '**bold** text');
    expect(_editorDisplayText(tester), 'bold text');
  });

  testWidgets('an inserted code fence renders as a fenced-code block', (
    tester,
  ) async {
    await tester.pumpWidget(const FlarkExampleApp());
    await _settleParsing(tester);
    await _loadScratch(tester);

    await _tapKey(tester, 'flark-example-command-code-fence');
    await _settleParsing(tester);

    expect(_controller(tester).markdown, contains('```'));
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
  });
}

FlarkFlutterController _controller(WidgetTester tester) {
  return tester
      .widget<FlarkMarkdownEditor>(find.byType(FlarkMarkdownEditor))
      .controller!;
}

String _editorDisplayText(WidgetTester tester) {
  return tester
      .widgetList<EditableText>(find.byType(EditableText))
      .map((editable) => editable.controller.text)
      .join('\n');
}

Finder _key(String value) => find.byKey(ValueKey<String>(value));

Future<void> _tapKey(WidgetTester tester, String value) async {
  final finder = _key(value);
  expect(finder, findsOneWidget);
  await tester.ensureVisible(finder);
  await tester.pump();
  await tester.tap(finder);
  await tester.pump();
}

Future<void> _loadScratch(WidgetTester tester) async {
  await _tapKey(tester, 'flark-example-scenario-scratch');
  await _settleParsing(tester);
  expect(_controller(tester).markdown, isEmpty);
  expect(find.byType(EditableText), findsOneWidget);
}

/// Pumps until the controller holds an authoritative render plan for the
/// current revision, driving the async parse forward with real time. Mirrors
/// the helper in `test/markdown_flow_test.dart` so the smoke behaves the same
/// as the exhaustive flow suite.
Future<void> _settleParsing(WidgetTester tester) async {
  await tester.pump();
  for (var i = 0; i < 80; i++) {
    final controller = _controller(tester);
    if (controller.markdown.isEmpty || controller.hasAuthoritativeRenderPlan) {
      await tester.pump();
      return;
    }
    // Keep the browser event loop and the binding's clock moving separately.
    // A Web parse can start before runAsync and must be pumped after its
    // root-zone WASM work completes; awaiting that already-started Future from
    // runAsync can otherwise wait on the very pump it prevents.
    await tester.pump(const Duration(milliseconds: 50));
    await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    });
    await tester.pump();
  }
  expect(_controller(tester).hasAuthoritativeRenderPlan, isTrue);
  await tester.pump();
}
