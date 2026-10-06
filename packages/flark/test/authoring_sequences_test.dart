/// Authoring sessions: realistic Markdown documents edited the way a person
/// edits them, where the matrix applies random single commands to spec
/// examples.
///
/// Each seed writes a document (a README with badges, code and an options
/// table; meeting minutes with tasks; nested lists with mixed markers and tab
/// or space indentation; quotes holding lists and code; footnotes, reference
/// links and definitions, images, HTML blocks, emoji, CJK and RTL text and
/// soft-wrapped paragraphs, in LF or CRLF, sometimes after a byte order mark)
/// and then edits it: words and sentences typed a character at a time,
/// Markdown shortcuts typed in order, Return through lists, quotes and code,
/// Backspace runs through structure, word deletes, selections replaced,
/// formatted or deleted, autocorrect replacements, input-method composition,
/// realistic pastes, cut and paste, Undo and Redo bursts, and formatting,
/// heading, task, indent and link commands.
///
/// Every command passes the matrix's step and structure checks and a refused
/// command leaves no trace; one the profile has no reason to refuse (a
/// letter typed in a row's text, Return at a row's end, a letter deleted
/// inside a row) must apply. Beyond them, a session checks what a person
/// sees: rows show in source order, typed or pasted text leaves the caret in
/// the text it changed, plain letters typed or deleted never make hidden
/// markup show nor make another letter vanish, and typing into one row or
/// deleting prose inside it leaves the other rows' kinds and containers as
/// they were. An Undo or Redo lands on a state the session reached, a
/// cancelled composition restores the state before it, and each session
/// ends with the matrix's host actions and history walk back to its first
/// source and forward again. A failure prints the seed, the state before
/// the failing command and the command log.
/// `FLARK_AUTHORING_SEED` is the first seed and `FLARK_AUTHORING_ITERATIONS`
/// the number of sessions; a seed with one iteration replays one session.
/// The run counts applied, inert and refused commands per kind
/// ([AuthoringStats]) and fails when a kind is refused above its ceiling;
/// `FLARK_AUTHORING_STATS=1` prints the counts, and `=refusals` lists each
/// refusal too.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:characters/characters.dart';
import 'package:flark/flark.dart';
import 'package:flark/render_model.dart' show BlockKind;
import 'package:test/test.dart';

import 'matrix_test.dart' as matrix;
import 'support/structure_oracles.dart'
    show checkMustApply, definitionsOf, inReference;

void main() {
  final backend = createParseBackend();
  final iterations =
      int.tryParse(Platform.environment['FLARK_AUTHORING_ITERATIONS'] ?? '') ??
      4;
  final seed =
      int.tryParse(Platform.environment['FLARK_AUTHORING_SEED'] ?? '') ?? 2026;
  test('authoring sessions keep every invariant '
      '(seeds $seed..${seed + iterations - 1})', () {
    final stats = AuthoringStats();
    for (var s = seed; s < seed + iterations; s++) {
      authoringSession(backend, s, stats: stats);
    }
    final verbose = Platform.environment['FLARK_AUTHORING_STATS'];
    if (verbose != null && verbose.isNotEmpty && verbose != '0') {
      // ignore: avoid_print
      print(stats.summary(refusals: verbose == 'refusals'));
    }
    stats.checkCeilings(sessions: iterations);
  }, timeout: const Timeout(Duration(minutes: 20)));
}

/// Runs the session [seed] names and throws its first failed check, after
/// printing what reproduces it. [stats] counts its commands.
void authoringSession(
  FlarkParseBackend backend,
  int seed, {
  AuthoringStats? stats,
}) {
  final author = _Author(backend, seed, stats);
  try {
    author.run();
  } catch (error) {
    // ignore: avoid_print
    print(author.report(error));
    rethrow;
  }
}

/// A refused command, with the state it was refused in.
typedef AuthoringRefusal = ({
  String label,
  FlarkEditorSnapshot before,
  FlarkCommand command,
  int context,
});

/// Commands applied, inert and refused, per kind, over the sessions of a run.
/// A refused command is one the editor reports as unsupported; an inert one
/// had nothing to do (Backspace at the document's start, Return on an empty
/// item whose leaving would move the blocks after it, a task toggle off any
/// task) and reports nothing, as navigation that does not move does. A
/// refusal of the same kind in the state the last one left (a run of
/// Backspaces against a join the profile refuses, a word typed over a
/// selection it refuses) is counted as a repeat, so one refused context
/// weighs once.
final class AuthoringStats {
  final _counts = <String, List<int>>{};
  final refusals = <AuthoringRefusal>[];
  (String, FlarkSelection, String)? _lastRefused;

  /// The share of each kind's commands refused at a caret, distinct
  /// contexts counted, in 500 sessions (seeds 10000..10299 and
  /// 20000..20199, 2026-10-04): joins and lifts the profile refuses, Return
  /// in literal HTML, typing beside a bare marker. Refusals over a
  /// selection, which the profile leaves unsupported across rows and inline
  /// owners, are not counted. [checkCeilings] allows these shares and six
  /// standard errors for the run's size, so a kernel that refuses a kind of
  /// command broadly fails the run; the must-apply contexts
  /// ([checkMustApply]) catch a refusal in a context the profile never
  /// refuses.
  static const ceilings = {
    'letter': 0.0002,
    'space': 0.0,
    'other text': 0.001,
    'Return': 0.02,
    'paragraph break': 0.04,
    'Backspace': 0.055,
    'Delete': 0.08,
    'word Backspace': 0.07,
    'word Delete': 0.12,
  };
  static const _least = 30;

  /// The kind [command] is counted as.
  static String kindOf(FlarkCommand command) => switch (command) {
    InsertText(:final text) when _letter.hasMatch(text) => 'letter',
    InsertText(:final text) when text.trim().isEmpty => 'space',
    InsertText() => 'other text',
    Newline(paragraph: false) => 'Return',
    Newline(paragraph: true) => 'paragraph break',
    DeleteBackward(word: false) => 'Backspace',
    DeleteBackward(word: true) => 'word Backspace',
    DeleteForward(word: false) => 'Delete',
    DeleteForward(word: true) => 'word Delete',
    _ => '${command.runtimeType}',
  };

  void count(
    FlarkCommand command, {
    required bool applied,
    required FlarkRejection? rejection,
    required FlarkEditorSnapshot before,
    required int context,
    required String label,
  }) {
    final kind = kindOf(command);
    final counts = _counts[kind] ??= [0, 0, 0, 0, 0];
    if (applied) {
      counts[0]++;
      _lastRefused = null;
    } else if (rejection == null) {
      counts[1]++;
    } else {
      final key = (before.source, before.selection, kind);
      if (key == _lastRefused) {
        counts[3]++;
        return;
      }
      _lastRefused = key;
      counts[2]++;
      if (before.selection.isCollapsed) counts[4]++;
      refusals.add((
        label: label,
        before: before,
        command: command,
        context: context,
      ));
    }
  }

  /// Applied, inert and refused commands of [kind], the repeated refusals,
  /// and the refusals at a caret.
  ({int applied, int inert, int refused, int repeats, int atCaret}) of(
    String kind,
  ) {
    final c = _counts[kind] ?? const [0, 0, 0, 0, 0];
    return (
      applied: c[0],
      inert: c[1],
      refused: c[2],
      repeats: c[3],
      atCaret: c[4],
    );
  }

  /// The share of [kind]'s commands refused at a caret, and the most its
  /// ceiling allows in a run of this size; null below [_least] commands or
  /// without a ceiling.
  (double, double)? caretShare(String kind) {
    final rate = ceilings[kind];
    final c = of(kind);
    final total = c.applied + c.inert + c.refused;
    if (rate == null || total < _least) return null;
    final spread = sqrt(max(rate, 0.003) * (1 - rate) / total);
    return (c.atCaret / total, rate + 6 * spread);
  }

  String summary({bool refusals = false}) {
    final out = StringBuffer(
      'authoring commands: applied, inert, refused (+ repeats), at a caret\n',
    );
    final kinds = _counts.keys.toList()..sort();
    for (final kind in kinds) {
      final c = of(kind);
      final share = caretShare(kind);
      out.writeln(
        '  ${kind.padRight(16)} ${'${c.applied}'.padLeft(6)} '
        '${'${c.inert}'.padLeft(5)} ${'${c.refused}'.padLeft(5)} '
        '${'(+${c.repeats})'.padRight(7)} ${'${c.atCaret}'.padLeft(4)}'
        '${share == null ? '' : '  ${(100 * share.$1).toStringAsFixed(2)}% '
                  'of ${(100 * share.$2).toStringAsFixed(2)}% allowed'}',
      );
    }
    if (refusals) {
      for (final r in this.refusals) {
        out.writeln(
          '  refused ${r.label}: ${jsonEncode(r.before.source)} '
          '${r.before.selection}',
        );
      }
    }
    return '$out';
  }

  /// Fails when a kind of command is refused at a caret more often than its
  /// ceiling allows, over a run of at least four [sessions]: one session
  /// replayed alone can hold a cluster of refusals a run averages out.
  void checkCeilings({required int sessions}) {
    if (sessions < 4) return;
    for (final kind in ceilings.keys) {
      final share = caretShare(kind);
      if (share == null) continue;
      expect(
        share.$1,
        lessThanOrEqualTo(share.$2),
        reason:
            '$kind refused at a caret ${of(kind).atCaret} times, over its '
            'ceiling; FLARK_AUTHORING_STATS=refusals lists the refusals',
      );
    }
  }
}

