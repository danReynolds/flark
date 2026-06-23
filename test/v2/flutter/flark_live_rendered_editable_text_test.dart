import 'dart:async';
import 'dart:ui' as ui show ImageByteFormat;
import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/rendering.dart' show RenderRepaintBoundary;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  testWidgets(
    'styles projected inline markdown while editing canonical source',
    (tester) async {
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            '**bold** and *em* and `code`',
            selection: const FlarkSelection.collapsed(6),
          ),
        ),
      );
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(_inlineParseResult(controller)),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'bold and em and code');
      expect(editable.textInputAction, TextInputAction.newline);
      expect(editable.paintCursorAboveText, isTrue);

      final span = editable.controller.buildTextSpan(
        context: tester.element(find.byType(EditableText)),
        style: editable.style,
        withComposing: false,
      );
      expect(_containsFontWeight(span, FontWeight.w700), isTrue);
      expect(_containsFontStyle(span, FontStyle.italic), isTrue);
      expect(_containsFontFamily(span, 'monospace'), isTrue);

      await tester.enterText(
        find.byType(EditableText),
        'bold! and em and code',
      );
      await tester.pump();

      expect(controller.markdown, '**bold!** and *em* and `code`');
      final updatedEditable = tester.widget<EditableText>(
        find.byType(EditableText),
      );
      final updatedSpan = updatedEditable.controller.buildTextSpan(
        context: tester.element(find.byType(EditableText)),
        style: updatedEditable.style,
        withComposing: false,
      );
      expect(_containsFontWeight(updatedSpan, FontWeight.w700), isTrue);
    },
  );

  testWidgets(
    'keeps partial bold delimiters literal until closing marker is complete',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown('**wow*');
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );

      var editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, '**wow*');
      var span = editable.controller.buildTextSpan(
        context: tester.element(find.byType(EditableText)),
        style: editable.style,
        withComposing: false,
      );
      expect(_containsFontStyle(span, FontStyle.italic), isFalse);
      expect(_containsFontWeight(span, FontWeight.w700), isFalse);

      await tester.enterText(find.byType(EditableText), '**wow**');
      await tester.pump();
      expect(controller.markdown, '**wow**');

      await _applyComrakParseResult(controller);
      await tester.pump();

      editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'wow');
      span = editable.controller.buildTextSpan(
        context: tester.element(find.byType(EditableText)),
        style: editable.style,
        withComposing: false,
      );
      expect(_containsFontWeight(span, FontWeight.w700), isTrue);
      expect(_containsFontStyle(span, FontStyle.italic), isFalse);
    },
  );

  testWidgets('keeps malformed inline syntax source-visible while editing', (
    tester,
  ) async {
    const cases = [
      _InlineEdgeCase(
        id: 'missing closing strong marker',
        markdown: '**wow*',
        displayText: '**wow*',
      ),
      _InlineEdgeCase(
        id: 'missing opening strong marker',
        markdown: '*wow**',
        displayText: '*wow**',
      ),
      _InlineEdgeCase(
        id: 'missing closing underscore strong marker',
        markdown: '__wow_',
        displayText: '__wow_',
      ),
      _InlineEdgeCase(
        id: 'missing opening underscore strong marker',
        markdown: '_wow__',
        displayText: '_wow__',
      ),
      _InlineEdgeCase(
        id: 'triple delimiter with only one closer',
        markdown: '***wow*',
        displayText: '***wow*',
      ),
      _InlineEdgeCase(
        id: 'escaped partial strong marker',
        markdown: r'\**wow*',
        displayText: '**wow*',
      ),
      _InlineEdgeCase(
        id: 'partial inline link destination',
        markdown: '[label](url',
        displayText: '[label](url',
      ),
      _InlineEdgeCase(
        id: 'partial inline image destination',
        markdown: '![alt](url',
        displayText: '![alt](url',
      ),
      _InlineEdgeCase(
        id: 'double backtick with short closing run',
        markdown: '``code`',
        displayText: '``code`',
      ),
      _InlineEdgeCase(
        id: 'inline code shields delimiter text',
        markdown: '`**wow*`',
        displayText: '**wow*',
        expectMonospace: true,
      ),
    ];

    for (final inlineCase in cases) {
      final controller = FlarkFlutterController.fromMarkdown(
        inlineCase.markdown,
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(
        editable.controller.text,
        inlineCase.displayText,
        reason: inlineCase.id,
      );
      final span = editable.controller.buildTextSpan(
        context: tester.element(find.byType(EditableText)),
        style: editable.style,
        withComposing: false,
      );
      expect(
        _containsFontWeight(span, FontWeight.w700),
        isFalse,
        reason: inlineCase.id,
      );
      expect(
        _containsFontStyle(span, FontStyle.italic),
        isFalse,
        reason: inlineCase.id,
      );
      expect(
        _containsFontFamily(span, 'monospace'),
        inlineCase.expectMonospace,
        reason: inlineCase.id,
      );
    }
  });

  testWidgets('keeps inline code text selectable in live editing', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '**bold** and *em* and `code`',
          selection: const FlarkSelection.collapsed(0),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_inlineParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final editableFinder = find.byType(EditableText);
    final editableState = tester.state<EditableTextState>(editableFinder);
    final codeCaretRect = editableState.renderEditable.getLocalRectForCaret(
      const TextPosition(offset: 18),
    );

    await tester.tapAt(
      editableState.renderEditable.localToGlobal(codeCaretRect.center),
    );
    await tester.pump();

    final editable = tester.widget<EditableText>(editableFinder);
    expect(
      editable.controller.selection.extentOffset,
      inInclusiveRange(16, 20),
    );
    expect(controller.selection.extentOffset, inInclusiveRange(23, 27));
  });

  testWidgets('keeps trailing newlines editable in plain live documents', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          'A\n',
          selection: const FlarkSelection.collapsed(2),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_paragraphParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'A\n');
    expect(editable.textInputAction, TextInputAction.newline);

    await tester.enterText(find.byType(EditableText), 'A\nB');
    await tester.pump();

    expect(controller.markdown, 'A\nB');
  });

  testWidgets('renders and edits HTML entities through replacement ranges', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          'A &amp; B',
          selection: const FlarkSelection.collapsed(7),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_entityParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'A & B');

    await tester.enterText(find.byType(EditableText), 'A X B');
    await tester.pump();

    expect(controller.markdown, 'A X B');
  });

  testWidgets('keeps marker-only quote lines source-visible', (tester) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '>',
          selection: const FlarkSelection.collapsed(1),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_bareQuoteParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockEditor')), findsNothing);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, '>');
    expect(editable.textInputAction, TextInputAction.newline);

    await tester.enterText(find.byType(EditableText), '>q');
    await tester.pump();

    expect(controller.markdown, '>q');
  });

  testWidgets('renders empty quote blocks immediately after marker space', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '> ',
          selection: const FlarkSelection.collapsed(2),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_emptyQuoteParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockEditor')), findsOneWidget);
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, isEmpty);

    await tester.enterText(find.byType(EditableText), 'quote');
    await tester.pump();

    expect(controller.markdown, '> quote');
  });

  testWidgets('renders multiline blockquotes as one editable rail', (
    tester,
  ) async {
    const markdown = '> first\n> second';
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          markdown,
          selection: const FlarkSelection.collapsed(markdown.length),
        ),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_multiLineQuoteParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'first\nsecond',
    );

    await tester.enterText(find.byType(EditableText), 'first!\nsecond');
    await tester.pump();

    expect(controller.markdown, '> first!\n> second');
  });

  testWidgets('routes Enter through live blockquotes to continue and exit', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '> quote',
          selection: const FlarkSelection.collapsed(7),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_quoteParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'quote',
    );
    await tester.showKeyboard(find.byType(EditableText));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '> quote\n> ');
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'quote\n',
    );
    expect(controller.selection, const FlarkSelection.collapsed(10));

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '> quote\n\n');
    expect(controller.selection, const FlarkSelection.collapsed(9));
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    final quoteEditable = find.descendant(
      of: find.byKey(const Key('FlarkLiveBlockBlockquote')),
      matching: find.byType(EditableText),
    );
    expect(quoteEditable, findsOneWidget);
    expect(tester.widget<EditableText>(quoteEditable).controller.text, 'quote');
    final editableTexts = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .map((editable) => editable.controller.text)
        .toList(growable: false);
    expect(editableTexts, ['quote', '']);

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, '> quote');
    expect(controller.selection, const FlarkSelection.collapsed(7));
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'quote',
    );
  });

  testWidgets('routes Backspace from empty live blockquotes to remove them', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '> ',
          selection: const FlarkSelection.collapsed(2),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_emptyQuoteParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    await tester.showKeyboard(find.byType(EditableText));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, isEmpty);
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsNothing);
    expect(find.byType(EditableText), findsOneWidget);
  });

  testWidgets('routes Backspace at live quote content start to unwrap it', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '> quote',
          selection: const FlarkSelection.collapsed(2),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_quoteParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
    await tester.showKeyboard(find.byType(EditableText));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, 'quote');
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsNothing);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'quote',
    );
  });

  testWidgets('routes live heading Backspace through hidden marker policy', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '## Heading',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);
    controller.applySelection(
      const FlarkSelection.collapsed(3),
      userEvent: 'test',
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'Heading');
    expect(
      editable.controller.selection,
      const TextSelection.collapsed(offset: 0),
    );
    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, 'Heading');
    expect(controller.selection, const FlarkSelection.collapsed(0));
  });

  testWidgets('routes live list Backspace through hidden marker policy', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '- item',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);
    controller.applySelection(
      const FlarkSelection.collapsed(2),
      userEvent: 'test',
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'item');
    expect(
      editable.controller.selection,
      const TextSelection.collapsed(offset: 0),
    );
    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, 'item');
    expect(controller.selection, const FlarkSelection.collapsed(0));
  });

  testWidgets('renders live unordered list marker beside editable item text', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '- item',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'item');
  });

  testWidgets('keeps marker-only unordered list input source-visible', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '*',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNothing);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, '*');
  });

  testWidgets('renders live unordered marker immediately after marker space', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '- ',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, isEmpty);
  });

  testWidgets('keeps unordered marker element stable while typing item text', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '* item',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    final markerFinder = find.byKey(const Key('FlarkLiveBlockListMarker'));
    expect(markerFinder, findsOneWidget);
    final markerElement = tester.element(markerFinder);

    await tester.showKeyboard(find.byType(EditableText));
    tester.testTextInput.enterText('items');
    await tester.pump();

    expect(controller.markdown, '* items');
    expect(markerFinder, findsOneWidget);
    expect(identical(tester.element(markerFinder), markerElement), isTrue);

    await _applyComrakParseResult(controller);
    await tester.pump();

    expect(markerFinder, findsOneWidget);
    expect(identical(tester.element(markerFinder), markerElement), isTrue);
  });

  testWidgets('moves focus to the continued unordered list item after Enter', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* item',
          selection: const FlarkSelection.collapsed(6),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    final editableFinder = find.byType(EditableText);
    expect(editableFinder, findsOneWidget);
    await tester.showKeyboard(editableFinder);
    await tester.pump();

    await tester.enterText(editableFinder, 'item\n');
    await tester.pumpAndSettle();

    expect(controller.markdown, '* item\n* ');
    expect(controller.selection, const FlarkSelection.collapsed(9));
    expect(find.byType(EditableText), findsNWidgets(2));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(editors.first.controller.text, 'item');
    expect(editors.first.focusNode.hasFocus, isFalse);
    expect(editors.last.controller.text, isEmpty);
    expect(
      editors.last.controller.selection,
      const TextSelection.collapsed(offset: 0),
    );
    expect(editors.last.focusNode.hasFocus, isTrue);
  });

  testWidgets('undoes and redoes live list continuation from Enter', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* item',
          selection: const FlarkSelection.collapsed(6),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    final editableFinder = find.byType(EditableText);
    await tester.showKeyboard(editableFinder);
    await tester.pump();

    await tester.enterText(editableFinder, 'item\n');
    await tester.pumpAndSettle();

    expect(controller.markdown, '* item\n* ');

    controller.undo();
    await tester.pump();
    expect(controller.markdown, '* item');
    expect(controller.selection, const FlarkSelection.collapsed(6));

    controller.redo();
    await tester.pump();
    expect(controller.markdown, '* item\n* ');
    expect(controller.selection, const FlarkSelection.collapsed(9));
  });

  testWidgets(
    'moves focus out of an empty final unordered list item after Enter',
    (tester) async {
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            '* item\n* ',
            selection: const FlarkSelection.collapsed(9),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      expect(find.byType(EditableText), findsNWidgets(2));
      await tester.showKeyboard(find.byType(EditableText).at(1));
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();

      expect(controller.markdown, '* item\n\n');
      expect(controller.selection, const FlarkSelection.collapsed(8));
      expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
      expect(find.byType(EditableText), findsNWidgets(3));
      final editors = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .toList(growable: false);
      expect(editors.first.controller.text, 'item');
      expect(editors.first.focusNode.hasFocus, isFalse);
      expect(editors.last.controller.text, isEmpty);
      expect(
        editors.last.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(editors.last.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('coalesced platform Enters exit a nonempty live list item', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* one\n* two',
          selection: const FlarkSelection.collapsed(11),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(EditableText), findsNWidgets(2));
    await tester.showKeyboard(find.byType(EditableText).at(1));
    await tester.pump();

    tester.testTextInput.enterText('two\n\n');
    await tester.pumpAndSettle();

    expect(controller.markdown, '* one\n* two\n\n');
    expect(controller.selection, const FlarkSelection.collapsed(13));
    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNWidgets(2));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(
      [for (final editor in editors.take(2)) editor.controller.text],
      ['one', 'two'],
    );
    expect([
      for (final editor in editors.skip(2)) editor.controller.text,
    ], everyElement(isEmpty));
    expect(editors.last.focusNode.hasFocus, isTrue);

    await _typeFocusedTextIncrementally(
      tester,
      '```dart\nprint(1);\n```\nDone',
    );
    await tester.pump();

    expect(
      controller.markdown,
      '* one\n* two\n\n```dart\nprint(1);\n```\nDone',
    );
    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNWidgets(2));
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(_codeEditableText(tester), 'print(1);');
    expect(_editableFinderWithText('Done'), findsOneWidget);
  });

  testWidgets('opens and exits a code fence after leaving a live list', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* one\n* two\n* ',
          selection: const FlarkSelection.collapsed(14),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(EditableText), findsNWidgets(3));
    await tester.showKeyboard(find.byType(EditableText).at(2));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '* one\n* two\n\n');
    expect(_focusedEditableText(tester).controller.text, isEmpty);

    await _typeFocusedTextIncrementally(
      tester,
      '```js\nconsole.log(1);\n```\nDone',
    );
    await tester.pump();

    expect(
      controller.markdown,
      '* one\n* two\n\n```javascript\nconsole.log(1);\n```\nDone',
    );
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(_codeEditableText(tester), 'console.log(1);');
    expect(_editableFinderWithText('Done'), findsOneWidget);
  });

  testWidgets('renders parser-omitted blank lines between live block widgets', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '* one\n\n\n* two',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNWidgets(2));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(
      [for (final editor in editors) editor.controller.text],
      ['one', '', '', 'two'],
    );
  });

  testWidgets('adds repeated empty lines from parser-omitted live gap hosts', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* item\n\n',
          selection: const FlarkSelection.collapsed(8),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(EditableText), findsNWidgets(3));
    await tester.showKeyboard(find.byType(EditableText).last);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '* item\n\n\n');
    expect(controller.selection, const FlarkSelection.collapsed(9));
    expect(find.byType(EditableText), findsNWidgets(4));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(editors.first.controller.text, 'item');
    expect(
      editors.skip(1).every((editor) => editor.controller.text.isEmpty),
      isTrue,
    );
    expect(editors.last.focusNode.hasFocus, isTrue);
  });

  testWidgets(
    'keeps typing in parser-omitted live blank lines source-visible',
    (tester) async {
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            '* one\n\n\n* two',
            selection: const FlarkSelection.collapsed(7),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      final editableFinder = find.byType(EditableText);
      expect(editableFinder, findsNWidgets(4));
      await tester.showKeyboard(editableFinder.at(2));
      await tester.enterText(editableFinder.at(2), 'between');
      await tester.pump();

      expect(controller.markdown, '* one\n\nbetween\n* two');
      expect(
        tester
            .widgetList<EditableText>(find.byType(EditableText))
            .map((editor) => editor.controller.text)
            .toList(growable: false),
        ['one', '', 'between', 'two'],
      );

      await _applyComrakParseResult(controller);
      await tester.pump();

      expect(
        tester
            .widgetList<EditableText>(find.byType(EditableText))
            .map((editor) => editor.controller.text)
            .toList(growable: false),
        contains('between'),
      );
    },
  );

  testWidgets('keeps caret on paragraph typed into blank line before a list', (
    tester,
  ) async {
    const markdown = 'Intro\n\n- [x] Task';
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          markdown,
          selection: const FlarkSelection.collapsed(6),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    var editableFinder = find.byType(EditableText);
    expect(
      tester
          .widgetList<EditableText>(editableFinder)
          .map((editor) => editor.controller.text)
          .toList(growable: false),
      ['Intro', '', 'Task'],
    );

    await tester.showKeyboard(editableFinder.at(1));
    await tester.enterText(editableFinder.at(1), 'f');
    await tester.pump();

    expect(controller.markdown, 'Intro\n\nf\n- [x] Task');
    expect(controller.selection, const FlarkSelection.collapsed(8));

    await _applyComrakParseResult(controller);
    await tester.pump();

    editableFinder = find.byType(EditableText);
    final editors = tester
        .widgetList<EditableText>(editableFinder)
        .toList(growable: false);
    expect(
      editors.map((editor) => editor.controller.text).toList(growable: false),
      ['Intro', '', 'f', 'Task'],
    );
    final focusedEditorIndex = editors.indexWhere(
      (editor) => editor.focusNode.hasFocus,
    );
    expect(focusedEditorIndex, 2);
    expect(editors[focusedEditorIndex].controller.text, 'f');
    expect(
      editors[focusedEditorIndex].controller.selection,
      const TextSelection.collapsed(offset: 1),
    );

    await tester.enterText(editableFinder.at(focusedEditorIndex), 'abcdef');
    await tester.pump();

    expect(controller.markdown, 'Intro\n\nabcdef\n- [x] Task');

    await _applyComrakParseResult(controller);
    await tester.pump();

    editableFinder = find.byType(EditableText);
    final updatedEditors = tester
        .widgetList<EditableText>(editableFinder)
        .toList(growable: false);
    expect(
      updatedEditors
          .map((editor) => editor.controller.text)
          .toList(growable: false),
      ['Intro', '', 'abcdef', 'Task'],
    );
    final updatedFocusedEditorIndex = updatedEditors.indexWhere(
      (editor) => editor.focusNode.hasFocus,
    );
    expect(updatedFocusedEditorIndex, 2);
  });

  testWidgets(
    'moves focus out of an empty final ordered list item after Enter',
    (tester) async {
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            '1. item\n2. ',
            selection: const FlarkSelection.collapsed(11),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      expect(find.byType(EditableText), findsNWidgets(2));
      await tester.showKeyboard(find.byType(EditableText).at(1));
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();

      expect(controller.markdown, '1. item\n\n');
      expect(controller.selection, const FlarkSelection.collapsed(9));
      expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
      expect(find.byType(EditableText), findsNWidgets(3));
      final editors = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .toList(growable: false);
      expect(editors.first.controller.text, 'item');
      expect(editors.first.focusNode.hasFocus, isFalse);
      expect(editors.last.controller.text, isEmpty);
      expect(
        editors.last.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(editors.last.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('moves focus out of an empty final task item after Enter', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '- [ ] item\n- [ ] ',
          selection: const FlarkSelection.collapsed(17),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(EditableText), findsNWidgets(2));
    await tester.showKeyboard(find.byType(EditableText).at(1));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '- [ ] item\n\n');
    expect(controller.selection, const FlarkSelection.collapsed(12));
    expect(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')), findsOneWidget);
    expect(find.byType(EditableText), findsNWidgets(3));
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(editors.first.controller.text, 'item');
    expect(editors.first.focusNode.hasFocus, isFalse);
    expect(editors.last.controller.text, isEmpty);
    expect(
      editors.last.controller.selection,
      const TextSelection.collapsed(offset: 0),
    );
    expect(editors.last.focusNode.hasFocus, isTrue);
  });

  testWidgets('select all delete clears hidden live list markers', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '* one\n* two',
          selection: const FlarkSelection.collapsed(11),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    final editableFinder = find.byType(EditableText);
    expect(editableFinder, findsNWidgets(2));
    await tester.showKeyboard(editableFinder.at(1));
    await tester.pump();

    Actions.invoke(
      tester.element(editableFinder.at(1)),
      const SelectAllTextIntent(SelectionChangedCause.keyboard),
    );
    await tester.pump();

    expect(
      controller.selection,
      const FlarkSelection(baseOffset: 0, extentOffset: 11),
    );
    final selectedEditors = tester
        .widgetList<EditableText>(editableFinder)
        .toList(growable: false);
    expect(
      selectedEditors.first.controller.selection,
      const TextSelection(baseOffset: 0, extentOffset: 3),
    );
    expect(
      selectedEditors.last.controller.selection,
      const TextSelection(baseOffset: 0, extentOffset: 3),
    );

    Actions.invoke(
      tester.element(editableFinder.at(1)),
      const DeleteCharacterIntent(forward: false),
    );
    await tester.pumpAndSettle();

    expect(controller.markdown, isEmpty);
    expect(controller.selection, const FlarkSelection.collapsed(0));
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(editableFinder).controller.text,
      isEmpty,
    );
    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNothing);
  });

  testWidgets('keeps focus while completing a bare star into a list marker', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    var editableFinder = find.byType(EditableText);
    await tester.showKeyboard(editableFinder);
    await tester.pump();
    expect(
      tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
      isTrue,
    );

    await tester.enterText(editableFinder, '*');
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNothing);
    var editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, '*');
    expect(editable.focusNode.hasFocus, isTrue);

    await tester.enterText(find.byType(EditableText), '* ');
    await tester.pump();
    await _applyComrakParseResult(controller);
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
    editableFinder = find.byType(EditableText);
    editable = tester.widget<EditableText>(editableFinder);
    expect(editable.controller.text, isEmpty);
    expect(editable.focusNode.hasFocus, isTrue);

    tester.testTextInput.enterText('item');
    await tester.pump();

    expect(controller.markdown, '* item');
    editable = tester.widget<EditableText>(editableFinder);
    expect(editable.controller.text, 'item');
  });

  testWidgets('renders live ordered list marker beside editable item text', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '3. item',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
    expect(find.text('3.'), findsOneWidget);
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'item');
  });

  testWidgets('renders code fences and quotes as editable block widgets', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_blockParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 240,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    expect(codeEditable.controller.text, 'print(1);');
    expect(codeEditable.textInputAction, TextInputAction.newline);
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);

    await tester.enterText(_codeEditableFinder(), 'print(2);');
    await tester.pump();

    expect(controller.markdown, '```dart\nprint(2);\n```\n\n> quote');
  });

  testWidgets('copy action writes live code body without fence markers', (
    tester,
  ) async {
    final clipboardPayloads = <Object?>[];
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') {
        clipboardPayloads.add(call.arguments);
        return null;
      }
      return null;
    });
    addTearDown(() {
      messenger.setMockMethodCallHandler(SystemChannels.platform, null);
    });
    final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_blockParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 360,
          height: 240,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14, height: 1.4),
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockCodeCopyButton')));
    await tester.runAsync(() async {
      await Future<void>.delayed(Duration.zero);
    });
    await tester.pump();

    expect(clipboardPayloads, [
      {'text': 'print(1);'},
    ]);
    expect(controller.markdown, _blockMarkdown);
  });

  testWidgets('code fence language selector edits the opening fence info', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_blockParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => FlarkMarkdownInteractions(
                controller: controller,
                config: const FlarkMarkdownInteractionConfig(),
                editable: true,
                child: SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );

    expect(
      find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')),
      findsOneWidget,
    );
    final buttonRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')),
    );
    final fenceRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeFence')),
    );
    final codeRect = tester.getRect(_codeEditableFinder());
    expect(buttonRect.center.dx, greaterThan(fenceRect.center.dx));
    expect(buttonRect.top, lessThan(fenceRect.top + 14));
    expect(codeRect.right, lessThan(buttonRect.left));

    await tester.tap(find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')));
    await tester.pump();

    expect(
      find.byKey(const Key('FlarkLiveBlockCodeLanguageMenu')),
      findsOneWidget,
    );
    final openFenceRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeFence')),
    );
    final menuRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeLanguageMenu')),
    );
    expect(openFenceRect.height, fenceRect.height);
    expect(menuRect.top, lessThan(openFenceRect.bottom));
    expect(menuRect.width, lessThanOrEqualTo(160));
    expect(menuRect.right, moreOrLessEquals(buttonRect.right, epsilon: 1));

    await tester.tap(
      find.byKey(const ValueKey('FlarkLiveBlockCodeLanguageOption:rust')),
    );
    await tester.pump();

    expect(controller.markdown, '```rust\nprint(1);\n```\n\n> quote');
    expect(find.text('Rust'), findsOneWidget);
  });

  testWidgets('undoes and redoes code fence language selections', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_blockParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => FlarkMarkdownInteractions(
                controller: controller,
                config: const FlarkMarkdownInteractionConfig(),
                editable: true,
                child: SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')));
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('FlarkLiveBlockCodeLanguageOption:rust')),
    );
    await tester.pump();

    expect(controller.markdown, '```rust\nprint(1);\n```\n\n> quote');

    controller.undo();
    await tester.pump();
    expect(controller.markdown, _blockMarkdown);

    controller.redo();
    await tester.pump();
    expect(controller.markdown, '```rust\nprint(1);\n```\n\n> quote');
  });

  testWidgets(
    'keeps code body as the active text input after language changes',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(_blockParseResult(controller)),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => FlarkMarkdownInteractions(
                  controller: controller,
                  config: const FlarkMarkdownInteractionConfig(),
                  editable: true,
                  child: SizedBox(
                    width: 320,
                    height: 240,
                    child: FlarkLiveRenderedEditableText(
                      controller: controller,
                      style: const TextStyle(fontSize: 14, height: 1.4),
                      expands: true,
                      maxLines: null,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      await tester.tap(_codeEditableFinder());
      await tester.showKeyboard(_codeEditableFinder());
      await tester.pump();

      await tester.tap(
        find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')),
      );
      await tester.pump();
      await tester.tap(
        find.byKey(const ValueKey('FlarkLiveBlockCodeLanguageOption:rust')),
      );
      await tester.pump();
      await tester.pump();
      final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
      expect(codeEditable.focusNode.hasFocus, isTrue);

      tester.testTextInput.enterText('let value = 1;');
      await tester.pump();

      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        'let value = 1;',
      );
      expect(controller.markdown, '```rust\nlet value = 1;\n```\n\n> quote');
    },
  );

  testWidgets(
    'focuses the code body when dismissing the language menu into it',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(_blockParseResult(controller)),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => FlarkMarkdownInteractions(
                  controller: controller,
                  config: const FlarkMarkdownInteractionConfig(),
                  editable: true,
                  child: SizedBox(
                    width: 320,
                    height: 240,
                    child: FlarkLiveRenderedEditableText(
                      controller: controller,
                      style: const TextStyle(fontSize: 14, height: 1.4),
                      expands: true,
                      maxLines: null,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      expect(
        tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
        isFalse,
      );

      await tester.tap(
        find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')),
      );
      await tester.pump();
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeLanguageMenu')),
        findsOneWidget,
      );

      await tester.tap(_codeEditableFinder());
      await tester.pump();

      expect(
        find.byKey(const Key('FlarkLiveBlockCodeLanguageMenu')),
        findsNothing,
      );
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
        isTrue,
      );

      tester.testTextInput.enterText('let value = 1;');
      await tester.pump();

      expect(controller.markdown, '```dart\nlet value = 1;\n```\n\n> quote');
    },
  );

  testWidgets(
    'keeps a manually typed code fence opener source-visible as plain editing',
    (tester) async {
      const markdown = '```fffffff';
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            markdown,
            selection: const FlarkSelection.collapsed(markdown.length),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
      );
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(
          _unclosedCodeFenceOpenerParseResult(controller),
        ),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownInteractions(
            controller: controller,
            config: const FlarkMarkdownInteractionConfig(),
            editable: true,
            child: FlarkLiveRenderedEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
            ),
          ),
        ),
      );
      await tester.pump();

      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsNothing);
      final editable = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
          matching: find.byType(EditableText),
        ),
      );
      expect(editable.controller.text, markdown);
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: markdown.length),
      );
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')),
        findsNothing,
      );
    },
  );

  testWidgets('transitions code fence opener to hidden markers after Enter', (
    tester,
  ) async {
    const markdown = '```dart\n';
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          markdown,
          selection: const FlarkSelection.collapsed(markdown.length),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(
        _unclosedCodeFenceBodyParseResult(controller),
      ),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownInteractions(
          controller: controller,
          config: const FlarkMarkdownInteractionConfig(),
          editable: true,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(
      find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
      findsNothing,
    );
    final editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, isEmpty);
    expect(editable.controller.text, isNot(contains('```')));
  });

  testWidgets(
    'typing a fence opener from a blank live editor creates a code block',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.controller.text, isNot(contains('```')));
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('the cursor blinks in a fence body right after the opener '
      'auto-closes', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));
    await tester.pump();

    // The body editable adopts an already-focused node, which fires no
    // focus-change event — the cursor must still be made visible without
    // waiting for the first keystroke.
    final state = tester.state<EditableTextState>(
      find.descendant(
        of: find.byKey(const Key('FlarkLiveBlockCodeEditable')),
        matching: find.byType(EditableText),
      ),
    );
    expect(state.widget.focusNode.hasPrimaryFocus, isTrue);
    expect(state.cursorCurrentlyVisible, isTrue);
  });

  testWidgets('typing after an immediate fence opener edits code body text', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('ggggg');
    await tester.pump();

    expect(controller.markdown, '```\nggggg');
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    final editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, 'ggggg');
    expect(editable.focusNode.hasFocus, isTrue);

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'ggggg\n\n',
        selection: TextSelection.collapsed(offset: 6),
      ),
    );
    await tester.pump();

    expect(controller.markdown, '```\nggggg\n');
    final echoedEditable = tester.widget<EditableText>(_codeEditableFinder());
    expect(echoedEditable.controller.text, 'ggggg\n');
    expect(echoedEditable.focusNode.hasFocus, isTrue);
  });

  testWidgets('normalizes coalesced web code body Enter at value end', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('ggggg');
    await tester.pump();

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'ggggg\n\n',
        selection: TextSelection.collapsed(offset: 7),
      ),
    );
    await tester.pump();

    expect(controller.markdown, '```\nggggg\n');
    final editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, 'ggggg\n');
    expect(editable.focusNode.hasFocus, isTrue);
  });

  testWidgets(
    'keyboard Enter after an immediate empty fence opener is ignored',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      await tester.showKeyboard(_codeEditableFinder());
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      tester.testTextInput.enterText('\n');
      await tester.pump();

      expect(controller.markdown, '```\n');
      var editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);

      tester.testTextInput.enterText('f');
      await tester.pump();

      expect(controller.markdown, '```\nf');
      editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, 'f');
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('ignores web newline echo after code body keyboard Enter', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('ggggg');
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    expect(controller.markdown, '```\nggggg\n');
    var editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, 'ggggg\n');
    expect(editable.focusNode.hasFocus, isTrue);

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'ggggg\n\n',
        selection: TextSelection.collapsed(offset: 6),
      ),
    );
    await tester.pump();

    expect(controller.markdown, '```\nggggg\n');
    editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, 'ggggg\n');
    expect(editable.focusNode.hasFocus, isTrue);
  });

  testWidgets(
    'ignores normalized web echo before code body snapshot refreshes',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      await tester.showKeyboard(_codeEditableFinder());
      tester.testTextInput.enterText('ggggg');
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'ggggg\n',
          selection: TextSelection.collapsed(offset: 6),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\nggggg\n');
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, 'ggggg\n');
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'known first code line promotes to fence language on Enter shortcut',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      await tester.showKeyboard(_codeEditableFinder());
      tester.testTextInput.enterText('dart');
      await tester.pump();
      tester.testTextInput.enterText('dart\n\n');
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 200));

      expect(controller.markdown, '```dart\n');
      expect(controller.selection, const FlarkSelection.collapsed(8));
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);

      tester.testTextInput.enterText('dart\n');
      await tester.pump();

      expect(controller.markdown, '```dart\n');
      final updatedEditable = tester.widget<EditableText>(
        _codeEditableFinder(),
      );
      expect(updatedEditable.controller.text, isEmpty);
      expect(updatedEditable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('coalesced language shortcut keeps following code body', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('js');
    await tester.pump();
    tester.testTextInput.enterText('js\nconsole.log(1);');
    await tester.pump();

    expect(controller.markdown, '```javascript\nconsole.log(1);');
    expect(controller.selection, const FlarkSelection.collapsed(29));
    final editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, 'console.log(1);');
    expect(editable.focusNode.hasFocus, isTrue);
  });

  testWidgets(
    'known first code line promotes to fence language on keyboard Enter',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      await tester.showKeyboard(_codeEditableFinder());
      tester.testTextInput.enterText('dart');
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 200));

      expect(controller.markdown, '```dart\n');
      expect(controller.selection, const FlarkSelection.collapsed(8));
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'opening a fence from a middle blank row preserves following blocks',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        'Intro\n\n- [x] task',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 220,
                  child: FlarkMarkdownEditor(
                    controller: controller,
                    editingMode: FlarkMarkdownEditingMode.liveRendered,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    autofocus: true,
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );
      await tester.pump();

      final blankEditableFinder = find.byWidgetPredicate(
        (widget) => widget is EditableText && widget.controller.text.isEmpty,
        description: 'blank live row editable',
      );
      expect(blankEditableFinder, findsOneWidget);

      await tester.tap(blankEditableFinder);
      await tester.showKeyboard(blankEditableFinder);
      tester.testTextInput.enterText('```');
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 200));

      expect(controller.markdown, 'Intro\n\n```\n```\n- [x] task');
      expect(controller.selection, const FlarkSelection.collapsed(11));
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockTaskCheckbox')),
        findsOneWidget,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('typing a closing fence exits a middle auto-closed code block', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      'Intro\n\n- [x] task',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 220,
                child: FlarkMarkdownEditor(
                  controller: controller,
                  editingMode: FlarkMarkdownEditingMode.liveRendered,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  autofocus: true,
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    final blankEditableFinder = find.byWidgetPredicate(
      (widget) => widget is EditableText && widget.controller.text.isEmpty,
      description: 'blank live row editable',
    );
    expect(blankEditableFinder, findsOneWidget);

    await tester.tap(blankEditableFinder);
    await tester.showKeyboard(blankEditableFinder);
    tester.testTextInput.enterText('```');
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('abc');
    await tester.pump();
    tester.testTextInput.enterText('abc\n');
    await tester.pump();
    tester.testTextInput.enterText('abc\n```');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(controller.markdown, 'Intro\n\n```\nabc\n```\n- [x] task');
    expect(controller.selection, const FlarkSelection.collapsed(19));
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')), findsOneWidget);
    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    expect(codeEditable.controller.text, 'abc');
    expect(codeEditable.focusNode.hasFocus, isFalse);
  });

  testWidgets('typing a closing fence exits a scratch-opened code block', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 220,
                child: FlarkMarkdownEditor(
                  controller: controller,
                  editingMode: FlarkMarkdownEditingMode.liveRendered,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  autofocus: true,
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    await _typeBlankFenceOpener(tester, find.byType(EditableText));

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('abc');
    await tester.pump();
    tester.testTextInput.enterText('abc\n');
    await tester.pump();
    tester.testTextInput.enterText('abc\n```');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(controller.markdown, '```\nabc\n```');
    expect(controller.selection, const FlarkSelection.collapsed(11));
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    expect(codeEditable.controller.text, 'abc');
    expect(codeEditable.focusNode.hasFocus, isFalse);
  });

  testWidgets(
    'ignores duplicate platform newline after opening a blank live code fence',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      expect(controller.markdown, '```\n');
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );

      await tester.showKeyboard(_codeEditableFinder());
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 0),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'opens a fence immediately before fence block parsing catches up',
    (tester) async {
      final parseBackend = _BlockingParseBackend();
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseBackend: parseBackend,
        parseDebounce: Duration.zero,
      );
      addTearDown(() {
        parseBackend.completeAll();
        controller.dispose();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'normalizes auto-closed platform Enter from standalone fallback fence',
    (tester) async {
      final parseBackend = _BlockingParseBackend();
      final controller = FlarkFlutterController.fromMarkdown(
        '```',
        parseBackend: parseBackend,
        parseDebounce: Duration.zero,
      );
      controller.applySelection(const FlarkSelection.collapsed(0));
      addTearDown(() {
        parseBackend.completeAll();
        controller.dispose();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsNothing);

      await tester.showKeyboard(find.byType(EditableText));
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '```\n```\n',
          selection: TextSelection.collapsed(offset: 8),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        tester
            .widgetList<EditableText>(find.byType(EditableText))
            .map((editable) => editable.controller.text),
        isNot(contains('```\n```\n')),
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'ignores duplicate platform newline from an immediate blank live code fence',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      await _typeBlankFenceOpener(tester, find.byType(EditableText));

      await tester.showKeyboard(_codeEditableFinder());
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 0),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'platform Enter on an already opened live fence moves to the body',
    (tester) async {
      const markdown = '```\n';
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            markdown,
            selection: const FlarkSelection.collapsed(3),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(
          _unclosedCodeFenceBodyParseResult(controller),
        ),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      final openingEditableFinder = find.descendant(
        of: find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        matching: find.byType(EditableText),
      );
      expect(openingEditableFinder, findsOneWidget);

      await tester.showKeyboard(openingEditableFinder);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '```\n',
          selection: TextSelection.collapsed(offset: 4),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'typing a fence opener predicts a code block while parsing is blocked',
    (tester) async {
      final parseBackend = _BlockingParseBackend();
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        parseBackend: parseBackend,
        parseDebounce: Duration.zero,
      );
      addTearDown(() {
        parseBackend.completeAll();
        controller.dispose();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 180,
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              style: const TextStyle(fontSize: 14, height: 1.4),
              autofocus: true,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();
      expect(parseBackend.requests, hasLength(1));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);

      final editableFinder = find.byType(EditableText);
      await tester.showKeyboard(editableFinder);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '`',
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      await tester.pump();
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '``',
          selection: TextSelection.collapsed(offset: 2),
        ),
      );
      await tester.pump();
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '```',
          selection: TextSelection.collapsed(offset: 3),
        ),
      );
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final editable = tester.widget<EditableText>(_codeEditableFinder());
      expect(editable.controller.text, isEmpty);
      expect(editable.controller.text, isNot(contains('```')));
      expect(editable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets('platform newline on root fallback fence predicts a code block', (
    tester,
  ) async {
    final parseBackend = _BlockingParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '',
      parseBackend: parseBackend,
      parseDebounce: Duration.zero,
    );
    addTearDown(() {
      parseBackend.completeAll();
      controller.dispose();
    });

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();

    final editableFinder = find.byType(EditableText);
    await tester.showKeyboard(editableFinder);
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '`',
        selection: TextSelection.collapsed(offset: 1),
      ),
    );
    await tester.pump();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '``',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    await tester.pump();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '```',
        selection: TextSelection.collapsed(offset: 3),
      ),
    );
    await tester.pump();

    expect(controller.markdown, '```\n');
    expect(controller.selection, const FlarkSelection.collapsed(4));
    expect(controller.hasAuthoritativeRenderPlan, isFalse);
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(
      find.byKey(const Key('FlarkLiveBlockCodeOpeningEditable')),
      findsNothing,
    );
    final editable = tester.widget<EditableText>(_codeEditableFinder());
    expect(editable.controller.text, isEmpty);
    expect(editable.focusNode.hasFocus, isTrue);
  });

  testWidgets('fast typed closing fence exits an auto-closed code block', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '```\n```',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 220,
                child: FlarkMarkdownEditor(
                  controller: controller,
                  editingMode: FlarkMarkdownEditingMode.liveRendered,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  autofocus: true,
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    await tester.pump(const Duration(milliseconds: 200));

    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    await tester.tap(_codeEditableFinder());
    await tester.showKeyboard(_codeEditableFinder());
    await tester.pump();

    tester.testTextInput.enterText('foo');
    await tester.pump();
    tester.testTextInput.enterText('foo\n');
    await tester.pump();
    tester.testTextInput.enterText('foo\n`');
    await tester.pump();
    tester.testTextInput.enterText('foo\n``');
    await tester.pump();
    tester.testTextInput.enterText('foo\n```');
    await tester.pump();

    expect(controller.markdown, '```\nfoo\n```');
    expect(controller.selection, const FlarkSelection.collapsed(11));
  });

  testWidgets(
    'Backspace removes an empty live code fence instead of exposing markers',
    (tester) async {
      const markdown = '```\n';
      final controller = FlarkFlutterController(
        runtime: FlarkEditorRuntime(
          state: FlarkEditorState.fromMarkdown(
            markdown,
            selection: const FlarkSelection.collapsed(markdown.length),
          ),
          extensions: FlarkMarkdownEditingExtensions.standard(),
        ),
      );
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(
          _unclosedCodeFenceBodyParseResult(controller),
        ),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 180,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      await tester.showKeyboard(_codeEditableFinder());
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(controller.markdown, isEmpty);
      expect(controller.selection, const FlarkSelection.collapsed(0));
      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsNothing);
      expect(find.text('```'), findsNothing);
    },
  );

  testWidgets(
    'Backspace after a terminal live code fence moves into the fence',
    (tester) async {
      const markdown = '```dart\nfoo\n```';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        DefaultTextEditingShortcuts(
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Overlay(
              initialEntries: [
                OverlayEntry(
                  builder: (context) => SizedBox(
                    width: 320,
                    height: 240,
                    child: FlarkLiveRenderedEditableText(
                      controller: controller,
                      style: const TextStyle(fontSize: 14, height: 1.4),
                      expands: true,
                      maxLines: null,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      );

      final codeEditableFinder = _codeEditableFinder();
      final codeEditable = tester.widget<EditableText>(codeEditableFinder);
      codeEditable.controller.selection = const TextSelection.collapsed(
        offset: 3,
      );
      await tester.tap(codeEditableFinder);
      await tester.showKeyboard(codeEditableFinder);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      await tester.pump();

      expect(controller.selection, const FlarkSelection.collapsed(15));

      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, markdown);
      expect(controller.selection, const FlarkSelection.collapsed(11));
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        'foo',
      );
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
        isTrue,
      );
      expect(find.text('```'), findsNothing);
    },
  );

  testWidgets(
    'Backspace from following live text enters a code fence without markers',
    (tester) async {
      const markdown = '```dart\nfoo\n```\nafter';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        DefaultTextEditingShortcuts(
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Overlay(
              initialEntries: [
                OverlayEntry(
                  builder: (context) => SizedBox(
                    width: 320,
                    height: 240,
                    child: FlarkLiveRenderedEditableText(
                      controller: controller,
                      style: const TextStyle(fontSize: 14, height: 1.4),
                      expands: true,
                      maxLines: null,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      );

      final afterFinder = _editableFinderWithText('after');
      await tester.tap(afterFinder);
      final afterEditable = tester.widget<EditableText>(afterFinder);
      afterEditable.controller.selection = const TextSelection.collapsed(
        offset: 0,
      );
      await tester.pump();
      await tester.showKeyboard(afterFinder);

      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, markdown);
      expect(controller.selection, const FlarkSelection.collapsed(11));
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        'foo',
      );
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
        isTrue,
      );
      expect(find.text('```'), findsNothing);
    },
  );

  testWidgets('highlights live code block syntax from the fence language', (
    tester,
  ) async {
    const markdown = '```dart\nfinal value = 1;\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(
              color: Color(0xFF17202A),
              fontSize: 14,
              height: 1.4,
            ),
          ),
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    final span = codeEditable.controller.buildTextSpan(
      context: tester.element(_codeEditableFinder()),
      style: codeEditable.style,
      withComposing: false,
    );
    expect(_textSpanHasColor(span, 'final', const Color(0xFF7C3AED)), isTrue);
  });

  testWidgets('auto-highlights confident unlabeled live code blocks', (
    tester,
  ) async {
    const markdown = '```\n{"name":"Ada","count":2}\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(
              color: Color(0xFF17202A),
              fontSize: 14,
              height: 1.4,
            ),
          ),
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    final span = codeEditable.controller.buildTextSpan(
      context: tester.element(_codeEditableFinder()),
      style: codeEditable.style,
      withComposing: false,
    );
    expect(_textSpanHasColor(span, '"Ada"', const Color(0xFF0F766E)), isTrue);
    expect(_textSpanHasColor(span, '2', const Color(0xFFB45309)), isTrue);
  });

  testWidgets('leaves ambiguous unlabeled live code blocks plain', (
    tester,
  ) async {
    const markdown = '```\nfoo\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(
              color: Color(0xFF17202A),
              fontSize: 14,
              height: 1.4,
            ),
          ),
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    final span = codeEditable.controller.buildTextSpan(
      context: tester.element(_codeEditableFinder()),
      style: codeEditable.style,
      withComposing: false,
    );
    expect(_textSpanHasSyntaxColor(span), isFalse);
  });

  testWidgets('explicit text language disables live code block highlighting', (
    tester,
  ) async {
    const markdown = '```text\n{"name":"Ada","count":2}\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(
              color: Color(0xFF17202A),
              fontSize: 14,
              height: 1.4,
            ),
          ),
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    final span = codeEditable.controller.buildTextSpan(
      context: tester.element(_codeEditableFinder()),
      style: codeEditable.style,
      withComposing: false,
    );
    expect(_textSpanHasSyntaxColor(span), isFalse);
  });

  testWidgets('indents live code blocks with Tab', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(_blockMarkdown);
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_blockParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 240,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(const FlarkSelection.collapsed(8));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();

    expect(controller.markdown, '```dart\n  print(1);\n```\n\n> quote');
  });

  testWidgets('indents selected live code block lines with Tab', (
    tester,
  ) async {
    const markdown = '```dart\nprint(1);\nprint(2);\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 180,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(
      const FlarkSelection(baseOffset: 8, extentOffset: 27),
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();

    expect(controller.markdown, '```dart\n  print(1);\n  print(2);\n```');
  });

  testWidgets('keeps code indentation when Enter creates a new line', (
    tester,
  ) async {
    const markdown = '```dart\n  foo\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 180,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(const FlarkSelection.collapsed(13));
    await tester.pump();
    final codeHeightBeforeEnter = tester.getRect(codeEditable).height;

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    expect(controller.markdown, '```dart\n  foo\n  \n```');
    expect(controller.selection, const FlarkSelection.collapsed(16));
    final codeEditableAfterEnter = tester.widget<EditableText>(
      _codeEditableFinder(),
    );
    expect(codeEditableAfterEnter.controller.text, '  foo\n  ');
    expect(
      tester.getRect(_codeEditableFinder()).height,
      greaterThan(codeHeightBeforeEnter),
    );
  });

  testWidgets('keeps an unindented code line visible after Enter at EOF', (
    tester,
  ) async {
    const markdown = '```dart\nfoo';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(
        _unclosedCodeFenceBodyParseResult(controller),
      ),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 180,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(const FlarkSelection.collapsed(11));
    await tester.pump();
    final codeHeightBeforeEnter = tester.getRect(codeEditable).height;

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    expect(controller.markdown, '```dart\nfoo\n');
    expect(controller.selection, const FlarkSelection.collapsed(12));
    final codeEditableAfterEnter = tester.widget<EditableText>(
      _codeEditableFinder(),
    );
    expect(codeEditableAfterEnter.controller.text, 'foo\n');
    expect(
      tester.getRect(_codeEditableFinder()).height,
      greaterThan(codeHeightBeforeEnter),
    );
  });

  testWidgets(
    'keeps an empty code line visible after Enter in an empty fence',
    (tester) async {
      const markdown = '```dart\n```';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      expect(
        controller.applyParseResult(_codeOnlyParseResult(controller)),
        isTrue,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 180,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final codeEditable = _codeEditableFinder();
      await tester.tap(codeEditable);
      controller.applySelection(const FlarkSelection.collapsed(8));
      await tester.pump();
      final codeHeightBeforeEnter = tester.getRect(codeEditable).height;

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.markdown, '```dart\n\n```');
      expect(controller.selection, const FlarkSelection.collapsed(9));
      final codeEditableAfterEnter = tester.widget<EditableText>(
        _codeEditableFinder(),
      );
      expect(codeEditableAfterEnter.controller.text, '\n');
      expect(
        tester.getRect(_codeEditableFinder()).height,
        greaterThan(codeHeightBeforeEnter),
      );
    },
  );

  testWidgets('normalizes multiline paste indentation in live code fences', (
    tester,
  ) async {
    const markdown = '```dart\n  \n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 180,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(const FlarkSelection.collapsed(10));
    await tester.pump();

    await tester.enterText(codeEditable, '  if (x) {\nprint(1);\n}');
    await tester.pump();

    const expected = '```dart\n  if (x) {\n  print(1);\n  }\n```';
    expect(controller.markdown, expected);
    expect(
      controller.selection,
      FlarkSelection.collapsed(expected.indexOf('\n```')),
    );
    final codeEditableAfterPaste = tester.widget<EditableText>(
      _codeEditableFinder(),
    );
    expect(
      codeEditableAfterPaste.controller.text,
      '  if (x) {\n  print(1);\n  }',
    );
  });

  testWidgets('groups live code IME composition updates into one undo step', (
    tester,
  ) async {
    const markdown = '```dart\nprint(1);\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 180,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    await tester.tap(codeEditableFinder);
    await tester.pump();

    var codeEditable = tester.widget<EditableText>(codeEditableFinder);
    codeEditable.controller.value = const TextEditingValue(
      text: 'a',
      selection: TextSelection.collapsed(offset: 1),
      composing: TextRange(start: 0, end: 1),
    );
    await tester.pump();

    codeEditable = tester.widget<EditableText>(codeEditableFinder);
    codeEditable.controller.value = const TextEditingValue(
      text: 'あ',
      selection: TextSelection.collapsed(offset: 1),
      composing: TextRange(start: 0, end: 1),
    );
    await tester.pump();

    codeEditable = tester.widget<EditableText>(codeEditableFinder);
    codeEditable.controller.value = const TextEditingValue(
      text: 'あ',
      selection: TextSelection.collapsed(offset: 1),
    );
    await tester.pump();

    expect(controller.markdown, '```dart\nあ\n```');
    controller.undo();
    await tester.pump();
    expect(controller.markdown, markdown);
  });

  testWidgets('moves out of live code fences from vertical boundary lines', (
    tester,
  ) async {
    const markdown = 'before\n```dart\nfirst\nsecond\n```\nafter';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      DefaultTextEditingShortcuts(
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    await tester.tap(codeEditableFinder);
    await tester.showKeyboard(codeEditableFinder);
    final bodyStart = markdown.indexOf('first');
    controller.applySelection(FlarkSelection.collapsed(bodyStart));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    await tester.pump();

    expect(_editableTextWithText(tester, 'before').focusNode.hasFocus, isTrue);
    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isFalse,
    );
    expect(controller.selection.extentOffset, lessThan(bodyStart));

    await tester.tap(codeEditableFinder);
    await tester.pump();
    final bodyEnd = markdown.indexOf('\n```');
    controller.applySelection(FlarkSelection.collapsed(bodyEnd));
    await tester.pump();
    await tester.pump();
    await tester.showKeyboard(codeEditableFinder);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    await tester.pump();

    expect(_editableTextWithText(tester, 'after').focusNode.hasFocus, isTrue);
    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isFalse,
    );
    expect(controller.selection.extentOffset, greaterThan(bodyEnd));
  });

  testWidgets('moves down from a terminal live code fence into a text host', (
    tester,
  ) async {
    const markdown = '```dart\nfoo\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      DefaultTextEditingShortcuts(
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    await tester.tap(codeEditableFinder);
    await tester.showKeyboard(codeEditableFinder);
    final bodyEnd = markdown.indexOf('\n```');
    controller.applySelection(FlarkSelection.collapsed(bodyEnd));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    await tester.pump();

    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isFalse,
    );
    expect(controller.selection, const FlarkSelection.collapsed(15));
    expect(
      tester
          .widgetList<EditableText>(find.byType(EditableText))
          .any(
            (editable) =>
                editable.focusNode.hasFocus && editable.controller.text.isEmpty,
          ),
      isTrue,
    );

    tester.testTextInput.enterText('after');
    await tester.pump();

    expect(controller.markdown, '```dart\nfoo\n```\nafter');
  });

  testWidgets('tapping below a terminal live code fence appends after it', (
    tester,
  ) async {
    const markdown = '```dart\nfoo\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 240,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final fenceRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeFence')),
    );
    await tester.tapAt(fenceRect.bottomLeft + const Offset(8, 24));
    await tester.pump();
    await tester.pump();

    expect(controller.selection, const FlarkSelection.collapsed(15));
    final focused = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .where((editable) => editable.focusNode.hasFocus)
        .toList(growable: false);
    expect(focused, hasLength(1));
    expect(focused.single.controller.text, isEmpty);

    tester.testTextInput.enterText('after');
    await tester.pump();

    expect(controller.markdown, '```dart\nfoo\n```\nafter');
  });

  testWidgets(
    'typing a fence opener below a terminal live code fence opens immediately',
    (tester) async {
      const markdown = '```dart\nfoo\n```\n';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 260,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final fenceRect = tester.getRect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
      );
      await tester.tapAt(fenceRect.bottomLeft + const Offset(8, 24));
      await tester.pump();
      await tester.pump();

      final appendHostFinder = find.byWidgetPredicate(
        (widget) =>
            widget is EditableText &&
            widget.focusNode.hasFocus &&
            widget.controller.text.isEmpty,
        description: 'focused empty terminal append EditableText',
      );
      expect(appendHostFinder, findsOneWidget);
      await tester.showKeyboard(appendHostFinder);

      await _typeBlankFenceOpener(tester, appendHostFinder);

      expect(controller.markdown, '```dart\nfoo\n```\n```\n');
      expect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
        findsNWidgets(2),
      );
      final codeEditors = tester
          .widgetList<EditableText>(_codeEditableFinder())
          .toList(growable: false);
      expect(codeEditors, hasLength(2));
      expect(codeEditors.last.controller.text, isEmpty);
      expect(codeEditors.last.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'typing a third fence marker in a live paragraph opens immediately',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(
        '``',
        parseDebounce: Duration.zero,
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);
      controller.applySelection(const FlarkSelection.collapsed(2));

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 180,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      await tester.tap(find.byType(EditableText));
      await tester.showKeyboard(find.byType(EditableText));
      tester.testTextInput.enterText('```');
      await tester.pump();

      expect(controller.markdown, '```\n');
      expect(controller.selection, const FlarkSelection.collapsed(4));

      await tester.runAsync(controller.parseNow);
      await tester.pump();

      expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
      final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
      expect(codeEditable.controller.text, isEmpty);
      expect(codeEditable.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'typing a terminal fence opener still opens after the append host becomes text',
    (tester) async {
      const markdown = '```dart\nfoo\n```\n';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 260,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final fenceRect = tester.getRect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
      );
      await tester.tapAt(fenceRect.bottomLeft + const Offset(8, 24));
      await tester.pump();
      await tester.pump();

      await tester.showKeyboard(
        find.byWidgetPredicate(
          (widget) =>
              widget is EditableText &&
              widget.focusNode.hasFocus &&
              widget.controller.text.isEmpty,
          description: 'focused empty terminal append EditableText',
        ),
      );
      tester.testTextInput.enterText('`');
      await tester.pump();
      expect(controller.markdown, '```dart\nfoo\n```\n`');
      expect(_focusedEditableText(tester).controller.text, '`');

      tester.testTextInput.enterText('``');
      await tester.pump();
      expect(controller.markdown, '```dart\nfoo\n```\n``');
      expect(_focusedEditableText(tester).controller.text, '``');

      tester.testTextInput.enterText('```');
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n```\n');
      expect(controller.selection, const FlarkSelection.collapsed(20));

      await tester.runAsync(controller.parseNow);
      await tester.pump();

      expect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
        findsNWidgets(2),
      );
      final codeEditors = tester
          .widgetList<EditableText>(_codeEditableFinder())
          .toList(growable: false);
      expect(codeEditors, hasLength(2));
      expect(codeEditors.last.controller.text, isEmpty);
      expect(codeEditors.last.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'typing below a terminal live code fence preserves focus per keystroke',
    (tester) async {
      const markdown = '```dart\nfoo\n```';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final fenceRect = tester.getRect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
      );
      await tester.tapAt(fenceRect.bottomLeft + const Offset(8, 24));
      await tester.pump();
      await tester.pump();

      tester.testTextInput.enterText('a');
      await tester.pump();
      await tester.pump();
      await tester.pump();
      expect(controller.markdown, '```dart\nfoo\n```\na');
      expect(
        tester
            .widgetList<EditableText>(find.byType(EditableText))
            .singleWhere((editable) => editable.focusNode.hasFocus)
            .controller
            .text,
        'a',
      );

      tester.testTextInput.enterText('af');
      await tester.pump();
      tester.testTextInput.enterText('aft');
      await tester.pump();
      tester.testTextInput.enterText('afte');
      await tester.pump();
      tester.testTextInput.enterText('after');
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\nafter');
      expect(
        tester
            .widgetList<EditableText>(find.byType(EditableText))
            .singleWhere((editable) => editable.focusNode.hasFocus)
            .controller
            .text,
        'after',
      );
    },
  );

  testWidgets(
    'typing after blank lines below a terminal live code fence keeps caret on latest line',
    (tester) async {
      const markdown = '```dart\nfoo\n```';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final fenceRect = tester.getRect(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
      );
      await tester.tapAt(fenceRect.bottomLeft + const Offset(8, 24));
      await tester.pump();
      await tester.pump();

      for (var blankLineCount = 1; blankLineCount <= 4; blankLineCount++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pump();
        await tester.pump();

        final blankLines = List.filled(blankLineCount, '\n').join();
        expect(controller.markdown, '```dart\nfoo\n```$blankLines');
        expect(
          controller.selection,
          FlarkSelection.collapsed(15 + blankLineCount),
        );
        final focused = tester
            .widgetList<EditableText>(find.byType(EditableText))
            .singleWhere((editable) => editable.focusNode.hasFocus);
        expect(focused.controller.text, isEmpty);
      }

      tester.testTextInput.enterText('x');
      await tester.pump();
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nx');
      expect(controller.selection, const FlarkSelection.collapsed(20));
      final focused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(focused.controller.text, 'x');
      expect(
        focused.controller.selection,
        const TextSelection.collapsed(offset: 1),
      );

      focused.controller.value = const TextEditingValue(
        text: 'xy',
        selection: TextSelection.collapsed(offset: 1),
      );
      expect(
        focused.controller.selection,
        const TextSelection.collapsed(offset: 2),
      );

      focused.controller.value = const TextEditingValue(
        text: 'xyz',
        selection: TextSelection.collapsed(offset: 2),
      );
      expect(
        focused.controller.selection,
        const TextSelection.collapsed(offset: 3),
      );
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nxyz');
      expect(controller.selection, const FlarkSelection.collapsed(22));
      final syntheticFastTypedFocused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(syntheticFastTypedFocused.controller.text, 'xyz');
      expect(
        syntheticFastTypedFocused.controller.selection,
        const TextSelection.collapsed(offset: 3),
      );

      await _applyComrakParseResult(controller);
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nxyz');
      expect(controller.selection, const FlarkSelection.collapsed(22));
      final reconciledFocused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(reconciledFocused.controller.text, 'xyz');
      expect(
        reconciledFocused.controller.selection,
        const TextSelection.collapsed(offset: 3),
      );

      reconciledFocused.controller.value = const TextEditingValue(
        text: 'xyzw',
        selection: TextSelection.collapsed(offset: 3),
      );
      expect(
        reconciledFocused.controller.selection,
        const TextSelection.collapsed(offset: 4),
      );
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nxyzw');
      expect(controller.selection, const FlarkSelection.collapsed(23));
      final fastTypedFocused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(fastTypedFocused.controller.text, 'xyzw');
      expect(
        fastTypedFocused.controller.selection,
        const TextSelection.collapsed(offset: 4),
      );
    },
  );

  testWidgets(
    'typing after blank lines from a live code fence exit keeps caret on latest line',
    (tester) async {
      const markdown = '```dart\nfoo\n\n```';
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 260,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      );

      final codeEditable = _codeEditableFinder();
      await tester.tap(codeEditable);
      await tester.showKeyboard(codeEditable);
      controller.applySelection(const FlarkSelection.collapsed(12));
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n');
      expect(controller.selection, const FlarkSelection.collapsed(16));

      for (var blankLineCount = 2; blankLineCount <= 4; blankLineCount++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pump();
        await tester.pump();

        final blankLines = List.filled(blankLineCount, '\n').join();
        expect(controller.markdown, '```dart\nfoo\n```$blankLines');
        expect(
          controller.selection,
          FlarkSelection.collapsed(15 + blankLineCount),
        );
        final focused = tester
            .widgetList<EditableText>(find.byType(EditableText))
            .singleWhere((editable) => editable.focusNode.hasFocus);
        expect(focused.controller.text, isEmpty);
      }

      tester.testTextInput.enterText('x');
      await tester.pump();
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nx');
      expect(controller.selection, const FlarkSelection.collapsed(20));
      final focused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(focused.controller.text, 'x');
      expect(
        focused.controller.selection,
        const TextSelection.collapsed(offset: 1),
      );

      await _applyComrakParseResult(controller);
      await tester.pump();
      await tester.pump();

      expect(controller.markdown, '```dart\nfoo\n```\n\n\n\nx');
      expect(controller.selection, const FlarkSelection.collapsed(20));
      final reconciledFocused = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .singleWhere((editable) => editable.focusNode.hasFocus);
      expect(reconciledFocused.controller.text, 'x');
      expect(
        reconciledFocused.controller.selection,
        const TextSelection.collapsed(offset: 1),
      );
    },
  );

  testWidgets('tapping below a terminal live list appends outside the list', (
    tester,
  ) async {
    const markdown = '- item';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => SizedBox(
                width: 320,
                height: 200,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14, height: 1.4),
                  expands: true,
                  maxLines: null,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final listMarkerRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockListMarker')),
    );
    await tester.tapAt(listMarkerRect.bottomLeft + const Offset(8, 28));
    await tester.pump();
    await tester.pump();

    expect(controller.selection, const FlarkSelection.collapsed(6));
    final focused = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .where((editable) => editable.focusNode.hasFocus)
        .toList(growable: false);
    expect(focused, hasLength(1));
    expect(focused.single.controller.text, isEmpty);

    tester.testTextInput.enterText('after');
    await tester.pump();

    expect(controller.markdown, '- item\n\nafter');
  });

  testWidgets('moves up from following text into a live code fence', (
    tester,
  ) async {
    const markdown = '```dart\nfoo\n```\nafter';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      DefaultTextEditingShortcuts(
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final afterFinder = _editableFinderWithText('after');
    await tester.tap(afterFinder);
    await tester.pump();
    final afterEditable = tester.widget<EditableText>(afterFinder);
    afterEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    await tester.showKeyboard(afterFinder);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    await tester.pump();

    final codeEditableFinder = _codeEditableFinder();
    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isTrue,
    );
    expect(controller.selection, const FlarkSelection.collapsed(11));
  });

  testWidgets('live code fences use a visible selection highlight', (
    tester,
  ) async {
    const markdown = '```dart\nfoo\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14, height: 1.4),
          ),
        ),
      ),
    );

    final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    expect(codeEditable.selectionColor, isNotNull);
    expect(codeEditable.selectionColor, isNot(const Color(0x00000000)));
  });

  testWidgets('drag selection works in plain live documents', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown('foo bar');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 20, height: 1.4),
          ),
        ),
      ),
    );

    final editableFinder = find.byType(EditableText);
    final rect = tester.getRect(editableFinder);
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.down(rect.centerLeft + const Offset(8, 0));
    await tester.pump();
    await gesture.moveTo(rect.centerLeft + const Offset(76, 0));
    await tester.pump();
    await gesture.up();
    await tester.pump();

    final editable = tester.widget<EditableText>(editableFinder);
    expect(editable.controller.selection.isCollapsed, isFalse);
    expect(controller.selection.isCollapsed, isFalse);
  });

  testWidgets('drag selection works inside live code fences', (tester) async {
    const markdown = '```dart\nfoo bar\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 20, height: 1.4),
          ),
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    final rect = tester.getRect(codeEditableFinder);
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.down(rect.centerLeft + const Offset(8, 0));
    await tester.pump();
    await gesture.moveTo(rect.centerLeft + const Offset(76, 0));
    await tester.pump();
    await gesture.up();
    await tester.pump();

    final codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.selection.isCollapsed, isFalse);
    expect(controller.selection.isCollapsed, isFalse);
  });

  testWidgets('double click selects a word inside live code fences', (
    tester,
  ) async {
    const markdown = '```dart\nfoo bar\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          height: 160,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 20, height: 1.4),
          ),
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    final rect = tester.getRect(codeEditableFinder);
    final wordOffset = rect.centerLeft + const Offset(18, 0);
    await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
    await tester.pump();

    final codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.selection.textInside('foo bar'), 'foo');
    expect(controller.selection.isCollapsed, isFalse);
  });

  testWidgets('keeps vertical arrows native inside live code fences', (
    tester,
  ) async {
    const markdown = 'before\n```dart\none\ntwo\nthree\n```\nafter';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      DefaultTextEditingShortcuts(
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: [
              OverlayEntry(
                builder: (context) => SizedBox(
                  width: 320,
                  height: 240,
                  child: FlarkLiveRenderedEditableText(
                    controller: controller,
                    style: const TextStyle(fontSize: 14, height: 1.4),
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final codeEditableFinder = _codeEditableFinder();
    await tester.tap(codeEditableFinder);
    await tester.showKeyboard(codeEditableFinder);
    final middleLineStart = markdown.indexOf('two');
    controller.applySelection(FlarkSelection.collapsed(middleLineStart));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isTrue,
    );
    expect(
      controller.selection.extentOffset,
      inInclusiveRange(markdown.indexOf('one'), markdown.indexOf('three')),
    );

    controller.applySelection(FlarkSelection.collapsed(middleLineStart));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isTrue,
    );
    expect(
      controller.selection.extentOffset,
      inInclusiveRange(markdown.indexOf('one'), markdown.indexOf('three')),
    );
  });

  testWidgets('outdents live code blocks with Shift+Tab', (tester) async {
    const markdown = '```dart\n  print(1);\n```';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_codeOnlyParseResult(controller)),
      isTrue,
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 180,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14, height: 1.4),
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );

    final codeEditable = _codeEditableFinder();
    await tester.tap(codeEditable);
    controller.applySelection(const FlarkSelection.collapsed(10));
    await tester.pump();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();

    expect(controller.markdown, '```dart\nprint(1);\n```');
  });

  testWidgets('toggles task checkboxes through live block widgets', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('- [ ] Write tests');
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_taskParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')));
    await tester.pump();

    expect(controller.markdown, '- [x] Write tests');
  });

  testWidgets('undoes and redoes task checkbox toggles', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown('- [ ] Write tests');
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_taskParseResult(controller)), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')));
    await tester.pump();

    expect(controller.markdown, '- [x] Write tests');
    expect(controller.runtime.canUndo, isTrue);

    controller.undo();
    await tester.pump();
    expect(controller.markdown, '- [ ] Write tests');
    expect(controller.runtime.canRedo, isTrue);

    controller.redo();
    await tester.pump();
    expect(controller.markdown, '- [x] Write tests');
  });

  testWidgets('checkbox toggles preserve focused task text editing', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('- [ ] Write tests');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    final editableFinder = find.byType(EditableText);
    await tester.tap(editableFinder);
    final editable = tester.widget<EditableText>(editableFinder);
    editable.controller.selection = const TextSelection.collapsed(offset: 5);
    await tester.pump();
    await tester.showKeyboard(editableFinder);
    expect(
      tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
      isTrue,
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')));
    await tester.pump();

    expect(controller.markdown, '- [x] Write tests');
    expect(
      tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
      isTrue,
    );
    expect(
      tester.widget<EditableText>(editableFinder).controller.text,
      'Write tests',
    );
    expect(controller.selection, const FlarkSelection.collapsed(11));
  });

  testWidgets('continues checked live task items as unchecked focused rows', (
    tester,
  ) async {
    const markdown = '- [x] done';
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          markdown,
          selection: const FlarkSelection.collapsed(markdown.length),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(controller.markdown, '- [x] done\n- [ ] ');
    expect(controller.selection, const FlarkSelection.collapsed(17));
    expect(
      find.byKey(const Key('FlarkLiveBlockTaskCheckbox')),
      findsNWidgets(2),
    );
    final editors = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .toList(growable: false);
    expect(editors.map((editor) => editor.controller.text), ['done', '']);
    expect(editors.first.focusNode.hasFocus, isFalse);
    expect(editors.last.focusNode.hasFocus, isTrue);
  });

  testWidgets('Backspace degrades empty live task items before removing list', (
    tester,
  ) async {
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '- [ ] ',
          selection: const FlarkSelection.collapsed(6),
        ),
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
          autofocus: true,
        ),
      ),
    );
    await tester.pump();

    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, '- ');
    expect(controller.selection, const FlarkSelection.collapsed(2));
    await _applyComrakParseResult(controller);
    await tester.pump();
    expect(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')), findsNothing);
    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      isEmpty,
    );

    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();

    expect(controller.markdown, isEmpty);
    expect(controller.selection, const FlarkSelection.collapsed(0));
    expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsNothing);
  });

  testWidgets('toggling one live checkbox leaves sibling tasks unchanged', (
    tester,
  ) async {
    const markdown = '- [ ] first\n- [x] second';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    expect(
      find.byKey(const Key('FlarkLiveBlockTaskCheckbox')),
      findsNWidgets(2),
    );

    await tester.tap(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')).at(1));
    await tester.pump();

    expect(controller.markdown, '- [ ] first\n- [ ] second');
    await _applyComrakParseResult(controller);
    await tester.pump();
    expect(
      tester
          .widgetList<EditableText>(find.byType(EditableText))
          .map((editor) => editor.controller.text)
          .toList(growable: false),
      ['first', 'second'],
    );
  });

  testWidgets('edits table cells through live block widgets', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockTable')), findsOneWidget);

    await tester.enterText(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      'Done',
    );
    await tester.pump();

    expect(
      controller.markdown,
      '| Area | Status |\n| --- | --- |\n| Preview | Done |',
    );
  });

  testWidgets('pads irregular table rows with editable source insertions', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      _irregularTableMarkdown,
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockTable')), findsOneWidget);
    expect(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      findsOneWidget,
    );

    await tester.enterText(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      'Done',
    );
    await tester.pump();

    expect(
      controller.markdown,
      '| Area | Status |\n| --- | --- |\n| Preview | Done |',
    );
  });

  testWidgets('keeps live table editing bounded to parser column count', (
    tester,
  ) async {
    const markdown =
        '| Area | Status |\n'
        '| --- | --- |\n'
        '| Preview | Guarded | Ignored |';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    final result = await FlarkNativeComrakParseBackend.withNativeBridge().parse(
      FlarkMarkdownParseRequest(
        revision: controller.state.revision,
        markdown: markdown,
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );
    expect(controller.applyParseResult(result), isTrue);
    expect(controller.renderPlan.tableBlocks.single.table!.rows, isNotEmpty);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockTable')), findsOneWidget);
    expect(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-0')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      findsOneWidget,
    );
    expect(find.byKey(const Key('FlarkLiveBlockTableCell-1-2')), findsNothing);

    await tester.enterText(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      'Done',
    );
    await tester.pump();

    expect(
      controller.markdown,
      '| Area | Status |\n| --- | --- |\n| Preview | Done | Ignored |',
    );
  });

  testWidgets('keeps separator-looking live table body rows editable', (
    tester,
  ) async {
    const markdown = '| Area | Status |\n| --- | --- |\n| --- | --- |';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    expect(find.byKey(const Key('FlarkLiveBlockTable')), findsOneWidget);
    expect(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-0')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      findsOneWidget,
    );
  });

  testWidgets('keeps table cell selection local after mid-cell edits', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final cellFinder = _tableCellEditableFinder(1, 0);
    await tester.tap(cellFinder);
    final cell = tester.widget<EditableText>(cellFinder);
    cell.controller.selection = const TextSelection.collapsed(offset: 3);
    await tester.showKeyboard(cellFinder);
    await tester.pump();

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'Pre-view',
        selection: TextSelection.collapsed(offset: 4),
      ),
    );
    await tester.pump();

    expect(
      controller.markdown,
      '| Area | Status |\n| --- | --- |\n| Pre-view | Guarded |',
    );
    final sourceStart = controller.markdown.indexOf('Pre-view');
    expect(controller.selection, FlarkSelection.collapsed(sourceStart + 4));
    expect(
      tester.widget<EditableText>(cellFinder).controller.selection,
      const TextSelection.collapsed(offset: 4),
    );
  });

  testWidgets(
    'groups live table cell IME composition updates into one undo step',
    (tester) async {
      final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 420,
            child: FlarkLiveRenderedEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14),
            ),
          ),
        ),
      );

      final cellFinder = _tableCellEditableFinder(1, 0);
      await tester.tap(cellFinder);
      await tester.pump();

      var cell = tester.widget<EditableText>(cellFinder);
      cell.controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      await tester.pump();

      cell = tester.widget<EditableText>(cellFinder);
      cell.controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      await tester.pump();

      cell = tester.widget<EditableText>(cellFinder);
      cell.controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
      );
      await tester.pump();

      expect(
        controller.markdown,
        '| Area | Status |\n| --- | --- |\n| あ | Guarded |',
      );
      controller.undo();
      await tester.pump();
      expect(controller.markdown, _tableMarkdown);
    },
  );

  testWidgets('maps table cell selection through escaped pipes', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final cellFinder = _tableCellEditableFinder(1, 0);
    await tester.tap(cellFinder);
    await tester.showKeyboard(cellFinder);
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'A|B',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    await tester.pump();

    expect(
      controller.markdown,
      r'| Area | Status |'
      '\n'
      r'| --- | --- |'
      '\n'
      r'| A\|B | Guarded |',
    );
    final sourceStart = controller.markdown.indexOf(r'A\|B');
    expect(controller.selection, FlarkSelection.collapsed(sourceStart + 3));
    expect(
      tester.widget<EditableText>(cellFinder).controller.selection,
      const TextSelection.collapsed(offset: 2),
    );
  });

  testWidgets('normalizes pasted table cell newlines and escaped pipes', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final cellFinder = _tableCellEditableFinder(1, 0);
    await tester.tap(cellFinder);
    await tester.showKeyboard(cellFinder);
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'A\nB|C',
        selection: TextSelection.collapsed(offset: 5),
      ),
    );
    await tester.pump();

    expect(
      controller.markdown,
      r'| Area | Status |'
      '\n'
      r'| --- | --- |'
      '\n'
      r'| A B\|C | Guarded |',
    );
    final sourceStart = controller.markdown.indexOf(r'A B\|C');
    expect(
      controller.selection,
      FlarkSelection.collapsed(sourceStart + r'A B\|C'.length),
    );
    expect(
      tester.widget<EditableText>(cellFinder).controller.selection,
      const TextSelection.collapsed(offset: 5),
    );
  });

  testWidgets('keeps table cell editing active after platform Enter', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final cellFinder = _tableCellEditableFinder(1, 1);
    await tester.tap(cellFinder);
    await tester.showKeyboard(cellFinder);

    final editable = tester.widget<EditableText>(cellFinder);
    expect(editable.keyboardType, TextInputType.multiline);
    expect(editable.textInputAction, TextInputAction.newline);

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'Ready\n',
        selection: TextSelection.collapsed(offset: 6),
      ),
    );
    await tester.pump();

    expect(tester.widget<EditableText>(cellFinder).focusNode.hasFocus, isTrue);
    expect(tester.widget<EditableText>(cellFinder).controller.text, 'Ready');

    tester.testTextInput.enterText('Ready line');
    await tester.pump();

    expect(
      tester.widget<EditableText>(cellFinder).controller.text,
      'Ready line',
    );
    expect(controller.markdown, contains('Ready line'));
  });

  testWidgets('undoes and redoes table cell edits through escaped source', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final cellFinder = _tableCellEditableFinder(1, 0);
    await tester.enterText(cellFinder, 'A|B');
    await tester.pump();
    expect(
      controller.markdown,
      r'| Area | Status |'
      '\n'
      r'| --- | --- |'
      '\n'
      r'| A\|B | Guarded |',
    );

    controller.undo();
    await tester.pump();
    expect(controller.markdown, _tableMarkdown);

    controller.redo();
    await tester.pump();
    expect(
      controller.markdown,
      r'| Area | Status |'
      '\n'
      r'| --- | --- |'
      '\n'
      r'| A\|B | Guarded |',
    );
  });

  testWidgets('keeps table selection stable across source and live widgets', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(_tableMarkdown);
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );

    final liveCellFinder = _tableCellEditableFinder(1, 0);
    await tester.tap(liveCellFinder);
    final liveCell = tester.widget<EditableText>(liveCellFinder);
    liveCell.controller.selection = const TextSelection.collapsed(offset: 3);
    await tester.pump();

    final sourceOffset = _tableMarkdown.indexOf('Preview') + 3;
    expect(controller.selection, FlarkSelection.collapsed(sourceOffset));

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(
      tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .selection,
      TextSelection.collapsed(offset: sourceOffset),
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 420,
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(
      tester
          .widget<EditableText>(_tableCellEditableFinder(1, 0))
          .controller
          .selection,
      const TextSelection.collapsed(offset: 3),
    );
  });

  testWidgets('high-level markdown editor exposes live rendered editing mode', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('hello');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          controller: controller,
          editingMode: FlarkMarkdownEditingMode.liveRendered,
        ),
      ),
    );

    expect(find.byType(FlarkLiveRenderedEditableText), findsOneWidget);
    expect(find.byType(EditableText), findsOneWidget);
  });

  group('live surface stability through predictive edits', () {
    for (final stabilityCase in _liveSurfaceStabilityCases) {
      testWidgets(
        '${stabilityCase.id} keeps the same rendered surface mounted',
        (tester) async {
          final controller = FlarkFlutterController.fromMarkdown(
            stabilityCase.markdown,
            extensions: FlarkMarkdownEditingExtensions.standard(),
          );
          addTearDown(controller.dispose);
          await _applyComrakParseResult(controller);

          await tester.pumpWidget(
            Directionality(
              textDirection: TextDirection.ltr,
              child: SizedBox(
                width: 520,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14),
                ),
              ),
            ),
          );
          await tester.pump();

          final surfaceFinder = find.byKey(stabilityCase.surfaceKey);
          expect(surfaceFinder, findsOneWidget, reason: stabilityCase.id);
          final surfaceElement = tester.element(surfaceFinder);

          final editableFinder = stabilityCase.editableFinder();
          expect(editableFinder, findsOneWidget, reason: stabilityCase.id);

          await tester.showKeyboard(editableFinder);
          await tester.enterText(editableFinder, stabilityCase.updatedText);
          await tester.pump();

          expect(
            controller.markdown,
            stabilityCase.expectedMarkdownAfterEdit,
            reason: stabilityCase.id,
          );
          expect(surfaceFinder, findsOneWidget, reason: stabilityCase.id);
          expect(
            identical(tester.element(surfaceFinder), surfaceElement),
            isTrue,
            reason: '${stabilityCase.id} predictive state remounted surface',
          );

          await _applyComrakParseResult(controller);
          await tester.pump();

          expect(surfaceFinder, findsOneWidget, reason: stabilityCase.id);
          expect(
            identical(tester.element(surfaceFinder), surfaceElement),
            isTrue,
            reason: '${stabilityCase.id} parse adoption remounted surface',
          );
        },
      );
    }
  });

  group('source-host structural transition matrix', () {
    for (final transitionCase in _sourceHostTransitionCases) {
      testWidgets(
        '${transitionCase.id} stays editable while pending and adopts parsed structure',
        (tester) async {
          final controller = FlarkFlutterController(
            runtime: FlarkEditorRuntime(
              state: FlarkEditorState.fromMarkdown(
                transitionCase.markdown,
                selection: FlarkSelection.collapsed(
                  transitionCase.selectionOffset,
                ),
              ),
              extensions: FlarkMarkdownEditingExtensions.standard(),
            ),
          );
          addTearDown(controller.dispose);
          await _applyComrakParseResult(controller);

          await tester.pumpWidget(
            Directionality(
              textDirection: TextDirection.ltr,
              child: SizedBox(
                width: 520,
                child: FlarkLiveRenderedEditableText(
                  controller: controller,
                  style: const TextStyle(fontSize: 14),
                  autofocus: true,
                ),
              ),
            ),
          );
          await tester.pump();

          final hostFinder = find
              .byType(EditableText)
              .at(transitionCase.hostEditorIndex);
          await tester.showKeyboard(hostFinder);
          tester.testTextInput.enterText(transitionCase.inputText);
          await tester.pump();

          expect(
            controller.markdown,
            transitionCase.expectedPendingMarkdown,
            reason: transitionCase.id,
          );
          final pendingFocused = _focusedEditableText(tester);
          expect(
            pendingFocused.controller.text,
            transitionCase.expectedPendingFocusedText ??
                transitionCase.inputText,
            reason: '${transitionCase.id} pending host text',
          );
          expect(
            pendingFocused.focusNode.hasFocus,
            isTrue,
            reason: '${transitionCase.id} pending focus',
          );

          await _applyComrakParseResult(controller);
          await tester.pump();

          expect(
            controller.markdown,
            transitionCase.expectedPendingMarkdown,
            reason: transitionCase.id,
          );
          expect(
            find.byKey(transitionCase.renderedKey),
            findsWidgets,
            reason: transitionCase.id,
          );
          for (final expectedText in transitionCase.expectedParsedTexts) {
            expect(
              _editableFinderWithText(expectedText),
              findsOneWidget,
              reason: '${transitionCase.id} parsed text $expectedText',
            );
          }
        },
      );
    }

    testWidgets(
      'batched code fence input before a following block auto-closes and preserves the block',
      (tester) async {
        const markdown = '* one\n\n\n* two';
        final controller = FlarkFlutterController(
          runtime: FlarkEditorRuntime(
            state: FlarkEditorState.fromMarkdown(
              markdown,
              selection: const FlarkSelection.collapsed(7),
            ),
            extensions: FlarkMarkdownEditingExtensions.standard(),
          ),
        );
        addTearDown(controller.dispose);
        await _applyComrakParseResult(controller);

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: SizedBox(
              width: 520,
              child: FlarkLiveRenderedEditableText(
                controller: controller,
                style: const TextStyle(fontSize: 14),
                autofocus: true,
              ),
            ),
          ),
        );
        await tester.pump();

        final hostFinder = find.byType(EditableText).at(2);
        await tester.showKeyboard(hostFinder);
        tester.testTextInput.enterText('```fffff');
        await tester.pump();

        expect(controller.markdown, '* one\n\n```\nfffff\n```\n* two');
        expect(
          find.byKey(const Key('FlarkLiveBlockCodeFence')),
          findsOneWidget,
        );
        expect(
          find.byKey(const Key('FlarkLiveBlockListMarker')),
          findsNWidgets(2),
        );
        expect(_codeEditableText(tester), 'fffff');
        expect(_editableFinderWithText('two'), findsOneWidget);
      },
    );
  });
  testWidgets('a caret at the trailing edge of an inline run re-enters it', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('a `test` b');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Place the caret in display space right after the last run character
    // ("a test| b") — the tap path. It must land inside the span, before
    // the hidden closing backtick.
    controller.applyProjectedSelection(const FlarkSelection.collapsed(6));
    expect(controller.selection, const FlarkSelection.collapsed(7));

    // Typing now continues the code span instead of escaping it.
    await tester.enterText(find.byType(EditableText), 'a tests b');
    await tester.pump();
    expect(controller.markdown, 'a `tests` b');
  });

  testWidgets('a trailing-edge caret re-enters strong runs too', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('**bold** x');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    controller.applyProjectedSelection(const FlarkSelection.collapsed(4));
    expect(controller.selection, const FlarkSelection.collapsed(6));

    await tester.enterText(find.byType(EditableText), 'bold! x');
    await tester.pump();
    expect(controller.markdown, '**bold!** x');
  });

  testWidgets('horizontal arrows step across a styled run trailing edge', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('a `test` b');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();
    await tester.tap(find.byType(EditableText));
    await tester.pump();

    // Caret at the trailing edge, inside the span.
    controller.applyProjectedSelection(const FlarkSelection.collapsed(6));
    expect(controller.selection, const FlarkSelection.collapsed(7));

    // Right arrow exits the run without moving the display caret.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(controller.selection, const FlarkSelection.collapsed(8));

    // Typing now lands after the closing marker (plain text).
    await tester.enterText(find.byType(EditableText), 'a test! b');
    await tester.pump();
    expect(controller.markdown, 'a `test`! b');

    // Undo the insertion to keep stepping on the same document.
    controller.undo();
    await tester.pump();
    expect(controller.markdown, 'a `test` b');

    // Left arrow from the outside position re-enters the run.
    controller.applySelection(const FlarkSelection.collapsed(8));
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pump();
    expect(controller.selection, const FlarkSelection.collapsed(7));
  });

  testWidgets('spaces typed inside an inline run stay inside it', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('a `test` b');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Caret inside at the run's end, then write another word. The space is
    // the regression: a space typed before the literal space that follows
    // the span used to diff-slide outside the hidden closing marker.
    controller.applyProjectedSelection(const FlarkSelection.collapsed(6));
    expect(controller.selection, const FlarkSelection.collapsed(7));

    await tester.enterText(find.byType(EditableText), 'a test  b');
    await tester.pump();
    expect(controller.markdown, 'a `test ` b');
    expect(controller.selection, const FlarkSelection.collapsed(8));

    await _applyComrakParseResult(controller);
    await tester.pump();

    await tester.enterText(find.byType(EditableText), 'a test m b');
    await tester.pump();
    expect(controller.markdown, 'a `test m` b');

    // Backspace over the word and the space also stays inside the run.
    await tester.enterText(find.byType(EditableText), 'a test  b');
    await tester.pump();
    expect(controller.markdown, 'a `test ` b');
    await tester.enterText(find.byType(EditableText), 'a test b');
    await tester.pump();
    expect(controller.markdown, 'a `test` b');
  });

  testWidgets('spaces typed inside an inline run stay inside in '
      'block-widget mode', (tester) async {
    // The list item forces per-block widgets, like the playground document.
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '- x\n\na `test` b',
          selection: const FlarkSelection.collapsed(12),
        ),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Caret at the run's trailing display edge ("a test|"), inside.
    // Display: "x\n\na test b" — offset 9 is right after the last run char.
    controller.applyProjectedSelection(const FlarkSelection.collapsed(9));
    expect(controller.selection, const FlarkSelection.collapsed(12));

    final paragraph = find.byWidgetPredicate(
      (widget) =>
          widget is EditableText && widget.controller.text == 'a test b',
    );
    expect(paragraph, findsOneWidget);

    await tester.enterText(paragraph, 'a test  b');
    await tester.pump();
    expect(controller.markdown, '- x\n\na `test ` b');
    expect(controller.selection, const FlarkSelection.collapsed(13));

    await _applyComrakParseResult(controller);
    await tester.pump();

    final grown = find.byWidgetPredicate(
      (widget) =>
          widget is EditableText && widget.controller.text == 'a test  b',
    );
    await tester.enterText(grown, 'a test m b');
    await tester.pump();
    expect(controller.markdown, '- x\n\na `test m` b');
  });

  testWidgets('select-all delete removes a run and its hidden markers', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('`test`');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Select all visible content ("test") and delete it: the orphaned
    // backticks must go too.
    controller.applyProjectedSelection(
      const FlarkSelection(baseOffset: 0, extentOffset: 4),
    );
    expect(
      controller.selection,
      const FlarkSelection(baseOffset: 1, extentOffset: 5),
    );

    await tester.enterText(find.byType(EditableText), '');
    await tester.pump();
    expect(controller.markdown, isEmpty);
  });

  testWidgets('document select-all + backspace clears everything in '
      'block-widget mode', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '- x\n\n`test `',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 320,
          height: 200,
          child: FlarkMarkdownEditor(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.liveRendered,
            style: const TextStyle(fontSize: 14, height: 1.4),
            autofocus: true,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final paragraph = find.byWidgetPredicate(
      (widget) =>
          widget is EditableText && widget.controller.text.contains('test'),
    );
    await tester.tap(paragraph);
    await tester.pump();

    // Document select-all (the intent the platform shortcut dispatches),
    // then a hardware backspace. The whole document goes, including all
    // hidden block and inline markers.
    Actions.invoke(
      FocusManager.instance.primaryFocus!.context!,
      const SelectAllTextIntent(SelectionChangedCause.keyboard),
    );
    await tester.pump();
    expect(
      controller.selection,
      FlarkSelection(baseOffset: 0, extentOffset: controller.markdown.length),
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(controller.markdown, isEmpty);
  });

  testWidgets('typing over a fully selected run keeps its style', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('`test`');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    controller.applyProjectedSelection(
      const FlarkSelection(baseOffset: 0, extentOffset: 4),
    );
    await tester.enterText(find.byType(EditableText), 'x');
    await tester.pump();
    // The replacement lands inside the markers, so the run survives and
    // typing continues styled — rich-text type-over behavior.
    expect(controller.markdown, '`x`');
  });

  testWidgets('an inline-code highlight covers a trailing space', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('`test `');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    final boundaryKey = GlobalKey();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: RepaintBoundary(
            key: boundaryKey,
            child: SizedBox(
              width: 200,
              child: FlarkLiveRenderedEditableText(
                controller: controller,
                style: const TextStyle(fontSize: 14),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    final highlight = FlarkMarkdownThemeData.light.inlineCodeBackgroundColor
        .toARGB32();
    await tester.runAsync(() async {
      final boundary =
          boundaryKey.currentContext!.findRenderObject()
              as RenderRepaintBoundary;
      final image = await boundary.toImage();
      final bytes = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
      int pixel(int x, int y) {
        final offset = (y * image.width + x) * 4;
        return (bytes!.getUint8(offset + 3) << 24) |
            (bytes.getUint8(offset) << 16) |
            (bytes.getUint8(offset + 1) << 8) |
            bytes.getUint8(offset + 2);
      }

      // The test font is Ahem: every glyph advances exactly fontSize px,
      // so 'test ' spans x 0..70 and the trailing space is x 56..70.
      final y = image.height ~/ 2;
      expect(pixel(60, y), highlight, reason: 'trailing space highlighted');
      expect(pixel(40, y), isNot(0), reason: 'run interior painted');
      expect(pixel(80, y), isNot(highlight), reason: 'past the run is plain');
    });
  });

  testWidgets('enter at the inside-end exits the run before splitting', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('`test `');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Caret inside at the run's end (before the closing backtick).
    controller.applySelection(const FlarkSelection.collapsed(6));

    // A platform line-break insertion (Enter) must not split the span
    // source — it steps past the closing marker first.
    await tester.enterText(find.byType(EditableText), 'test \n');
    await tester.pump();
    expect(controller.markdown, startsWith('`test `'));
    expect(controller.markdown, isNot(contains('` ')));
    await _applyComrakParseResult(controller);
    await tester.pump();

    // A second Enter still leaves the span intact.
    final display = tester
        .widget<EditableText>(find.byType(EditableText).first)
        .controller
        .text;
    await tester.enterText(find.byType(EditableText).first, '$display\n');
    await tester.pump();
    expect(controller.markdown, startsWith('`test `'));
    final reparsed = await FlarkNativeComrakParseBackend.withNativeBridge()
        .parse(
          FlarkMarkdownParseRequest(
            revision: controller.state.revision,
            markdown: controller.markdown,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );
    expect(
      reparsed.inlineTokens.any(
        (token) => token.kind == FlarkMarkdownInlineKind.inlineCode,
      ),
      isTrue,
      reason: 'the code span survives both line breaks',
    );
  });

  testWidgets('typing the closing marker closes the run and continues '
      'outside it', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown('`this');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // Unclosed: the backtick is literal in the display.
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      '`this',
    );
    controller.applySelection(const FlarkSelection.collapsed(5));

    // Type the closing backtick. The span forms on parse and the caret
    // stays after the marker, outside the run: the typed closer is the
    // user speaking markdown source, and the source stays what was typed.
    await tester.enterText(find.byType(EditableText), '`this`');
    await tester.pump();
    expect(controller.markdown, '`this`');
    await _applyComrakParseResult(controller);
    await tester.pump();
    expect(controller.selection, const FlarkSelection.collapsed(6));

    // Keep writing: text lands after the closing marker as plain text.
    await tester.enterText(find.byType(EditableText), 'this works');
    await tester.pump();
    expect(controller.markdown, '`this` works');
    await _applyComrakParseResult(controller);
    await tester.pump();
    expect(controller.selection, const FlarkSelection.collapsed(12));

    // Left arrow re-enters the run across its trailing edge; typing there
    // still extends the span.
    controller.applySelection(const FlarkSelection.collapsed(6));
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pump();
    expect(controller.selection, const FlarkSelection.collapsed(5));
    await tester.enterText(find.byType(EditableText), 'this! works');
    await tester.pump();
    expect(controller.markdown, '`this!` works');
  });

  testWidgets('per-character typing keeps the source byte-for-byte literal', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();
    await tester.showKeyboard(find.byType(EditableText));

    // Typing a sentence with an emphasis run, one character at a time with
    // a parse between keystrokes, must leave the source exactly what was
    // typed: the closing `*` must not drift toward the end of the line as
    // the sentence continues.
    const sentence = 'I *really* think so';
    for (var index = 0; index < sentence.length; index++) {
      final value = tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .value;
      final caret = value.selection.isValid
          ? value.selection.extentOffset
          : value.text.length;
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text:
              value.text.substring(0, caret) +
              sentence[index] +
              value.text.substring(caret),
          selection: TextSelection.collapsed(offset: caret + 1),
        ),
      );
      await tester.pump();
      await _applyComrakParseResult(controller);
      await tester.pump();
    }

    expect(controller.markdown, sentence);
  });

  testWidgets('typing the marker character at the inside-end exits the run', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('a `test` b');
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    controller.applyProjectedSelection(const FlarkSelection.collapsed(6));
    expect(controller.selection, const FlarkSelection.collapsed(7));

    // A literal backtick at the inside-end is the exit gesture: no text
    // is inserted and the caret steps past the closing marker.
    await tester.enterText(find.byType(EditableText), 'a test` b');
    await tester.pump();
    expect(controller.markdown, 'a `test` b');
    expect(controller.selection, const FlarkSelection.collapsed(8));

    // Typing after the exit is plain text.
    await tester.enterText(find.byType(EditableText), 'a test! b');
    await tester.pump();
    expect(controller.markdown, 'a `test`! b');
  });

  testWidgets('an empty heading styles immediately instead of showing raw '
      'source until text arrives', (tester) async {
    // The list item forces block-widget editing, matching the playground.
    final controller = FlarkFlutterController(
      runtime: FlarkEditorRuntime(
        state: FlarkEditorState.fromMarkdown(
          '- a\n\n### ',
          selection: const FlarkSelection.collapsed(9),
        ),
      ),
    );
    addTearDown(controller.dispose);
    await _applyComrakParseResult(controller);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    // No editable shows the raw marker.
    final editables = tester.widgetList<EditableText>(
      find.byType(EditableText),
    );
    expect(
      editables.where((editable) => editable.controller.text.contains('#')),
      isEmpty,
    );

    // The empty heading renders as a block editable with heading styling
    // (h3 from a 14px base = 14 + (7 - 3) * 2 = 22).
    final headingFinder = find.byWidgetPredicate(
      (widget) =>
          widget is EditableText &&
          widget.controller.text.isEmpty &&
          widget.style.fontSize == 22 &&
          widget.style.fontWeight == FontWeight.w700,
    );
    expect(headingFinder, findsOneWidget);

    // Typing into it extends the heading source; no mode flip, no jump.
    await tester.enterText(headingFinder, 'h');
    await tester.pump();
    expect(controller.markdown, '- a\n\n### h');
    final typedHeading = tester.widget<EditableText>(
      find.byWidgetPredicate(
        (widget) =>
            widget is EditableText &&
            widget.style.fontSize == 22 &&
            widget.controller.text == 'h',
      ),
    );
    expect(typedHeading.style.fontWeight, FontWeight.w700);
  });
}

