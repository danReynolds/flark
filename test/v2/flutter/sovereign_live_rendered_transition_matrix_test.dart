import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('Sovereign live-rendered transition matrix', () {
    for (final transitionCase in _sourceToRenderedCases) {
      testWidgets('${transitionCase.id} preserves typing through activation',
          (tester) async {
        final controller = SovereignFlutterController.fromMarkdown(
          '',
          extensions: SovereignMarkdownEditingExtensions.standard(),
        );
        addTearDown(controller.dispose);

        await _pumpLiveEditor(tester, controller);
        await tester.showKeyboard(_editableFinder());
        await tester.pump();

        tester.testTextInput.enterText(transitionCase.markerOnlyMarkdown);
        await tester.pump();
        await _applyComrakParseResult(controller);
        await tester.pump();

        expect(
          _activeEditable(tester).controller.text,
          transitionCase.markerOnlyDisplayText,
          reason: transitionCase.id,
        );
        expect(
          _activeEditable(tester).focusNode.hasFocus,
          isTrue,
          reason: transitionCase.id,
        );
        transitionCase.expectMarkerOnly?.call(tester);

        tester.testTextInput.enterText(transitionCase.activatedMarkdown);
        await tester.pump();
        await _applyComrakParseResult(controller);
        await tester.pump();

        expect(
          _activeEditable(tester).controller.text,
          transitionCase.activatedDisplayText,
          reason: transitionCase.id,
        );
        expect(
          _activeEditable(tester).focusNode.hasFocus,
          isTrue,
          reason: transitionCase.id,
        );
        transitionCase.expectActivated?.call(tester);

        tester.testTextInput.enterText(transitionCase.bodyText);
        await tester.pump();

        expect(
          controller.markdown,
          transitionCase.expectedMarkdownAfterBody,
          reason: transitionCase.id,
        );
        expect(
          _activeEditable(tester).controller.text,
          transitionCase.bodyText,
          reason: transitionCase.id,
        );
      });
    }

    testWidgets('task list marker preserves typing through checkbox activation',
        (tester) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '',
        extensions: SovereignMarkdownEditingExtensions.standard(),
      );
      addTearDown(controller.dispose);

      await _pumpLiveEditor(tester, controller);
      await tester.showKeyboard(_editableFinder());
      await tester.pump();

      tester.testTextInput.enterText('- ');
      await tester.pump();
      await _applyComrakParseResult(controller);
      await tester.pump();
      expect(find.byKey(const Key('SovereignLiveBlockListMarker')),
          findsOneWidget);
      expect(_activeEditable(tester).controller.text, isEmpty);

      tester.testTextInput.enterText('[ ] ');
      await tester.pump();
      await _applyComrakParseResult(controller);
      await tester.pump();
      expect(
        find.byKey(const Key('SovereignLiveBlockTaskCheckbox')),
        findsOneWidget,
      );
      expect(_activeEditable(tester).controller.text, isEmpty);
      expect(_activeEditable(tester).focusNode.hasFocus, isTrue);

      tester.testTextInput.enterText('done');
      await tester.pump();

      expect(controller.markdown, '- [ ] done');
      expect(_activeEditable(tester).controller.text, 'done');
    });
  });
}

final _sourceToRenderedCases = [
  _LiveTransitionCase(
    id: 'heading',
    markerOnlyMarkdown: '#',
    markerOnlyDisplayText: '#',
    activatedMarkdown: '# ',
    activatedDisplayText: '',
    bodyText: 'Title',
    expectedMarkdownAfterBody: '# Title',
  ),
  _LiveTransitionCase(
    id: 'blockquote',
    markerOnlyMarkdown: '>',
    markerOnlyDisplayText: '>',
    activatedMarkdown: '> ',
    activatedDisplayText: '',
    bodyText: 'quote',
    expectedMarkdownAfterBody: '> quote',
    expectMarkerOnly: (tester) {
      expect(
          find.byKey(const Key('SovereignLiveBlockBlockquote')), findsNothing);
    },
    expectActivated: (tester) {
      expect(find.byKey(const Key('SovereignLiveBlockBlockquote')),
          findsOneWidget);
    },
  ),
  _LiveTransitionCase(
    id: 'unordered list',
    markerOnlyMarkdown: '*',
    markerOnlyDisplayText: '*',
    activatedMarkdown: '* ',
    activatedDisplayText: '',
    bodyText: 'item',
    expectedMarkdownAfterBody: '* item',
    expectMarkerOnly: (tester) {
      expect(
          find.byKey(const Key('SovereignLiveBlockListMarker')), findsNothing);
    },
    expectActivated: (tester) {
      expect(find.byKey(const Key('SovereignLiveBlockListMarker')),
          findsOneWidget);
    },
  ),
  _LiveTransitionCase(
    id: 'ordered list',
    markerOnlyMarkdown: '1.',
    markerOnlyDisplayText: '1.',
    activatedMarkdown: '1. ',
    activatedDisplayText: '',
    bodyText: 'item',
    expectedMarkdownAfterBody: '1. item',
    expectMarkerOnly: (tester) {
      expect(
          find.byKey(const Key('SovereignLiveBlockListMarker')), findsNothing);
    },
    expectActivated: (tester) {
      expect(find.byKey(const Key('SovereignLiveBlockListMarker')),
          findsOneWidget);
    },
  ),
  _LiveTransitionCase(
    id: 'fenced code',
    markerOnlyMarkdown: '``',
    markerOnlyDisplayText: '``',
    activatedMarkdown: '```\n',
    activatedDisplayText: '',
    bodyText: 'code',
    expectedMarkdownAfterBody: '```\ncode',
    expectMarkerOnly: (tester) {
      expect(
          find.byKey(const Key('SovereignLiveBlockCodeFence')), findsNothing);
    },
    expectActivated: (tester) {
      expect(
          find.byKey(const Key('SovereignLiveBlockCodeFence')), findsOneWidget);
    },
  ),
];

Future<void> _pumpLiveEditor(
  WidgetTester tester,
  SovereignFlutterController controller,
) async {
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: SovereignLiveRenderedEditableText(
        controller: controller,
        style: const TextStyle(fontSize: 14),
      ),
    ),
  );
  await tester.pump();
}

Future<void> _applyComrakParseResult(
  SovereignFlutterController controller,
) async {
  final result =
      await SovereignNativeComrakParseBackend.withNativeBridge().parse(
    SovereignMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: SovereignMarkdownProfile.commonMarkGfm,
    ),
  );
  expect(controller.applyParseResult(result), isTrue);
}

Finder _editableFinder() => find.byType(EditableText).first;

EditableText _activeEditable(WidgetTester tester) {
  return tester.widget<EditableText>(_editableFinder());
}

final class _LiveTransitionCase {
  const _LiveTransitionCase({
    required this.id,
    required this.markerOnlyMarkdown,
    required this.markerOnlyDisplayText,
    required this.activatedMarkdown,
    required this.activatedDisplayText,
    required this.bodyText,
    required this.expectedMarkdownAfterBody,
    this.expectMarkerOnly,
    this.expectActivated,
  });

  final String id;
  final String markerOnlyMarkdown;
  final String markerOnlyDisplayText;
  final String activatedMarkdown;
  final String activatedDisplayText;
  final String bodyText;
  final String expectedMarkdownAfterBody;
  final void Function(WidgetTester tester)? expectMarkerOnly;
  final void Function(WidgetTester tester)? expectActivated;
}