final _letter = RegExp(r'^[\p{L}\p{N}]\p{M}*$', unicode: true);

// ------------------------------------------------------------- documents

const _words = [
  'the', 'editor', 'keeps', 'every', 'change', 'source', 'exact', 'while', //
  'you', 'type', 'notes', 'release', 'build', 'tests', 'parser', 'caret',
  'review', 'draft', 'update', 'plan', 'team', 'project', 'docs', 'version',
  'feature', 'support', 'render', 'preview', 'install', 'config', 'option',
  'value', 'default', 'example', 'quick', 'brown', 'fox', 'jumps', 'over',
  'lazy', 'dog', 'meeting', 'agenda', 'action', 'owner', 'deadline', 'Friday',
  'API', 'CLI', 'iOS', 'Android', 'web', 'macOS', 'list', 'quote', 'table',
  'link', 'image', 'heading', 'paragraph', 'markdown', 'code', 'snippet',
];
const _intl = [
  'café', 'naïve', 'résumé', 'Zoë', 'Ångström', 'e\u0301te\u0301', '日本語', //
  '编辑器', '한국어', 'テスト', 'مرحبا', 'العالم', 'שלום', 'Привет', 'Ελληνικά',
];
const _emoji = [
  '😀',
  '👍🏽',
  '🎉',
  '👨\u200D👩\u200D👧',
  '🇨🇦',
  '❤️',
  '✅',
  '🚀',
];
const _slugs = ['intro', 'setup', 'usage', 'api', 'faq', 'changelog', 'guide'];
const _code = {
  '': ['plain text output', '  indented line', 'done'],
  'dart': ['void main() {', "  print('hello');", '}'],
  'bash': ['flutter pub get', 'dart test --reporter expanded'],
  'sh': [r'export PATH="$HOME/bin:$PATH"', 'make build'],
  'json': ['{', '  "name": "widget",', '  "version": "0.5.0"', '}'],
  'yaml': ['dependencies:', '  widget: ^0.5.0'],
  'python': ['def greet(name):', '    return f"Hello, {name}"'],
  'js': [
    'const doubled = [1, 2, 3].map((n) => n * 2);',
    'console.log(doubled);',
  ],
  'diff': ['- old line', '+ new line'],
  'text': ['``not a fence``', '~~~ nor this'],
};
const _footnotes = ['1', '2', 'note', 'source'];
const _references = ['docs', 'home', 'issue', 'spec'];

/// Writes realistic Markdown, LF only; the session respells line endings.
final class _Writer {
  _Writer(this.r);
  final Random r;
  final _usedFootnotes = <String>{};
  final _usedReferences = <String>{};

  T pick<T>(List<T> items) => items[r.nextInt(items.length)];
  bool chance(double p) => r.nextDouble() < p;
  int count(int low, int high) => low + r.nextInt(high - low + 1);

  String word() {
    final k = r.nextInt(100);
    if (k < 85) return pick(_words);
    if (k < 96) return pick(_intl);
    return pick(_emoji);
  }

  String capitalized(String text) => text.isEmpty
      ? text
      : '${text.characters.first.toUpperCase()}${text.characters.skip(1)}';

  String footnote() {
    final label = pick(_footnotes);
    _usedFootnotes.add(label);
    return '[^$label]';
  }

  String reference(String text) {
    final label = pick(_references);
    _usedReferences.add(label);
    return switch (r.nextInt(3)) {
      0 => '[$text][$label]',
      1 => '[$label][]',
      _ => '[$label]',
    };
  }

  /// One token of prose: mostly a word, sometimes inline Markdown.
  String token() {
    final w = word();
    return switch (r.nextInt(48)) {
      0 || 1 => '**$w**',
      2 => '*$w*',
      3 => '_${w}_',
      4 || 5 => '`$w`',
      6 => '~~$w~~',
      7 || 8 => '[$w](https://example.com/${pick(_slugs)})',
      9 => reference(w),
      10 => '<https://example.org/${pick(_slugs)}>',
      11 => 'https://example.com/${pick(_slugs)}',
      12 => '![$w](images/${pick(_slugs)}.png)',
      13 => '$w${footnote()}',
      14 => '&amp;',
      15 => '<kbd>Ctrl</kbd>',
      16 => '**$w ${word()}**',
      17 => r'\*literal\*',
      18 => '@$w',
      19 => '#${count(1, 400)}',
      20 => 'v${count(0, 3)}.${count(0, 9)}.${count(0, 9)}',
      _ => w,
    };
  }

  String sentence() {
    final tokens = [for (var i = count(3, 13); i > 0; i--) token()];
    return '${capitalized(tokens.join(' '))}${pick(['.', '.', '.', '!', '?', ':'])}';
  }

  /// Short inline text, as in a list item or table cell.
  String phrase() =>
      capitalized([for (var i = count(1, 6); i > 0; i--) token()].join(' '));

  List<String> paragraph() {
    final text = [for (var i = count(1, 4); i > 0; i--) sentence()].join(' ');
    if (!chance(0.4)) return [text];
    // Soft-wrapped at a column, as many editors save prose, sometimes with
    // a hard break.
    final width = count(36, 80);
    final lines = <String>[];
    var line = '';
    for (final w in text.split(' ')) {
      if (line.isNotEmpty && line.length + 1 + w.length > width) {
        lines.add(line);
        line = '';
      }
      line = line.isEmpty ? w : '$line $w';
    }
    lines.add(line);
    for (var i = 0; i < lines.length - 1; i++) {
      if (chance(0.1)) lines[i] = '${lines[i]}${chance(0.5) ? '  ' : r'\'}';
    }
    return lines;
  }

