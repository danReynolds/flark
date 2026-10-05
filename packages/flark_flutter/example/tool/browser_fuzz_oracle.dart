// The Dart VM half of tool/browser_fuzz.mjs: replays the logical edits of a
// browser sequence on a FlarkEditor and compares the result after every step
// with what the page showed. See the README's "Browser fuzzing" section.
//
// Protocol: one JSON request per stdin line, one JSON reply per stdout line.
//   {"presets": true}  -> {"presets": {name: source}}
//   {"sequence": {...}} -> {"id": ..., "ok": bool, ...first mismatch}
import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_dogfood/presets.dart';
import 'package:flark_dogfood/qualification.dart';

final _backend = createParseBackend();
final _code = FlarkCodeMirror();

Future<void> main() async {
  await for (final line
      in stdin.transform(utf8.decoder).transform(const LineSplitter())) {
    if (line.trim().isEmpty) continue;
    final request = jsonDecode(line) as Map<String, Object?>;
    Object? reply;
    try {
      if (request['presets'] == true) {
        reply = {'presets': presets};
      } else {
        reply = check(request['sequence']! as Map<String, Object?>);
      }
    } catch (error, stack) {
      reply = {'ok': false, 'why': 'oracle error', 'detail': '$error\n$stack'};
    }
    stdout.writeln(jsonEncode(reply));
  }
}

/// Checks a sequence, trying the other direction of each selection the page
/// showed without one (a textarea keeps only a range) when a guess fails.
Map<String, Object?> check(Map<String, Object?> sequence) {
  final first = _Replay(sequence, const {}).run();
  if (first.ok || first.ambiguous.isEmpty) return first.report(sequence);
  // Flip the guesses made before the mismatch, fewest first.
  final points = first.ambiguous.where((a) => a < first.step).toList();
  final tries = <Set<int>>[
    for (var size = 1; size <= points.length && size <= 3; size++)
      ..._subsets(points, size),
  ];
  for (final flips in tries.take(64)) {
    final retry = _Replay(sequence, flips).run();
    if (retry.ok) return retry.report(sequence);
  }
  return first.report(sequence);
}

Iterable<Set<int>> _subsets(List<int> items, int size, [int from = 0]) sync* {
  if (size == 0) {
    yield <int>{};
    return;
  }
  for (var i = from; i <= items.length - size; i++) {
    for (final rest in _subsets(items, size - 1, i + 1)) {
      yield {items[i], ...rest};
    }
  }
}

final class _Replay {
  _Replay(this.sequence, this.flips);
  final Map<String, Object?> sequence;
  final Set<int> flips;

  late FlarkEditor editor;
  Duration _now = Duration.zero;
  String clipboard = '';
  final ambiguous = <int>[];
  bool ok = true;
  int step = -1;

  /// The last steps' events, with the state the oracle and the page had.
  final trace = <Map<String, Object?>>[];

  /// The step from which the page cannot be predicted, and why: a caret in
  /// a windowed document whose text repeats, or a known issue. Only errors
  /// are checked after it.
  (int, String)? unchecked;
  String? why;
  Map<String, Object?>? expected, observed;
  String? event;

  _Replay run() {
    final doc = sequence['doc']! as String;
    clipboard = (sequence['clipboard'] as String?) ?? '';
    editor = _open(doc);
    final initial = sequence['initial'] as Map<String, Object?>?;
    if (initial != null && !_compare(-1, initial)) return this;
    final events = (sequence['events']! as List).cast<Map<String, Object?>>();
    for (var i = 0; i < events.length; i++) {
      final e = events[i];
      event = jsonEncode(e['event']);
      final obs = e['obs'] as Map<String, Object?>?;
      try {
        _apply(i, e['event']! as Map<String, Object?>, e, obs);
      } catch (error) {
        return _fail(i, 'oracle threw: $error', obs);
      }
      trace.add({
        'step': i,
        'event': e['event'],
        'expected': '${jsonEncode(editor.source)} ${editor.selection}',
        'observed': obs == null
            ? null
            : '${jsonEncode(obs['saved'])} ${obs['start']}..${obs['end']}',
      });
      if (trace.length > 6) trace.removeAt(0);
      if (obs == null) continue;
      if (!_compare(i, obs)) return this;
    }
    return this;
  }

