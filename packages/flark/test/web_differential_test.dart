/// The web differential: seeded command sequences, replayed on whichever
/// platform runs this file, with every state the editor passes through
/// recorded and digested per sequence. `tool/web_differential.sh` runs it on
/// the Dart VM (FFI parser) and under node with dart2js and dart2wasm (the
/// bundled Wasm parser) and compares the digests, so behavior that differs
/// between platforms anywhere from the parser's model to the projection, the
/// caret model and the commands shows up as a sequence whose digests
/// disagree. Run alone, it only checks that every sequence completes.
///
/// A sequence is shaped like the matrix's (`matrix_test.dart`): a document in
/// LF or CRLF spelling, a seeded caret and forty commands from its
/// [randomCommand], then three of its host actions and the walk through
/// history, undoing to the start and redoing to the end. The documents are
/// the matrix's corpus, one sequence in ten starts from a dense document, and
/// one in four joins up to two dozen. One command in five comes from
/// [extraCommand] instead: commands and host calls the matrix does not
/// generate (links, images, languages, explicit styles, table cells, word
/// deletion, raw selections and splices, composition, staying in source
/// mode) and text that stresses UTF-16 and grapheme handling. The editor's
/// clock is fixed, so history coalesces the same way however fast a platform
/// runs.
///
/// Each state records the source, selection, typing context, refusal and
/// rejection, history and composition flags, the session-level formatting
/// state, every projected row in full ([describeRow]), every legal caret
/// offset with its display position, the hidden intervals, the caret's
/// anchors, the resources and the parser's render-model bytes.
///
/// Environment knobs, read on every platform:
///   FLARK_WEB_DIFF_ITERATIONS  sequences to run (default 200)
///   FLARK_WEB_DIFF_SEED        master seed (default 2026)
///   FLARK_WEB_DIFF_FIRST       first sequence index (default 0); with
///                              ITERATIONS, one shard of a longer run
///   FLARK_WEB_DIFF_STEPS       commands per sequence (default 40)
///   FLARK_WEB_DIFF_OUT         file to append `index seed digest` lines to
///   FLARK_WEB_DIFF_TRACE       comma-separated indexes to run alone, their
///                              states appended in full, for diffing
@TestOn('vm || node')
library;

import 'dart:convert';
import 'dart:math';

import 'package:crypto/crypto.dart';
import 'package:flark/flark.dart';
import 'package:flark/session.dart';
import 'package:test/test.dart';

import 'matrix_test.dart'
    show describeCommand, hostAction, matrixCorpus, randomCommand;
import 'support/host.dart';
import 'support/invariants.dart' show describeRow;

void main() {
  late final FlarkParseBackend backend;
  setUpAll(() async => backend = await loadTestBackend());
  int knob(String name, int fallback) =>
      int.tryParse(hostEnvironment(name) ?? '') ?? fallback;
  final iterations = knob('FLARK_WEB_DIFF_ITERATIONS', 200);
  final seed = knob('FLARK_WEB_DIFF_SEED', 2026);
  final first = knob('FLARK_WEB_DIFF_FIRST', 0);
  final steps = knob('FLARK_WEB_DIFF_STEPS', 40);
  final out = hostEnvironment('FLARK_WEB_DIFF_OUT');
  final traced = {
    for (final index in (hostEnvironment('FLARK_WEB_DIFF_TRACE') ?? '').split(
      ',',
    ))
      ?int.tryParse(index.trim()),
  };
  // The matrix's documents, and one sequence in ten from a dense one, so
  // that table, task, list, code and resource commands find their targets.
  final corpus = [...matrixCorpus(), for (var k = 0; k < 30; k++) ..._dense];

  test(
    '$hostPlatform: sequences $first..${first + iterations} of seed $seed',
    () {
      final watch = Stopwatch()..start();
      final master = Random(seed);
      var failed = 0;
      final lines = StringBuffer();
      for (var i = 0; i < first + iterations; i++) {
        final s = master.nextInt(1 << 30);
        // Each sequence has a seed of its own, so a trace runs only its own.
        if (i < first || traced.isNotEmpty && !traced.contains(i)) continue;
        final trace = traced.contains(i) ? StringBuffer() : null;
        final record = _Record(i, trace);
        _runSequence(backend, corpus, i, s, steps, record);
        failed += record.failed ? 1 : 0;
        if (out == null) continue;
        if (trace != null) {
          appendHostFile(out, '=== sequence $i seed $s\n$trace');
        } else {
          lines.writeln('$i $s ${record.digest}');
          // Appended as it goes, so a run that dies still leaves the
          // sequences before it.
          if (i % 50 == 49) {
            appendHostFile(out, lines.toString());
            lines.clear();
          }
        }
      }
      if (out != null && lines.isNotEmpty) appendHostFile(out, '$lines');
      // ignore: avoid_print
      print(
        'web differential $hostPlatform: $iterations sequences from $first '
        '(seed $seed) in ${watch.elapsedMilliseconds} ms, '
        '$failed ended in an error',
      );
    },
    timeout: const Timeout(Duration(hours: 2)),
  );
}

