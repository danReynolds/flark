/// Random editing sequences in fenced code with this package's delegate.
///
/// The kernel's matrix runs without a code delegate, so it never reaches the
/// paths a host with one takes: Enter's indentation and exit from code, typed
/// closers, Tab and Shift-Tab over lines and language changes. A quarter of
/// the sequences edit without one, through the kernel's own code paths. After
/// every command the projection invariants hold and the selection is legal; a
/// refused command leaves no trace; an edit that opens a history step undoes
/// to the exact prior source and selection and redoes to its result. An edit
/// inside a code body keeps that block where it was, so the source around it
/// is unchanged and fence characters typed, pasted or deleted in the body stay
/// code, and a paste into a closed fence shows exactly the pasted text in
/// place of the selection, with the caret after it.
///
/// FLARK_CODE_MATRIX_SEED and FLARK_CODE_MATRIX_ITERATIONS widen a run. A
/// failure prints the seed and command log so it can be minimized into a
/// directly named regression.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:test/test.dart';

import '../../flark/test/support/invariants.dart';

const _documents = [
  '```js\nfunction f() {\n  return 1;\n}\n```\n\nafter',
  '> ```py\n> def f():\n>     pass\n> ```\n',
  '- a\n\n  ```\n  x\n  ```\n- b\n',
  '```\n```\n',
  '~~~\n~~~',
  '\t```\n\tx\n\t```\n',
  '1. ```dart\n   void main() {}\n   ```\n',
  '```json\n{"a": [1, 2]}\n```',
  '> - ```\n>   if (x) {\n>   }\n>   ```\n',
  'p\n\n```\n\n```\n',
  '```\nunclosed {\n',
  // Fence characters in a body, beside a delimiter run the edit can extend.
  '```\n``x`\n~~~\n```\n',
  '~~~text\n~~\n```\n~~~\n\nafter\n',
  '```go\nfunc f() {\n}\n```\n',
];

const _typed = [
  // What indentation and typed closers react to.
  '{', '}', '(', ')', '[', ']', ':', ' ', '\t', '"', '/', '*',
  // Fence characters, and plain and wide text.
  '`', '~', 'a', 'x', '😀', 'é',
];

const _pasted = ['x\ny', '  z\n', '```', '~~~\n', 'a\r\nb', '}\n', '`'];

FlarkCommand _command(Random r, FlarkEditor e) {
  final k = r.nextInt(100);
  final length = e.source.length;
  if (k < 35) return InsertText(_typed[r.nextInt(_typed.length)]);
  if (k < 47) return Newline(paragraph: r.nextInt(4) == 0);
  if (k < 55) return DeleteBackward(word: r.nextInt(5) == 0);
  if (k < 59) return DeleteForward(word: r.nextInt(5) == 0);
  if (k < 65) return r.nextBool() ? const Indent() : const Outdent();
  if (k < 70) return Paste(_pasted[r.nextInt(_pasted.length)]);
  if (k < 73) {
    final start = r.nextInt(length + 1);
    return ReplaceRange(
      start,
      min(length, start + r.nextInt(4)),
      ['', '`', 'q', '\n'][r.nextInt(4)],
    );
  }
  if (k < 81) {
    return MoveCaret(
      r.nextBool() ? MoveDirection.forward : MoveDirection.backward,
      unit: MoveUnit.values[r.nextInt(MoveUnit.values.length)],
      extend: r.nextInt(3) == 0,
    );
  }
  if (k < 86) {
    final start = r.nextInt(length + 1);
    return r.nextBool()
        ? SetSelection.caret(start)
        : SetSelection(start, r.nextInt(length + 1));
  }
  if (k < 88) return const SelectAll();
  if (k < 91) {
    return SetCodeLanguage(['', 'text', 'python', 'js', 'dart'][r.nextInt(5)]);
  }
  if (k < 94) return const Undo();
  if (k < 97) return const Redo();
  final rows = e.projection.rows;
  final row = r.nextInt(rows.length);
  return PlaceCaret(row, r.nextInt(rows[row].text.length + 1));
}