  List<String> heading(int level) {
    final title = capitalized(
      [for (var i = count(1, 5); i > 0; i--) word()].join(' '),
    );
    if (level <= 2 && chance(0.15)) {
      return [title, (level == 1 ? '=' : '-') * count(3, 24)];
    }
    final closing = chance(0.06) ? ' ${'#' * count(1, 3)}' : '';
    return ['${'#' * level} $title$closing'];
  }

  List<String> fence({String? language}) {
    final lang = language ?? pick(_code.keys.toList());
    final marker = chance(0.82) ? '```' : '~~~';
    final body = [..._code[lang]!];
    if (chance(0.15)) body.insert(count(0, body.length), '');
    return ['$marker$lang', ...body, marker];
  }

  List<String> indentedCode() => [
    for (final line in _code['dart']!) '    $line',
  ];

  List<String> table() {
    final columns = count(2, 4);
    String cell() => switch (r.nextInt(10)) {
      0 => '`${word()}`',
      1 => '**${word()}**',
      2 => '${count(0, 999)}',
      3 => '',
      4 => r'a \| b',
      _ => word(),
    };
    final pipes = chance(0.85);
    String row(List<String> cells) =>
        pipes ? '| ${cells.join(' | ')} |' : cells.join(' | ');
    return [
      row([for (var c = 0; c < columns; c++) capitalized(word())]),
      row([
        for (var c = 0; c < columns; c++)
          pick(['---', ':--', ':-:', '--:', '------', ':---:']),
      ]),
      for (var i = count(1, 4); i > 0; i--)
        row([for (var c = 0; c < columns; c++) cell()]),
    ];
  }

  /// A list of 2-5 items, nested up to four levels, with one marker style,
  /// indented by spaces to its content or by tabs.
  List<String> list(int depth, {bool? ordered, bool? task, bool? tabs}) {
    ordered ??= chance(0.35);
    task ??= !ordered && chance(0.3);
    tabs ??= chance(0.2);
    final bullet = pick(['-', '-', '*', '+']);
    final delimiter = pick(['.', '.', ')']);
    final start = chance(0.85) ? 1 : count(0, 9);
    final loose = chance(0.2);
    final lines = <String>[];
    final items = count(2, 5);
    for (var i = 0; i < items; i++) {
      final marker = ordered ? '${start + i}$delimiter' : bullet;
      var first = phrase();
      if (task) first = '[${chance(0.4) ? 'x' : ' '}] $first';
      final body = [first];
      if (chance(0.12)) body.addAll(['', ...paragraph()]);
      if (depth < 4 && chance(0.4)) {
        body.addAll(list(depth + 1, tabs: tabs));
      }
      if (chance(0.06)) body.addAll(['', ...fence()]);
      final indent = tabs ? '\t' : ' ' * (marker.length + 1);
      lines.add('$marker $first');
      for (final line in body.skip(1)) {
        lines.add(line.isEmpty ? '' : '$indent$line');
      }
      if (loose && i < items - 1) lines.add('');
    }
    return lines;
  }

  /// A quote of paragraphs, sometimes holding a list, code or another quote,
  /// with lazy continuation lines now and then.
  List<String> quote(int depth) {
    final lines = <String>[];
    for (var part = count(1, 3); part > 0; part--) {
      if (lines.isNotEmpty) lines.add('>');
      final k = r.nextInt(10);
      if (k < 5) {
        final text = paragraph();
        final lazy = chance(0.15);
        for (var i = 0; i < text.length; i++) {
          lines.add(i > 0 && lazy ? text[i] : '> ${text[i]}');
        }
      } else if (k < 7) {
        lines.addAll([for (final l in list(2)) l.isEmpty ? '>' : '> $l']);
      } else if (k < 9) {
        lines.addAll([for (final l in fence()) '> $l']);
      } else if (depth < 3) {
        lines.addAll([for (final l in quote(depth + 1)) '> $l']);
      }
    }
    return lines.isEmpty ? ['> ${sentence()}'] : lines;
  }

  List<String> html() => switch (r.nextInt(4)) {
    0 => [
      '<details>',
      '<summary>${phrase()}</summary>',
      '',
      ...paragraph(),
      '',
      '</details>',
    ],
    1 => ['<!-- TODO: ${phrase()} -->'],
    2 => [
      '<div align="center">',
      '  <img src="images/logo.png" width="120" alt="Logo">',
      '</div>',
    ],
    _ => ['<p>${phrase()}<br>${phrase()}</p>'],
  };

  List<String> definitions() => [
    for (final label in [..._usedReferences])
      if (!chance(0.1))
        '[$label]: ${pick(['https://example.com/$label', '<https://example.com/$label>', '/$label.md'])}'
            '${pick(['', '', ' "${capitalized(label)}"', " '${capitalized(label)}'"])}',
    for (final label in [..._usedFootnotes])
      if (!chance(0.1)) '[^$label]: ${sentence()}',
  ];

  List<String> block() => switch (r.nextInt(20)) {
    < 7 => paragraph(),
    < 10 => list(1),
    < 11 => list(1, task: true),
    < 12 => quote(1),
    < 14 => fence(),
    < 15 => table(),
    < 16 => heading(count(2, 4)),
    < 17 => html(),
    < 18 => [
      pick(['---', '***', '___', '- - -']),
    ],
    < 19 => indentedCode(),
    _ => ['![${phrase()}](images/${pick(_slugs)}.png "${phrase()}")'],
  };

  String document() {
    final blocks = <List<String>>[];
    switch (r.nextInt(100)) {
      case < 20: // A README.
        blocks.add(heading(1));
        if (chance(0.7)) {
          blocks.add([
            '[![CI](https://github.com/acme/widget/actions/workflows/ci.yml/badge.svg)](https://github.com/acme/widget/actions)'
                ' [![pub](https://img.shields.io/pub/v/widget.svg)](https://pub.dev/packages/widget)',
          ]);
        }
        blocks.add(paragraph());
        blocks.add(heading(2));
        blocks.add(list(1, ordered: false, task: false));
        blocks.add(heading(2));
        blocks.add(fence(language: pick(['bash', 'sh'])));
        blocks.add(heading(2));
        blocks.add(paragraph());
        blocks.add(fence(language: pick(['dart', 'js', 'python', 'json'])));
        if (chance(0.7)) blocks.addAll([heading(2), table()]);
        blocks.addAll([heading(2), paragraph()]);
      case < 38: // Meeting minutes.
        blocks.add([
          '# ${pick(['Weekly sync', 'Design review', 'Standup', 'Retro'])} — 2026-10-0${count(1, 9)}',
        ]);
        blocks.add([
          '**Attendees:** ${[
            for (var i = count(2, 5); i > 0; i--) pick(['Alice', 'Bob', 'Chen', 'Dana', 'Émile', 'Fatima', 'Gil']),
          ].join(', ')}',
        ]);
        blocks.addAll([heading(2), list(1, ordered: true)]);
        blocks.addAll([heading(2), list(1, ordered: false, task: false)]);
        if (chance(0.5)) blocks.add(quote(1));
        blocks.addAll([heading(2), list(1, task: true)]);
      case < 53: // Nested lists.
        for (var i = count(1, 4); i > 0; i--) {
          if (chance(0.4)) blocks.add(heading(count(2, 3)));
          blocks.add(list(1));
          if (chance(0.4)) blocks.add(paragraph());
        }
      case < 68: // A journal or notes page.
        blocks.add(heading(count(1, 2)));
        for (var i = count(2, 6); i > 0; i--) {
          blocks.add(
            pick<List<String> Function()>([paragraph, () => quote(1), html])(),
          );
        }
      case < 90:
        for (var i = count(2, 9); i > 0; i--) {
          blocks.add(block());
        }
      default: // Almost nothing yet.
        for (var i = count(0, 2); i > 0; i--) {
          blocks.add(pick([paragraph(), heading(1), list(1)]));
        }
    }
    final definitions = this.definitions();
    if (definitions.isNotEmpty) blocks.add(definitions);
    final text = [for (final b in blocks) b.join('\n')].join('\n\n');
    return text.isEmpty || chance(0.2) ? text : '$text\n';
  }
}

// ------------------------------------------------------------- sessions

const _pastes = [
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit.',
  'https://example.com/docs/getting-started?tab=install#flutter',
  '- first item\n- second item\n  - nested item\n- third item\n',
  '1. one\n2. two\n3. three',
  '- [ ] write tests\n- [x] fix the parser',
  "```dart\nvoid main() {\n  print('hi');\n}\n```\n",
  '| Name | Value |\n| ---- | ----- |\n| a | 1 |\n| b | 2 |\n',
  '## Notes\n\nSome **bold** text with a [link](https://x.y).\n\n> A quote\n\n'
      '1. one\n2. two\n',
  '> quoted line\n> another line',
  'line one\r\nline two\r\n',
  '日本語のテキストと English mixed 🎉',
  '`inline code`',
  '**important**',
  '![diagram](docs/diagram.png)',
  'See the note[^1].\n\n[^1]: The footnote text.',
  '<details>\n<summary>More</summary>\n\nHidden text.\n\n</details>\n',
  'word',
  '    indented code line\n',
  '---',
  '\ttabbed text',
  'Two\n\nparagraphs',
  'trailing space ',
];

/// Misspellings a keyboard corrects, and their corrections.
const _typos = [
  ('teh', 'the'),
  ('recieve', 'receive'),
  ('adn', 'and'),
  ('wiht', 'with'),
  ('taht', 'that'),
  ('becuase', 'because'),
  ('seperate', 'separate'),
  ('definately', 'definitely'),
  ('dont', "don't"),
  ('monday', 'Monday'),
  ('i', 'I'),
  ('cafe', 'café'),
];

/// Input-method preedit runs and what each commits.
const _compositions = [
  (['n', 'に', 'にh', 'にほ', 'にほn', 'にほん'], '日本'),
  (['n', 'ni', 'ni h', 'ni ha', 'ni hao'], '你好'),
  (['ㅎ', '하', '한'], '한'),
  (['h', 'he', 'hel', 'hell', 'hello'], 'hello'),
  (['t', 'te', 'teh'], 'the'),
  (['V', 'Vi', 'Vie', 'Viê', 'Việ', 'Việt'], 'Việt'),
  (['s', 'sm', 'smi', 'smil', 'smile'], '😊'),
];

const _styles = [Style.strong, Style.emphasis, Style.code, Style.strikethrough];

/// The state history restores, as the matrix records it.
typedef _State = (String, FlarkSelection, int);

final class _Author {
  _Author(this.backend, this.seed, this.stats) : r = Random(seed) {
    writer = _Writer(r);
  }

  final FlarkParseBackend backend;
  final int seed;
  final AuthoringStats? stats;
  final Random r;
  late final _Writer writer;
  FlarkEditor? _editor;
  FlarkEditor get editor => _editor!;
  String first = '';
  final log = <String>[];
  final reached = <_State>{};
  var time = Duration.zero;

  /// The state before the command being checked, for the failure report.
  ({String source, FlarkSelection selection, String command})? current;

