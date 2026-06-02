import 'package:example/main.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flark/flark.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('example supports common markdown editing flows', (tester) async {
    await tester.pumpWidget(const FlarkExampleApp());
    await _settleParsing(tester);

    expect(find.text('Comrak'), findsOneWidget);
    expect(_editorText(tester), contains('Flark Markdown'));
    expect(_editorText(tester), isNot(contains('# Flark Markdown')));

    await tester.tap(_key('flark-example-scenario-article'));
    await _settleParsing(tester);
    expect(_editorText(tester), contains('Release Notes'));

    await tester.tap(_key('flark-example-mode-source'));
    await _settleParsing(tester);
    expect(_editorText(tester), contains('# Release Notes'));
    expect(_editorText(tester), contains('```dart'));

    await tester.tap(_key('flark-example-scenario-scratch'));
    await _settleParsing(tester);
    expect(_editorText(tester), isEmpty);
    expect(_editingMode(tester), FlarkMarkdownEditingMode.liveRendered);

    await tester.enterText(find.byType(EditableText), 'A\n\nB\nC');
    await _settleParsing(tester);
    expect(_editorText(tester), 'A\n\nB\nC');

    await tester.enterText(find.byType(EditableText), _manualMarkdown);
    await _settleParsing(tester);
    expect(_editorText(tester), contains('Manual check'));
    expect(_editorText(tester), contains('bold'));
    expect(_editorText(tester), contains('inline code'));
    expect(_editorText(tester), isNot(contains('**bold**')));
    expect(_editorText(tester), isNot(contains('```dart')));

    await tester.tap(_key('flark-example-mode-rendered'));
    await _settleParsing(tester);
    expect(find.byType(MarkdownEditor), findsNothing);
    expect(
      find.byKey(const Key('FlarkReadOnlyPreviewCodeBlock')),
      findsWidgets,
    );
    expect(
      find.byKey(const Key('FlarkReadOnlyPreviewBlockquote')),
      findsWidgets,
    );

    await tester.tap(_key('flark-example-mode-live'));
    await _settleParsing(tester);
    final liveText = _editorText(tester);
    expect(liveText, contains('Manual check'));
    expect(liveText, contains('bold'));
    expect(liveText, contains('italic'));
    expect(liveText, contains('inline code'));
    expect(liveText, contains('A quoted line'));
    expect(liveText, contains('final value = 1;'));
    expect(liveText, isNot(contains('**bold**')));
    expect(liveText, isNot(contains('```dart')));

    await tester.tap(_key('flark-example-scenario-tables'));
    await _settleParsing(tester);
    await tester.tap(_key('flark-example-mode-source'));
    await _settleParsing(tester);
    expect(_editorText(tester), contains('| Feature | Status |'));
    expect(_footerText(tester), contains('Caret'));
  });
}

Finder _key(String value) => find.byKey(ValueKey<String>(value));

Future<void> _settleParsing(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 200));
  await tester.pump(const Duration(milliseconds: 200));
}

String _editorText(WidgetTester tester) {
  return find
      .byType(EditableText)
      .evaluate()
      .map((element) => (element.widget as EditableText).controller.text)
      .join('\n');
}

FlarkMarkdownEditingMode _editingMode(WidgetTester tester) {
  return tester.widget<MarkdownEditor>(find.byType(MarkdownEditor)).editingMode;
}

String _footerText(WidgetTester tester) {
  final finder = find.textContaining('| selection');
  expect(finder, findsOneWidget);
  return tester.widget<Text>(finder).data!;
}

const _manualMarkdown = '''
# Manual check

A **bold** phrase, an _italic_ phrase, and `inline code`.

> A quoted line

```dart
final value = 1;
```

- [x] done
- [ ] todo

| Left | Right |
| --- | --- |
| one | two |
''';