  /// The workbench's editor over [text].
  FlarkEditor _open(String text) => FlarkEditor(
    _backend,
    text: text,
    caret: 0,
    codeEditing: _code,
    syncLimit: candidateLiveBytes,
    liveLimits: candidateLiveLimits,
    sourceLimit: candidateSourceBytes,
    clock: () => _now,
  );

  /// The toolbar's buttons, as the editor presses them.
  void _toolbar(String button) {
    final style = switch (button) {
      'Bold' => Style.strong,
      'Italic' => Style.emphasis,
      'Strikethrough' => Style.strikethrough,
      'Inline code' => Style.code,
      _ => null,
    };
    if (style != null) {
      final state = editor.styleState(style);
      if (state.canToggle) {
        editor.apply(SetStyle(style, enabled: !state.isOn));
      }
    } else if (button == 'Undo') {
      editor.apply(const Undo());
    } else if (button == 'Redo') {
      editor.apply(const Redo());
    } else if (button == 'Source') {
      editor.setSourceMode(!editor.sourceMode);
    }
  }

  _Replay _fail(int at, String reason, Map<String, Object?>? obs) {
    ok = false;
    step = at;
    why = reason;
    expected = _state();
    observed = obs;
    return this;
  }

  Map<String, Object?> _state() => {
    'source': editor.source,
    'base': editor.selection.base,
    'extent': editor.selection.extent,
  };

  Map<String, Object?> report(Map<String, Object?> sequence) => {
    'id': sequence['id'],
    'ok': ok,
    if (ok) 'final': _state(),
    if (unchecked != null)
      'unchecked': {'step': unchecked!.$1, 'why': unchecked!.$2},
    if (!ok) ...{
      'step': step,
      'why': why,
      'event': event,
      'expected': expected,
      'observed': observed,
      'trace': trace,
    },
  };

  void _at(Map<String, Object?> e, [int index = 0]) {
    final times = e['t'];
    final ms = times is List
        ? (times[index.clamp(0, times.length - 1)] as num)
        : (times as num? ?? 0);
    _now = Duration(microseconds: (ms * 1000).round());
  }

  void _apply(
    int i,
    Map<String, Object?> event,
    Map<String, Object?> e,
    Map<String, Object?>? obs,
  ) {
    _at(e);
    final kind = event['k'];
    final shift = event['shift'] == true,
        alt = event['alt'] == true,
        meta = event['meta'] == true,
        ctrl = event['ctrl'] == true;
    switch (kind) {
      case 'type':
        // One key per grapheme, as the driver typed them.
        final keys = (event['keys']! as List).cast<String>();
        for (var n = 0; n < keys.length; n++) {
          _at(e, n);
          editor.apply(InsertText(keys[n]));
        }
      case 'insert':
        final text = event['text']! as String;
        editor.apply(text == '\n' ? const Newline() : InsertText(text));
      case 'ime':
        final end = event['end'];
        final commit = event['commit'] as String?;
        // The kernel composes text as the input element holds it and types
        // it when the composition commits. A browser drops a composition
        // whose text the page changes under it: one the kernel refuses, as
        // typing its text there is refused, or that code reshapes (an empty
        // fence's first body line). Its next update, or its commit, then
        // lands elsewhere, so the page is not predicted from there.
        final steps = (event['steps']! as List).cast<String>();
        final before = editor.source, s = editor.selection;
        final where = editor.sourceMode
            ? 'source'
            : !s.isCollapsed
            ? 'over a selection'
            : editor.document.caretRow.kind == RowKind.tableCell
            ? 'in a table cell'
            : editor.typingContext != editor.document.typingContextAt(s.extent)
            ? 'with a pending style'
            : editor.document.caretRow.kind == RowKind.codeBlock
            ? 'in code'
            : 'in text';
        editor.beginComposition();
        editor.apply(InsertText(steps.first));
        final rewritten =
            editor.source != before.replaceRange(s.start, s.end, steps.first);
        editor.cancelComposition();
        if (rewritten) {
          unchecked ??= (i, 'composition the kernel refuses ($where)');
        }
        if ((end == 'commit' || end == 'enter') &&
            commit != null &&
            commit.isNotEmpty) {
          editor.beginComposition();
          editor.apply(InsertText(commit));
          editor.commitComposition();
        } else if (!editor.selection.isCollapsed) {
          // Composing replaced the selection; cancelling leaves it deleted,
          // as a platform text field does.
          editor.apply(const DeleteBackward());
        }
      case 'key':
        _key(i, event['key']! as String, shift, alt, meta, ctrl, obs);
      case 'shortcut':
        _shortcut(event['key']! as String, shift);
      case 'click' || 'dblclick' || 'drag' || 'geometry':
        _resync(i, event, obs);
      case 'clipboard':
        clipboard = event['text']! as String;
      case 'reload':
        // The saved draft reopens with its first caret and no history.
        editor = _open(editor.source);
      case 'toolbar' when e['missing'] != true:
        _toolbar(event['button']! as String);
      default:
      // Focus, visibility, pauses and accessibility focus change nothing.
    }
  }