Future<void> _applyComrakParseResult(FlarkFlutterController controller) async {
  final result = await FlarkNativeComrakParseBackend.withNativeBridge().parse(
    FlarkMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
  expect(controller.applyParseResult(result), isTrue);
}

Future<void> _typeBlankFenceOpener(
  WidgetTester tester,
  Finder editableFinder,
) async {
  await tester.showKeyboard(editableFinder);
  tester.testTextInput.updateEditingValue(
    const TextEditingValue(
      text: '`',
      selection: TextSelection.collapsed(offset: 1),
    ),
  );
  await tester.pump();
  tester.testTextInput.updateEditingValue(
    const TextEditingValue(
      text: '``',
      selection: TextSelection.collapsed(offset: 2),
    ),
  );
  await tester.pump();
  tester.testTextInput.updateEditingValue(
    const TextEditingValue(
      text: '```',
      selection: TextSelection.collapsed(offset: 3),
    ),
  );
  await tester.pump();
}

Future<void> _typeFocusedTextIncrementally(
  WidgetTester tester,
  String text,
) async {
  for (final codeUnit in text.codeUnits) {
    if (codeUnit == 0x0A) {
      final focused = _focusedEditableText(tester);
      tester.testTextInput.enterText('${focused.controller.text}\n');
      await tester.pump();
      continue;
    }
    final focused = _focusedEditableText(tester);
    tester.testTextInput.enterText(
      focused.controller.text + String.fromCharCode(codeUnit),
    );
    await tester.pump();
  }
}

final class _BlockingParseBackend implements FlarkMarkdownParseBackend {
  final requests = <FlarkMarkdownParseRequest>[];
  final _pending = <Completer<FlarkMarkdownParseResult>>[];

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'blocking_test_backend',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(FlarkMarkdownParseRequest request) {
    requests.add(request);
    final completer = Completer<FlarkMarkdownParseResult>();
    _pending.add(completer);
    return completer.future;
  }

  void completeAll() {
    for (var index = 0; index < _pending.length; index += 1) {
      final completer = _pending[index];
      if (completer.isCompleted) continue;
      final request = requests[index];
      completer.complete(
        FlarkMarkdownParseResult(
          schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
          revision: request.revision,
          sourceTextLength: request.markdown.length,
          blocks: const [],
          inlineTokens: const [],
        ),
      );
    }
  }
}

FlarkMarkdownParseResult _bareQuoteParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: const FlarkSourceRange(0, 1),
      ),
    ],
    inlineTokens: const [],
  );
}

