@TestOn('browser')
library;

import 'dart:ui_web' as ui_web;
import 'package:flark/wasm.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;

void main() {
  // Flutter's browser test runner suppresses all engine platform messages by
  // default. Enable the real transport while retaining deterministic test fonts.
  ui_web.TestEnvironment.setUp(
    const ui_web.TestEnvironment(
      forceTestFonts: true,
      disableFontFallbacks: true,
      keepSemanticsDisabledOnUpdate: true,
      defaultToTestUrlStrategy: true,
    ),
  );
  final binding = WidgetsFlutterBinding.ensureInitialized();
  late FlarkParseBackend backend;
  setUpAll(() async {
    backend = await WasmParseBackend.load(
      candidates: [
        Uri.base.resolve('/packages/flark/assets/wasm/flark_parse.wasm'),
      ],
    );
  });
  test(
    'browser bold authoring, shortening, spaces and continued typing',
    () async {
      final c = FlarkController(FlarkEditor(backend, text: 'say ', caret: 4));
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      focus.requestFocus();
      await binding.endOfFrame;
      final input = web.document.querySelector('textarea.flt-text-editing')!;
      expect(await pressPrimaryKey(input, 'KeyB', 'b'), isTrue);
      for (final (character, source, visible, caret) in [
        ('w', 'say **w**', 'say w', 7),
        ('h', 'say **wh**', 'say wh', 8),
        ('a', 'say **wha**', 'say wha', 9),
        ('t', 'say **what**', 'say what', 10),
        ('', 'say **wha**', 'say wha', 9),
        (' ', 'say **wha** ', 'say wha ', 12),
        (' ', 'say **wha**  ', 'say wha  ', 13),
        ('x', 'say **wha**  **x**', 'say wha  x', 16),
        ('y', 'say **wha**  **xy**', 'say wha  xy', 17),
      ]) {
        final input =
            web.document.querySelector('textarea.flt-text-editing')!
                as web.HTMLTextAreaElement;
        final insert = character.isNotEmpty;
        final at = input.selectionStart;
        final end = input.selectionEnd;
        final type = insert ? 'insertText' : 'deleteContentBackward';
        input.dispatchEvent(
          web.InputEvent(
            'beforeinput',
            web.InputEventInit(
              bubbles: true,
              cancelable: true,
              inputType: type,
              data: insert ? character : null,
            ),
          ),
        );
        input.value = input.value.replaceRange(
          insert ? at : at - 1,
          end,
          character,
        );
        final next = at + (insert ? character.length : -1);
        input.setSelectionRange(next, next);
        paints.clear();
        input.dispatchEvent(
          web.InputEvent(
            'input',
            web.InputEventInit(
              bubbles: true,
              inputType: type,
              data: insert ? character : null,
            ),
          ),
        );
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(c.text, source);
        expect(c.editor.selection.extent, caret);
        expect(c.editor.typingContext, Style.strong);
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.rows, [visible]);
          expect(paint.styles.single, contains(Style.strong));
          expect(paint.caretSource, caret);
          expect(identical(paint.snapshot, c.editor.snapshot), isTrue);
        }
      }
      expect(c.command(const Undo()), isTrue);
      expect(c.command(const Redo()), isTrue);
      expect(c.text, 'say **wha**  **xy**');
    },
  );
  for (final hasBody in [true, false]) {
    test('browser literal fence paste, existing body: $hasBody', () async {
      final source = '```text\n${hasBody ? 'here\n' : ''}```\n\n# after';
      const expected = '````text\n```\ninside\n````\n\n# after';
      final at = hasBody ? source.indexOf('here') : source.indexOf('\n```') + 4;
      final c = FlarkController(FlarkEditor(backend, text: source, caret: at));
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      focus.requestFocus();
      if (hasBody) c.command(SetSelection(at, at + 4));
      await binding.endOfFrame;
      final selection = c.editor.selection;
      final input = web.document.querySelector('textarea.flt-text-editing')!;
      final data = web.DataTransfer()..setData('text/plain', '```\ninside');
      final event = web.ClipboardEvent(
        'paste',
        web.ClipboardEventInit(
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        ),
      );
      paints.clear();
      final revision = c.editor.revision;
      input.dispatchEvent(event);
      expect(event.defaultPrevented, isTrue);
      expect(c.text, expected);
      expect(c.editor.revision, revision + 1);
      await binding.endOfFrame;
      expect(paints, isNotEmpty);
      for (final paint in paints) {
        expect(identical(paint.snapshot, c.editor.snapshot), isTrue);
        expect(paint.rows, ['```\ninside', '', 'after']);
        expect(paint.caretSource, expected.indexOf('\n````'));
      }
      expect(c.editor.projection.rows.last.kind, RowKind.heading);
      c.command(const InsertText('!'));
      expect(
        c.editor.document.rowAt(c.editor.selection.extent).text,
        '```\ninside!',
      );
      c.command(const Undo());
      expect(c.text, expected);
      c.command(const Undo());
      expect((c.text, c.editor.selection), (source, selection));
    });
  }
  test(
    'browser paste keeps a closer literal and commits exactly once',
    () async {
      const source = '```dart\nvoid main() {\n  ';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
      );
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      focus.requestFocus();
      await binding.endOfFrame;
      // The shortcut must leave the browser's default paste action enabled.
      // A synthetic ClipboardEvent alone cannot detect a swallowed Cmd+V.
      final input = web.document.querySelector('textarea.flt-text-editing')!;
      for (final modifier in ['Meta', 'Control']) {
        expect(
          await pressPrimaryKey(input, 'KeyV', 'v', modifier: modifier),
          isFalse,
          reason: 'the browser must deliver its paste event',
        );
        expect(c.text, source);
      }
      final data = web.DataTransfer()..setData('text/plain', '}');
      final event = web.ClipboardEvent(
        'paste',
        web.ClipboardEventInit(
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        ),
      );
      final revision = c.editor.revision;
      paints.clear();
      input.dispatchEvent(event);
      expect(event.defaultPrevented, isTrue);
      expect(c.text, '$source}');
      expect(c.editor.revision, revision + 1);
      await binding.endOfFrame;
      expect(paints, isNotEmpty);
      for (final paint in paints) {
        expect(paint.rows, ['void main() {\n  }']);
        expect(paint.caretSource, source.length + 1);
      }
      c.command(const InsertText(';'));
      expect(c.text, '$source};');
      c.command(const Undo());
      expect(c.text, '$source}');
      c.command(const Undo());
      expect((c.text, c.editor.selection.extent), (source, source.length));
      runApp(const SizedBox());
      await binding.endOfFrame;
      final detached = web.ClipboardEvent(
        'paste',
        web.ClipboardEventInit(
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        ),
      );
      web.document.dispatchEvent(detached);
      expect(detached.defaultPrevented, isFalse);
    },
  );
  test(
    'browser code selection copies exact body lines and cuts once',
    () async {
      const source = '```dart\none\ntwo\n```';
      final c = FlarkController(FlarkEditor(backend, text: source, caret: 8));
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      focus.requestFocus();
      final input = web.document.querySelector('textarea.flt-text-editing')!;
      expect(await pressPrimaryKey(input, 'KeyA', 'a'), isTrue);
      expect(c.editor.selection, const FlarkSelection(8, 15));
      expect(c.selectedText, 'one\ntwo');
      expect(await pressPrimaryKey(input, 'KeyA', 'a'), isTrue);
      expect(c.editor.selection, const FlarkSelection(0, source.length));
      c.command(const SetSelection(15, 8));
      await binding.endOfFrame;
      for (final action in ['copy', 'cut']) {
        final input = web.document.querySelector('textarea.flt-text-editing')!;
        final value = action == 'copy' ? 'c' : 'x';
        expect(
          await pressPrimaryKey(input, 'Key${value.toUpperCase()}', value),
          isFalse,
        );
        expect(c.text, source);
        final data = web.DataTransfer();
        final event = web.ClipboardEvent(
          action,
          web.ClipboardEventInit(
            clipboardData: data,
            bubbles: true,
            cancelable: true,
          ),
        );
        paints.clear();
        input.dispatchEvent(event);
        expect(event.defaultPrevented, isTrue);
        expect(data.getData('text/plain'), 'one\ntwo');
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(c.text, action == 'copy' ? source : '```dart\n\n```');
        if (action == 'cut') {
          expect(paints, isNotEmpty);
          for (final p in paints) {
            expect(p.rows, ['']);
            expect(p.caretSource, 8);
          }
        }
      }
      c.command(const Undo());
      expect(
        (c.text, c.editor.selection),
        (source, const FlarkSelection(15, 8)),
      );
    },
  );
  test(
    'browser copy and cut export visible text and commit one semantic deletion',
    () async {
      final c = FlarkController(
        FlarkEditor(backend, text: 'one **two** three'),
      );
      final focus = FocusNode();
      final paints = <FlarkPaintObservation>[];
      addTearDown(() async {
        runApp(const SizedBox());
        await binding.endOfFrame;
        c.dispose();
        focus.dispose();
      });
      runApp(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await binding.endOfFrame;
      await Future<void>.delayed(Duration.zero);
      focus.requestFocus();
      await binding.endOfFrame;
      c.command(const SetSelection(11, 6));
      await binding.endOfFrame;
      for (final action in ['copy', 'cut']) {
        final input = web.document.querySelector('textarea.flt-text-editing')!;
        final data = web.DataTransfer();
        final event = web.ClipboardEvent(
          action,
          web.ClipboardEventInit(
            clipboardData: data,
            bubbles: true,
            cancelable: true,
          ),
        );
        paints.clear();
        input.dispatchEvent(event);
        expect(event.defaultPrevented, isTrue);
        expect(data.getData('text/plain'), 'two');
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(c.text, action == 'copy' ? 'one **two** three' : 'one  three');
        if (action == 'cut') {
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.rows, ['one  three']);
            expect(paint.revision, c.editor.revision);
          }
        }
      }
      c.command(const InsertText('x'));
      await binding.endOfFrame;
      expect(c.text, 'one **x** three');
      c.command(const Undo());
      c.command(const Undo());
      await binding.endOfFrame;
      expect(c.text, 'one **two** three');
      expect(c.editor.selection, const FlarkSelection(11, 6));
      // Unmount unregisters the document handlers; unrelated inputs retain the
      // browser's clipboard behavior even if an old controller still exists.
      runApp(const SizedBox());
      await binding.endOfFrame;
      final detachedEvent = web.ClipboardEvent(
        'copy',
        web.ClipboardEventInit(
          clipboardData: web.DataTransfer(),
          bubbles: true,
          cancelable: true,
        ),
      );
      web.document.dispatchEvent(detachedEvent);
      expect(detachedEvent.defaultPrevented, isFalse);
    },
  );
  for (final entry in {
    'long line': 'word ' * 1000,
    'multiple blocks': '# Heading\n\n- first\n- **second**',
  }.entries) {
    test(
      'browser replacement without beforeinput survives next key: ${entry.key}',
      () async {
        // Exercise Flutter's real browser input transport, including its delta
        // inference, rather than TestTextInput's already-decoded Dart messages.
        final c = FlarkController(FlarkEditor(backend, text: entry.value));
        final paints = <FlarkPaintObservation>[];
        final focus = FocusNode();
        addTearDown(() async {
          runApp(const SizedBox());
          await binding.endOfFrame;
          c.dispose();
          focus.dispose();
        });
        runApp(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                focusNode: focus,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await binding.endOfFrame;
        await Future<void>.delayed(Duration.zero);
        focus.requestFocus();
        await binding.endOfFrame;
        c.command(SetSelection(0, c.text.length));
        await binding.endOfFrame;
        await Future<void>.delayed(Duration.zero);
        final input =
            web.document.querySelector('textarea.flt-text-editing')!
                as web.HTMLTextAreaElement;
        expect(input.value, entry.value);
        const pasted = 'Paste probe\n\n**bold** and café 👩🏽‍💻';
        paints.clear();
        input.value = pasted;
        input.setSelectionRange(pasted.length, pasted.length);
        // Browser-driven replacements are not required to provide beforeinput
        // metadata. The resulting DOM value is still authoritative input.
        input.dispatchEvent(
          web.InputEvent('input', web.InputEventInit(bubbles: true)),
        );
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(c.text, pasted);
        expect(c.editor.selection.extent, pasted.length);
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.rows.join('\n'), contains('bold and café'));
          expect(paint.rows.join('\n'), isNot(contains('**')));
          expect(paint.caretSource, pasted.length);
          expect(paint.revision, c.editor.revision);
        }
        final next =
            web.document.querySelector('textarea.flt-text-editing')!
                as web.HTMLTextAreaElement;
        next.value = '$pasted!';
        next.setSelectionRange(pasted.length + 1, pasted.length + 1);
        paints.clear();
        next.dispatchEvent(
          web.InputEvent('input', web.InputEventInit(bubbles: true)),
        );
        await Future<void>.delayed(Duration.zero);
        await binding.endOfFrame;
        expect(c.text, '$pasted!');
        expect(paints, isNotEmpty);
        for (final paint in paints) {
          expect(paint.caretSource, pasted.length + 1);
          expect(paint.revision, c.editor.revision);
        }
      },
    );
  }
}

// Send actual DOM key events through the engine and focused editor. Clipboard
// events are dispatched separately because untrusted keys have no OS default.
Future<bool> pressPrimaryKey(
  web.Element input,
  String code,
  String value, {
  String modifier = 'Meta',
}) async {
  web.KeyboardEvent key(String type, String code, String value, bool pressed) =>
      web.KeyboardEvent(
        type,
        web.KeyboardEventInit(
          code: code,
          key: value,
          metaKey: pressed && modifier == 'Meta',
          ctrlKey: pressed && modifier == 'Control',
          bubbles: true,
          cancelable: true,
        ),
      );
  input.dispatchEvent(key('keydown', '${modifier}Left', modifier, true));
  final shortcut = key('keydown', code, value, true);
  input.dispatchEvent(shortcut);
  input.dispatchEvent(key('keyup', code, value, true));
  input.dispatchEvent(key('keyup', '${modifier}Left', modifier, false));
  await Future<void>.delayed(Duration.zero);
  return shortcut.defaultPrevented;
}
