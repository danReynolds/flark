import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  testWidgets('blank live rows remain visible between list blocks', (
    tester,
  ) async {
    final controller = SovereignFlutterController.fromMarkdown(
      '- one\n\n\n- two',
      extensions: SovereignMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await _pumpLiveEditor(tester, controller, height: 240);

    expect(find.byKey(_listMarkerKey), findsNWidgets(2));
    expect(find.byType(EditableText), findsNWidgets(4));

    final editorRects = _editableRects(tester);
    expect(editorRects[1].top, greaterThan(editorRects[0].top + 8));
    expect(editorRects[2].top, greaterThan(editorRects[1].top + 8));
    expect(editorRects[3].top, greaterThan(editorRects[2].top + 8));

    final markerRects = _rectsFor(tester, find.byKey(_listMarkerKey));
    expect(
      (markerRects[0].center.dy - editorRects[0].center.dy).abs(),
      lessThan(8),
    );
    expect(
      (markerRects[1].center.dy - editorRects[3].center.dy).abs(),
      lessThan(8),
    );
  });

  testWidgets('empty final list item exits into a visible focused blank row', (
    tester,
  ) async {
    final controller = SovereignFlutterController(
      runtime: SovereignEditorRuntime(
        state: SovereignEditorState.fromMarkdown(
          '- item\n- ',
          selection: const SovereignSelection.collapsed(9),
        ),
        extensions: SovereignMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await _pumpLiveEditor(tester, controller, autofocus: true);
    await tester.showKeyboard(find.byType(EditableText).at(1));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '- item\n\n');
    expect(find.byKey(_listMarkerKey), findsOneWidget);
    expect(find.byType(EditableText), findsNWidgets(3));

    final editorRects = _editableRects(tester);
    expect(editorRects.last.top, greaterThan(editorRects.first.top + 18));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(editors.last.controller.text, isEmpty);
    expect(editors.last.focusNode.hasFocus, isTrue);
  });

  testWidgets(
    'closed code fence stays visually bounded before following quote',
    (tester) async {
      const markdown = '```dart\ncode\n```\n\n> quote';
      final controller = SovereignFlutterController.fromMarkdown(
        markdown,
        extensions: SovereignMarkdownEditingExtensions.standard(),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await _pumpLiveEditor(tester, controller, height: 260);

      final fenceRect = tester.getRect(find.byKey(_codeFenceKey));
      final quoteRect = tester.getRect(find.byKey(_blockquoteKey));
      expect(fenceRect.bottom, lessThan(quoteRect.top));

      final codeEditable = _editableInside(
        tester,
        find.byKey(_codeEditableKey),
      );
      expect(codeEditable.controller.text, 'code');
      final quoteEditable = tester
          .widgetList<EditableText>(
            find.descendant(
              of: find.byKey(_blockquoteKey),
              matching: find.byType(EditableText),
            ),
          )
          .single;
      expect(quoteEditable.controller.text, 'quote');
    },
  );

  testWidgets('open code fence after a list gap does not overlap the list', (
    tester,
  ) async {
    const markdown = '- before\n\n```\nopen fence\n  code';
    final controller = SovereignFlutterController.fromMarkdown(
      markdown,
      extensions: SovereignMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await _pumpLiveEditor(tester, controller, height: 280);

    final markerRect = tester.getRect(find.byKey(_listMarkerKey));
    final fenceRect = tester.getRect(find.byKey(_codeFenceKey));
    expect(markerRect.bottom, lessThan(fenceRect.top));

    final codeEditable = _editableInside(tester, find.byKey(_codeEditableKey));
    expect(codeEditable.controller.text, 'open fence\n  code');
  });
}

const _blockquoteKey = Key('SovereignLiveBlockBlockquote');
const _codeEditableKey = Key('SovereignLiveBlockCodeEditable');
const _codeFenceKey = Key('SovereignLiveBlockCodeFence');
const _listMarkerKey = Key('SovereignLiveBlockListMarker');

Future<void> _pumpLiveEditor(
  WidgetTester tester,
  SovereignFlutterController controller, {
  double width = 360,
  double height = 220,
  bool autofocus = false,
}) async {
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: SizedBox(
        width: width,
        height: height,
        child: SovereignLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14, height: 1.4),
          autofocus: autofocus,
        ),
      ),
    ),
  );
  await tester.pump();
}

Future<void> _applyComrakParseResult(
  SovereignFlutterController controller,
) async {
  final result = await SovereignNativeComrakParseBackend.withNativeBridge()
      .parse(
        SovereignMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: controller.markdown,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );
  expect(controller.applyParseResult(result), isTrue);
}

List<Rect> _editableRects(WidgetTester tester) {
  final finder = find.byType(EditableText);
  return [
    for (var i = 0; i < finder.evaluate().length; i++)
      tester.getRect(finder.at(i)),
  ];
}

List<Rect> _rectsFor(WidgetTester tester, Finder finder) {
  return [
    for (var i = 0; i < finder.evaluate().length; i++)
      tester.getRect(finder.at(i)),
  ];
}

EditableText _editableInside(WidgetTester tester, Finder container) {
  return tester.widget<EditableText>(
    find.descendant(of: container, matching: find.byType(EditableText)),
  );
}
