import 'package:example/main.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark.dart';

void main() {
  testWidgets('example supports common live markdown editing flows', (
    tester,
  ) async {
    await tester.pumpWidget(const FlarkExampleApp());
    await _settleParsing(tester);

    expect(find.text('Flark Markdown Editor'), findsOneWidget);
    expect(_controller(tester).markdown, contains('# Flark Markdown'));
    expect(find.byKey(const Key('FlarkLiveBlockEditor')), findsOneWidget);
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);

    await tester.ensureVisible(
      find.byKey(const Key('FlarkLiveBlockCodeFence')),
    );
    await tester.pump();
    final sampleFenceRect = tester.getRect(
      find.byKey(const Key('FlarkLiveBlockCodeFence')),
    );
    await tester.tapAt(sampleFenceRect.bottomLeft + const Offset(8, 24));
    await tester.pump();
    await tester.pump();
    await _typeFocusedTextIncrementally(tester, '```fffff');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, contains('```\n```\nfffff'));
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsNWidgets(2));
    final sampleCodeEditors = tester
        .widgetList<EditableText>(_codeEditableFinder())
        .toList(growable: false);
    expect(sampleCodeEditors, hasLength(2));
    expect(sampleCodeEditors.last.controller.text, 'fffff');

    await _tapKey(tester, 'flark-example-scenario-article');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, contains('# Release Notes'));
    expect(find.byKey(const Key('FlarkLiveBlockEditor')), findsOneWidget);
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'Title');
    await _tapKey(tester, 'flark-example-command-heading1');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '# Title');
    expect(_editorText(tester), 'Title');
    expect(_footerText(tester), contains('Caret'));

    await _tapKey(tester, 'flark-example-undo');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, 'Title');
    await _tapKey(tester, 'flark-example-redo');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '# Title');

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'bold italic');
    await _selectSourceRange(tester, 0, 4);
    await _tapKey(tester, 'flark-example-command-bold');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '**bold** italic');
    expect(_editorText(tester), 'bold italic');
    final italicStart = _controller(tester).markdown.indexOf('italic');
    await _selectSourceRange(
      tester,
      italicStart,
      italicStart + 'italic'.length,
    );
    await _tapKey(tester, 'flark-example-command-italic');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '**bold** *italic*');
    expect(_editorText(tester), 'bold italic');

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'item');
    await _tapKey(tester, 'flark-example-command-bulleted-list');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '- item');
    await tester.showKeyboard(_editableWithText('item'));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '- item\n- ');
    expect(_editableTexts(tester), contains(''));

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'item');
    await _tapKey(tester, 'flark-example-command-ordered-list');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '1. item');
    await tester.showKeyboard(_editableWithText('item'));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '1. item\n2. ');
    expect(find.text('2.'), findsOneWidget);

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'todo');
    await _tapKey(tester, 'flark-example-command-task-list');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '- [ ] todo');
    await tester.showKeyboard(_editableWithText('todo'));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '- [ ] todo\n- [ ] ');
    await tester.tap(find.byKey(const Key('FlarkLiveBlockTaskCheckbox')).at(0));
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '- [x] todo\n- [ ] ');

    await _loadScratch(tester);
    await _replaceOnlyEditableText(tester, 'A quoted line');
    await _tapKey(tester, 'flark-example-command-quote');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '> A quoted line');
    expect(_editorText(tester), contains('A quoted line'));
    expect(_editorText(tester), isNot(contains('> A quoted line')));

    await _loadScratch(tester);
    await _tapKey(tester, 'flark-example-command-code-fence');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```dart\n\n```');
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(_codeEditableText(tester), '\n');
    await tester.tap(find.byKey(const Key('FlarkLiveBlockCodeLanguageButton')));
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('FlarkLiveBlockCodeLanguageOption:rust')),
    );
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```rust\n\n```');

    await _loadScratch(tester);
    await _typeBlankFenceOpener(tester, find.byType(EditableText));
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```\n');
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(_codeEditableText(tester), isEmpty);

    await _loadScratch(tester);
    await tester.tap(find.byType(EditableText));
    await tester.showKeyboard(find.byType(EditableText));
    await _typeFocusedTextIncrementally(tester, '```fffff');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```\nfffff');
    expect(find.byKey(const Key('FlarkLiveBlockCodeFence')), findsOneWidget);
    expect(_codeEditableText(tester), 'fffff');

    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('ggggg');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```\nggggg');
    expect(_codeEditableText(tester), 'ggggg');

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```\nggggg\n');
    expect(_codeEditableText(tester), 'ggggg\n');
    expect(_codeEditableText(tester), isNot('ggggg\n\n'));

    tester.testTextInput.enterText('ggggg\n```');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```\nggggg\n```');
    expect(_codeEditableText(tester), 'ggggg');

    await _loadScratch(tester);
    await _typeBlankFenceOpener(tester, find.byType(EditableText));
    await _settleParsing(tester);
    await tester.showKeyboard(_codeEditableFinder());
    tester.testTextInput.enterText('dart');
    await _settleParsing(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _settleParsing(tester);
    expect(_controller(tester).markdown, '```dart\n');
    expect(_codeEditableText(tester), isEmpty);

    await _loadScratch(tester);
    await _tapKey(tester, 'flark-example-command-table');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, contains('| Header 1 | Header 2 |'));
    final cellFinder = find.descendant(
      of: find.byKey(const Key('FlarkLiveBlockTableCell-1-1')),
      matching: find.byType(EditableText),
    );
    expect(cellFinder, findsOneWidget);
    await tester.enterText(cellFinder, 'cell|value');
    await _settleParsing(tester);
    expect(_controller(tester).markdown, contains(r'cell\|value'));
  });
}