  T pick<T>(List<T> items) => items[r.nextInt(items.length)];
  bool chance(double p) => r.nextDouble() < p;
  int count(int low, int high) => low + r.nextInt(high - low + 1);

  bool get live => !editor.sourceMode;

  /// Whether Indent keeps the caret row inside the live tier's eight levels
  /// of quotes and items.
  bool get _shallow => !live || editor.document.caretRow.shells.length < 6;
  List<ProjectedRow> get rows => editor.projection.rows;

  String report(Object error) {
    final failing = current;
    return 'authoring failure: seed $seed\n'
        '  document ${jsonEncode(first)}\n'
        '${failing == null ? '' : '  before ${jsonEncode(failing.source)} ${failing.selection}\n'
                  '  command ${failing.command}\n'}'
        '${_editor == null ? '' : '  result ${jsonEncode(editor.source)} ${editor.selection}\n'}'
        '  log:\n    ${log.join('\n    ')}\n'
        '  error: $error';
  }

  void run() {
    // Documents stay inside the live tier's 16 KiB with room for edits.
    var text = writer.document();
    while (utf8.encode(text).length > 12 * 1024) {
      text = writer.document();
    }
    if (r.nextInt(3) == 0) text = text.replaceAll('\n', '\r\n');
    if (r.nextInt(12) == 0) text = '\uFEFF$text';
    first = text;
    _editor = FlarkEditor(
      backend,
      text: text,
      caret: chance(0.5) ? 0 : r.nextInt(text.length + 1),
    );
    matrix.checkStep(editor, 'seed $seed load');
    checkRowOrder(editor, 'seed $seed load');
    reached.add(matrix.stateOf(editor));
    // History keeps 100 entries. Ending sessions before that many keeps
    // their first source reachable for the walk below, and ending them
    // short of the live tier's 16 KiB, with room for one action's largest
    // paste, keeps them rendered.
    final actions = count(8, 40);
    for (
      var i = 0;
      i < actions &&
          editor.history.openGroup < 80 &&
          utf8.encode(editor.source).length < 12 * 1024;
      i++
    ) {
      time += Duration(milliseconds: count(300, 2500));
      _act();
    }
    current = null;
    final host = Random(seed ^ 0x5eed);
    for (var k = 0; k < 2; k++) {
      matrix.hostAction(host, editor, log, 'seed $seed host $k');
      reached.add(matrix.stateOf(editor));
    }
    matrix.checkHistory(editor, first, reached, 'seed $seed');
  }

  void _act() {
    final k = r.nextInt(133);
    switch (k) {
      case < 20:
        typeWords();
      case < 26:
        newParagraph();
      case < 38:
        newBlock();
      case < 45:
        continueContainer();
      case < 55:
        backspaceRun();
      case < 59:
        deleteRun();
      case < 64:
        wordDeletes();
      case < 72:
        selectWordThen();
      case < 76:
        selectLineThen();
      case < 80:
        shiftSelectThen();
      case < 85:
        autocorrect();
      case < 91:
        compose();
      case < 97:
        paste();
      case < 100:
        cutAndPaste();
      case < 106:
        undoRedo();
      case < 115:
        format();
      case < 120:
        navigate();
      case < 123:
        deleteToLineStart();
      case < 128:
        splitRow();
      case < 132:
        dragSelectThen();
      default:
        selectAllThen();
    }
  }

  // --------------------------------------------------------- commands

  /// Applies [command] [gap] milliseconds after the last one and checks it.
  bool step(FlarkCommand command, {int? gap}) {
    time += Duration(milliseconds: gap ?? count(60, 180));
    final description = _describe(command);
    log.add(description);
    final label = 'seed $seed step ${log.length - 1} $description';
    final before = editor.snapshot, state = matrix.stateOf(editor);
    final revision = editor.revision;
    current = (
      source: editor.source,
      selection: editor.selection,
      command: description,
    );
    final context = editor.typingContext;
    final applied = editor.apply(command, at: time);
    stats?.count(
      command,
      applied: applied,
      rejection: editor.lastRejection,
      before: before,
      context: context,
      label: label,
    );
    // Sessions stay inside the live tier's limits, so source mode means a
    // command gave up rendering.
    expect(editor.sourceMode, isFalse, reason: '$label: left rendered mode');
    if (applied) {
      matrix.checkStep(editor, label);
      checkRowOrder(editor, label);
      matrix.checkStructure(before, command, editor, label, backend);
      // Composed text goes in as it is; its commit is checked as typing.
      if (!editor.composing) {
        checkCaretInEdit(before, command, editor, label);
        checkVisibleEdit(before, command, editor, label);
        checkEditKeepsRows(before, command, editor, label);
      }
      if (command is Undo || command is Redo) {
        expect(
          reached,
          contains(matrix.stateOf(editor)),
          reason: '$label: history restored a state the session never had',
        );
      }
    } else {
      expect(matrix.stateOf(editor), state, reason: '$label: refused');
      expect(editor.revision, revision, reason: '$label: refused');
      checkMustApply(before, command, editor, label);
    }
    if (!editor.composing) reached.add(matrix.stateOf(editor));
    return applied;
  }

  /// A host call that is not a command: composition begin, commit, cancel.
  void hostCall(String description, void Function() call) {
    log.add(description);
    final label = 'seed $seed step ${log.length - 1} $description';
    current = (
      source: editor.source,
      selection: editor.selection,
      command: description,
    );
    call();
    matrix.checkStep(editor, label);
    if (!editor.composing) reached.add(matrix.stateOf(editor));
  }

  void type(String text, {int low = 55, int high = 190}) {
    for (final g in text.characters) {
      step(g == '\n' ? const Newline() : InsertText(g), gap: count(low, high));
    }
  }

  // --------------------------------------------------------- places

  List<int> _stops(String text) {
    final stops = [0];
    var offset = 0;
    for (final g in text.characters) {
      offset += g.length;
      stops.add(offset);
    }
    return stops;
  }

  ProjectedRow? _row({bool Function(ProjectedRow row)? where}) {
    if (!live) return null;
    final candidates = rows.where(where ?? (_) => true).toList();
    return candidates.isEmpty ? null : pick(candidates);
  }

  bool _prose(ProjectedRow row) =>
      row.kind == RowKind.paragraph ||
      row.kind == RowKind.heading ||
      row.kind == RowKind.blank;

  /// A click: on a row's text, its start or its end.
  void click({ProjectedRow? row, bool? end}) {
    if (!live) {
      step(SetSelection.caret(r.nextInt(editor.source.length + 1)));
      return;
    }
    final target = row ?? pick<ProjectedRow>(rows);
    final offset = switch (end) {
      true => target.text.length,
      false => 0,
      null => pick(_stops(target.text)),
    };
    step(PlaceCaret(target.index, offset, leadingHalf: r.nextBool()));
  }

  void maybeClick() {
    if (chance(0.45)) click(end: chance(0.4) ? true : null);
  }

  /// The display range of a word in [row], if it shows one.
  (int, int)? _word(ProjectedRow row) {
    final words = RegExp(r'\S+').allMatches(row.text).toList();
    if (words.isEmpty) return null;
    final w = pick(words);
    return (w.start, w.end);
  }

  /// Selects a word as a double-click does.
  bool selectWord() {
    if (!live) return false;
    final row = _row(where: (row) => row.text.trim().isNotEmpty);
    if (row == null) return false;
    final (start, end) = _word(row)!;
    return step(
      SetSelection(
        row.sourceForDisplay(start, anchor: Anchor.after),
        row.sourceForDisplay(end, anchor: Anchor.before),
      ),
    );
  }

  bool get _afterWord {
    if (!live || !editor.selection.isCollapsed) return false;
    final at = editor.document.displayOf(editor.selection.extent);
    final text = rows[at.row].text;
    return at.offset > 0 &&
        text.substring(at.offset - 1, at.offset).trim() != '';
  }

  // --------------------------------------------------------- what people type

  /// Words as a person types them, sometimes with inline Markdown typed in
  /// order: `**bold**`, `_em_`, backticks, links and images.
  String phrase() {
    final words = [
      for (var i = count(1, 8); i > 0; i--)
        switch (r.nextInt(32)) {
          0 => '**${writer.word()}**',
          1 => '_${writer.word()}_',
          2 => '*${writer.word()}*',
          3 => '`${writer.word()}`',
          4 => '~~${writer.word()}~~',
          5 => '[${writer.word()}](https://example.com)',
          6 => '![${writer.word()}](img.png)',
          7 => '**${writer.word()} ${writer.word()}**',
          _ => writer.word(),
        },
    ];
    return '${words.join(' ')}${chance(0.4) ? pick(['.', ',', '!', '?', ':']) : ''}';
  }

  void typeWords() {
    maybeClick();
    type('${_afterWord && chance(0.8) ? ' ' : ''}${phrase()}');
  }

  /// Return at the end of a row, then a new paragraph: Return twice, or a
  /// paragraph break.
  void newParagraph() {
    final row = _row(where: _prose);
    if (row != null) click(row: row, end: true);
    if (chance(0.6)) {
      step(const Newline());
      step(const Newline());
    } else {
      step(const Newline(paragraph: true));
    }
    type(writer.capitalized(phrase()));
  }

  /// A new block typed from an empty line: Markdown shortcuts in order.
  void newBlock() {
    final row = _row(where: _prose);
    if (row != null) click(row: row, end: true);
    step(const Newline());
    if (live && editor.document.caretRow.text.isNotEmpty) {
      step(const Newline());
    }
    if (chance(0.3)) step(const Newline());
    switch (r.nextInt(13)) {
      case 0:
        type('# ${writer.capitalized(writer.word())} ${writer.word()}');
      case 1:
        type('## ${writer.capitalized(phrase())}');
        if (chance(0.5)) {
          step(const Newline());
          type(writer.capitalized(phrase()));
        }
      case 2:
        typeList('- ');
      case 3:
        typeList('1. ');
      case 4:
        typeList('- [ ] ');
      case 5:
        type('> ${writer.capitalized(phrase())}');
        for (var i = count(0, 2); i > 0; i--) {
          step(const Newline());
          type(writer.capitalized(phrase()));
        }
        step(const Newline());
        step(const Newline());
        type(writer.capitalized(phrase()));
      case 6:
        typeFence();
      case 7:
        type('---');
        step(const Newline());
        type(writer.capitalized(phrase()));
      case 8:
        typeTable();
      case 9:
        type('[^${pick(_footnotes)}]: ${writer.capitalized(phrase())}');
      case 10:
        type('![${writer.word()}](images/${pick(_slugs)}.png)');
      case 11:
        type('<details>');
        step(const Newline());
        type('<summary>${writer.word()}</summary>');
        step(const Newline());
        step(const Newline());
        type(writer.capitalized(phrase()));
      default:
        type('*${writer.word()}* and **${writer.word()}** with `code`');
    }
  }

  /// Items typed after a marker: Return continues the list, Tab nests the
  /// new item, and Return on an empty item leaves the list.
  void typeList(String marker) {
    type('$marker${writer.capitalized(phrase())}');
    for (var i = count(1, 3); i > 0; i--) {
      step(const Newline());
      if (chance(0.25) && _shallow) step(const Indent());
      if (chance(0.1)) step(const Outdent());
      type(writer.capitalized(phrase()));
    }
    if (chance(0.7)) {
      step(const Newline());
      step(const Newline());
      if (chance(0.3)) step(const Newline());
      type(writer.capitalized(phrase()));
    }
  }

  /// Three backticks complete a fence; code lines follow, and Return twice
  /// after code leaves it.
  void typeFence() {
    type(chance(0.85) ? '```' : '~~~');
    final language = pick(_code.keys.toList());
    if (chance(0.4)) step(SetCodeLanguage(language));
    final lines = _code[language]!;
    for (var i = 0; i < lines.length; i++) {
      if (i > 0) step(const Newline());
      type(lines[i], low: 40, high: 120);
    }
    if (chance(0.8)) {
      step(const Newline());
      step(const Newline());
      type(writer.capitalized(phrase()));
    }
  }

  /// A table typed as Markdown: a header row, the delimiter row, and cells
  /// filled with Tab.
  void typeTable() {
    final columns = count(2, 3);
    type(
      '| ${[for (var c = 0; c < columns; c++) writer.capitalized(writer.word())].join(' | ')} |',
    );
    step(const Newline());
    type('| ${List.filled(columns, '---').join(' | ')} |');
    step(const Newline());
    for (var c = 0; c < columns * count(1, 2); c++) {
      if (c > 0) step(const MoveTableCell());
      type(writer.word());
    }
  }

  /// Return at the end of a list item or quote line continues it, and a
  /// second Return on the empty line leaves it.
  void continueContainer() {
    final row = _row(where: (row) => row.shells.isNotEmpty && !row.fenced);
    if (row == null) return typeWords();
    click(row: row, end: true);
    step(const Newline());
    if (chance(0.3)) {
      step(chance(0.7) && _shallow ? const Indent() : const Outdent());
    }
    if (chance(0.35)) step(const Newline());
    type(writer.capitalized(phrase()));
  }

  /// Return inside a row's text splits it: a paragraph, heading, item or
  /// quote line, sometimes inside a styled span.
  void splitRow() {
    final row = _row(where: (row) => row.text.trim().isNotEmpty);
    if (row == null) return typeWords();
    final stops = _stops(row.text);
    step(
      PlaceCaret(
        row.index,
        stops[r.nextInt(stops.length)],
        leadingHalf: r.nextBool(),
      ),
    );
    step(Newline(paragraph: chance(0.3)));
    if (chance(0.5)) type(writer.capitalized(phrase()));
  }

  void backspaceRun() {
    // From a row's start, Backspace lifts and joins structure.
    if (chance(0.45)) {
      click(end: false);
    } else {
      maybeClick();
    }
    for (var i = count(1, 14); i > 0; i--) {
      step(const DeleteBackward(), gap: count(40, 140));
    }
    if (chance(0.5)) type(phrase());
  }

  void deleteRun() {
    if (chance(0.5)) {
      click(end: true);
    } else {
      maybeClick();
    }
    for (var i = count(1, 6); i > 0; i--) {
      step(const DeleteForward(), gap: count(40, 140));
    }
    if (chance(0.4)) type(phrase());
  }

  void wordDeletes() {
    maybeClick();
    final backward = chance(0.8);
    for (var i = count(1, 3); i > 0; i--) {
      step(
        backward
            ? const DeleteBackward(word: true)
            : const DeleteForward(word: true),
        gap: count(120, 300),
      );
    }
    if (chance(0.6)) type(phrase());
  }

  /// What a person does with a selection.
  void _withSelection() {
    switch (r.nextInt(10)) {
      case 0 || 1 || 2:
        type(writer.word());
      case 3:
        step(const DeleteBackward());
        if (chance(0.5)) type(writer.word());
      case 4:
        step(ToggleStyle(pick(_styles)));
      case 5:
        step(Paste(pick(_pastes)));
      case 6:
        step(const SetLink('https://example.com/page'));
      case 7:
        compose();
      case 8:
        step(const DeleteForward());
      default:
        step(SetStyle(pick(_styles), enabled: r.nextBool()));
    }
  }

  void selectWordThen() {
    if (!selectWord()) return typeWords();
    _withSelection();
  }

  /// Home, then Shift-End: the caret row's line.
  void selectLineThen() {
    maybeClick();
    step(const MoveCaret(MoveDirection.backward, unit: MoveUnit.line));
    step(
      const MoveCaret(MoveDirection.forward, unit: MoveUnit.line, extend: true),
    );
    if (chance(0.25)) {
      step(SetHeadingLevel(count(0, 3)));
    } else {
      _withSelection();
    }
  }

  /// A drag from one row to a nearby one, then what people do with it.
  void dragSelectThen() {
    if (!live) return typeWords();
    final from = pick<ProjectedRow>(rows);
    final to = rows[(from.index + count(-2, 2)).clamp(0, rows.length - 1)];
    click(row: from);
    step(
      PlaceCaret(
        to.index,
        pick(_stops(to.text)),
        leadingHalf: r.nextBool(),
        extend: true,
      ),
    );
    _withSelection();
  }

  void shiftSelectThen() {
    maybeClick();
    final forward = chance(0.6);
    for (var i = count(1, 8); i > 0; i--) {
      step(
        MoveCaret(
          forward ? MoveDirection.forward : MoveDirection.backward,
          unit: chance(0.7) ? MoveUnit.grapheme : MoveUnit.word,
          extend: true,
        ),
      );
    }
    _withSelection();
  }

  /// A keyboard's corrections: a finished word replaced after its space,
  /// with the caret put back after the space; a suggestion that replaces the
  /// word being typed and adds the space; and the double-space period.
  void autocorrect() {
    maybeClick();
    final (wrong, right) = pick(_typos);
    if (_afterWord) type(' ');
    type(wrong);
    if (!live) return;
    final caret = editor.selection.extent;
    final row = editor.document.caretRow;
    final at = editor.document.displayOf(caret).offset;
    final start = at - wrong.length;
    if (start < 0 || row.text.substring(start, at) != wrong) return;
    final from = row.sourceForDisplay(start, anchor: Anchor.after);
    switch (r.nextInt(3)) {
      case 0:
        type(' ');
        step(ReplaceRange(from, caret, right));
        step(const MoveCaret(MoveDirection.forward));
      case 1:
        step(ReplaceRange(from, caret, '$right '));
      default:
        type(' ');
        step(ReplaceRange(caret, caret, '.'));
        step(const MoveCaret(MoveDirection.forward));
    }
    if (chance(0.6)) type(phrase());
  }

  /// An input method's preedit runs: the first preedit is typed at the
  /// selection, later ones replace the composing range, which the source
  /// holds as the platform does; the run commits, typed, or is cancelled.
  void compose() {
    if (editor.composing) return;
    if (_afterWord && editor.selection.isCollapsed) type(' ');
    final (preedits, committed) = pick(_compositions);
    final before = matrix.stateOf(editor), start = editor.snapshot;
    final undo = editor.history.undoTarget, redo = editor.history.redoTarget;
    hostCall('beginComposition()', editor.beginComposition);
    final at = editor.selection.start;
    var composed = '';
    for (final preedit in [...preedits, committed]) {
      final applied = composed.isEmpty
          ? step(InsertText(preedit), gap: count(80, 200))
          : step(
              ReplaceRange(at, at + composed.length, preedit),
              gap: count(80, 200),
            );
      if (applied) composed = preedit;
    }
    if (chance(0.8)) {
      hostCall('commitComposition()', editor.commitComposition);
      final label = 'seed $seed step ${log.length - 1} commitComposition()';
      if (editor.lastRejection != null) {
        // Typing refused what was composed, which is withdrawn.
        expect(matrix.stateOf(editor), before, reason: '$label: withdrawn');
      } else if (composed.isNotEmpty) {
        final typed = InsertText(composed);
        matrix.checkStructure(start, typed, editor, label, backend);
        checkCaretInEdit(start, typed, editor, label);
        checkVisibleEdit(start, typed, editor, label);
        checkEditKeepsRows(start, typed, editor, label);
      }
      if (chance(0.5)) type(' ${phrase()}');
    } else {
      // Cancelling restores the state before the preedit and leaves history
      // as it was.
      hostCall('cancelComposition()', editor.cancelComposition);
      expect(matrix.stateOf(editor), before, reason: 'seed $seed: cancel');
      expect(
        identical(editor.history.undoTarget, undo) &&
            identical(editor.history.redoTarget, redo),
        isTrue,
        reason: 'seed $seed: cancel kept history',
      );
    }
  }

  void paste() {
    maybeClick();
    final text = chance(0.8)
        ? pick(_pastes)
        : (chance(0.5)
              ? writer.paragraph().join('\n')
              : writer.list(1).join('\n'));
    step(Paste(chance(0.25) ? text.replaceAll('\n', '\r\n') : text));
  }

  /// Cut a selection and paste its visible text elsewhere.
  void cutAndPaste() {
    if (!selectWord()) return paste();
    if (chance(0.5)) {
      for (var i = count(1, 4); i > 0; i--) {
        step(
          const MoveCaret(
            MoveDirection.forward,
            unit: MoveUnit.word,
            extend: true,
          ),
        );
      }
    }
    if (!live || editor.selection.isCollapsed) return;
    final text = editor.document.visibleText(
      editor.selection.start,
      editor.selection.end,
    );
    step(const DeleteBackward());
    click(end: chance(0.5) ? true : null);
    step(Paste(text));
  }

  void undoRedo() {
    final undos = count(1, 5);
    for (var i = undos; i > 0; i--) {
      step(const Undo(), gap: count(150, 450));
    }
    if (chance(0.6)) {
      for (var i = count(1, undos); i > 0; i--) {
        step(const Redo(), gap: count(150, 450));
      }
    } else if (chance(0.5)) {
      type(phrase());
    }
  }

  void format() {
    switch (r.nextInt(10)) {
      case 0 || 1:
        if (selectWord()) step(ToggleStyle(pick(_styles)));
      case 2:
        // Command-B, a word, Command-B, more words.
        maybeClick();
        final style = pick(_styles);
        if (_afterWord) type(' ');
        step(ToggleStyle(style));
        type(writer.word());
        step(ToggleStyle(style));
        type(' ${writer.word()}');
      case 3:
        final row = _row(where: chance(0.7) ? _prose : null);
        if (row != null) click(row: row);
        step(SetHeadingLevel(count(0, 4)));
      case 4:
        final row = _row(
          where: (row) => row.shells.any(
            (s) => chance(0.3) ? s.kind == ShellKind.item : s.task,
          ),
        );
        if (row != null) click(row: row);
        step(const ToggleTask());
      case 5:
        // Tab and Shift-Tab: table cells move, anything else indents.
        final row = _row(
          where: (row) =>
              row.shells.any((s) => s.kind == ShellKind.item) ||
              row.kind == RowKind.tableCell,
        );
        if (row != null) click(row: row);
        final backward = chance(0.4);
        step(
          live && editor.document.caretRow.kind == RowKind.tableCell
              ? MoveTableCell(backward: backward)
              : (backward || !_shallow ? const Outdent() : const Indent()),
        );
        if (chance(0.5)) type(writer.word());
      case 6:
        if (chance(0.3)) {
          // A link or image inserted at the caret from the link dialog.
          maybeClick();
          step(
            chance(0.6)
                ? SetLink('https://example.com/new', text: writer.word())
                : SetImage('images/new.png', alt: writer.word()),
          );
        } else if (selectWord()) {
          step(
            chance(0.7)
                ? const SetLink('https://example.com/x')
                : const RemoveLink(),
          );
        }
      case 7:
        final row = _row(where: (row) => row.fenced);
        if (row == null) return;
        click(row: row);
        step(SetCodeLanguage(pick(['', 'dart', 'python', 'text', 'json'])));
      case 8:
        if (selectWord()) step(SetStyle(pick(_styles), enabled: chance(0.6)));
      default:
        final row = _row(where: (row) => row.fenced);
        if (row == null) return;
        click(row: row);
        step(const SelectAll());
        step(Paste(_code[pick(_code.keys.toList())]!.join('\n')));
    }
  }

  void navigate() {
    if (chance(0.6)) click();
    for (var i = count(1, 6); i > 0; i--) {
      step(
        MoveCaret(
          chance(0.5) ? MoveDirection.forward : MoveDirection.backward,
          unit: pick(MoveUnit.values),
          extend: chance(0.15),
        ),
      );
    }
    if (chance(0.5)) type(phrase());
  }

  /// Command-Backspace: from the caret to the start of its line.
  void deleteToLineStart() {
    maybeClick();
    if (!live || !editor.selection.isCollapsed) {
      step(const DeleteBackward());
      return;
    }
    final caret = editor.selection.extent;
    final at = editor.document.displayOf(caret);
    final row = rows[at.row];
    final lineStart = row.text.lastIndexOf('\n', max(0, at.offset - 1)) + 1;
    final edge = at.offset == 0
        ? caret
        : row.sourceForDisplay(
            lineStart > at.offset ? 0 : lineStart,
            anchor: Anchor.after,
          );
    step(edge < caret ? ReplaceRange(edge, caret, '') : const DeleteBackward());
    if (chance(0.5)) type(phrase());
  }

  void selectAllThen() {
    step(const SelectAll());
    if (chance(0.5)) step(const SelectAll());
    if (chance(0.5)) {
      type(writer.capitalized(phrase()));
    } else {
      step(Paste(pick(_pastes)));
    }
  }
}

