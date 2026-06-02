import 'dart:ui' show PointerDeviceKind;

import 'package:example/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  setUp(() {
    TestWidgetsFlutterBinding.ensureInitialized();
  });

  testWidgets('example exposes v2 editor and preview surfaces', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    expect(find.byType(MarkdownEditor), findsOneWidget);
    expect(find.byType(Markdown), findsOneWidget);
    expect(find.widgetWithText(Flexible, 'Sovereign Markdown'), findsOneWidget);
    expect(find.text('Comrak'), findsOneWidget);
    expect(find.text('Editor'), findsOneWidget);
    expect(find.text('Preview'), findsOneWidget);
  });

  testWidgets('playground controls remain usable on narrow web viewports', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(720, 820));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    expect(find.byType(MarkdownEditor), findsOneWidget);
    expect(find.byType(Markdown), findsOneWidget);
    expect(
      find.byKey(const ValueKey('sovereign-example-command-quote')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('sovereign-example-command-code-fence')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('sovereign-example-command-table')),
      findsOneWidget,
    );
  });

  testWidgets('toolbar can load common markdown scenarios', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(find.text('Tables'));
    await tester.pump();

    expect(find.textContaining('Feature'), findsWidgets);
    expect(
      find.byKey(const ValueKey('sovereign-example-command-table')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('sovereign-example-command-quote')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('sovereign-example-command-code-fence')),
      findsOneWidget,
    );
  });

  testWidgets('toolbar commands edit the scratch document', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), 'quote me');
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-command-quote')),
    );
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '> quote me');

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-command-code-fence')),
    );
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```dart\n\n```');

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-command-table')),
    );
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), contains('| Header 1 | Header 2 | Header 3 |'));
  });

  testWidgets('mode switch can show source editing', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(find.text('Source'));
    await tester.pump();

    final editor = tester.widget<MarkdownEditor>(find.byType(MarkdownEditor));
    expect(editor.editingMode, SovereignMarkdownEditingMode.source);
  });

  testWidgets('live edit mode keeps rendered editing beside preview', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(find.text('Live Edit'));
    await tester.pump();

    final editor = tester.widget<MarkdownEditor>(find.byType(MarkdownEditor));
    expect(editor.editingMode, SovereignMarkdownEditingMode.liveRendered);
    expect(find.byType(Markdown), findsOneWidget);
    expect(find.text('Preview'), findsOneWidget);
  });

  testWidgets('rendered mode shows a live read-only render surface', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(find.text('Rendered'));
    await tester.pump();

    expect(find.byType(MarkdownEditor), findsNothing);
    expect(find.byType(Markdown), findsOneWidget);
    expect(find.text('Rendered'), findsWidgets);
  });

  testWidgets('scratch document is a blank full-pane live editor', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await tester.pump();

    final editor = tester.widget<MarkdownEditor>(find.byType(MarkdownEditor));
    expect(editor.editingMode, SovereignMarkdownEditingMode.liveRendered);

    final editableFinder = find.byType(EditableText);
    final editable = tester.widget<EditableText>(editableFinder);
    expect(editable.controller.text, isEmpty);
    expect(editable.expands, isTrue);
    expect(editable.textInputAction, TextInputAction.newline);

    final editableRect = tester.getRect(editableFinder);
    expect(editableRect.height, greaterThan(400));

    await tester.tapAt(editableRect.center);
    await tester.pump();
    expect(
      tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
      isTrue,
      reason: 'blank scratch editor should focus from a pane tap',
    );

    await tester.enterText(editableFinder, 'A\n\nB\nC');
    await tester.pump(const Duration(milliseconds: 220));

    var editorText = find
        .byType(EditableText)
        .evaluate()
        .map((element) => (element.widget as EditableText).controller.text)
        .join('\n');
    expect(editorText, 'A\n\nB\nC');

    await tester.enterText(editableFinder, '# Scratch\n\nA **bold** phrase.');
    await tester.pump(const Duration(milliseconds: 220));

    editorText = find
        .byType(EditableText)
        .evaluate()
        .map((element) => (element.widget as EditableText).controller.text)
        .join('\n');
    expect(editorText, contains('Scratch'));
    expect(editorText, contains('bold'));
    expect(find.textContaining('bold'), findsWidgets);
  });

  testWidgets('scratch keeps awkward partial markdown states editable', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), _awkwardScratchMarkdown);
    await _settleParsing(tester);

    final liveText = _editorText(tester);
    expect(liveText, contains('**wow*'));
    expect(liveText, contains('*wow**'));
    expect(liveText, contains('__wow_'));
    expect(liveText, contains('_wow__'));
    expect(liveText, contains('[label](url'));
    expect(liveText, contains('![alt](url'));
    expect(liveText, contains('final value = 1;'));
    expect(liveText, contains('after fence'));
    expect(liveText, isNot(contains('```dart')));

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), _awkwardScratchMarkdown);
  });

  testWidgets('scratch keeps marker-only quote input source-visible', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await tester.pump();

    final editableFinder = find.byType(EditableText);
    await tester.enterText(editableFinder, '>');
    await _settleParsing(tester);

    final markerEditable = tester.widget<EditableText>(editableFinder);
    expect(markerEditable.controller.text, '>');
    expect(find.byKey(const Key('SovereignLiveBlockBlockquote')), findsNothing);
    expect(
      find.byKey(const Key('SovereignReadOnlyPreviewBlockquote')),
      findsNothing,
    );

    await tester.enterText(editableFinder, '> ');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockBlockquote')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignReadOnlyPreviewBlockquote')),
      findsOneWidget,
    );

    await tester.enterText(find.byType(EditableText), 'quote');
    await _settleParsing(tester);

    final quoteEditable = tester.widget<EditableText>(
      find.byType(EditableText),
    );
    expect(quoteEditable.controller.text, 'quote');
    expect(find.textContaining('quote'), findsWidgets);
  });

  testWidgets('scratch renders multiline quotes with one continuous rail', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await tester.pump();

    await tester.enterText(find.byType(EditableText), '> first\n> second');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockBlockquote')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignReadOnlyPreviewBlockquote')),
      findsOneWidget,
    );
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'first\nsecond',
    );
  });

  testWidgets('scratch keeps GitHub alert syntax as editable quote text', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '> [!NOTE]\n> useful');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockBlockquote')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignLiveBlockTaskCheckbox')),
      findsNothing,
    );
    expect(find.byKey(const Key('SovereignLiveBlockCodeFence')), findsNothing);
    expect(_editorText(tester), '[!NOTE]\nuseful');

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '> [!NOTE]\n> useful');
  });

  testWidgets('scratch keeps unsupported footnotes source-visible', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(
      find.byType(EditableText),
      'Text[^1]\n\n[^1]: Footnote',
    );
    await _settleParsing(tester);

    expect(_editorText(tester), 'Text[^1]\n\n[^1]: Footnote');
    expect(
      find.byKey(const Key('SovereignInlineLinkMenuButton')),
      findsNothing,
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), 'Text[^1]\n\n[^1]: Footnote');
  });

  testWidgets('scratch renders unordered list marker immediately', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '- ');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockListMarker')),
      findsOneWidget,
    );
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, isEmpty);
  });

  testWidgets('scratch keeps extra blank lines visible in live list editing', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '- one\n\n\n- two');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockListMarker')),
      findsNWidgets(2),
    );
    expect(_editorText(tester), 'one\n\n\ntwo');

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '- one\n\n\n- two');
  });

  testWidgets(
    'scratch keeps bare star source-visible until list marker space',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(const SovereignExampleApp());
      await tester.pump();

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);

      var editableFinder = find.byType(EditableText);
      await tester.tap(editableFinder);
      await tester.pump();
      expect(
        tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
        isTrue,
      );

      await tester.enterText(editableFinder, '*');
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsNothing,
      );
      var editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, '*');
      expect(editable.focusNode.hasFocus, isTrue);

      await tester.enterText(find.byType(EditableText), '* ');
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsOneWidget,
      );
      editableFinder = find.byType(EditableText);
      editable = tester.widget<EditableText>(editableFinder);
      expect(editable.controller.text, isEmpty);
      expect(editable.focusNode.hasFocus, isTrue);
      final markerElement = tester.element(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
      );

      tester.testTextInput.enterText('item');
      await tester.pump();

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsOneWidget,
      );
      expect(
        identical(
          tester.element(find.byKey(const Key('SovereignLiveBlockListMarker'))),
          markerElement,
        ),
        isTrue,
      );

      await _settleParsing(tester);

      editable = tester.widget<EditableText>(editableFinder);
      expect(editable.controller.text, 'item');

      await tester.showKeyboard(editableFinder);
      await tester.enterText(editableFinder, 'item\n');
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsNWidgets(2),
      );
      expect(find.byType(EditableText), findsNWidgets(2));
      await tester.showKeyboard(find.byType(EditableText).at(1));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsOneWidget,
      );
      final editors = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .toList(growable: false);
      expect(editors.first.controller.text, 'item');
      expect(editors.first.focusNode.hasFocus, isFalse);
      expect(editors.last.controller.text, isEmpty);
      expect(editors.last.focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'scratch exits ordered and task lists into blank live paragraphs',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(const SovereignExampleApp());
      await tester.pump();

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);

      await tester.enterText(find.byType(EditableText), '1. ');
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsOneWidget,
      );
      expect(
        tester.widget<EditableText>(find.byType(EditableText)).controller.text,
        isEmpty,
      );

      await tester.showKeyboard(find.byType(EditableText));
      await tester.enterText(find.byType(EditableText), 'ordered');
      await _settleParsing(tester);
      await tester.showKeyboard(find.byType(EditableText));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsNWidgets(2),
      );
      await tester.showKeyboard(find.byType(EditableText).at(1));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockListMarker')),
        findsOneWidget,
      );
      var editors = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .toList(growable: false);
      expect(editors.first.controller.text, 'ordered');
      expect(editors.first.focusNode.hasFocus, isFalse);
      expect(editors.last.controller.text, isEmpty);
      expect(editors.last.focusNode.hasFocus, isTrue);

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-mode-source')),
      );
      await _settleParsing(tester);
      expect(_editorText(tester), '1. ordered\n\n');

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);

      await tester.enterText(find.byType(EditableText), '- [ ] ');
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockTaskCheckbox')),
        findsOneWidget,
      );
      expect(
        tester.widget<EditableText>(find.byType(EditableText)).controller.text,
        isEmpty,
      );

      await tester.showKeyboard(find.byType(EditableText));
      await tester.enterText(find.byType(EditableText), 'todo');
      await _settleParsing(tester);
      await tester.showKeyboard(find.byType(EditableText));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockTaskCheckbox')),
        findsNWidgets(2),
      );
      await tester.showKeyboard(find.byType(EditableText).at(1));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _settleParsing(tester);

      expect(
        find.byKey(const Key('SovereignLiveBlockTaskCheckbox')),
        findsOneWidget,
      );
      editors = tester
          .widgetList<EditableText>(find.byType(EditableText))
          .toList(growable: false);
      expect(editors.first.controller.text, 'todo');
      expect(editors.first.focusNode.hasFocus, isFalse);
      expect(editors.last.controller.text, isEmpty);
      expect(editors.last.focusNode.hasFocus, isTrue);

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-mode-source')),
      );
      await _settleParsing(tester);
      expect(_editorText(tester), '- [ ] todo\n\n');
    },
  );

  testWidgets('scratch toggles task checkboxes without losing text editing', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '- [ ] todo');
    await _settleParsing(tester);

    final editableFinder = find.byType(EditableText);
    await tester.tap(editableFinder);
    await tester.showKeyboard(editableFinder);
    expect(
      tester.widget<EditableText>(editableFinder).focusNode.hasFocus,
      isTrue,
    );

    await tester.tap(find.byKey(const Key('SovereignLiveBlockTaskCheckbox')));
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'todo',
    );
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).focusNode.hasFocus,
      isTrue,
      reason: 'checkbox toggle should keep the live task row focused',
    );

    tester.testTextInput.enterText('todo!');
    await _settleParsing(tester);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '- [x] todo!');
  });

  testWidgets('scratch keeps Shift+Enter as a soft line break in lists', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '- ');
    await _settleParsing(tester);
    await tester.enterText(find.byType(EditableText), 'item');
    await _settleParsing(tester);

    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockListMarker')),
      findsOneWidget,
    );
    expect(find.byType(EditableText), findsNWidgets(2));

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '- item\n');
  });

  testWidgets('scratch keeps blank code lines visible after Enter', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```dart\nfoo\n```');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockCodeFence')),
      findsOneWidget,
    );

    final codeEditableFinder = _codeEditableFinder();
    final codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.text, 'foo');
    final codeHeightBeforeEnter = tester.getRect(codeEditableFinder).height;

    codeEditable.controller.selection = const TextSelection.collapsed(
      offset: 3,
    );
    await tester.pump();
    await tester.showKeyboard(codeEditableFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);

    final expandedCodeEditable = tester.widget<EditableText>(
      _codeEditableFinder(),
    );
    expect(expandedCodeEditable.controller.text, 'foo\n');
    expect(
      tester.getRect(_codeEditableFinder()).height,
      greaterThan(codeHeightBeforeEnter),
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```dart\nfoo\n\n```');
  });

  testWidgets('scratch expands an empty code fence after Enter', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```dart\n```');
    await _settleParsing(tester);

    final codeEditableFinder = _codeEditableFinder();
    final codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.text, isEmpty);
    final codeHeightBeforeEnter = tester.getRect(codeEditableFinder).height;

    await tester.showKeyboard(codeEditableFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);

    final expandedCodeEditable = tester.widget<EditableText>(
      _codeEditableFinder(),
    );
    expect(expandedCodeEditable.controller.text, '\n');
    expect(
      tester.getRect(_codeEditableFinder()).height,
      greaterThan(codeHeightBeforeEnter),
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```dart\n\n```');
  });

  testWidgets('scratch keeps a typed fence opener visible until Enter', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```fffffff');
    await _settleParsing(tester);

    expect(find.byKey(const Key('SovereignLiveBlockCodeFence')), findsNothing);
    final editable = tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const Key('SovereignLiveBlockCodeOpeningEditable')),
        matching: find.byType(EditableText),
      ),
    );
    expect(editable.controller.text, '```fffffff');
    expect(
      find.byKey(const Key('SovereignLiveBlockCodeLanguageButton')),
      findsNothing,
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```fffffff');
  });

  testWidgets('scratch keeps fast typed fence language on the opening line', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.tap(find.byType(EditableText));
    await tester.showKeyboard(find.byType(EditableText));
    await tester.pump();

    const markdown = '```dart\nfoo';
    await _typeFocusedTextIncrementally(tester, markdown);
    await _settleParsing(tester);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);

    expect(_editorText(tester), markdown);
  });

  testWidgets('scratch keeps fast typed fence closing outside the code body', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.tap(find.byType(EditableText));
    await tester.showKeyboard(find.byType(EditableText));
    await tester.pump();

    const markdown = '```dart\nfoo\n```\n\n\nabcdef';
    await _typeFocusedTextIncrementally(tester, markdown);
    await _settleParsing(tester);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);

    expect(_editorText(tester), markdown);
  });

  testWidgets('scratch renders a fence region after triple backticks', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    final editableFinder = find.byType(EditableText);
    await tester.enterText(editableFinder, '```');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockCodeFence')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignLiveBlockCodeOpeningEditable')),
      findsNothing,
    );
    expect(
      tester.widget<EditableText>(_codeEditableFinder()).controller.text,
      isEmpty,
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```\n');
  });

  testWidgets('scratch backspace removes a newly opened empty code fence', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```');
    await _settleParsing(tester);

    expect(
      find.byKey(const Key('SovereignLiveBlockCodeFence')),
      findsOneWidget,
    );
    await tester.showKeyboard(_codeEditableFinder());
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await _settleParsing(tester);

    expect(find.byKey(const Key('SovereignLiveBlockCodeFence')), findsNothing);
    expect(find.text('```'), findsNothing);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), isEmpty);
  });

  testWidgets('scratch backspace enters code fences without exposing markers', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(
      find.byType(EditableText),
      '```dart\nfoo\n```\nafter',
    );
    await _settleParsing(tester);

    final afterFinder = _editableFinderWithText('after');
    await tester.tap(afterFinder);
    final afterEditable = tester.widget<EditableText>(afterFinder);
    afterEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    await tester.showKeyboard(afterFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
      isTrue,
    );
    expect(
      tester.widget<EditableText>(_codeEditableFinder()).controller.text,
      'foo',
    );
    expect(find.text('```'), findsNothing);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```dart\nfoo\n```\nafter');
  });

  testWidgets(
    'scratch hides fence markers and keeps fence mounted while typing',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(const SovereignExampleApp());
      await tester.pump();

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);

      await tester.enterText(find.byType(EditableText), '```dart\nfoo\n```');
      await _settleParsing(tester);

      final fenceFinder = find.byKey(const Key('SovereignLiveBlockCodeFence'));
      expect(fenceFinder, findsOneWidget);
      expect(
        find.byKey(const Key('SovereignLiveBlockCodeOpeningEditable')),
        findsNothing,
      );
      final fenceElement = tester.element(fenceFinder);
      final codeEditableFinder = _codeEditableFinder();
      expect(
        tester.widget<EditableText>(codeEditableFinder).controller.text,
        'foo',
      );

      await tester.showKeyboard(codeEditableFinder);
      tester.testTextInput.enterText('foobar');
      await tester.pump();

      expect(tester.element(fenceFinder), same(fenceElement));
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        'foobar',
      );
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        isNot(contains('```')),
      );

      await _settleParsing(tester);

      expect(tester.element(fenceFinder), same(fenceElement));
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).controller.text,
        'foobar',
      );
    },
  );

  testWidgets(
    'scratch routes typing into a fence after opening language menu',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(const SovereignExampleApp());
      await tester.pump();

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);
      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-command-code-fence')),
      );
      await _settleParsing(tester);

      await tester.tap(
        find.byKey(const Key('SovereignLiveBlockCodeLanguageButton')),
      );
      await tester.pump();
      expect(
        find.byKey(const Key('SovereignLiveBlockCodeLanguageMenu')),
        findsOneWidget,
      );

      await tester.tap(_codeEditableFinder());
      await tester.pump();
      expect(
        find.byKey(const Key('SovereignLiveBlockCodeLanguageMenu')),
        findsNothing,
      );

      final codeEditable = tester.widget<EditableText>(_codeEditableFinder());
      expect(codeEditable.focusNode.hasFocus, isTrue);

      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'abc\n',
          selection: TextSelection.collapsed(offset: 3),
        ),
      );
      await _settleParsing(tester);

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-mode-source')),
      );
      await _settleParsing(tester);
      expect(_editorText(tester), '```dart\nabc\n```');
    },
  );

  testWidgets(
    'scratch language selection remains compact and keeps code focus',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(const SovereignExampleApp());
      await tester.pump();

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
      );
      await _settleParsing(tester);
      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-command-code-fence')),
      );
      await _settleParsing(tester);
      await tester.enterText(_codeEditableFinder(), 'foo');
      await _settleParsing(tester);

      final buttonFinder = find.byKey(
        const Key('SovereignLiveBlockCodeLanguageButton'),
      );
      final buttonRect = tester.getRect(buttonFinder);
      await tester.tap(buttonFinder);
      await tester.pump();

      final menuFinder = find.byKey(
        const Key('SovereignLiveBlockCodeLanguageMenu'),
      );
      expect(menuFinder, findsOneWidget);
      final menuRect = tester.getRect(menuFinder);
      expect(menuRect.width, lessThanOrEqualTo(160));
      expect(menuRect.right, moreOrLessEquals(buttonRect.right, epsilon: 1));

      await tester.tap(
        find.byKey(const ValueKey('SovereignLiveBlockCodeLanguageOption:rust')),
      );
      await _settleParsing(tester);

      expect(menuFinder, findsNothing);
      expect(find.text('Rust'), findsOneWidget);
      expect(_previewCodeText(tester), 'foo');
      expect(
        tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
        isTrue,
      );

      tester.testTextInput.enterText('foobar');
      await _settleParsing(tester);
      expect(_previewCodeText(tester), 'foobar');

      await tester.tap(
        find.byKey(const ValueKey('sovereign-example-mode-source')),
      );
      await _settleParsing(tester);
      expect(_editorText(tester), '```rust\nfoobar\n```');
    },
  );

  testWidgets('scratch table cells preserve caret through escaped edits', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(
      find.byType(EditableText),
      '| Area | Status |\n| --- | --- |\n| Cell | Other |',
    );
    await _settleParsing(tester);

    expect(find.byKey(const Key('SovereignLiveBlockTable')), findsOneWidget);
    final cellFinder = _tableCellEditableFinder(1, 0);
    await tester.tap(cellFinder);
    await tester.showKeyboard(cellFinder);
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'Ce|ll',
        selection: TextSelection.collapsed(offset: 3),
      ),
    );
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(cellFinder).controller.selection,
      const TextSelection.collapsed(offset: 3),
    );

    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'A\nB|C',
        selection: TextSelection.collapsed(offset: 5),
      ),
    );
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(cellFinder).controller.selection,
      const TextSelection.collapsed(offset: 5),
    );

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(
      _editorText(tester),
      r'| Area | Status |'
      '\n'
      r'| --- | --- |'
      '\n'
      r'| A B\|C | Other |',
    );
  });

  testWidgets('scratch moves out of code fences with vertical arrows', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(
      find.byType(EditableText),
      'before\n```dart\nfoo\n```\nafter',
    );
    await _settleParsing(tester);

    final codeEditableFinder = _codeEditableFinder();
    var codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.text, 'foo');

    await tester.tap(codeEditableFinder);
    codeEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    await tester.showKeyboard(codeEditableFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await _settleParsing(tester);

    expect(_editableTextWithText(tester, 'before').focusNode.hasFocus, isTrue);

    await tester.tap(_codeEditableFinder());
    codeEditable = tester.widget<EditableText>(_codeEditableFinder());
    codeEditable.controller.selection = const TextSelection.collapsed(
      offset: 3,
    );
    await tester.pump();
    await tester.showKeyboard(_codeEditableFinder());
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await _settleParsing(tester);

    expect(_editableTextWithText(tester, 'after').focusNode.hasFocus, isTrue);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), 'before\n```dart\nfoo\n```\nafter');
  });

  testWidgets('scratch moves down out of a terminal code fence', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```dart\nfoo\n```');
    await _settleParsing(tester);

    final codeEditableFinder = _codeEditableFinder();
    final codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.text, 'foo');

    await tester.tap(codeEditableFinder);
    codeEditable.controller.selection = const TextSelection.collapsed(
      offset: 3,
    );
    await tester.pump();
    await tester.showKeyboard(codeEditableFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(codeEditableFinder).focusNode.hasFocus,
      isFalse,
    );
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
    await _settleParsing(tester);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '```dart\nfoo\n```\nafter');
  });

  testWidgets('scratch moves up into a code fence from following text', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(
      find.byType(EditableText),
      '```dart\nfoo\n```\nafter',
    );
    await _settleParsing(tester);

    final afterFinder = _editableFinderWithText('after');
    await tester.tap(afterFinder);
    await tester.pump();
    final afterEditable = tester.widget<EditableText>(afterFinder);
    afterEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    await tester.showKeyboard(afterFinder);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await _settleParsing(tester);

    expect(
      tester.widget<EditableText>(_codeEditableFinder()).focusNode.hasFocus,
      isTrue,
    );
  });

  testWidgets('scratch supports drag and double-click code selection', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '```dart\nfoo bar\n```');
    await _settleParsing(tester);

    final codeEditableFinder = _codeEditableFinder();
    final rect = tester.getRect(codeEditableFinder);
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.down(rect.centerLeft + const Offset(8, 0));
    await tester.pump();
    await gesture.moveTo(rect.centerLeft + const Offset(76, 0));
    await tester.pump();
    await gesture.up();
    await tester.pump();

    var codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.selection.isCollapsed, isFalse);

    codeEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    final wordOffset = rect.centerLeft + const Offset(18, 0);
    await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
    await tester.pump();

    codeEditable = tester.widget<EditableText>(codeEditableFinder);
    expect(codeEditable.controller.selection.textInside('foo bar'), 'foo');
  });

  testWidgets('scratch quote keyboard flows exit and unwrap correctly', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);

    await tester.enterText(find.byType(EditableText), '> quote');
    await _settleParsing(tester);
    expect(
      find.byKey(const Key('SovereignLiveBlockBlockquote')),
      findsOneWidget,
    );

    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(find.byType(EditableText), findsOneWidget);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'quote\n',
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), '> quote\n\n');

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);
    await tester.enterText(find.byType(EditableText), '> ');
    await _settleParsing(tester);
    expect(
      find.byKey(const Key('SovereignLiveBlockBlockquote')),
      findsOneWidget,
    );
    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), isEmpty);

    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-scenario-scratch')),
    );
    await _settleParsing(tester);
    await tester.enterText(find.byType(EditableText), '> quote');
    await _settleParsing(tester);
    final quoteEditable = tester.widget<EditableText>(
      find.byType(EditableText),
    );
    quoteEditable.controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await tester.pump();
    await tester.showKeyboard(find.byType(EditableText));
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await _settleParsing(tester);
    await tester.tap(
      find.byKey(const ValueKey('sovereign-example-mode-source')),
    );
    await _settleParsing(tester);
    expect(_editorText(tester), 'quote');
  });
}