Finder _key(String value) => find.byKey(ValueKey<String>(value));

Future<void> _tapKey(WidgetTester tester, String value) async {
  final finder = _key(value);
  expect(finder, findsOneWidget);
  await tester.ensureVisible(finder);
  await tester.pump();
  await tester.tap(finder);
}

Future<void> _settleParsing(WidgetTester tester) async {
  await tester.pump();
  for (var i = 0; i < 80; i++) {
    final controller = _controller(tester);
    if (controller.markdown.isEmpty || controller.hasAuthoritativeRenderPlan) {
      await tester.pump();
      return;
    }
    await tester.runAsync(controller.parseNow);
    await tester.pump();
    if (controller.hasAuthoritativeRenderPlan) return;
    await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 50));
    });
    await tester.pump();
  }
  expect(_controller(tester).hasAuthoritativeRenderPlan, isTrue);
  await tester.pump();
}

Future<void> _loadScratch(WidgetTester tester) async {
  await _tapKey(tester, 'flark-example-scenario-scratch');
  await _settleParsing(tester);
  expect(_controller(tester).markdown, isEmpty);
  expect(_editingMode(tester), FlarkMarkdownEditingMode.liveRendered);
  expect(find.byType(EditableText), findsOneWidget);
}

Future<void> _replaceOnlyEditableText(WidgetTester tester, String text) async {
  final editableFinder = find.byType(EditableText);
  expect(editableFinder, findsOneWidget);
  await tester.enterText(editableFinder, text);
  await _settleParsing(tester);
  expect(_controller(tester).markdown, text);
}

Future<void> _selectSourceRange(WidgetTester tester, int start, int end) async {
  expect(
    _controller(
      tester,
    ).applySelection(FlarkSelection(baseOffset: start, extentOffset: end)),
    isTrue,
  );
  await tester.pump();
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
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      continue;
    }
    final focused = await _focusedEditableText(tester);
    tester.testTextInput.enterText(
      focused.controller.text + String.fromCharCode(codeUnit),
    );
    await tester.pump();
  }
}

Future<EditableText> _focusedEditableText(WidgetTester tester) async {
  for (var attempt = 0; attempt < 3; attempt += 1) {
    final focused = tester
        .widgetList<EditableText>(find.byType(EditableText))
        .where((editable) => editable.focusNode.hasFocus)
        .toList(growable: false);
    if (focused.length == 1) return focused.single;
    if (focused.length > 1) {
      throw StateError('Multiple focused EditableText widgets');
    }
    await tester.pump();
  }

  final editableFinder = find.byType(EditableText);
  if (editableFinder.evaluate().length == 1) {
    await tester.showKeyboard(editableFinder);
    await tester.pump();
    return tester.widget<EditableText>(editableFinder);
  }

  return tester
      .widgetList<EditableText>(editableFinder)
      .singleWhere((editable) => editable.focusNode.hasFocus);
}

FlarkFlutterController _controller(WidgetTester tester) {
  return tester.widget<FlarkMarkdownEditor>(find.byType(FlarkMarkdownEditor)).controller!;
}

FlarkMarkdownEditingMode _editingMode(WidgetTester tester) {
  return tester.widget<FlarkMarkdownEditor>(find.byType(FlarkMarkdownEditor)).editingMode;
}

String _editorText(WidgetTester tester) => _editableTexts(tester).join('\n');

List<String> _editableTexts(WidgetTester tester) {
  return tester
      .widgetList<EditableText>(find.byType(EditableText))
      .map((editable) => editable.controller.text)
      .toList();
}

Finder _editableWithText(String text) {
  return find.byWidgetPredicate(
    (widget) => widget is EditableText && widget.controller.text == text,
    description: 'EditableText with text "$text"',
  );
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

String _footerText(WidgetTester tester) {
  final finder = find.textContaining('Caret');
  expect(finder, findsOneWidget);
  return tester.widget<Text>(finder).data!;
}