FlarkMarkdownParseResult _emptyQuoteParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.blockquote,
        type: 'blockquote',
        sourceRange: const FlarkSourceRange(0, 2),
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(0, 2),
      ),
    ],
  );
}

FlarkMarkdownParseResult _quoteParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.blockquote,
        type: 'blockquote',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(0, 2),
      ),
    ],
  );
}

FlarkMarkdownParseResult _multiLineQuoteParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.blockquote,
        type: 'blockquote',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(0, 2),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(8, 10),
      ),
    ],
  );
}

FlarkMarkdownParseResult _paragraphParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: const FlarkSourceRange(0, 1),
      ),
    ],
    inlineTokens: const [],
  );
}

FlarkMarkdownParseResult _entityParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: const [],
    replacementRanges: [
      FlarkMarkdownReplacementRange(
        kind: FlarkMarkdownReplacementRangeKind.htmlEntity,
        type: 'htmlEntity',
        sourceRange: const FlarkSourceRange(2, 7),
        replacementText: '&',
      ),
    ],
  );
}

FlarkMarkdownParseResult _inlineParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: [
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.strong,
        type: 'strong',
        sourceRange: FlarkSourceRange(0, 8),
      ),
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.emphasis,
        type: 'emphasis',
        sourceRange: FlarkSourceRange(13, 17),
      ),
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.inlineCode,
        type: 'inlineCode',
        sourceRange: FlarkSourceRange(22, 28),
      ),
    ],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(0, 2),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(6, 8),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(13, 14),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(16, 17),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(22, 23),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(27, 28),
      ),
    ],
  );
}