Future<void> _settleParsing(WidgetTester tester) async {
  await tester.pump();
  for (var attempt = 0; attempt < 12; attempt++) {
    await tester.pump(const Duration(milliseconds: 100));
    await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    });
    await tester.pump();
  }
}

String _editorText(WidgetTester tester) {
  return find
      .byType(EditableText)
      .evaluate()
      .map((element) => (element.widget as EditableText).controller.text)
      .join('\n');
}

Future<void> _typeFocusedTextIncrementally(
  WidgetTester tester,
  String text,
) async {
  for (final codeUnit in text.codeUnits) {
    if (codeUnit == 0x0A) {
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      continue;
    }
    final focused = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .singleWhere((editable) => editable.focusNode.hasFocus);
    tester.testTextInput.enterText(
      focused.controller.text + String.fromCharCode(codeUnit),
    );
    await tester.pump();
  }
}

String _previewCodeText(WidgetTester tester) {
  final codeBlockFinder = find.byKey(
    const Key('SovereignReadOnlyPreviewCodeBlock'),
  );
  final richText = tester
      .widgetList<RichText>(
        find.descendant(of: codeBlockFinder, matching: find.byType(RichText)),
      )
      .singleWhere((widget) => widget.text.toPlainText() != 'Copy');
  return richText.text.toPlainText();
}

Finder _codeEditableFinder() {
  return find.descendant(
    of: find.byKey(const Key('SovereignLiveBlockCodeEditable')),
    matching: find.byType(EditableText),
  );
}

Finder _tableCellEditableFinder(int rowIndex, int columnIndex) {
  return find.descendant(
    of: find.byKey(Key('SovereignLiveBlockTableCell-$rowIndex-$columnIndex')),
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

const _awkwardScratchMarkdown = '''**wow*
*wow**
__wow_
_wow__
[label](url
![alt](url

```dart
final value = 1;
```


after fence

- [
- [x
``code`
''';