/// Run sequence [index] from [seed] and record every state it reaches. An
/// error ends the sequence and is recorded by kind, so platforms that fail
/// alike still agree.
void _runSequence(
  FlarkParseBackend backend,
  List<String> corpus,
  int index,
  int seed,
  int steps,
  _Record record,
) {
  final r = Random(seed);
  var source = corpus[r.nextInt(corpus.length)];
  // One document in four joins up to two dozen: more blocks for projection
  // reuse, offsets past the parser's initial input buffer. Drawn from a
  // stream of its own, so that it does not shift the commands'.
  final shape = Random(seed ^ 0xd0c5);
  if (shape.nextInt(4) == 0) {
    source = [
      source,
      for (var k = shape.nextInt(23); k >= 0; k--)
        corpus[shape.nextInt(corpus.length)],
    ].join('\n\n');
  }
  source = source.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  if (index.isOdd) source = source.replaceAll('\n', '\r\n');
  try {
    final editor = FlarkEditor(
      backend,
      text: source,
      caret: r.nextInt(source.length + 1),
      clock: () => Duration.zero,
    );
    record.state('load', null, editor);
    for (var step = 0; step < steps; step++) {
      final at = Duration(milliseconds: step * 100);
      if (r.nextInt(5) == 0) {
        final (label, run) = extraCommand(r, editor, at);
        record.state(label, run(), editor);
      } else {
        final FlarkCommand command;
        try {
          command = randomCommand(r, editor);
        } on StateError {
          // A display position needs the projection source mode lacks.
          if (!editor.sourceMode) rethrow;
          record.state('PlaceCaret in source mode', null, editor);
          continue;
        }
        record.state(
          describeCommand(command),
          editor.apply(command, at: at),
          editor,
        );
      }
    }
    // The host actions' oracles hold from where the matrix leaves them.
    if (editor.composing) {
      editor.commitComposition();
      record.state('commitComposition()', null, editor);
    }
    if (editor.sourceMode) {
      editor.setSourceMode(false);
      record.state('setSourceMode(false)', null, editor);
    }
    final host = Random(seed ^ 0x5eed);
    for (var k = 0; k < 3; k++) {
      final log = <String>[];
      hostAction(host, editor, log, 'seed $seed host $k');
      record.state(log.join('; '), null, editor);
    }
    var undone = 0;
    while (editor.history.canUndo && undone < 100) {
      record.state('Undo()', editor.apply(const Undo()), editor);
      undone++;
    }
    for (var k = 0; k < undone; k++) {
      record.state('Redo()', editor.apply(const Redo()), editor);
    }
  } catch (error, stack) {
    record.error(error, stack);
  }
}

const _styles = [Style.emphasis, Style.strong, Style.code, Style.strikethrough];

/// Documents with every structure some command acts on.
const _dense = [
  '| Left | Center | Right |\n| :--- | :----: | ----: |\n'
      '| a *b* | `c` | [d](e) |\n|  | x |  |\n| \u{1F600} | \u00E9 | \u6F22 |\n',
  '- [ ] todo **bold**\n- [x] done\n  - [ ] nested *em*\n    1. ordered\n'
      '    2. second\n- plain\n\n1. one\n2. [ ] two\n   > quoted\n',
  '```dart\nvoid main() {\n  print(1);\n}\n```\n\n~~~\ntilde\n~~~\n\n'
      '    indented\n\n```\nunclosed',
  'See [link](http://a.b "title"), ![img](i.png), <http://auto.link>, '
      'www.x.y, a@b.co, [ref][r] and a note[^1].\n\n'
      '[r]: http://ref.example "Ref"\n[^1]: The note with *style*.\n',
  '# H1 #\nSetext\n======\n\n> quote with **bold**\n> - item\n>\n'
      '> ```\n> code\n> ```\n\n<div>\nhtml *block*\n</div>\n\n---\n'
      '***em strong*** ~~strike~~ `code` \\*escaped\\* &amp; &#128512;\n',
];