/// Rows show in source order: each starts at or after the one before it, so
/// text never shows above source that precedes it.
void checkRowOrder(FlarkEditor editor, String label) {
  final rows = editor.projection.rows;
  for (var i = 1; i < rows.length; i++) {
    if (rows[i].sourceStart < rows[i - 1].sourceStart) {
      fail(
        '$label: row $i (${jsonEncode(rows[i].text)} from '
        '${rows[i].sourceStart}) shows after row ${i - 1} '
        '(${jsonEncode(rows[i - 1].text)} from ${rows[i - 1].sourceStart}) '
        'in ${jsonEncode(editor.source)}',
      );
    }
  }
}

/// Text inserted, pasted or put over a range leaves the caret in the
/// source it changed or at its end, wherever whitespace normalization,
/// fence completion or cell escaping put that text: never in source the
/// edit did not touch, unless only hidden markup on the same line lies
/// between, or the edit completed an inline construct the caret ends, or
/// the text was pasted and all of it is markup the parser hides. The
/// changed region is the span of the smallest differences, from the
/// leftmost to the rightmost alignment, since repeated characters admit
/// several. Inserted text that ends as the text after it begins reads as
/// unchanged there, so the caret may also follow that much more of it, on
/// the same line unless the text has line breaks of its own.
void checkCaretInEdit(
  FlarkEditorSnapshot before,
  FlarkCommand command,
  FlarkEditor editor,
  String label,
) {
  final inserted = switch (command) {
    InsertText(:final text) || Paste(:final text) => text,
    ReplaceRange(:final text) => text,
    _ => '',
  };
  if (inserted.isEmpty) return;
  final a = before.source, b = editor.source;
  if (a == b) return;
  final caret = editor.selection.extent;
  final shorter = min(a.length, b.length);
  var p = 0;
  while (p < shorter && a.codeUnitAt(p) == b.codeUnitAt(p)) {
    p++;
  }
  var s = 0;
  while (s < shorter - p &&
      a.codeUnitAt(a.length - 1 - s) == b.codeUnitAt(b.length - 1 - s)) {
    s++;
  }
  var q = 0;
  while (q < shorter &&
      a.codeUnitAt(a.length - 1 - q) == b.codeUnitAt(b.length - 1 - q)) {
    q++;
  }
  var o = 0;
  while (o < shorter - q && a.codeUnitAt(o) == b.codeUnitAt(o)) {
    o++;
  }
  final end = b.length - s;
  // Pasted and replaced text keeps its literal meaning: when all of it is
  // markup the parser hides, a pasted setext underline or table delimiter
  // row, the caret has nowhere in it to go.
  final literal = command is! InsertText;
  final lineBreak = RegExp('[\r\n]');
  bool repeats(String gap) =>
      (!gap.contains(lineBreak) || inserted.contains(lineBreak)) &&
      _bare(inserted).endsWith(_bare(gap));
  bool hidden(int from, int to, {bool lines = false}) {
    if (editor.sourceMode) return false;
    if (!lines && b.substring(from, to).contains(lineBreak)) return false;
    return !editor.projection.rows.any(
      (row) => row.segments.any(
        (segment) =>
            !segment.lineBreak &&
            segment.sourceStart < to &&
            segment.sourceEnd > from &&
            segment.sourceEnd > segment.sourceStart,
      ),
    );
  }

  bool completed() =>
      !editor.sourceMode &&
      editor.document
          .ownersTouching(caret)
          .any((owner) => owner.start <= o && owner.end == caret);
  // The inserted text's last line, which ends right before a caret at the
  // end of the text, wherever prefixes went on the lines before it.
  final tail = inserted.substring(inserted.lastIndexOf('\n') + 1);
  if (!((o <= caret && caret <= end) ||
      (caret > end &&
          (repeats(b.substring(end, caret)) ||
              (tail.isNotEmpty && b.substring(0, caret).endsWith(tail)) ||
              hidden(end, caret, lines: literal) ||
              completed())) ||
      (caret < o && hidden(caret, o)) ||
      (literal && hidden(o, end, lines: true)))) {
    fail(
      '$label: caret $caret ${caret < o ? 'before' : 'after'} the edit '
      '$o..$end, ${jsonEncode(a)} -> ${jsonEncode(b)}',
    );
  }
}