FlarkMarkdownParseResult _blockParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: FlarkSourceRange(0, 21),
        attributes: {'language': 'dart'},
      ),
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.blockquote,
        type: 'blockquote',
        sourceRange: FlarkSourceRange(23, 30),
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(0, 8),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(17, 21),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: FlarkSourceRange(23, 25),
      ),
    ],
  );
}

FlarkMarkdownParseResult _codeOnlyParseResult(
  FlarkFlutterController controller,
) {
  final openerEnd = controller.markdown.indexOf('\n');
  final openerLine = openerEnd < 0
      ? controller.markdown
      : controller.markdown.substring(0, openerEnd);
  final language = openerLine.startsWith('```') && openerLine.length > 3
      ? openerLine.substring(3).trim()
      : '';
  final bodyStart = openerEnd < 0 ? controller.markdown.length : openerEnd + 1;
  final closerStart = controller.markdown.lastIndexOf('```');
  final rawBody = closerStart >= bodyStart
      ? controller.markdown.substring(bodyStart, closerStart)
      : '';
  final closingHiddenStart =
      _containsNonLineBreak(rawBody) && closerStart > bodyStart
      ? closerStart - 1
      : closerStart;
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: language.isEmpty
            ? const <String, Object?>{}
            : <String, Object?>{'language': language},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(0, bodyStart),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(
          closingHiddenStart,
          controller.markdown.length,
        ),
      ),
    ],
  );
}