/// Text that crosses UTF-16, grapheme and Markdown edges: combining marks,
/// ZWJ sequences, flags, modifiers, astral letters, other scripts, the
/// whitespace JavaScript and Dart classify differently, entities and syntax.
const _unicode = [
  '\u0301',
  'e\u0301',
  '\u{1F469}\u200D\u{1F4BB}',
  '\u{1F1E8}\u{1F1E6}',
  '\u{1F44D}\u{1F3FD}',
  '1\uFE0F\u20E3',
  '\u200D',
  '\uFE0F',
  '\u{1D49C}',
  '\u6F22',
  '\u05D0',
  '\u0627\u0644',
  '\u00A0',
  '\u3000',
  '\uFEFF',
  '\u0085',
  '\u2028',
  '\u180E',
  '\uFB00',
  '\u00DF',
  '\u0130',
  '\u03A3',
  '&amp;',
  '&#128512;',
  '\\*',
  '<b>',
  'http://x.y',
  'a@b.co',
  '[^1]',
  '[x] ',
  '~~',
  '***',
  '| c |',
  '    ',
];

const _pastes = [
  '- \u{1F600} **\u00E9**\n- [ ] \u6F22',
  '> ```\n> \u{1F469}\u200D\u{1F4BB}\n> ```',
  '[^n]: note \u{1F1E8}\u{1F1E6}\n',
  '| \u{1F600} | \u00E9 |\n| :- | -: |\n| \u6F22 | \u05D0 |',
  'a\u0301\u0301b',
  '***bold*** ~~s~~ `c`',
  '<div>\n\u{1F600}\n</div>',
  '1. a\n   1. b\n      - c',
  '# h \u{1F600} #\n\nsetext\n---',
  '[t](<u v> "w") ![i](j)',
  '\u{1F600}\r\n\u{1F600}',
];

/// A command or host call the matrix's generator does not make, with its
/// label; running it returns whether it changed anything, as [FlarkEditor.apply]
/// does, or null for a host call that does not say.
(String, bool? Function()) extraCommand(Random r, FlarkEditor e, Duration at) {
  (String, bool? Function()) command(FlarkCommand c) =>
      (describeCommand(c), () => e.apply(c, at: at));
  String pick(List<String> options) => options[r.nextInt(options.length)];
  final length = e.source.length;
  switch (r.nextInt(16)) {
    case 0 || 1:
      return command(InsertText(pick(_unicode)));
    case 2:
      return command(Paste(pick(_pastes)));
    case 3:
      return command(
        SetLink(
          pick(['https://x.y/\u00FC?q=\u{1F600}', '', 'a b', '<c>']),
          text: r.nextBool() ? null : pick(['l \u{1F600}', '', '*e*']),
          title: r.nextBool() ? null : pick(['t', '"q"']),
        ),
      );
    case 4:
      return command(
        SetImage(
          pick(['i \u{1F600}.png', 'j.png', '']),
          alt: r.nextBool() ? null : pick(['a \u00E9', '']),
          title: r.nextBool() ? null : 't',
        ),
      );
    case 5:
      return command(r.nextBool() ? const RemoveLink() : const RemoveImage());
    case 6:
      return command(
        SetCodeLanguage(pick(['dart', '', 'text', 'c++', '\u00E9moji'])),
      );
    case 7:
      return command(
        SetStyle(_styles[r.nextInt(_styles.length)], enabled: r.nextBool()),
      );
    case 8:
      return command(MoveTableCell(backward: r.nextBool()));
    case 9:
      return command(
        r.nextBool()
            ? const DeleteBackward(word: true)
            : const DeleteForward(word: true),
      );
    case 10:
      // Raw offsets: inside surrogate pairs, hidden syntax and graphemes.
      return command(
        SetSelection(r.nextInt(length + 1), r.nextInt(length + 1)),
      );
    case 11:
      final a = r.nextInt(length + 1);
      return command(
        ReplaceRange(a, min(length, a + r.nextInt(4)), pick(_unicode)),
      );
    case 12:
      final a = r.nextInt(length + 1);
      final b = min(length, a + r.nextInt(4));
      final text = pick(['', '\u{1F600}', '\r\n', '\n', '**']);
      return (
        'replaceSourceRange($a, $b, ${jsonEncode(text)})',
        () => e.replaceSourceRange(a, b, text),
      );
    case 13:
      final enabled = !e.sourceMode;
      return (
        'setSourceMode($enabled)',
        () {
          e.setSourceMode(enabled);
          return null;
        },
      );
    case 14:
      return switch (r.nextInt(3)) {
        0 => (
          'beginComposition()',
          () {
            e.beginComposition();
            return null;
          },
        ),
        1 => (
          'commitComposition()',
          () {
            e.commitComposition();
            return null;
          },
        ),
        _ => (
          'cancelComposition()',
          () {
            e.cancelComposition();
            return null;
          },
        ),
      };
    default:
      final codeBlock = r.nextBool();
      return (
        'selectAll(codeBlock: $codeBlock)',
        () => e.selectAll(codeBlock: codeBlock),
      );
  }
}