/// Plain text typed into one row, or prose deleted inside one, changes that
/// row only: every other row keeps its kind and its containers, and the
/// edited row stays in its containers. Typing on the empty line between
/// blocks must not pull the block after it into a table or paragraph, nor
/// carry the text into a quote or list item above; emptying a row must not
/// end its list item or table. Joins at a row's edge are
/// `matrix.checkStructure`'s. Text typed after a line's bare markup, which it
/// may complete (`- ` becomes an item once it has text), is left out, as are
/// edits beside delimiters and edits a letter or a deleted character can
/// make or stop matching a reference, entity, tag or HTML block ([_tagged]),
/// as in [checkVisibleEdit].
void checkEditKeepsRows(
  FlarkEditorSnapshot before,
  FlarkCommand command,
  FlarkEditor editor,
  String label,
) {
  if (before is! FlarkLiveSnapshot || editor.sourceMode) return;
  final old = before.document, next = editor.document;
  final sel = old.selection, rows = old.projection.rows;
  final a = old.source, b = next.source;
  if (!sel.isCollapsed && sel.start == 0 && sel.end == a.length) return;
  late int start, end;
  var typed = false;
  switch (command) {
    case InsertText(:final text) || Paste(:final text)
        when sel.isCollapsed && _plain.hasMatch(text):
      start = end = sel.extent;
      typed = true;
    case DeleteBackward(:final word) || DeleteForward(:final word)
        when sel.isCollapsed:
      final at = old.displayOf(sel.extent);
      final row = rows[at.row];
      final backward = command is DeleteBackward;
      final int from, to;
      if (backward) {
        if (at.offset == 0) return;
        from = word
            ? _wordStart(row.text, at.offset)
            : at.offset -
                  row.text.substring(0, at.offset).characters.last.length;
        to = at.offset;
      } else {
        if (at.offset == row.text.length) return;
        from = at.offset;
        to = word
            ? _wordEnd(row.text, at.offset)
            : at.offset + row.text.substring(at.offset).characters.first.length;
      }
      if (from == to || !_prose.hasMatch(row.text.substring(from, to))) return;
      start = row.sourceForDisplay(from, anchor: Anchor.after);
      end = row.sourceForDisplay(to, anchor: Anchor.before);
    case DeleteBackward() || DeleteForward():
      final x = old.displayOf(sel.start), y = old.displayOf(sel.end);
      if (x.row != y.row ||
          !_prose.hasMatch(rows[x.row].text.substring(x.offset, y.offset))) {
        return;
      }
      start = sel.start;
      end = sel.end;
    default:
      return;
  }
  bool collides(String text, int offset) =>
      offset >= 0 && offset < text.length && '_*~`'.contains(text[offset]);
  final landed = next.selection.extent;
  if (collides(a, start - 1) ||
      collides(a, end) ||
      collides(b, landed - 1) ||
      collides(b, landed)) {
    return;
  }
  // After a line's bare markup, typed text can complete the block that
  // markup opens.
  final lineStart = a.lastIndexOf('\n', max(0, start - 1)) + 1;
  if (typed &&
      lineStart < start &&
      _markupOnly.hasMatch(a.substring(lineStart, start))) {
    return;
  }
  final edited = old.displayOf(start).row;
  final caret = next.displayOf(landed).row;
  if (_tagged(old, next, start, end, landed)) return;
  String shells(ProjectedRow row) =>
      row.shells.map((shell) => shell.kind.name).join('/');
  // A deletion that empties its line leaves no text to keep: a blank line
  // belongs to whichever container its neighbours give it.
  final emptied =
      next.projection.rows[caret].text.isEmpty &&
      next.projection.rows[caret].kind == RowKind.blank;
  if (!emptied && shells(next.projection.rows[caret]) != shells(rows[edited])) {
    fail(
      '$label: the edited text left the containers of its row '
      '(${shells(rows[edited])} to ${shells(next.projection.rows[caret])}), '
      '${jsonEncode(a)} -> ${jsonEncode(b)}',
    );
  }
  final delta = b.length - a.length;
  final starting = <int, List<ProjectedRow>>{};
  for (final row in next.projection.rows) {
    (starting[row.sourceStart] ??= []).add(row);
  }
  for (final row in rows) {
    if (row.index == edited || row.kind == RowKind.blank) continue;
    final int mapped;
    if (row.sourceEnd < start) {
      mapped = row.sourceStart;
    } else if (row.sourceStart >= end) {
      mapped = row.sourceStart + delta;
    } else {
      continue;
    }
    final now =
        starting[mapped]?.where((now) => now.kind == row.kind).firstOrNull ??
        next.rowAt(mapped);
    if (now.kind != row.kind || shells(now) != shells(row)) {
      fail(
        '$label: editing row $edited changed row ${row.index} '
        '${jsonEncode(row.text)} from ${row.kind.name} ${shells(row)} to '
        '${now.kind.name} ${shells(now)}, ${jsonEncode(a)} -> '
        '${jsonEncode(b)}',
      );
    }
  }
}