FlarkMarkdownParseResult _unclosedCodeFenceOpenerParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: {'language': controller.markdown.substring(3)},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
  );
}

FlarkMarkdownParseResult _unclosedCodeFenceBodyParseResult(
  FlarkFlutterController controller,
) {
  final openerEnd = controller.markdown.indexOf('\n');
  final openerHiddenEnd = openerEnd < 0
      ? controller.markdown.length
      : openerEnd + 1;
  final opener = controller.markdown.substring(0, openerHiddenEnd).trimRight();
  final language = opener.length > 3 ? opener.substring(3) : '';
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: {'language': language},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(0, openerHiddenEnd),
      ),
    ],
  );
}

bool _containsNonLineBreak(String text) {
  for (var index = 0; index < text.length; index++) {
    final codeUnit = text.codeUnitAt(index);
    if (codeUnit != 0x0A && codeUnit != 0x0D) return true;
  }
  return false;
}

FlarkMarkdownParseResult _taskParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: const {'checked': false},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(0, 6),
      ),
    ],
  );
}

bool _containsFontWeight(InlineSpan span, FontWeight fontWeight) {
  return _anyTextSpan(span, (span) => span.style?.fontWeight == fontWeight);
}

bool _containsFontStyle(InlineSpan span, FontStyle fontStyle) {
  return _anyTextSpan(span, (span) => span.style?.fontStyle == fontStyle);
}

