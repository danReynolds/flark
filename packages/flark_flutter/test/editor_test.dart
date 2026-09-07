import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final delta in [false, true]) {
    for (final (source, caret, inserted) in [
      ('| alpha |\n| --- |', 7, ' '),
      ('alpha beta', 5, ' '),
      ('aaa', 1, 'a'),
    ]) {
      test(
        'ambiguous repeated input uses its selected location delta=$delta $source',
        () {
          final c = FlarkController(
            FlarkEditor(backend, text: source, caret: caret),
          );
          final next = source.replaceRange(caret, caret, inserted);
          final selection = TextSelection.collapsed(
            offset: caret + inserted.length,
          );
          expect(
            delta
                ? c.receiveDeltas([
                    TextEditingDeltaInsertion(
                      oldText: source,
                      textInserted: inserted,
                      insertionOffset: caret,
                      selection: selection,
                      composing: TextRange.empty,
                    ),
                  ])
                : c.receive(TextEditingValue(text: next, selection: selection)),
            isTrue,
          );
          expect(c.text, next);
          expect(c.value.selection, selection);
          expect(c.command(const InsertText('X')), isTrue);
          expect(c.text, source.replaceRange(caret, caret, '${inserted}X'));
          c.dispose();
        },
      );
    }
    for (final backward in [false, true]) {
      test(
        'repeated-letter deletion stays at the requested caret delta=$delta backward=$backward',
        () {
          final c = FlarkController(
            FlarkEditor(backend, text: 'aaaa', caret: 2),
          );
          final selection = TextSelection.collapsed(offset: backward ? 1 : 2);
          expect(
            delta
                ? c.receiveDeltas([
                    TextEditingDeltaDeletion(
                      oldText: 'aaaa',
                      deletedRange: TextRange(
                        start: backward ? 1 : 2,
                        end: backward ? 2 : 3,
                      ),
                      selection: selection,
                      composing: TextRange.empty,
                    ),
                  ])
                : c.receive(
                    TextEditingValue(text: 'aaa', selection: selection),
                  ),
            isTrue,
          );
          expect(c.value.selection, selection);
          expect(c.command(const InsertText('X')), isTrue);
          expect(c.text, backward ? 'aXaa' : 'aaXa');
          c.dispose();
        },
      );
    }
  }
  for (final delta in [false, true]) {
    testWidgets(
      'word-by-word input keeps spaces and its painted caret delta=$delta',
      (tester) async {
        final c = FlarkController(
          FlarkEditor(backend, text: 'alpha', caret: 5),
        );
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        var expected = 'alpha';
        for (final character in ' beta  gamma!'.split('')) {
          final before = c.value;
          expected += character;
          paints.clear();
          if (delta) {
            final client =
                (tester.testTextInput.log
                            .lastWhere(
                              (call) => call.method == 'TextInput.setClient',
                            )
                            .arguments
                        as List)
                    .first;
            await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
              SystemChannels.textInput.name,
              SystemChannels.textInput.codec.encodeMethodCall(
                MethodCall('TextInputClient.updateEditingStateWithDeltas', [
                  client,
                  {
                    'deltas': [
                      {
                        'oldText': before.text,
                        'deltaText': character,
                        'deltaStart': before.selection.extentOffset,
                        'deltaEnd': before.selection.extentOffset,
                        'selectionBase': expected.length,
                        'selectionExtent': expected.length,
                        'selectionAffinity': 'TextAffinity.downstream',
                        'selectionIsDirectional': false,
                        'composingBase': -1,
                        'composingExtent': -1,
                      },
                    ],
                  },
                ]),
              ),
              (_) {},
            );
          } else {
            tester.testTextInput.updateEditingValue(
              TextEditingValue(
                text: expected,
                selection: TextSelection.collapsed(offset: expected.length),
              ),
            );
          }
          expect(c.text, expected);
          expect(c.value.selection.extentOffset, expected.length);
          await tester.pump();
          expect(paints, isNotEmpty);
          expect(paints.last.rows, [expected]);
          expect(paints.last.caretSource, expected.length);
          expect(paints.last.caret, isNotNull);
        }
        await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
        expect(c.text, 'alpha');
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      },
    );
  }
  test(
    'platform replacements widen over combining and surrogate boundaries',
    () {
      for (final (before, after) in [
        ('e\u0301', 'e\u0300'),
        ('\u{1f601}', '\u{1e601}'),
      ]) {
        final c = FlarkController(
          FlarkEditor(backend, text: before, caret: before.length),
        );
        expect(
          c.receive(
            TextEditingValue(
              text: after,
              selection: TextSelection.collapsed(offset: after.length),
            ),
          ),
          isTrue,
        );
        expect(c.text, after);
        c.dispose();
      }
    },
  );
  test('composition publishes source and composing range together once', () {
    final c = FlarkController(FlarkEditor(backend));
    final values = <TextEditingValue>[];
    c.addListener(() => values.add(c.value));
    c.receive(
      const TextEditingValue(
        text: 'n',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      ),
    );
    expect(values, hasLength(1));
    expect(values.single.composing, const TextRange(start: 0, end: 1));
    c.dispose();
  });
  test('full-value, delta, duplicate and stale input share semantics', () {
    final c = FlarkController(FlarkEditor(backend, text: '*t*', caret: 2));
    final before = c.value;
    expect(
      c.receive(
        const TextEditingValue(
          text: '**',
          selection: TextSelection.collapsed(offset: 1),
        ),
      ),
      isTrue,
    );
    expect(c.text, '');
    expect(c.editor.typingContext, Style.emphasis);
    expect(
      c.receive(
        const TextEditingValue(
          text: '**',
          selection: TextSelection.collapsed(offset: 1),
        ),
      ),
      isFalse,
    );
    expect(
      c.receiveDeltas([
        const TextEditingDeltaInsertion(
          oldText: '',
          textInserted: 'x',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]),
      isTrue,
    );
    expect(c.text, '*x*');
    expect(c.receive(before, expectedRevision: 0), isFalse);
    expect(c.text, '*x*');
    c.dispose();
  });
  test('composition updates undo as one action and cancel restores', () {
    final c = FlarkController(FlarkEditor(backend, text: 'a', caret: 1));
    c.receive(
      const TextEditingValue(
        text: 'an',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      ),
    );
    c.receive(
      const TextEditingValue(
        text: 'a你',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      ),
    );
    c.receive(
      const TextEditingValue(
        text: 'a你',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    expect(c.editor.composing, isFalse);
    c.command(const Undo());
    expect(c.text, 'a');
    c.receive(
      const TextEditingValue(
        text: 'an',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      ),
    );
    c.receive(
      const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
      ),
    );
    expect(c.text, 'a');
    expect(c.editor.history.canRedo, isTrue);
    c.dispose();
  });
  for (final burst in [false, true]) {
    testWidgets(
      'delete-to-empty then type paints current styled result burst=$burst',
      (tester) async {
        final c = FlarkController(FlarkEditor(backend, text: '*t*', caret: 2));
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        paints.clear();
        await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
        expect(c.text, '');
        if (!burst) {
          await tester.pump();
          expect(paints, isNotEmpty);
          expect(paints.last.rows, ['']);
          expect(paints.last.caretSource, 0);
          paints.clear();
        }
        tester.testTextInput.updateEditingValue(
          const TextEditingValue(
            text: 'x',
            selection: TextSelection.collapsed(offset: 1),
          ),
        );
        expect(c.text, '*x*');
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(p.rows, ['x']);
          expect(p.styles.single, [Style.emphasis]);
          expect(p.resolvedStyles.single.single.fontStyle, FontStyle.italic);
          expect(p.caretSource, 2);
          expect(p.revision, c.editor.revision);
          expect(p.snapshot.source, '*x*');
          expect(p.caret, isNotNull);
        }
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      },
    );
  }
  testWidgets('Return, list exit and immediate typing use actual paints', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: '- **ab**', caret: 6));
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pump();
    paints.clear();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(c.text, '- **ab**\n- ');
    await tester.pump();
    expect(paints.last.rows, ['ab', '']);
    expect(paints.last.styles.first, [Style.strong]);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    final value = c.value;
    tester.testTextInput.updateEditingValue(
      value.copyWith(
        text: '${value.text}p',
        selection: TextSelection.collapsed(offset: value.text.length + 1),
      ),
    );
    await tester.pump();
    expect(paints.last.rows, ['ab', '', 'p']);
    expect(c.editor.projection.rows.last.shells, isEmpty);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets('wrapped up/down uses glyph geometry and preserves source', (
    tester,
  ) async {
    final c = FlarkController(
      FlarkEditor(backend, text: 'ordinary words ' * 12, caret: 80),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 240,
            child: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    final y = paints.last.caret!.top;
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(paints.last.caret!.top, lessThan(y));
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(paints.last.caret!.top, y);
    expect(c.text, 'ordinary words ' * 12);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  test(
    'canceling a composition after pending formatting restores its typing intent',
    () {
      final c = FlarkController(FlarkEditor(backend));
      c.command(const ToggleStyle(Style.emphasis));
      expect(
        c.receive(
          const TextEditingValue(
            text: 'n',
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange(start: 0, end: 1),
          ),
        ),
        isTrue,
      );
      expect(c.text, '*n*');
      expect(c.value.composing, const TextRange(start: 1, end: 2));
      expect(
        c.receive(
          const TextEditingValue(
            text: '**',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        isTrue,
      );
      expect(c.text, '');
      expect(c.editor.composing, isFalse);
      expect(c.editor.history.canUndo, isFalse);
      expect(c.editor.typingContext, Style.emphasis);
      expect(c.command(const InsertText('x')), isTrue);
      expect(c.text, '*x*');
      c.dispose();
    },
  );
}
