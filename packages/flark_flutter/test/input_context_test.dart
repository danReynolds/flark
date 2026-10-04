import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/input_context.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

TextEditingValue at(String text, int caret) => TextEditingValue(
  text: text,
  selection: TextSelection.collapsed(offset: caret),
);

void main() {
  test('platform input reports the source ceiling and then recovers', () {
    final c = FlarkController(
      FlarkEditor(
        createParseBackend(),
        text: 'abcd',
        caret: 4,
        syncLimit: 2,
        sourceLimit: 4,
      ),
    );
    expect(c.receive(at('abcde', 5)), isFalse);
    expect(c.text, 'abcd');
    expect(c.notice, 'This document exceeds the writable source limit.');
    c.command(const SetSelection(0, 4));
    expect(c.receive(at('ab', 2)), isTrue);
    expect(c.text, 'ab');
    expect(c.notice, isNull);
    expect(c.receive(at('abc', 3)), isTrue);
    expect(c.text, 'abc');
    c.dispose();
  });
  test('surrounding input maps an edit back into exact full source', () {
    final source = '${'before ' * 300}${'after ' * 300}';
    final input = InputContext.of(at(source, 2100));
    expect(input.value.text.length, lessThanOrEqualTo(514));
    final local = input.value;
    final caret = local.selection.extentOffset;
    final next = at(
      local.text.replaceRange(caret, caret, 'two  words'),
      caret + 10,
    );
    final full = input.expand(next)!;
    expect(full.text, source.replaceRange(2100, 2100, 'two  words'));
    expect(full.selection.extentOffset, 2110);
    final kept = InputContext.of(full, previous: input);
    expect(kept.rebased, isFalse);
    expect(kept.value, next);
  });

  test('context edges preserve complete Unicode graphemes and CRLF', () {
    final source = 'a👩🏽‍💻e\u0301\r\n' * 500;
    final boundaries = <int>{0};
    var offset = 0;
    for (final grapheme in source.characters) {
      offset += grapheme.length;
      boundaries.add(offset);
    }
    for (final caret in boundaries.where((p) => p > 1000 && p < 1200)) {
      final input = InputContext.of(at(source, caret));
      expect(boundaries, contains(input.start));
      expect(boundaries, contains(input.end));
      expect(input.expand(input.value), at(source, caret));
    }
  });

  test('a CRLF document reaches the platform with LF line breaks', () {
    // Browsers keep a textarea's line breaks as LF only, so the platform is
    // sent LF text, and its edits map back into the CRLF source.
    const source = 'alpha\r\n\r\nbeta\r\n';
    final input = InputContext.of(at(source, 13));
    expect(input.value, at('alpha\n\nbeta\n', 11));
    expect(input.expand(input.value), at(source, 13));
    // Text typed at the end of a line goes before its CRLF.
    expect(
      input.expand(at('alpha\n\nbetaX\n', 12)),
      at('alpha\r\n\r\nbetaX\r\n', 14),
    );
    // Deleting a line break deletes all of it.
    expect(input.expand(at('alpha\nbeta\n', 6)), at('alpha\r\nbeta\r\n', 7));
  });

  test('a line break typed at the caret of a CRLF window stays there', () {
    // Matched from either end, a break typed beside another reads as that
    // one. The edit is read at the selection the platform was sent.
    const source = 'alpha\r\nbeta\r\n';
    final input = InputContext.of(at(source, 5));
    expect(
      input.expand(at('alpha\n\nbeta\n', 6)),
      at('alpha\n\r\nbeta\r\n', 6),
    );
  });

  test('a value that only moves a multiline selection keeps its CRLFs', () {
    // Read as a replacement of the selection it was sent, the unchanged
    // selected text came back with LF line breaks and replaced the source.
    const source = 'one\r\ntwo\r\nthree';
    for (final next in [
      const TextSelection.collapsed(offset: 1),
      const TextSelection(baseOffset: 2, extentOffset: 6),
    ]) {
      final input = InputContext.of(
        const TextEditingValue(
          text: source,
          selection: TextSelection(baseOffset: 1, extentOffset: 12),
        ),
      );
      final full = input.expand(input.value.copyWith(selection: next))!;
      expect(full.text, source);
      expect(
        full.selection,
        next.isCollapsed
            ? const TextSelection.collapsed(offset: 1)
            : const TextSelection(baseOffset: 2, extentOffset: 7),
      );
    }
    // A selection replaced by text that keeps its line breaks keeps them
    // as they were spelled.
    final input = InputContext.of(
      const TextEditingValue(
        text: 'a\r\nb\r\nc',
        selection: TextSelection(baseOffset: 0, extentOffset: 7),
      ),
    );
    expect(input.expand(at('a\nX\nc', 3)), at('a\r\nX\r\nc', 4));
  });

  test('a line break deleted beside the caret keeps its own spelling', () {
    // Matched from either end, a deletion in a run of line breaks read as
    // the last of them: with mixed line endings, Backspace after a CRLF
    // deleted a later LF instead, which the host then read as Delete.
    for (final (source, caret, next, full) in [
      ('a\r\n\nb', 3, at('a\nb', 1), at('a\nb', 1)),
      ('a\n\r\nb', 2, at('a\nb', 1), at('a\r\nb', 1)),
      ('a\r\n\nb', 1, at('a\nb', 1), at('a\nb', 1)),
    ]) {
      final input = InputContext.of(at(source, caret));
      expect(input.expand(next), full, reason: '$source at $caret');
    }
  });

  test('wide reverse selection and active composition are never clipped', () {
    final source = 'a' * 5000;
    final selected = TextEditingValue(
      text: source,
      selection: const TextSelection(baseOffset: 4000, extentOffset: 1000),
    );
    final input = InputContext.of(selected);
    expect(input.expand(input.value), selected);
    final local = input.value;
    final replaced = local.copyWith(
      text: local.text.replaceRange(
        local.selection.start,
        local.selection.end,
        'x',
      ),
      selection: TextSelection.collapsed(offset: local.selection.start + 1),
    );
    expect(input.expand(replaced)!.text, '${'a' * 1000}x${'a' * 1000}');
    expect(input.expand(replaced)!.selection.extentOffset, 1001);

    var full = at(
      source,
      2501,
    ).copyWith(composing: const TextRange(start: 2500, end: 2501));
    var composing = InputContext.of(full);
    final start = composing.start;
    for (var i = 0; i < 1100; i++) {
      full = full.copyWith(
        text: full.text.replaceRange(2501 + i, 2501 + i, 'x'),
        selection: TextSelection.collapsed(offset: 2502 + i),
        composing: TextRange(start: 2500, end: 2502 + i),
      );
      composing = InputContext.of(full, previous: composing);
      expect(composing.start, start);
      expect(composing.rebased, isFalse);
      expect(composing.expand(composing.value), full);
    }
  });

  test('a full-value correction before the caret applies there', () {
    // A full value's minimal difference leaves out the unchanged text
    // between a correction and the caret: macOS and iOS turn two typed
    // spaces into ". ", and autocorrect replaces the word before a space
    // already typed. Read as stale replacements away from the selection,
    // both were refused and the platform resynchronized.
    final backend = createParseBackend();
    for (final (source, caret, next, nextCaret) in [
      ('word ', 5, 'word. ', 6),
      ('teh ', 4, 'the ', 4),
      ('teh, ', 5, 'the, ', 5),
      ('say **teh** ', 12, 'say **the** ', 12),
      ('- one two ', 10, '- one two. ', 11),
    ]) {
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: caret),
      );
      expect(c.receive(at(next, nextCaret)), isTrue, reason: source);
      expect(
        (c.text, c.editor.selection, c.notice),
        (next, FlarkSelection.collapsed(nextCaret), null),
        reason: source,
      );
      final typed = next.replaceRange(nextCaret, nextCaret, 'x');
      expect(c.receive(at(typed, nextCaret + 1)), isTrue);
      expect(c.text, typed);
      c.command(const Undo());
      c.command(const Undo());
      expect(
        (c.text, c.editor.selection),
        (source, FlarkSelection.collapsed(caret)),
        reason: source,
      );
      c.dispose();
    }
    // Text on another line is not the caret's correction: a value that
    // changes it is still refused as stale.
    final c = FlarkController(FlarkEditor(backend, text: 'teh\nab', caret: 6));
    expect(c.receive(at('the\nab', 6)), isFalse);
    expect((c.text, c.notice), ('teh\nab', 'Input was resynchronized.'));
    c.dispose();
  });

  test('a cancelled composition over a selection leaves it deleted', () {
    // An input method composing over a selection replaces it; cancelling
    // removes what it composed and leaves the selection deleted, as a
    // platform text field does. The composed backtick closed a code span,
    // whose closer is hidden: removing it by range was refused, so the
    // cancelled text stayed and the composition was kept.
    for (final (source, composed) in [('a `bcd', '`'), ('ab cd', 'か')]) {
      final c = FlarkController(
        FlarkEditor(createParseBackend(), text: source, caret: 0),
      );
      final end = source.length;
      c.command(SetSelection(end - 1, end));
      final kept = source.substring(0, end - 1);
      expect(
        c.receive(
          at('$kept$composed', end).copyWith(
            composing: TextRange(start: end - 1, end: end),
          ),
        ),
        isTrue,
      );
      expect((c.text, c.editor.composing), ('$kept$composed', true));
      expect(
        c.receive(
          at(kept, end - 1).copyWith(composing: TextRange.collapsed(end - 1)),
        ),
        isTrue,
        reason: source,
      );
      expect(
        (c.text, c.editor.selection, c.editor.composing),
        (kept, FlarkSelection.collapsed(end - 1), false),
        reason: source,
      );
      c.command(const Undo());
      expect(
        (c.text, c.editor.selection),
        (source, FlarkSelection(end - 1, end)),
        reason: source,
      );
      c.dispose();
    }
  });

  test('Undo and Redo with no history are not refused edits', () {
    // Command-Z on a fresh document said "This edit needs source mode."
    final c = FlarkController(
      FlarkEditor(createParseBackend(), text: 'abc', caret: 3),
    );
    expect(c.command(const Undo()), isFalse);
    expect(c.notice, isNull);
    expect(c.command(const Redo()), isFalse);
    expect(c.notice, isNull);
    // A refused edit still says so; an empty Undo leaves its notice.
    expect(c.command(const InsertText('\uD800')), isFalse);
    final notice = c.notice;
    expect(notice, isNotNull);
    expect(c.command(const Undo()), isFalse);
    expect(c.notice, notice);
    c.dispose();
  });

  test('a notice stays while the platform composes', () {
    // Taking the notice away moves the document under the composition, and
    // a browser with accessibility on ends its composition when the editor's
    // semantics move: the composed text was committed and the next appended.
    final c = FlarkController(
      FlarkEditor(createParseBackend(), text: 'abc', caret: 0),
    );
    expect(c.command(const InsertText('\uD800')), isFalse);
    final notice = c.notice;
    expect(notice, isNotNull);
    TextEditingValue composing(String text) =>
        at(text, 1).copyWith(composing: const TextRange(start: 0, end: 1));
    expect(c.receive(composing('nabc')), isTrue);
    expect((c.text, c.editor.composing, c.notice), ('nabc', true, notice));
    expect(c.receive(composing('日abc')), isTrue);
    expect((c.text, c.editor.composing, c.notice), ('日abc', true, notice));
    c.receive(at('日abc', 1));
    expect((c.text, c.editor.composing, c.notice), ('日abc', false, null));
    c.dispose();
  });

  test('invalid local ranges cannot edit neighboring source', () {
    final input = InputContext.of(at('a' * 5000, 2500));
    expect(input.expand(at('x', 2)), isNull);
    expect(
      input.expand(
        at('x', 1).copyWith(composing: const TextRange(start: 0, end: 2)),
      ),
      isNull,
    );
  });

  final backend = createParseBackend();
  Future<void> mount(WidgetTester tester, FlarkController c) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
  }

  TextEditingValue remote(WidgetTester tester) =>
      TextEditingValue.fromJSON(tester.testTextInput.editingState!);
  int client(WidgetTester tester) =>
      (tester.testTextInput.log
                      .lastWhere((call) => call.method == 'TextInput.setClient')
                      .arguments
                  as List)
              .first
          as int;
  Future<void> delta(
    WidgetTester tester,
    int clientId,
    TextEditingValue before,
    String inserted,
  ) async {
    final offset = before.selection.extentOffset;
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      SystemChannels.textInput.name,
      SystemChannels.textInput.codec.encodeMethodCall(
        MethodCall('TextInputClient.updateEditingStateWithDeltas', [
          clientId,
          {
            'deltas': [
              {
                'oldText': before.text,
                'deltaText': inserted,
                'deltaStart': offset,
                'deltaEnd': offset,
                'selectionBase': offset + inserted.length,
                'selectionExtent': offset + inserted.length,
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
  }

  testWidgets(
    'platform deltas edit full source without echoing accepted input',
    (tester) async {
      final source = 'a' * 4000;
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 2000),
      );
      await mount(tester, c);
      final before = remote(tester), id = client(tester);
      expect(before.text.length, lessThan(1024));
      tester.testTextInput.log.clear();
      await delta(tester, id, before, 'x');
      expect(c.text, source.replaceRange(2000, 2000, 'x'));
      expect(c.editor.selection.extent, 2001);
      await tester.pump();
      expect(
        tester.testTextInput.log.where(
          (call) => call.method == 'TextInput.setEditingState',
        ),
        isEmpty,
      );
      await delta(tester, id, before, 'stale');
      expect(c.text, source.replaceRange(2000, 2000, 'x'));
      expect(c.editor.selection.extent, 2001);
      expect(
        remote(tester).text,
        before.text.replaceRange(
          before.selection.extentOffset,
          before.selection.extentOffset,
          'x',
        ),
      );
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets('typing in a CRLF document edits its source', (tester) async {
    const source = 'alpha\r\n\r\nbeta\r\n';
    final c = FlarkController(FlarkEditor(backend, text: source, caret: 13));
    await mount(tester, c);
    final before = remote(tester);
    expect(before, at('alpha\n\nbeta\n', 11));
    await delta(tester, client(tester), before, 'X');
    expect(c.text, 'alpha\r\n\r\nbetaX\r\n');
    expect(c.editor.selection.extent, 14);
    expect(c.notice, isNull);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets('platform Return and Backspace edit a CRLF document as keys do', (
    tester,
  ) async {
    // iOS delivers Return and Backspace as edits of the LF text it holds.
    // Each must do what the kernel's own command does at the caret.
    for (final (source, caret, FlarkCommand command) in [
      ('alpha\r\nbeta\r\n', 5, const Newline()),
      ('- one\r\n- two\r\n', 5, const Newline()),
      ('one\r\n\r\ntwo', 7, const DeleteBackward()),
    ]) {
      final expected = FlarkEditor(backend, text: source, caret: caret)
        ..apply(command);
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: caret),
      );
      await mount(tester, c);
      final before = remote(tester);
      final at = before.selection.extentOffset;
      tester.testTextInput.updateEditingValue(
        command is Newline
            ? TextEditingValue(
                text: before.text.replaceRange(at, at, '\n'),
                selection: TextSelection.collapsed(offset: at + 1),
              )
            : TextEditingValue(
                text: before.text.replaceRange(at - 1, at, ''),
                selection: TextSelection.collapsed(offset: at - 1),
              ),
      );
      expect(c.text, expected.source, reason: source);
      expect(c.editor.selection, expected.selection, reason: source);
      expect(c.notice, isNull);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    }
  });

  testWidgets(
    'windowed input resynchronizes a formatting correction before the next key',
    (tester) async {
      final prefix = '${'p' * 1500}\n\n';
      final suffix = '\n\n${'q' * 1500}';
      final c = FlarkController(
        FlarkEditor(
          backend,
          text: '${prefix}a *x* b$suffix',
          caret: prefix.length + 4,
        ),
      );
      await mount(tester, c);
      final before = remote(tester),
          caret = remote(tester).selection.extentOffset;
      tester.testTextInput.updateEditingValue(
        at(before.text.replaceRange(caret - 1, caret, ''), caret - 1),
      );
      expect(c.text, '${prefix}a  b$suffix');
      final corrected = remote(tester);
      tester.testTextInput.updateEditingValue(
        at(
          corrected.text.replaceRange(
            corrected.selection.extentOffset,
            corrected.selection.extentOffset,
            'y',
          ),
          corrected.selection.extentOffset + 1,
        ),
      );
      expect(c.text, '${prefix}a *y* b$suffix');
      expect(c.editor.selection.extent, prefix.length + 4);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'rebasing invalidates queued input from the old local coordinates',
    (tester) async {
      final source = '${'a' * 2000}\n\n${'b' * 2000}';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 1000),
      );
      await mount(tester, c);
      final before = remote(tester), oldClient = client(tester);
      c.command(const SetSelection.caret(3000));
      await tester.pump();
      expect(client(tester), isNot(oldClient));
      await delta(tester, oldClient, before, 'stale');
      expect(c.text, source);
      await delta(tester, client(tester), remote(tester), 'next');
      expect(c.text, source.replaceRange(3000, 3000, 'next'));
      expect(c.editor.selection.extent, 3004);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'continued Unicode typing crosses input contexts without losing the next key',
    (tester) async {
      final source = '${'a' * 1500}\n\n${'b' * 1500}';
      // One undo step for the whole run, however long each keystroke takes
      // on a loaded machine: history coalesces typing within a second.
      final c = FlarkController(
        FlarkEditor(
          backend,
          text: source,
          caret: 750,
          clock: () => Duration.zero,
        ),
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
      var expected = source, caret = 750;
      var platform = remote(tester);
      final firstClient = client(tester);
      for (final character in (' delta  👩🏽‍💻' * 50).characters) {
        final before = platform, offset = platform.selection.extentOffset;
        paints.clear();
        platform = at(
          before.text.replaceRange(offset, offset, character),
          offset + character.length,
        );
        final logStart = tester.testTextInput.log.length;
        tester.testTextInput.updateEditingValue(platform);
        // The stub records outgoing corrections only. A real platform already
        // holds the value it sent, even when the client correctly avoids echo.
        for (final call in tester.testTextInput.log.skip(logStart)) {
          if (call.method == 'TextInput.setEditingState') {
            platform = TextEditingValue.fromJSON(
              Map<String, dynamic>.from(call.arguments as Map),
            );
          }
        }
        expected = expected.replaceRange(caret, caret, character);
        caret += character.length;
        expect(c.text, expected);
        expect(c.editor.selection.extent, caret);
        expect(c.editor.sourceMode, isFalse);
        await tester.pump();
        expect(paints, isNotEmpty);
        expect(paints.last.caretSource, caret);
        expect(paints.last.snapshot.source, expected);
      }
      expect(client(tester), isNot(firstClient));
      c.command(const Undo());
      expect(c.text, source);
      expect(c.editor.selection.extent, 750);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'windowed composition commits and undoes as one exact source edit',
    (tester) async {
      final source = 'a' * 4000;
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 2000),
      );
      await mount(tester, c);
      final before = remote(tester),
          caret = remote(tester).selection.extentOffset;
      final composing = before.copyWith(
        text: before.text.replaceRange(caret, caret, 'é'),
        selection: TextSelection.collapsed(offset: caret + 1),
        composing: TextRange(start: caret, end: caret + 1),
      );
      tester.testTextInput.updateEditingValue(composing);
      expect(c.value.composing, const TextRange(start: 2000, end: 2001));
      tester.testTextInput.updateEditingValue(
        composing.copyWith(composing: TextRange.empty),
      );
      expect(c.text, source.replaceRange(2000, 2000, 'é'));
      expect(c.value.composing, TextRange.empty);
      c.command(const Undo());
      expect(c.text, source);
      expect(c.editor.selection.extent, 2000);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  // One platform delta batch: each edit replaces [start, end) of the text
  // before it with [text] and leaves the selection [base, extent].
  Future<void> deltas(
    WidgetTester tester,
    List<(String, int, int, String, int, int)> edits,
  ) => tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    SystemChannels.textInput.name,
    SystemChannels.textInput.codec.encodeMethodCall(
      MethodCall('TextInputClient.updateEditingStateWithDeltas', [
        client(tester),
        {
          'deltas': [
            for (final (old, start, end, text, base, extent) in edits)
              {
                'oldText': old,
                'deltaText': text,
                'deltaStart': start,
                'deltaEnd': end,
                'selectionBase': base,
                'selectionExtent': extent,
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

  testWidgets(
    'moving the platform selection out of a multiline CRLF selection keeps the document',
    (tester) async {
      // Gboard's cursor control or a browser's caret reports a new selection
      // over the same text. It replaced the selected lines with their LF
      // spelling, or was refused as a cross-block edit and left the
      // selection where it was.
      const source = '- one\r\n- two\r\n\r\nend';
      final c = FlarkController(FlarkEditor(backend, text: source));
      await mount(tester, c);
      c.command(const SetSelection(2, 12));
      await tester.pump();
      final before = remote(tester);
      expect(
        before.selection,
        const TextSelection(baseOffset: 2, extentOffset: 11),
      );
      await deltas(tester, [(before.text, -1, -1, '', 4, 4)]);
      expect(c.text, source);
      expect(c.editor.selection, const FlarkSelection.collapsed(4));
      expect(c.notice, isNull);
      expect(c.editor.history.canUndo, isFalse);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'a Return the editor handled is not typed again by the newline action',
    (tester) async {
      // A browser's textarea sends the newline action from its own keydown
      // listener after Flutter handled the Return key: every Return made
      // two line breaks, so a list item continued and then ended.
      final c = FlarkController(FlarkEditor(backend, text: '- one', caret: 5));
      await mount(tester, c);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.testTextInput.receiveAction(TextInputAction.newline);
      expect(c.text, '- one\n- ');
      expect(c.editor.selection, const FlarkSelection.collapsed(8));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'an iOS Return keeps the autocorrection it sends and continues the list',
    (tester) async {
      // UIKit sends the newline action, then the line break it inserted,
      // after any autocorrection of the word before it, in one delta batch.
      for (final (source, edits) in [
        ('- one', [('- one', 5, 5, '\n', 6, 6)]),
        ('- teh', [('- teh', 2, 5, 'the', 5, 5), ('- the', 5, 5, '\n', 6, 6)]),
      ]) {
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: source.length),
        );
        await mount(tester, c);
        await tester.testTextInput.receiveAction(TextInputAction.newline);
        await deltas(tester, edits);
        expect(c.text, '- ${source == '- one' ? 'one' : 'the'}\n- ');
        expect(c.editor.selection, const FlarkSelection.collapsed(8));
        expect(remote(tester).text, c.text);
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      }
    },
  );

  testWidgets(
    'platform Backspace after a CRLF in mixed line endings deletes that break',
    (tester) async {
      // A CRLF note with an LF paste: the deletion read as a later LF away
      // from the caret, so Backspace deleted forward or was refused.
      for (final (source, caret) in [('a\r\n\nb', 3), ('one\r\n\n\ntwo', 5)]) {
        final expected = FlarkEditor(backend, text: source, caret: caret)
          ..apply(const DeleteBackward());
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: caret),
        );
        await mount(tester, c);
        final platform = remote(tester);
        final at = platform.selection.extentOffset;
        await deltas(tester, [(platform.text, at - 1, at, '', at - 1, at - 1)]);
        expect(
          (c.text, c.editor.selection),
          (expected.source, expected.selection),
          reason: source,
        );
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      }
    },
  );

  testWidgets('an edit that repeats an earlier platform value is applied', (
    tester,
  ) async {
    // Return before a space begins a line whose leading space is hidden,
    // and the caret moves past it. Backspace there deletes the line break
    // the kernel's way, not the space the platform deleted. The next
    // Return then sent the value that Backspace had, at the same revision,
    // and it was dropped as a duplicate.
    final c = FlarkController(FlarkEditor(backend, text: 'one two', caret: 3));
    await mount(tester, c);
    await deltas(tester, [('one two', 3, 3, '\n', 4, 4)]);
    expect((c.text, c.editor.selection.extent), ('one\n two', 5));
    await deltas(tester, [('one\n two', 4, 5, '', 4, 4)]);
    expect((c.text, c.editor.selection.extent), ('onetwo', 3));
    await deltas(tester, [('onetwo', 3, 3, '\n', 4, 4)]);
    expect((c.text, c.editor.selection.extent), ('one\ntwo', 4));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets(
    'an input method correction away from the caret applies and keeps the caret',
    (tester) async {
      // Autocorrect replaces the word before a space the platform already
      // delivered. The delta's old text authenticates it; it was refused as
      // a stale replacement, and the caret must not jump back over the space.
      final c = FlarkController(FlarkEditor(backend, text: 'teh ', caret: 4));
      await mount(tester, c);
      await deltas(tester, [('teh ', 0, 3, 'the', 4, 4)]);
      expect(c.text, 'the ');
      expect(c.editor.selection, const FlarkSelection.collapsed(4));
      expect(c.notice, isNull);
      await deltas(tester, [('the ', 4, 4, 'x', 5, 5)]);
      expect(c.text, 'the x');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'a full-value double-space period and autocorrect apply before the caret',
    (tester) async {
      // A browser reports its textarea's whole value. The difference of the
      // period that replaces the space before the caret, or of the word
      // corrected before a typed space, ends before the caret; it was
      // refused as a stale value and the platform resynchronized.
      for (final (source, caret, next, nextCaret) in [
        ('word ', 5, 'word. ', 6),
        ('teh ', 4, 'the ', 4),
      ]) {
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: caret),
        );
        await mount(tester, c);
        tester.testTextInput.log.clear();
        tester.testTextInput.updateEditingValue(at(next, nextCaret));
        await tester.pump();
        expect(
          (c.text, c.editor.selection, c.notice),
          (next, FlarkSelection.collapsed(nextCaret), null),
        );
        // The platform already holds the value; it is not resynchronized.
        expect(
          tester.testTextInput.log.map((call) => call.method),
          isNot(contains('TextInput.setEditingState')),
        );
        tester.testTextInput.updateEditingValue(
          at('${next}x', next.length + 1),
        );
        await tester.pump();
        expect(c.text, '${next}x');
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      }
    },
  );
}
