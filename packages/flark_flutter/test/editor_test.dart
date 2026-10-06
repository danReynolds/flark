import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart' show kDoubleTapTimeout;
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
      const composed = TextEditingValue(
        text: 'n',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      expect(c.receive(composed), isTrue);
      // The platform's text stays as it composed it.
      expect(c.value, composed);
      expect(c.editor.typingContext, Style.emphasis);
      expect(
        c.receive(
          const TextEditingValue(
            text: '',
            selection: TextSelection.collapsed(offset: 0),
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
  test('composed text takes pending formatting when it commits', () {
    // Wrapped as it was composed, the platform's text changed under its
    // input method, and the next preedit or the commit landed after the
    // delimiters (`**n日本**`).
    final c = FlarkController(FlarkEditor(backend, text: 'abc', caret: 3));
    c.command(const ToggleStyle(Style.strong));
    final values = <TextEditingValue>[];
    c.addListener(() => values.add(c.value));
    for (final preedit in ['n', 'に', '日本']) {
      final value = TextEditingValue(
        text: 'abc$preedit',
        selection: TextSelection.collapsed(offset: 3 + preedit.length),
        composing: TextRange(start: 3, end: 3 + preedit.length),
      );
      expect(c.receive(value), isTrue);
      expect(c.value, value);
    }
    expect(
      c.receive(
        const TextEditingValue(
          text: 'abc日本',
          selection: TextSelection.collapsed(offset: 5),
        ),
      ),
      isTrue,
    );
    expect(c.text, 'abc**日本**');
    expect(c.value.selection, const TextSelection.collapsed(offset: 7));
    expect(c.value.composing, TextRange.empty);
    expect(c.editor.composing, isFalse);
    expect(values.last, c.value);
    expect(c.command(const Undo()), isTrue);
    expect(c.text, 'abc');
    expect(c.editor.typingContext, Style.strong);
    c.dispose();
  });
  test('a commit that typing refuses is withdrawn with a notice', () {
    // Composed before a cell's delimiter, a backslash would escape it, which
    // typing refuses.
    const table = '| a | b |\n| - | - |\n| 1| 2 |';
    final c = FlarkController(FlarkEditor(backend, text: table, caret: 23));
    for (final preedit in ['x', r'\']) {
      expect(
        c.receive(
          TextEditingValue(
            text: table.replaceRange(23, 23, preedit),
            selection: const TextSelection.collapsed(offset: 24),
            composing: const TextRange(start: 23, end: 24),
          ),
        ),
        isTrue,
      );
    }
    expect(
      c.receive(
        TextEditingValue(
          text: table.replaceRange(23, 23, r'\'),
          selection: const TextSelection.collapsed(offset: 24),
        ),
      ),
      isTrue,
    );
    expect((c.text, c.editor.composing), (table, false));
    expect(c.value.selection, const TextSelection.collapsed(offset: 23));
    expect(c.notice, 'This edit needs source mode.');
    expect(c.editor.history.canUndo, isFalse);
    c.dispose();
  });

  for (final ending in ['a toolbar button', 'a click']) {
    testWidgets('a composition that $ending ends says why typing withdrew it', (
      tester,
    ) async {
      // As above, typing refuses the backslash composed before the cell's
      // delimiter. Here a command ends the composition and applies, and the
      // notice still says why the composed text went.
      const table = '| a | b |\n| - | - |\n| 1| 2 |';
      final c = FlarkController(FlarkEditor(backend, text: table, caret: 23));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      for (final preedit in ['x', r'\']) {
        tester.testTextInput.updateEditingValue(
          TextEditingValue(
            text: table.replaceRange(23, 23, preedit),
            selection: const TextSelection.collapsed(offset: 24),
            composing: const TextRange(start: 23, end: 24),
          ),
        );
        await tester.pump();
      }
      expect(c.editor.composing, isTrue);
      expect(c.notice, isNull);
      if (ending == 'a toolbar button') {
        await tester.tap(find.byTooltip('Bold'));
      } else {
        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkSurface),
        );
        await tester.tapAt(
          surface.localToGlobal(surface.caretRectAt(2).center),
        );
      }
      // Past the double tap's timeout, which a lone tap leaves running.
      await tester.pump(kDoubleTapTimeout);
      expect((c.text, c.editor.composing), (table, false));
      expect(c.notice, 'This edit needs source mode.');
      expect(find.text('This edit needs source mode.'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }

  TextEditingValue composing(String text, int caret, [TextRange? range]) =>
      TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: caret),
        composing: range ?? TextRange.empty,
      );

  for (final before in ['', 'x ']) {
    test('a syllable committed inside a composition stays when the next '
        'letter is deleted "$before"', () {
      // A Korean input method commits 한 and composes ㄱ in one update
      // (Android's batch edit, iOS's deltas of one run loop turn), then
      // Backspace takes the ㄱ and ends the composition. Read as a cancel of
      // the whole composition, that deleted the committed syllable too.
      final c = FlarkController(
        FlarkEditor(backend, text: before, caret: before.length),
      );
      final p = before.length;
      for (final value in [
        composing('$beforeㅎ', p + 1, TextRange(start: p, end: p + 1)),
        composing('$before한', p + 1, TextRange(start: p, end: p + 1)),
        composing('$before한ㄱ', p + 2, TextRange(start: p + 1, end: p + 2)),
      ]) {
        expect(c.receive(value), isTrue);
        // The platform's text is the document's while it composes.
        expect(c.value, value);
      }
      expect(c.receive(composing('$before한', p + 1)), isTrue);
      expect(c.text, '$before한');
      expect(c.value.selection, TextSelection.collapsed(offset: p + 1));
      expect(c.editor.composing, isFalse);
      // What the input method composed is one undo step.
      expect(c.command(const Undo()), isTrue);
      expect(c.text, before);
      expect(c.editor.history.canUndo, isFalse);
      c.dispose();
    });
  }
  test(
    'a clause committed in a delta batch stays when the rest is deleted',
    () {
      // Mozc converts きょう to 今日 and commits it, keeping は composing, in
      // one batch; Backspace then deletes は.
      final c = FlarkController(FlarkEditor(backend));
      expect(
        c.receiveDeltas([
          const TextEditingDeltaInsertion(
            oldText: '',
            textInserted: 'きょうは',
            insertionOffset: 0,
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange(start: 0, end: 4),
          ),
        ]),
        isTrue,
      );
      expect(
        c.receiveDeltas([
          const TextEditingDeltaReplacement(
            oldText: 'きょうは',
            replacementText: '今日',
            replacedRange: TextRange(start: 0, end: 3),
            selection: TextSelection.collapsed(offset: 2),
            composing: TextRange.empty,
          ),
          const TextEditingDeltaNonTextUpdate(
            oldText: '今日は',
            selection: TextSelection.collapsed(offset: 3),
            composing: TextRange(start: 2, end: 3),
          ),
        ]),
        isTrue,
      );
      expect(c.value, composing('今日は', 3, const TextRange(start: 2, end: 3)));
      expect(
        c.receiveDeltas([
          const TextEditingDeltaDeletion(
            oldText: '今日は',
            deletedRange: TextRange(start: 2, end: 3),
            selection: TextSelection.collapsed(offset: 2),
            composing: TextRange.empty,
          ),
        ]),
        isTrue,
      );
      expect(c.text, '今日');
      expect(c.editor.composing, isFalse);
      expect(c.command(const Undo()), isTrue);
      expect(c.text, '');
      c.dispose();
    },
  );
  test('a correction before the composing range stays when the composition '
      'is removed', () {
    // The input method corrects the word before the one it composes, which
    // moves its composing range, then removes the composed word.
    final c = FlarkController(FlarkEditor(backend, text: 'teh ', caret: 4));
    expect(
      c.receive(composing('teh wor', 7, const TextRange(start: 4, end: 7))),
      isTrue,
    );
    expect(
      c.receive(composing('thee wor', 8, const TextRange(start: 5, end: 8))),
      isTrue,
    );
    expect(c.receive(composing('thee ', 5)), isTrue);
    expect(c.text, 'thee ');
    expect(c.editor.composing, isFalse);
    c.dispose();
  });
  test('a pending style takes all that a partly committed composition '
      'composed', () {
    // Committing the first syllable as it was committed would have wrapped
    // it while the input method still composed the next, changing the text
    // under it.
    final c = FlarkController(FlarkEditor(backend));
    c.command(const ToggleStyle(Style.strong));
    for (final value in [
      composing('한', 1, const TextRange(start: 0, end: 1)),
      composing('한ㄱ', 2, const TextRange(start: 1, end: 2)),
      composing('한글', 2, const TextRange(start: 1, end: 2)),
    ]) {
      expect(c.receive(value), isTrue);
      expect(c.value, value);
    }
    expect(c.receive(composing('한글', 2)), isTrue);
    expect(c.text, '**한글**');
    expect(c.command(const Undo()), isTrue);
    expect(c.text, '');
    c.dispose();
  });
  for (final (source, selected, steps, committed) in [
    (
      '',
      const FlarkSelection.collapsed(0),
      [
        composing('ㅎ', 1, const TextRange(start: 0, end: 1)),
        composing('한', 1, const TextRange(start: 0, end: 1)),
        composing('한ㄱ', 2, const TextRange(start: 1, end: 2)),
      ],
      '한',
    ),
    (
      '',
      const FlarkSelection.collapsed(0),
      [
        composing('きょうは', 4, const TextRange(start: 0, end: 4)),
        composing('今日は', 3, const TextRange(start: 2, end: 3)),
      ],
      '今日',
    ),
    // Composed over a selection, which the committed syllable replaces.
    (
      'say cat',
      const FlarkSelection(4, 7),
      [
        composing('say 한', 5, const TextRange(start: 4, end: 5)),
        composing('say 한ㄱ', 6, const TextRange(start: 5, end: 6)),
      ],
      'say 한',
    ),
  ]) {
    testWidgets('Escape after a partial commit cancels only what still '
        'composes: $committed', (tester) async {
      // Off the web the editor sees Escape before the input method and
      // cancels the composition itself.
      final c = FlarkController(FlarkEditor(backend, text: source));
      c.command(SetSelection(selected.base, selected.extent));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      for (final value in steps) {
        tester.testTextInput.updateEditingValue(value);
        await tester.pump();
      }
      expect(c.editor.composing, isTrue);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      expect(c.text, committed);
      expect(c.editor.composing, isFalse);
      expect(c.value.composing, TextRange.empty);
      expect(c.command(const Undo()), isTrue);
      expect(c.text, source);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }

  for (final (typed, committed) in [('milk', 'milk'), ('teh', 'the')]) {
    for (final batched in [false, true]) {
      test('Return after a composed word continues the list: '
          '$typed->$committed batched=$batched', () {
        const source = '- eggs\n- ';
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: source.length),
        );
        // Android input methods compose the word being typed.
        for (var n = 1; n <= typed.length; n++) {
          c.receive(
            TextEditingValue(
              text: '$source${typed.substring(0, n)}',
              selection: TextSelection.collapsed(offset: source.length + n),
              composing: TextRange(
                start: source.length,
                end: source.length + n,
              ),
            ),
          );
        }
        expect(c.editor.composing, isTrue);
        final word = '$source$committed';
        if (!batched) {
          c.receive(
            TextEditingValue(
              text: word,
              selection: TextSelection.collapsed(offset: word.length),
            ),
          );
        }
        // LatinIME commits the (corrected) word and "\n" inside one batch
        // edit, which the platform publishes as one value.
        expect(
          c.receive(
            TextEditingValue(
              text: '$word\n',
              selection: TextSelection.collapsed(offset: word.length + 1),
            ),
          ),
          isTrue,
        );
        expect(c.text, '$word\n- ');
        expect(c.editor.composing, isFalse);
        expect(c.value.composing, TextRange.empty);
        // The word and Return undo as separate steps either way.
        expect(c.command(const Undo()), isTrue);
        expect(c.text, word);
        expect(c.command(const Undo()), isTrue);
        expect(c.text, source);
        c.dispose();
      });
    }
  }
  test(
    'a composition that ends with the source it began with keeps the platform caret',
    () {
      // Read as a cancel, the platform's caret was ignored and the state
      // before the composition restored: retyping a selected word left it
      // selected, so the next key replaced it, and Gboard finishing a
      // composed word as the caret moved on kept the caret behind.
      const source = 'say cat now';
      final retyped = FlarkController(FlarkEditor(backend, text: source));
      retyped.command(const SetSelection(4, 7));
      for (final (text, end) in [('say c now', 5), ('say cat now', 7)]) {
        retyped.receive(
          TextEditingValue(
            text: text,
            selection: TextSelection.collapsed(offset: end),
            composing: TextRange(start: 4, end: end),
          ),
        );
      }
      retyped.receive(
        const TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: 7),
        ),
      );
      expect(retyped.text, source);
      expect(retyped.editor.selection, const FlarkSelection.collapsed(7));
      expect(retyped.editor.composing, isFalse);
      retyped.dispose();

      final moved = FlarkController(
        FlarkEditor(backend, text: source, caret: 7),
      );
      // Gboard composes the word before the caret, then the caret moves.
      moved.receive(
        const TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: 7),
          composing: TextRange(start: 4, end: 7),
        ),
      );
      expect(moved.editor.composing, isTrue);
      moved.receive(
        const TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      expect(moved.editor.selection, const FlarkSelection.collapsed(1));
      expect(moved.editor.composing, isFalse);
      moved.dispose();
    },
  );
  test(
    'a platform update at the caret keeps it in an unwritten table cell',
    () {
      // An unwritten cell shares its source offset with the end of the cell
      // before it. A value that only set a composing region (Gboard
      // composing the word before its caret) applied those offsets again,
      // which moved the caret into the cell before, where typing then went.
      const source = '| a | b |\n| --- | --- |\n| x |\n';
      final c = FlarkController(FlarkEditor(backend, text: source));
      c.command(SetSelection.caret(source.indexOf('x')));
      c.command(const MoveTableCell());
      final cell = c.editor.selection;
      expect(cell.tableCell, isNotNull);
      final caret = cell.extent;
      c.receive(
        c.value.copyWith(
          composing: TextRange(start: caret - 1, end: caret),
        ),
      );
      expect(c.editor.selection, cell);
      c.receive(c.value.copyWith(composing: TextRange.empty));
      expect(c.editor.selection, cell);
      expect(c.command(const InsertText('Z')), isTrue);
      expect(c.editor.document.caretRow.column, 1);
      expect(c.editor.document.caretRow.text.trim(), 'Z');
      c.dispose();
    },
  );
  test(
    'a platform deletion of half a surrogate pair deletes its character',
    () {
      // iOS deletes one UTF-16 unit before the caret unless its code point is
      // an emoji, so Backspace after a supplementary character that is not
      // one (CJK Extension B, mathematical letters) leaves half of it. That
      // was refused as invalid Unicode: the character could not be deleted.
      for (final character in ['\u{20000}', '\u{1D4B3}']) {
        final source = 'a$character b';
        final c = FlarkController(FlarkEditor(backend, text: source, caret: 3));
        final half = source.replaceRange(2, 3, '');
        expect(
          c.receive(
            TextEditingValue(
              text: half,
              selection: const TextSelection.collapsed(offset: 2),
            ),
          ),
          isTrue,
        );
        expect(c.text, 'a b');
        expect(c.editor.selection, const FlarkSelection.collapsed(1));
        expect(c.notice, isNull);
        c.dispose();
      }
    },
  );
  test(
    'a cancelled composition leaves no trace where typing added a line break',
    () {
      // Typing at the end of an empty fenced block's opening line starts its
      // first body line, and the platform is sent that line break too. An
      // input method that cancels removes only the text it composed; that
      // was read as deleting it, which kept the line break and an undo step.
      final c = FlarkController(FlarkEditor(backend, text: '```\n', caret: 3));
      c.receive(
        const TextEditingValue(
          text: '```t\n',
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange(start: 3, end: 4),
        ),
      );
      expect(c.text, '```\nt\n');
      expect(c.value.composing, const TextRange(start: 4, end: 5));
      c.receive(
        const TextEditingValue(
          text: '```\n\n',
          selection: TextSelection.collapsed(offset: 4),
        ),
      );
      expect(c.text, '```\n');
      expect(c.editor.selection, const FlarkSelection.collapsed(3));
      expect(c.editor.composing, isFalse);
      expect(c.editor.history.canUndo, isFalse);
      c.dispose();
    },
  );
  test(
    'a composition opened by a rejected platform value does not stay open',
    () {
      const source = '# Head\n\npara';
      final c = FlarkController(FlarkEditor(backend, text: source, caret: 4));
      c.command(const SetSelection(4, 11));
      // An input method starts composing over a cross-block selection.
      expect(
        c.receive(
          TextEditingValue(
            text: source.replaceRange(4, 11, 'x'),
            selection: const TextSelection.collapsed(offset: 5),
            composing: const TextRange(start: 4, end: 5),
          ),
        ),
        isFalse,
      );
      expect(c.text, source);
      expect(c.notice, 'This edit needs source mode.');
      // The platform is resynchronized without this composition. Left open,
      // it would make the editor leave every editing key to the input method.
      expect(c.editor.composing, isFalse);
      expect(c.value.composing, TextRange.empty);
      expect(c.editor.history.canUndo, isFalse);
      c.dispose();
    },
  );
}