bool _containsFontFamily(InlineSpan span, String fontFamily) {
  return _anyTextSpan(span, (span) => span.style?.fontFamily == fontFamily);
}

bool _textSpanHasColor(InlineSpan span, String text, Color color) {
  return _anyTextSpan(
    span,
    (span) =>
        span.style?.color == color && (span.text?.contains(text) ?? false),
  );
}

bool _textSpanHasSyntaxColor(InlineSpan span) {
  return _anyTextSpan(
    span,
    (span) =>
        span.style?.color == const Color(0xFF64748B) ||
        span.style?.color == const Color(0xFF0F766E) ||
        span.style?.color == const Color(0xFFB45309) ||
        span.style?.color == const Color(0xFF7C3AED) ||
        span.style?.color == const Color(0xFF0369A1) ||
        span.style?.color == const Color(0xFF047857) ||
        span.style?.color == const Color(0xFF1D4ED8) ||
        span.style?.color == const Color(0xFFC2410C) ||
        span.style?.color == const Color(0xFF475569) ||
        span.style?.color == const Color(0xFFB91C1C),
  );
}

bool _anyTextSpan(InlineSpan span, bool Function(TextSpan span) test) {
  if (span is TextSpan) {
    if (test(span)) return true;
    final children = span.children;
    if (children != null &&
        children.any((child) => _anyTextSpan(child, test))) {
      return true;
    }
  }
  return false;
}

