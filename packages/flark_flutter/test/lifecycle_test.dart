import 'dart:async';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/gestures.dart';
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

  testWidgets(
    'a connection the platform closes is sent the document when input reattaches',
    (tester) async {
      const source = '# Notes\n\nkeep **this** text';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      // iOS closes the connection when its input view resigns first
      // responder, as the iPad hide-keyboard key does. The editor keeps
      // focus and its caret, and a press reattaches input.
      tester.testTextInput.closeConnection();
      await tester.pump();
      final logStart = tester.testTextInput.log.length;
      await tester.tapAt(
        tester.getCenter(find.byType(FlarkEditorWidget)),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump(kDoubleTapTimeout);
      // The new platform client starts empty, so it must receive the
      // document before its first input refers to it.
      expect(
        tester.testTextInput.log.skip(logStart).map((call) => call.method),
        containsAllInOrder([
          'TextInput.setClient',
          'TextInput.setEditingState',
        ]),
      );
      final platform = TextEditingValue.fromJSON(
        tester.testTextInput.editingState!,
      );
      expect(platform.text, source);
      tester.testTextInput.updateEditingValue(
        platform.copyWith(
          text: '$source!',
          selection: const TextSelection.collapsed(offset: source.length + 1),
        ),
      );
      expect(c.text, '$source!');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  // The engine reports the app's lifecycle on this channel. On the web a
  // window that loses focus makes the app inactive, a hidden tab hides it,
  // and either returning resumes it.
  Future<void> setLifecycle(WidgetTester tester, AppLifecycleState state) =>
      tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SystemChannels.lifecycle.name,
        SystemChannels.lifecycle.codec.encodeMessage(state.toString()),
        (_) {},
      );

  Widget focusedEditor(FlarkController c, FocusNode focus) => MaterialApp(
    home: Scaffold(
      body: FlarkEditorWidget(controller: c, focusNode: focus, autofocus: true),
    ),
  );

  Iterable<String> methodsSince(WidgetTester tester, int start) =>
      tester.testTextInput.log.skip(start).map((call) => call.method);

  // Types at the end of the value the platform was last sent, as an input
  // method holding it would.
  void typeAtEnd(WidgetTester tester, String text) {
    final platform = TextEditingValue.fromJSON(
      tester.testTextInput.editingState!,
    );
    final typed = platform.text + text;
    tester.testTextInput.updateEditingValue(
      platform.copyWith(
        text: typed,
        selection: TextSelection.collapsed(offset: typed.length),
      ),
    );
  }

  testWidgets(
    'a key reopens input the platform closed while the editor kept focus',
    (tester) async {
      const source = 'keep this text';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
      );
      final focus = FocusNode();
      await tester.pumpWidget(focusedEditor(c, focus));
      await tester.pump();
      // The iPad hide-keyboard key, or the web engine on a blur without a
      // related target, closes the connection. The editor keeps focus and
      // its caret, and keys still reach it, but typed characters are lost.
      tester.testTextInput.closeConnection();
      await tester.pump();
      expect(focus.hasFocus, isTrue);
      typeAtEnd(tester, '?');
      expect(c.text, source);
      final logStart = tester.testTextInput.log.length;
      await tester.sendKeyEvent(LogicalKeyboardKey.keyH);
      await tester.pump();
      // The new client is sent the document, then where the editor is.
      expect(
        methodsSince(tester, logStart),
        containsAllInOrder([
          'TextInput.setClient',
          'TextInput.setEditingState',
          'TextInput.show',
          'TextInput.setEditableSizeAndTransform',
        ]),
      );
      expect(
        TextEditingValue.fromJSON(tester.testTextInput.editingState!).text,
        source,
      );
      typeAtEnd(tester, '!');
      expect(c.text, '$source!');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      focus.dispose();
    },
    variant: TargetPlatformVariant.all(),
  );

  // On the web and desktop Flutter parks focus while the app is away and
  // restores it on return, so a window or tab that lost focus reattaches
  // input through the focus change and needs no lifecycle handler.
  for (final (place, away) in [
    ('window', AppLifecycleState.inactive),
    ('tab', AppLifecycleState.hidden),
  ]) {
    testWidgets(
      'a $place that lost focus reattaches input when it returns',
      (tester) async {
        const source = 'keep this text';
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: source.length),
        );
        final focus = FocusNode();
        await setLifecycle(tester, AppLifecycleState.resumed);
        await tester.pumpWidget(focusedEditor(c, focus));
        await tester.pump();
        // Leaving blurs the web engine's input element with no related
        // target, which closes the connection, and then the window.
        tester.testTextInput.closeConnection();
        await setLifecycle(tester, away);
        await tester.pump();
        expect(focus.hasFocus, isFalse);
        final logStart = tester.testTextInput.log.length;
        await setLifecycle(tester, AppLifecycleState.resumed);
        await tester.pump();
        expect(focus.hasFocus, isTrue);
        expect(
          methodsSince(tester, logStart),
          containsAllInOrder([
            'TextInput.setClient',
            'TextInput.setEditingState',
            'TextInput.show',
          ]),
        );
        typeAtEnd(tester, '!');
        expect(c.text, '$source!');
        await tester.pumpWidget(const SizedBox());
        c.dispose();
        focus.dispose();
      },
      variant: TargetPlatformVariant.desktop(),
    );
  }

  testWidgets(
    'resuming iOS leaves a keyboard its user hid down until a press',
    (tester) async {
      const source = 'keep this text';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
      );
      final focus = FocusNode();
      await setLifecycle(tester, AppLifecycleState.resumed);
      await tester.pumpWidget(focusedEditor(c, focus));
      await tester.pump();
      // The hide-keyboard key resigns the input view, which closes the
      // connection. Leaving the app and returning must not raise it again.
      tester.testTextInput.closeConnection();
      await tester.pump();
      final logStart = tester.testTextInput.log.length;
      await setLifecycle(tester, AppLifecycleState.paused);
      await setLifecycle(tester, AppLifecycleState.resumed);
      await tester.pump();
      expect(focus.hasFocus, isTrue);
      expect(methodsSince(tester, logStart), isNot(contains('TextInput.show')));
      expect(
        methodsSince(tester, logStart),
        isNot(contains('TextInput.setClient')),
      );
      typeAtEnd(tester, '!');
      expect(c.text, source);
      await tester.tapAt(tester.getCenter(find.byType(FlarkEditorWidget)));
      await tester.pump(kDoubleTapTimeout);
      expect(
        methodsSince(tester, logStart),
        containsAllInOrder(['TextInput.setClient', 'TextInput.show']),
      );
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      focus.dispose();
    },
    variant: TargetPlatformVariant.only(TargetPlatform.iOS),
  );

  // Android composes the word being typed.
  void composeWord(WidgetTester tester) =>
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'an',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 1, end: 2),
        ),
      );

  // An application widget outside the editor that rebuilds when [listened]
  // changes, as a toolbar or status line does. The framework refuses that
  // rebuild while it builds or unmounts the editor.
  Widget composingApp(FlarkController listened, {Widget? body}) => MaterialApp(
    home: Scaffold(
      appBar: AppBar(
        title: ListenableBuilder(
          listenable: listened,
          builder: (context, _) =>
              Text(listened.editor.composing ? 'composing' : 'idle'),
        ),
      ),
      body: body,
    ),
  );

  testWidgets(
    'removing the editor mid-composition ends the controller composition',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: 'a', caret: 1));
      Widget app({required bool editor}) => composingApp(
        c,
        body: editor ? FlarkEditorWidget(controller: c, autofocus: true) : null,
      );
      await tester.pumpWidget(app(editor: true));
      await tester.pump();
      composeWord(tester);
      await tester.pump();
      expect(find.text('composing'), findsOneWidget);
      // The view goes away while the application keeps the controller.
      await tester.pumpWidget(app(editor: false));
      expect(c.editor.composing, isFalse);
      expect(c.value.composing, TextRange.empty);
      await tester.pump();
      expect(find.text('idle'), findsOneWidget);
      // A later view sends no stale composing range and handles keys again.
      await tester.pumpWidget(app(editor: true));
      await tester.pump();
      expect(
        TextEditingValue.fromJSON(tester.testTextInput.editingState!).composing,
        TextRange.empty,
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      expect(c.text, 'a');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets('replacing the focus node mid-composition ends the composition', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: 'a', caret: 1));
    final first = FocusNode(), next = FocusNode();
    Widget app(FocusNode focus) => composingApp(
      c,
      body: FlarkEditorWidget(controller: c, autofocus: true, focusNode: focus),
    );
    await tester.pumpWidget(app(first));
    await tester.pump();
    composeWord(tester);
    expect(c.editor.composing, isTrue);
    await tester.pumpWidget(app(next));
    await tester.pump();
    expect(c.editor.composing, isFalse);
    expect(c.text, 'an');
    expect(c.value.composing, TextRange.empty);
    expect(find.text('idle'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
    first.dispose();
    next.dispose();
  });

  testWidgets('replacing the controller mid-composition ends its composition', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: 'a', caret: 1));
    final next = FlarkController(FlarkEditor(backend, text: 'b', caret: 1));
    Widget app(FlarkController controller) => composingApp(
      c,
      body: FlarkEditorWidget(controller: controller, autofocus: true),
    );
    await tester.pumpWidget(app(c));
    await tester.pump();
    composeWord(tester);
    expect(c.editor.composing, isTrue);
    await tester.pumpWidget(app(next));
    await tester.pump();
    expect(c.editor.composing, isFalse);
    expect(c.text, 'an');
    expect(find.text('idle'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
    next.dispose();
  });
}