/// Words and sentence punctuation: prose whose deletion should leave the
/// blocks around it as they are.
final _prose = RegExp(
  r'^[\p{L}\p{M}\p{Nd}\p{Extended_Pictographic}\u{200D}\u{FE0F}'
  r'''\u{1F1E6}-\u{1F1FF}\u{1F3FB}-\u{1F3FF} .,!?:;'"]+$''',
  unicode: true,
);

/// Source before the caret on its line that holds only block markup.
final _markupOnly = RegExp(r'^[\s>#*+\-=`~|:.)\[\]\d]*$');

/// The editor's word boundaries: runs of non-whitespace.
int _wordStart(String text, int offset) {
  var i = offset;
  while (i > 0 && text[i - 1].trim().isEmpty) {
    i--;
  }
  while (i > 0 && text[i - 1].trim().isNotEmpty) {
    i--;
  }
  return i;
}

int _wordEnd(String text, int offset) {
  var i = offset;
  while (i < text.length && text[i].trim().isEmpty) {
    i++;
  }
  while (i < text.length && text[i].trim().isNotEmpty) {
    i++;
  }
  return i;
}

/// Letters, marks and emoji: text no Markdown construct opens, closes or
/// completes.
final _plain = RegExp(
  r'^[\p{L}\p{M}\p{Extended_Pictographic}\u{200D}\u{FE0F}'
  r'\u{1F1E6}-\u{1F1FF}\u{1F3FB}-\u{1F3FF}]+$',
  unicode: true,
);