Finder _codeEditableFinder() {
  return find.descendant(
    of: find.byKey(const Key('FlarkLiveBlockCodeEditable')),
    matching: find.byType(EditableText),
  );
}

String _codeEditableText(WidgetTester tester) {
  return tester.widget<EditableText>(_codeEditableFinder()).controller.text;
}

EditableText _focusedEditableText(WidgetTester tester) {
  final focused = tester
      .widgetList<EditableText>(find.byType(EditableText))
      .where((editable) => editable.focusNode.hasFocus)
      .toList(growable: false);
  expect(focused, hasLength(1));
  return focused.single;
}

Finder _tableCellEditableFinder(int rowIndex, int columnIndex) {
  return find.descendant(
    of: find.byKey(Key('FlarkLiveBlockTableCell-$rowIndex-$columnIndex')),
    matching: find.byType(EditableText),
  );
}

EditableText _editableTextWithText(WidgetTester tester, String text) {
  return tester
      .widgetList<EditableText>(find.byType(EditableText))
      .singleWhere((editable) => editable.controller.text == text);
}

Finder _editableFinderWithText(String text) {
  return find.byWidgetPredicate(
    (widget) => widget is EditableText && widget.controller.text == text,
  );
}

final class _InlineEdgeCase {
  const _InlineEdgeCase({
    required this.id,
    required this.markdown,
    required this.displayText,
    this.expectMonospace = false,
  });

