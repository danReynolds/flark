import 'dart:async';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'delayed paste cannot cross a controller replacement at the same revision',
    (tester) async {
      final first = FlarkController(
        FlarkEditor(backend, text: 'first', caret: 5),
      );
      final next = FlarkController(
        FlarkEditor(backend, text: 'next', caret: 4),
      );
      final clipboard = Completer<Object?>();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async =>
            call.method == 'Clipboard.getData' ? clipboard.future : null,
      );
      Widget app(FlarkController c) => MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      );
      await tester.pumpWidget(app(first));
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpWidget(app(next));
      clipboard.complete({'text': ' pasted'});
      await tester.pump();
      expect(first.text, 'first');
      expect(next.text, 'next');
      await tester.pumpWidget(const SizedBox());
      first.dispose();
      next.dispose();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      );
    },
  );
  testWidgets(
    'readonly transition closes input and focus replacement reopens it',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend));
      final first = FocusNode(), next = FocusNode();
      Widget app(FocusNode focus, bool readOnly) => MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            focusNode: focus,
            readOnly: readOnly,
          ),
        ),
      );
      await tester.pumpWidget(app(first, false));
      await tester.pump();
      expect(tester.testTextInput.hasAnyClients, isTrue);
      await tester.pumpWidget(app(first, true));
      await tester.pump();
      expect(tester.testTextInput.hasAnyClients, isFalse);
      await tester.pumpWidget(app(next, false));
      next.requestFocus();
      await tester.pump();
      expect(next.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      tester.testTextInput.enterText('recovered');
      await tester.pump();
      expect(c.text, 'recovered');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      first.dispose();
      next.dispose();
    },
  );
  testWidgets('a rejected platform value resends exact canonical state once', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: '**ab** cd'));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    c.command(const SetSelection(3, 8));
    await tester.pump();
    tester.testTextInput.log.clear();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '**axd',
        selection: TextSelection.collapsed(offset: 4),
      ),
    );
    await tester.pump();
    expect(c.text, '**ab** cd');
    final sent = tester.testTextInput.log
        .where((call) => call.method == 'TextInput.setEditingState')
        .toList();
    expect(sent, hasLength(1));
    expect((sent.single.arguments as Map)['text'], '**ab** cd');
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  test('duplicate acknowledgments do not publish another controller state', () {
    final c = FlarkController(FlarkEditor(backend));
    var publications = 0;
    c.addListener(() => publications++);
    const value = TextEditingValue(
      text: 'a',
      selection: TextSelection.collapsed(offset: 1),
    );
    expect(c.receive(value), isTrue);
    expect(publications, 1);
    expect(c.receive(value), isFalse);
    expect(publications, 1);
    c.dispose();
  });
}