/// A sequence's states, digested, and written out in full when traced.
final class _Record {
  _Record(this._sequence, this._trace) {
    _input = sha256.startChunkedConversion(_sink);
  }

  final int _sequence;
  final StringBuffer? _trace;
  final _sink = _DigestSink();
  late final ByteConversionSink _input;
  var _step = 0;
  bool failed = false;

  String get digest {
    _input.close();
    return '${_sink.value}';
  }

  void state(String event, bool? changed, FlarkEditor editor) {
    final text = StringBuffer('#$_sequence.${_step++} $event -> $changed\n');
    describeState(text, editor);
    _input.add(utf8.encode('$text'));
    final model = editor.sourceMode ? null : editor.document.model.bytes;
    if (model != null) _input.add(model);
    final trace = _trace;
    if (trace != null) {
      trace.write(text);
      if (model != null) trace.writeln('model ${sha256.convert(model)}');
    }
  }

  void error(Object error, StackTrace stack) {
    failed = true;
    final kind = switch (error) {
      FlarkParseException(:final code) => 'FlarkParseException($code)',
      TestFailure() => 'TestFailure',
      RangeError() => 'RangeError',
      ArgumentError() => 'ArgumentError',
      StateError() => 'StateError',
      FormatException() => 'FormatException',
      UnsupportedError() => 'UnsupportedError',
      TypeError() => 'TypeError',
      _ => 'error',
    };
    final step = '#$_sequence.${_step++}';
    _input.add(utf8.encode('$step threw $kind\n'));
    _trace?.write('$step threw $kind: $error\n$stack\n');
  }
}

final class _DigestSink implements Sink<Digest> {
  Digest? value;
  @override
  void add(Digest data) => value = data;
  @override
  void close() {}
}

/// Everything about [e]'s state a host or a later command can observe, as
/// text with no platform-dependent formatting: no `toString` of doubles,
/// records, minified types or errors.
void describeState(StringBuffer out, FlarkEditor e) {
  final s = e.selection;
  out
    ..writeln('source ${jsonEncode(e.source)}')
    ..writeln(
      'selection ${s.base} ${s.extent} cell ${s.tableCell} '
      'typing ${e.typingContext} sourceMode ${e.sourceMode} '
      'composing ${e.composing} revision ${e.revision} '
      'rejection ${e.lastRejection?.name} '
      'undo ${e.history.canUndo} redo ${e.history.canRedo}',
    );
  final state = FlarkState(
    markdown: e.source,
    revision: e.revision,
    status: FlarkStatus.ready,
    error: null,
    editor: e,
  );
  out.writeln(
    'heading ${state.heading.level} ${state.heading.isMixed} '
    '${state.heading.canSet} link ${_resource(state.link.resource)} '
    '${state.link.canSet} code ${jsonEncode(state.code.language)} '
    '${state.code.canSetLanguage} resource ${e.canSetResource()} '
    '${e.canSetResource(image: true)}',
  );
  for (final style in FlarkStyle.values) {
    final value = state.styles[style];
    out.writeln(
      'style ${style.name} ${value.value.name} '
      '${value.canEnable} ${value.canDisable}',
    );
  }
  if (e.sourceMode) return;
  final doc = e.document;
  for (final row in doc.projection.rows) {
    out.writeln(describeRow(row));
  }
  // Every offset of a short document; around the selection's ends in a long
  // one, whose rows above already pin down the rest.
  final length = doc.source.length;
  final windows = length <= 2048
      ? [(0, length)]
      : [
          for (final end in [s.base, s.extent])
            (max(0, end - 64), min(length, end + 64)),
        ];
  out.write('legal');
  for (final (from, to) in windows) {
    for (var o = from; o <= to; o++) {
      if (!doc.isLegal(o)) continue;
      final d = doc.displayOf(o);
      out.write(' $o:${d.row}.${d.offset}${d.snapped ? 's' : ''}');
    }
  }
  out.write('\nhidden');
  for (final (a, b) in doc.hiddenIntervals) {
    out.write(' $a..$b');
  }
  final caret = doc.caretPosition;
  out
    ..writeln()
    ..writeln(
      'anchors ${doc.anchorsAt(s.extent).join(',')} '
      'caret ${caret.row}.${caret.offset} '
      'visible ${jsonEncode(doc.visibleText(s.start, s.end))}',
    );
  out.write('resources');
  for (final resource in doc.resources) {
    out.write(' ${_resource(resource)}');
  }
  out.writeln();
}

String _resource(InlineResource? r) => r == null
    ? 'none'
    : '${r.isImage ? 'image' : 'link'} ${r.run} ${r.block} '
          '${r.start}..${r.end} ${r.contentStart}..${r.contentEnd} '
          '${jsonEncode(r.text)} ${jsonEncode(r.destination)} '
          '${jsonEncode(r.title)}';