  final String id;
  final String markdown;
  final String displayText;
  final bool expectMonospace;
}

final _sourceHostTransitionCases = [
  _SourceHostTransitionCase(
    id: 'blockquote before following list',
    markdown: '* one\n\n\n* two',
    selectionOffset: 7,
    hostEditorIndex: 2,
    inputText: '> quote',
    expectedPendingMarkdown: '* one\n\n> quote\n* two',
    renderedKey: const Key('FlarkLiveBlockBlockquote'),
    expectedParsedTexts: const ['quote\n', 'two'],
  ),
  _SourceHostTransitionCase(
    id: 'unordered list before following list',
    markdown: '* one\n\n\n* two',
    selectionOffset: 7,
    hostEditorIndex: 2,
    inputText: '* inserted',
    expectedPendingMarkdown: '* one\n\n* inserted\n* two',
    renderedKey: const Key('FlarkLiveBlockListMarker'),
    expectedParsedTexts: const ['inserted', 'two'],
  ),
  _SourceHostTransitionCase(
    id: 'task list before following list',
    markdown: '* one\n\n\n* two',
    selectionOffset: 7,
    hostEditorIndex: 2,
    inputText: '- [ ] todo',
    expectedPendingMarkdown: '* one\n\n- [ ] todo\n* two',
    renderedKey: const Key('FlarkLiveBlockTaskCheckbox'),
    expectedParsedTexts: const ['todo', 'two'],
  ),
  _SourceHostTransitionCase(
    id: 'table before following list',
    markdown: '* one\n\n\n* two',
    selectionOffset: 7,
    hostEditorIndex: 2,
    inputText: '| A | B |\n| --- | --- |\n| x | y |',
    expectedPendingFocusedText: '| x | y |',
    expectedPendingMarkdown:
        '* one\n\n| A | B |\n| --- | --- |\n| x | y |\n* two',
    renderedKey: const Key('FlarkLiveBlockTable'),
    expectedParsedTexts: const ['x', 'y', 'two'],
  ),
];

final class _SourceHostTransitionCase {
  const _SourceHostTransitionCase({
    required this.id,
    required this.markdown,
    required this.selectionOffset,
    required this.hostEditorIndex,
    required this.inputText,
    this.expectedPendingFocusedText,
    required this.expectedPendingMarkdown,
    required this.renderedKey,
    required this.expectedParsedTexts,
  });

  final String id;
  final String markdown;
  final int selectionOffset;
  final int hostEditorIndex;
  final String inputText;
  final String? expectedPendingFocusedText;
  final String expectedPendingMarkdown;
  final Key renderedKey;
  final List<String> expectedParsedTexts;
}

final _liveSurfaceStabilityCases = [
  _LiveSurfaceStabilityCase(
    id: 'blockquote',
    markdown: '> quote',
    surfaceKey: const Key('FlarkLiveBlockBlockquote'),
    updatedText: 'quote!',
    expectedMarkdownAfterEdit: '> quote!',
    editableFinder: () => find.byType(EditableText),
  ),
  _LiveSurfaceStabilityCase(
    id: 'unordered list',
    markdown: '* item',
    surfaceKey: const Key('FlarkLiveBlockListMarker'),
    updatedText: 'items',
    expectedMarkdownAfterEdit: '* items',
    editableFinder: () => find.byType(EditableText),
  ),
  _LiveSurfaceStabilityCase(
    id: 'ordered list',
    markdown: '1. item',
    surfaceKey: const Key('FlarkLiveBlockListMarker'),
    updatedText: 'items',
    expectedMarkdownAfterEdit: '1. items',
    editableFinder: () => find.byType(EditableText),
  ),
  _LiveSurfaceStabilityCase(
    id: 'task list',
    markdown: '- [ ] done',
    surfaceKey: const Key('FlarkLiveBlockTaskCheckbox'),
    updatedText: 'done!',
    expectedMarkdownAfterEdit: '- [ ] done!',
    editableFinder: () => find.byType(EditableText),
  ),
  _LiveSurfaceStabilityCase(
    id: 'code block',
    markdown: _blockMarkdown,
    surfaceKey: const Key('FlarkLiveBlockCodeFence'),
    updatedText: 'print(2);',
    expectedMarkdownAfterEdit: '```dart\nprint(2);\n```\n\n> quote',
    editableFinder: _codeEditableFinder,
  ),
  _LiveSurfaceStabilityCase(
    id: 'table',
    markdown: _tableMarkdown,
    surfaceKey: const Key('FlarkLiveBlockTable'),
    updatedText: 'Done',
    expectedMarkdownAfterEdit:
        '| Area | Status |\n| --- | --- |\n| Preview | Done |',
    editableFinder: () => find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
  ),
];

final class _LiveSurfaceStabilityCase {
  const _LiveSurfaceStabilityCase({
    required this.id,
    required this.markdown,
    required this.surfaceKey,
    required this.updatedText,
    required this.expectedMarkdownAfterEdit,
    required this.editableFinder,
  });

  final String id;
  final String markdown;
  final Key surfaceKey;
  final String updatedText;
  final String expectedMarkdownAfterEdit;
  final Finder Function() editableFinder;
}

const _blockMarkdown = '```dart\nprint(1);\n```\n\n> quote';
const _tableMarkdown =
    '| Area | Status |\n| --- | --- |\n| Preview | Guarded |';
const _irregularTableMarkdown = '| Area | Status |\n| --- | --- |\n| Preview |';