/// Plain text typed, pasted or put over a range, or a plain grapheme or
/// selection deleted, changes what shows only there. What shows afterwards,
/// whitespace aside since rows split and join, is what showed with the
/// selection's text replaced, or that with markup hidden: typed letters may
/// complete a construct (`**` around a letter becomes emphasis), but nothing
/// hidden may show, and what markup hides is punctuation: no letter or digit
/// vanishes beyond the deleted text. With nothing hidden, the caret shows
/// right after the edit. Edits beside `_`, `*`, `~` or a backtick, whose
/// pairing an adjacent letter can decide (delimiter collisions the edit
/// profile leaves unsupported), and edits a letter or a deleted character
/// can make or stop matching a reference, entity, tag or HTML block
/// ([_tagged]) are left out.
void checkVisibleEdit(
  FlarkEditorSnapshot before,
  FlarkCommand command,
  FlarkEditor editor,
  String label,
) {
  if (before is! FlarkLiveSnapshot || editor.sourceMode) return;
  final old = before.document, next = editor.document;
  final sel = old.selection;
  final source = old.source;
  final whole = !sel.isCollapsed && sel.start == 0 && sel.end == source.length;
  late int start, end;
  var text = '';
  switch (command) {
    case InsertText(text: final t) || Paste(text: final t)
        when _plain.hasMatch(t):
      start = sel.start;
      end = sel.end;
      text = t;
    case ReplaceRange(start: final a, end: final b, text: final t)
        when _plain.hasMatch(t) && old.isLegal(a) && old.isLegal(b):
      start = min(a, b);
      end = max(a, b);
      text = t;
    case DeleteBackward(word: false) || DeleteForward(word: false)
        when !sel.isCollapsed && !whole:
      start = sel.start;
      end = sel.end;
    case DeleteBackward(word: false) || DeleteForward(word: false):
      final at = old.displayOf(sel.extent);
      final row = old.projection.rows[at.row];
      final backward = command is DeleteBackward;
      if (backward ? at.offset == 0 : at.offset == row.text.length) return;
      final g = backward
          ? row.text.substring(0, at.offset).characters.last
          : row.text.substring(at.offset).characters.first;
      final from = backward ? at.offset - g.length : at.offset;
      if (!_plain.hasMatch(g) ||
          !row.segments.any(
            (s) =>
                s.exact &&
                s.displayStart <= from &&
                from + g.length <= s.displayEnd,
          )) {
        return;
      }
      start = row.sourceForDisplay(from, anchor: Anchor.after);
      end = row.sourceForDisplay(from + g.length, anchor: Anchor.before);
    default:
      return;
  }
  // A letter beside `_`, `*`, `~` or a backtick can change which delimiters
  // pair, also when a deletion takes an emptied span and brings them
  // together.
  bool collides(String text, int offset) =>
      offset >= 0 && offset < text.length && '_*~`'.contains(text[offset]);
  final landed = next.selection.extent;
  if (!whole &&
      (collides(source, start - 1) ||
          collides(source, end) ||
          collides(next.source, landed - 1) ||
          collides(next.source, landed))) {
    return;
  }
  final a = whole ? null : old.displayOf(start);
  final b = whole ? null : old.displayOf(end);
  final caret = next.displayOf(next.selection.extent);
  if (whole
      ? _tagged(next, next, landed, landed, landed)
      : _tagged(old, next, start, end, landed)) {
    return;
  }
  // After a line's bare markup, typed text can complete the block that
  // markup opens (`1. ` becomes an item once text follows), which then
  // hides it, digits too.
  final lineStart = source.lastIndexOf('\n', max(0, start - 1)) + 1;
  if (text.isNotEmpty &&
      !whole &&
      lineStart < start &&
      _markupOnly.hasMatch(source.substring(lineStart, start))) {
    return;
  }
  final (shown, rowStarts) = _shown(old);
  final i = a == null ? 0 : rowStarts[a.row] + a.offset;
  final j = b == null ? shown.length : rowStarts[b.row] + b.offset;
  final kept = _bare(shown.substring(0, i));
  final (now, nowStarts) = _shown(next);
  final expected = '$kept${_bare(text)}${_bare(shown.substring(j))}';
  final actual = _bare(now);
  var d = 0;
  while (d < expected.length &&
      d < actual.length &&
      expected.codeUnitAt(d) == actual.codeUnitAt(d)) {
    d++;
  }
  String reason() {
    String around(String text) =>
        text.substring(max(0, d - 16), min(text.length, d + 24));
    final edit = old.displayOf(sel.extent);
    return '$label: shows ${jsonEncode(around(actual))} where '
        '${jsonEncode(around(expected))} was expected, editing row '
        '${jsonEncode(old.projection.rows[edit.row].text)} at ${edit.offset}';
  }

  // Hidden markup only: what shows is what was expected with characters
  // left out, and none of them a letter or digit: markup hides punctuation.
  var k = 0;
  for (var m = 0; m < expected.length && k < actual.length; m++) {
    if (expected.codeUnitAt(m) == actual.codeUnitAt(k)) k++;
  }
  if (k < actual.length) fail(reason());
  String words(String text) => text.replaceAll(_notWord, '');
  if (words(actual) != words(expected)) {
    fail('$label: a letter or digit vanished, ${reason()}');
  }
  if (actual == expected &&
      _bare(now.substring(0, nowStarts[caret.row] + caret.offset)).length !=
          kept.length + _bare(text).length) {
    fail('$label: caret not after the edit, ${reason()}');
  }
}

/// What [document] shows, rows joined by line breaks, and where each row
/// starts in it.
(String, List<int>) _shown(FlarkDocument document) =>
    _shownOf[document.projection] ??= () {
      final buffer = StringBuffer();
      final starts = <int>[];
      for (final row in document.projection.rows) {
        starts.add(buffer.length);
        buffer
          ..write(row.text)
          ..write('\n');
      }
      return (buffer.toString(), starts);
    }();

/// A step's document is the next step's previous one: show each once.
final _shownOf = Expando<(String, List<int>)>();

final _space = RegExp(r'\s');

/// Brackets, entities and tags: whether the edit of [start]..[end] in [old],
/// which left the caret at [landed] in [next], is one a letter or a deleted
/// character can make or stop matching a definition, reference, entity, tag
/// or HTML block: the run of non-whitespace around the edit or the caret
/// holds a bracket, `&` or angle bracket, the edit changes a definition or a
/// reference's label, or an HTML block holds it or starts on its line (a
/// letter can complete a start or end condition, and a whole-line tag is
/// one only alone on its line).
bool _tagged(
  FlarkDocument old,
  FlarkDocument next,
  int start,
  int end,
  int landed,
) {
  bool run(String text, int from, int to) {
    while (from > 0 && !_space.hasMatch(text[from - 1])) {
      from--;
    }
    while (to < text.length && !_space.hasMatch(text[to])) {
      to++;
    }
    return text.substring(from, to).contains(_tagCharacters);
  }

  bool html(FlarkDocument doc, int offset) {
    final line = doc.model.lineOfUtf16(offset);
    return doc.rowAt(offset).kind == RowKind.htmlBlock ||
        doc.model.blocks.any(
          (b) => b.kind == BlockKind.htmlBlock && b.firstLine == line,
        );
  }

  final defined = definitionsOf(old), redefined = definitionsOf(next);
  return run(old.source, start, end) ||
      run(next.source, landed, landed) ||
      defined.length != redefined.length ||
      !defined.containsAll(redefined) ||
      inReference(old, start) ||
      inReference(old, end) ||
      inReference(next, landed) ||
      html(old, start) ||
      html(old, end) ||
      html(next, landed);
}

final _tagCharacters = RegExp(r'[\[\]&<>]');

/// What is not a letter, mark or digit.
final _notWord = RegExp(r'[^\p{L}\p{M}\p{N}]', unicode: true);

String _bare(String text) => text.replaceAll(_space, '');

/// The matrix's description, with the fields it leaves out.
String _describe(FlarkCommand command) => switch (command) {
  DeleteBackward(word: true) => 'DeleteBackward(word: true)',
  DeleteForward(word: true) => 'DeleteForward(word: true)',
  SetLink(:final destination, :final text?) =>
    'SetLink(${jsonEncode(destination)}, text: ${jsonEncode(text)})',
  SetImage(:final destination, :final alt?) =>
    'SetImage(${jsonEncode(destination)}, alt: ${jsonEncode(alt)})',
  _ => matrix.describeCommand(command),
};