  void _key(
    int i,
    String key,
    bool shift,
    bool alt,
    bool meta,
    bool ctrl,
    Map<String, Object?>? obs,
  ) {
    final primary = meta || ctrl;
    switch (key) {
      case 'Enter':
        if (!primary) editor.apply(Newline(paragraph: shift));
      case 'Backspace':
        editor.apply(DeleteBackward(word: alt || ctrl));
      case 'Delete':
        editor.apply(DeleteForward(word: alt || ctrl));
      case 'ArrowLeft' || 'ArrowRight':
        if (primary) {
          _resync(i, {'k': 'geometry', 'shift': shift}, obs);
        } else {
          editor.apply(
            MoveCaret(
              key == 'ArrowRight'
                  ? MoveDirection.forward
                  : MoveDirection.backward,
              unit: alt ? MoveUnit.word : MoveUnit.grapheme,
              extend: shift,
            ),
          );
        }
      case 'ArrowUp' || 'ArrowDown':
        if (meta) {
          final target = key == 'ArrowDown' ? editor.source.length : 0;
          editor.apply(
            SetSelection(shift ? editor.selection.base : target, target),
          );
        } else {
          _resync(i, {'k': 'geometry', 'shift': shift}, obs);
        }
      case 'Home' || 'End':
        if (ctrl) {
          final target = key == 'End' ? editor.source.length : 0;
          editor.apply(
            SetSelection(shift ? editor.selection.base : target, target),
          );
        } else {
          _resync(i, {'k': 'geometry', 'shift': shift}, obs);
        }
      case 'Tab':
        if (!editor.sourceMode &&
            editor.document.caretRow.kind == RowKind.tableCell) {
          editor.apply(MoveTableCell(backward: shift));
        } else {
          editor.apply(shift ? const Outdent() : const Indent());
        }
      default:
      // Escape and keys the editor leaves alone change nothing.
    }
  }

  void _shortcut(String key, bool shift) {
    switch (key) {
      case 'z':
        editor.apply(shift ? const Redo() : const Undo());
      case 'b':
        editor.apply(const ToggleStyle(Style.strong));
      case 'i':
        editor.apply(const ToggleStyle(Style.emphasis));
      case 'a':
        editor.apply(const SelectAll());
      case 'c':
        if (!editor.selection.isCollapsed) clipboard = _selectedText();
      case 'x':
        if (!editor.selection.isCollapsed) {
          clipboard = _selectedText();
          editor.apply(const DeleteBackward());
        }
      case 'v':
        if (clipboard.isNotEmpty) editor.apply(Paste(clipboard));
    }
  }