String _describe(FlarkCommand command) => switch (command) {
  InsertText(:final text) => 'InsertText(${jsonEncode(text)})',
  Paste(:final text) => 'Paste(${jsonEncode(text)})',
  Newline(:final paragraph) => 'Newline(paragraph: $paragraph)',
  DeleteBackward(:final word) => 'DeleteBackward(word: $word)',
  DeleteForward(:final word) => 'DeleteForward(word: $word)',
  ReplaceRange(:final start, :final end, :final text) =>
    'ReplaceRange($start, $end, ${jsonEncode(text)})',
  SetSelection(:final base, :final extent) => 'SetSelection($base, $extent)',
  MoveCaret(:final direction, :final unit, :final extend) =>
    'MoveCaret(${direction.name}, unit: ${unit.name}, extend: $extend)',
  PlaceCaret(:final row, :final offset) => 'PlaceCaret($row, $offset)',
  SetCodeLanguage(:final language) =>
    'SetCodeLanguage(${jsonEncode(language)})',
  _ => '${command.runtimeType}()',
};

void _checkStep(FlarkEditor editor, String label) {
  if (editor.sourceMode) return;
  final doc = editor.document;
  final whole =
      !doc.selection.isCollapsed &&
      doc.selection.start == 0 &&
      doc.selection.end == doc.source.length;
  expect(
    whole || doc.isLegal(doc.selection.base),
    isTrue,
    reason: '$label: base ${doc.selection.base} legal',
  );
  expect(
    whole || doc.isLegal(doc.selection.extent),
    isTrue,
    reason: '$label: extent ${doc.selection.extent} legal',
  );
  checkInvariants(doc.source, doc.model, doc.projection, label);
}

/// The fenced code block whose body the command edits, as its source range
/// and flags, or null when the command is not an edit inside one body. Return
/// may leave code by design, and Backspace at a body's start or Delete at its
/// end joins the row beside it, so those are not edits inside the body.
(int, int, int)? _editedBody(FlarkEditor editor, FlarkCommand command) {
  if (editor.sourceMode) return null;
  final doc = editor.document;
  var (base, extent) = (doc.selection.base, doc.selection.extent);
  switch (command) {
    case InsertText() || Paste() || Indent() || Outdent():
    case Newline(paragraph: true):
      break;
    case ReplaceRange(:final start, :final end):
      (base, extent) = (start, end);
    case DeleteBackward() || DeleteForward():
      if (doc.selection.isCollapsed) {
        final at = doc.displayOf(extent);
        final edge = command is DeleteBackward
            ? at.offset == 0
            : at.offset == doc.projection.rows[at.row].text.length;
        if (edge) return null;
      }
    default:
      return null;
  }
  // A range over the whole source replaces the document.
  if (min(base, extent) == 0 &&
      max(base, extent) == doc.source.length &&
      base != extent) {
    return null;
  }
  final row = doc.rowAt(extent);
  if (!row.fenced ||
      doc.rowAt(base).index != row.index ||
      row.contentStarts.every((start) => start < 0)) {
    return null;
  }
  final block = doc.model.blockAt(row.block);
  return (block.startUtf16, block.endUtf16, block.flags);
}

void _checkBodyKept(
  String before,
  (int, int, int) body,
  FlarkEditor editor,
  String label,
) {
  final (start, end, flags) = body;
  final after = editor.source, delta = after.length - before.length;
  expect(
    after.substring(0, start),
    before.substring(0, start),
    reason: '$label: an edit in code changed the source before its block',
  );
  expect(
    after.substring(end + delta),
    before.substring(end),
    reason: '$label: an edit in code changed the source after its block',
  );
  // An unclosed fence runs to the end of its container, wherever the edit
  // leaves that; a closed one ends at its closing fence.
  expect(
    editor.sourceMode ||
        editor.document.model.blocks.any(
          (block) =>
              block.kind == BlockKind.codeBlock &&
              block.flags == flags &&
              block.startUtf16 == start &&
              (flags & BlockFlag.closed == 0 || block.endUtf16 == end + delta),
        ),
    isTrue,
    reason:
        '$label: an edit in code moved its block\'s bounds, '
        '${jsonEncode(before)} -> ${jsonEncode(after)}',
  );
}

