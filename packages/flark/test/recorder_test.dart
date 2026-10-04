import 'package:flark/flark.dart';
import 'package:flark/recorder.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  Duration ms(int n) => Duration(milliseconds: n);

  /// What a replay must reproduce: the state, and the history behind it.
  List<(String, int, int, int)> undoTrail(FlarkEditor editor) {
    final trail = [
      (
        editor.source,
        editor.selection.base,
        editor.selection.extent,
        editor.typingContext,
      ),
    ];
    while (editor.apply(const Undo())) {
      trail.add((
        editor.source,
        editor.selection.base,
        editor.selection.extent,
        editor.typingContext,
      ));
    }
    return trail;
  }

  test('a recorded session replays to the same state and history', () {
    final editor = FlarkEditor(
      backend,
      text: '# Notes\n\n- one\n- two\n\nSome text.',
      caret: 7,
    );
    final recorder = FlarkEditRecorder();
    editor.recorder = recorder;
    var t = 0;
    void type(String text) {
      for (final c in text.split('')) {
        editor.apply(InsertText(c), at: ms(t += 80));
      }
    }

    type(' today');
    editor.apply(const DeleteBackward(word: true), at: ms(t += 900));
    type('tomorrow');
    editor.apply(const SetSelection(15, 18), at: ms(t += 900));
    editor.apply(const ToggleStyle(Style.strong), at: ms(t += 50));
    editor.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.line));
    editor.apply(const Newline(), at: ms(t += 900));
    type('three');
    editor.apply(const ToggleTask(), at: ms(t += 900));
    editor.apply(const Indent(), at: ms(t += 900));
    editor.beginComposition();
    editor.apply(const InsertText('k'), at: ms(t += 80));
    editor.apply(const ReplaceRange(37, 38, 'ka'), at: ms(t += 80));
    editor.commitComposition();
    editor.beginComposition();
    editor.apply(const InsertText('x'), at: ms(t += 80));
    editor.cancelComposition();
    editor.apply(const Undo(), at: ms(t += 900));
    editor.apply(const Redo(), at: ms(t += 900));
    editor.applyAfterComposition(
      const SetLink('https://x.y', text: 'link', title: r'a "$" title'),
      at: ms(t += 900),
    );
    editor.selectAll();
    editor.apply(const MoveCaret(MoveDirection.backward), at: ms(t += 900));
    editor.replaceSourceRange(0, 1, '##');
    editor.setSourceMode(true);
    editor.apply(const InsertText('!'), at: ms(t += 900));
    editor.setSourceMode(false);
    editor.apply(const SetHeadingLevel(3), at: ms(t += 900));

    final replayed = recorder.replay(backend);
    expect(undoTrail(replayed), undoTrail(editor));
  });

  test('the repro is the Dart that makes the same calls', () {
    final editor = FlarkEditor(backend, text: r'a $b', caret: 1);
    final recorder = FlarkEditRecorder();
    editor.recorder = recorder;
    editor.apply(const InsertText('x'), at: ms(10));
    editor.apply(const DeleteForward(word: true), at: ms(20));
    editor.beginComposition();
    editor.apply(const InsertText('y'), at: ms(30));
    // Undo commits the composition first; that commit is part of the Undo.
    editor.apply(const Undo(), at: ms(40));
    editor.replaceSourceRange(0, 0, '"');
    expect(recorder.repro, r'''
// Flark repro: 6 calls.
final editor = FlarkEditor(
  createParseBackend(),
  text: "a \$b",
  caret: 1,
);
editor.apply(const InsertText("x"), at: const Duration(microseconds: 10000)); // true
editor.apply(const DeleteForward(word: true), at: const Duration(microseconds: 20000)); // true
editor.beginComposition(); // done
editor.apply(const InsertText("y"), at: const Duration(microseconds: 30000)); // true
editor.apply(const Undo(), at: const Duration(microseconds: 40000)); // true
editor.replaceSourceRange(0, 0, "\""); // true
expect(editor.source, "\"ax");
expect((editor.selection.base, editor.selection.extent), (3, 3));
''');
  });

  test('the repro above runs as written', () {
    // The golden repro of the previous test, pasted as code: it compiles and
    // reproduces the session.
    final editor = FlarkEditor(createParseBackend(), text: "a \$b", caret: 1);
    editor.apply(
      const InsertText("x"),
      at: const Duration(microseconds: 10000),
    ); // true
    editor.apply(
      const DeleteForward(word: true),
      at: const Duration(microseconds: 20000),
    ); // true
    editor.beginComposition(); // done
    editor.apply(
      const InsertText("y"),
      at: const Duration(microseconds: 30000),
    ); // true
    editor.apply(const Undo(), at: const Duration(microseconds: 40000)); // true
    editor.replaceSourceRange(0, 0, "\""); // true
    expect(editor.source, "\"ax");
    expect((editor.selection.base, editor.selection.extent), (3, 3));
  });

  test('a full recorder folds its oldest calls into where it starts', () {
    final editor = FlarkEditor(backend, text: 'start', caret: 5);
    final recorder = FlarkEditRecorder(capacity: 3);
    editor.recorder = recorder;
    for (var i = 0; i < 10; i++) {
      editor.apply(InsertText('$i'), at: ms(i * 1000));
    }
    expect(recorder.length, 3);
    expect(recorder.folded, 7);
    expect(recorder.repro, contains('after 7 earlier ones'));
    expect(recorder.repro, contains('text: "start0123456"'));
    final replayed = recorder.replay(backend);
    expect(replayed.source, editor.source);
    expect(replayed.selection.extent, editor.selection.extent);
    recorder.clear();
    expect(recorder.length, 0);
    expect(recorder.repro, contains('text: "start0123456789"'));
  });

  test('a call refused for a stale revision is noted, not replayed', () {
    final editor = FlarkEditor(backend, text: 'abc', caret: 3);
    final recorder = FlarkEditRecorder();
    editor.recorder = recorder;
    expect(
      editor.apply(
        const InsertText('d'),
        expectedRevision: editor.revision + 1,
      ),
      isFalse,
    );
    editor.apply(const InsertText('e'), at: ms(10));
    expect(recorder.repro, contains('// editor.apply(const InsertText("d")'));
    expect(recorder.repro, contains('// false, stale revision'));
    expect(recorder.replay(backend).source, 'abce');
  });

  test('a recorder serves one editor and stops when detached', () {
    final recorder = FlarkEditRecorder();
    final editor = FlarkEditor(backend, text: 'a', caret: 1)
      ..recorder = recorder;
    expect(() => FlarkEditor(backend)..recorder = recorder, throwsStateError);
    editor.recorder = null;
    editor.apply(const InsertText('b'));
    expect(recorder.length, 0);
    expect(editor.recorder, isNull);
  });

  test('commands are written as the Dart that constructs them', () {
    for (final (command, dart) in [
      (const DeleteBackward(), 'const DeleteBackward()'),
      (const DeleteBackward(word: true), 'const DeleteBackward(word: true)'),
      (const Newline(paragraph: true), 'const Newline(paragraph: true)'),
      (
        const MoveTableCell(backward: true),
        'const MoveTableCell(backward: true)',
      ),
      (
        const MoveCaret(
          MoveDirection.backward,
          unit: MoveUnit.word,
          extend: true,
        ),
        'const MoveCaret(MoveDirection.backward, unit: MoveUnit.word, extend: true)',
      ),
      (
        const PlaceCaret(2, 3, leadingHalf: true),
        'const PlaceCaret(2, 3, leadingHalf: true, extend: false)',
      ),
      (
        const ToggleStyle(Style.strikethrough),
        'const ToggleStyle(Style.strikethrough)',
      ),
      (
        const SetStyle(Style.code, enabled: false),
        'const SetStyle(Style.code, enabled: false)',
      ),
      (const SetCodeLanguage('dart'), 'const SetCodeLanguage("dart")'),
      (const SetLink('u'), 'const SetLink("u")'),
      (
        const SetImage('i.png', alt: 'an \$alt', title: 't'),
        r'const SetImage("i.png", alt: "an \$alt", title: "t")',
      ),
      (const Paste('a\r\nb'), r'const Paste("a\r\nb")'),
    ]) {
      expect(FlarkEditRecorder.describeCommand(command), dart);
    }
  });
}