  String _selectedText() {
    final s = editor.selection;
    return editor.sourceMode
        ? editor.source.substring(s.start, s.end)
        : editor.document.visibleText(s.start, s.end);
  }

  /// Events placed by glyph geometry: take the selection the page shows. A
  /// pressed task box toggles that task, which the page's source shows too.
  void _resync(int i, Map<String, Object?> event, Map<String, Object?>? obs) {
    if (obs == null || obs['start'] == null) return;
    final lf = _Lf(editor.source);
    final window = _window(obs, lf, editor.source);
    if (window == null) {
      unchecked ??= (i, 'caret in a repeating window');
      return;
    }
    final start = lf.toSource((obs['start']! as int) + window);
    final end = lf.toSource((obs['end']! as int) + window);
    var base = start, extent = end;
    if (start != end) {
      // The textarea keeps no direction. A Shift extension keeps the base;
      // a drag or a click otherwise ends where the pointer went.
      final previous = editor.selection.base;
      var backward =
          event['backward'] == true ||
          (event['shift'] == true && previous == end);
      ambiguous.add(i);
      if (flips.contains(i)) backward = !backward;
      if (backward) (base, extent) = (end, start);
    }
    final saved = obs['saved'] as String?;
    editor.apply(SetSelection(base, extent));
    if (saved != null && saved != editor.source) {
      editor.apply(const ToggleTask());
    }
  }

  /// Where the page's input window starts in the LF text: the one place its
  /// text occurs. Null when it repeats (a long line of one word), as the
  /// window can then be any of them.
  int? _window(Map<String, Object?> obs, _Lf lf, String source) {
    final value = obs['value'] as String?;
    if (value == null || source.length <= 1024) return 0;
    final text = lf.text, at = text.indexOf(value);
    if (at < 0 || text.indexOf(value, at + 1) >= 0) return null;
    return at;
  }

  bool _compare(int i, Map<String, Object?> obs) {
    final errors = obs['errors'] as List?;
    if (errors != null && errors.isNotEmpty) {
      _fail(i, 'console: ${errors.first}', obs);
      return false;
    }
    // The page cannot be predicted from here: only errors are checked.
    if (unchecked != null) return true;
    final saved = obs['saved'] as String?;
    if (saved != null && saved != editor.source) {
      _fail(i, 'source', obs);
      return false;
    }
    final value = obs['value'] as String?;
    if (value == null) {
      if (obs['expectInput'] == true) {
        _fail(i, 'no input element', obs);
        return false;
      }
      return true;
    }
    final lf = _Lf(editor.source);
    final selection = editor.selection;
    final min = lf.toLf(selection.start), max = lf.toLf(selection.end);
    final start = obs['start'] as int, end = obs['end'] as int;
    if (editor.source.length <= 1024) {
      if (value != lf.text) {
        _fail(i, 'platform text', obs);
        return false;
      }
      if ((start, end) != (min, max)) {
        _fail(i, 'selection', obs);
        return false;
      }
      return true;
    }
    // A window: it must be the document's text around the selection.
    final offset = min - start;
    if (offset < 0 ||
        max - end != offset ||
        offset + value.length > lf.text.length ||
        lf.text.substring(offset, offset + value.length) != value) {
      _fail(i, 'windowed selection or text', obs);
      return false;
    }
    return true;
  }
}

/// A source and its text with each CRLF as LF, as a textarea holds it.
final class _Lf {
  _Lf(this.source) : text = source.replaceAll('\r\n', '\n') {
    for (
      var i = source.indexOf('\r\n');
      i >= 0;
      i = source.indexOf('\r\n', i + 2)
    ) {
      crs.add(i);
    }
  }
  final String source, text;
  final crs = <int>[];

  int toLf(int offset) {
    var n = 0;
    while (n < crs.length && crs[n] < offset) {
      n++;
    }
    return offset - n;
  }

  int toSource(int offset) {
    var n = 0;
    while (n < crs.length && crs[n] - n < offset) {
      n++;
    }
    return offset + n;
  }
}