/// The body and caret a paste into code leaves: the text in place of the
/// selection, with its line breaks as the body shows them.
(String, int) _pastedBody(FlarkEditor editor, String text) {
  final doc = editor.document;
  final row = doc.rowAt(doc.selection.extent);
  final start = row.displayForSource(doc.selection.start).$1;
  final end = row.displayForSource(doc.selection.end).$1;
  final literal = text.replaceAll('\r\n', '\n');
  return (row.text.replaceRange(start, end, literal), start + literal.length);
}

void main() {
  final backend = createParseBackend();
  final code = FlarkCodeMirror();
  final environment = Platform.environment;
  final iterations =
      int.tryParse(environment['FLARK_CODE_MATRIX_ITERATIONS'] ?? '') ?? 500;
  final seed =
      int.tryParse(environment['FLARK_CODE_MATRIX_SEED'] ?? '') ?? 2026;
  test('random code editing keeps invariants, history and code blocks '
      '(seed $seed, $iterations sequences)', () {
    final master = Random(seed);
    for (var i = 0; i < iterations; i++) {
      final s = master.nextInt(1 << 30);
      final r = Random(s);
      var source = _documents[r.nextInt(_documents.length)];
      if (r.nextBool()) source = source.replaceAll('\n', '\r\n');
      final editor = FlarkEditor(
        backend,
        text: source,
        caret: r.nextInt(source.length + 1),
        codeEditing: r.nextInt(4) == 0 ? null : code,
      );
      final log = <String>[];
      var time = Duration.zero;
      try {
        _checkStep(editor, 'seed $s load');
        for (var step = 0; step < 40; step++) {
          final command = _command(r, editor);
          log.add(_describe(command));
          final label = 'seed $s step $step ${_describe(command)}';
          final before = (
            source: editor.source,
            selection: editor.selection,
            revision: editor.revision,
            undo: editor.history.undoTarget,
          );
          final body = _editedBody(editor, command);
          // At the end of an unclosed fence, a pasted last line break ends
          // the document instead of opening an empty line of code.
          final pasted = command is Paste && body != null && body.$3 & 2 != 0
              ? _pastedBody(editor, command.text)
              : null;
          time += const Duration(seconds: 2);
          final applied = editor.apply(command, at: time);
          _checkStep(editor, label);
          if (!applied) {
            expect(editor.source, before.source, reason: '$label: refused');
            expect(
              editor.selection,
              before.selection,
              reason: '$label: refused',
            );
            expect(editor.revision, before.revision, reason: '$label: refused');
            continue;
          }
          if (body != null) {
            _checkBodyKept(before.source, body, editor, label);
          }
          if (pasted != null && !editor.sourceMode) {
            final at = editor.document.displayOf(editor.selection.extent);
            expect(
              (editor.projection.rows[at.row].text, at.offset),
              pasted,
              reason: '$label: pasted code',
            );
          }
          // A command that recorded no undo step (one that moved the
          // selection or set a pending style) leaves the step to undo as it
          // was.
          if (command is Undo ||
              command is Redo ||
              identical(editor.history.undoTarget, before.undo)) {
            continue;
          }
          final after = (source: editor.source, selection: editor.selection);
          time += const Duration(seconds: 5);
          expect(editor.apply(const Undo(), at: time), isTrue);
          _checkStep(editor, '$label undo');
          expect(editor.source, before.source, reason: '$label: undo');
          expect(editor.selection, before.selection, reason: '$label: undo');
          expect(editor.apply(const Redo(), at: time), isTrue);
          _checkStep(editor, '$label redo');
          expect(editor.source, after.source, reason: '$label: redo');
          expect(editor.selection, after.selection, reason: '$label: redo');
        }
      } catch (error) {
        // ignore: avoid_print
        print(
          'code edit failure: seed $s source ${jsonEncode(source)}\n'
          'result ${jsonEncode(editor.source)} selection ${editor.selection}\n'
          '  ${log.join('\n  ')}',
        );
        rethrow;
      }
    }
  });
}
