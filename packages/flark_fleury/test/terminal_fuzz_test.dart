/// Deterministic terminal fuzzing of the mounted host. Editors (bare, in an
/// app with the toolbar, and the public composer) and the read-only reader,
/// mounted as the other host tests mount them, take random terminal input:
/// typed text with wide, emoji, combining and control graphemes; whole and
/// segmented bracketed pastes of Markdown with LF, CRLF and CR; movement, word
/// movement and deletion keys with and without Shift; Enter, Tab and the
/// formatting, link and history shortcuts; clicks, drags and wheel scrolls;
/// resizes down to one cell; width policies; focus changes; IME composition;
/// toolbar and accessibility actions; remounts that change read-only, focus
/// node or controller; and commands applied to the controller in between.
///
/// After every painted event nothing has thrown, the source is valid and the
/// selection legal, and the frame is a window of the same state painted by a
/// freshly mounted editor, so nothing an edit invalidated survives. That
/// fresh paint shows exactly the projection's graphemes (never hidden
/// source), keeps wide glyphs whole inside the grid, and paints one caret
/// cell, inside its line, where the projection puts the caret; a typed
/// character appears where the caret was. Undo and Redo return exact states,
/// a cancelled composition restores the state it began from, only its last
/// preedit survives a commit, and a segmented paste equals a whole one.
///
/// FLARK_FLEURY_SEED and FLARK_FLEURY_ITERATIONS choose the run; the default
/// takes seconds. A failure prints its session seed and a step log;
/// FLARK_FLEURY_MINIMIZE=1 first drops every step the failure does not need,
/// and FLARK_FLEURY_SESSION replays one session.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:characters/characters.dart';
import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_fleury/flark_fleury.dart' as public;
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

const _caretColor = RgbColor(1, 2, 3);
const _selectionColor = RgbColor(4, 5, 6);

const _typed = [
  'a',
  'b',
  'x',
  'Z',
  '0',
  '1',
  'a',
  'q',
  ' ',
  ' ',
  '  ',
  'word ',
  '\u00e9',
  'e\u0301',
  'a\u0308\u0323',
  '*',
  '**',
  '_',
  '`',
  '```',
  '#',
  '## ',
  '-',
  '- ',
  '+',
  '>',
  '> ',
  '[',
  ']',
  '(',
  ')',
  '!',
  '|',
  '~',
  '~~',
  '\\',
  '<',
  '&',
  '.',
  '1. ',
  ':',
  '=',
  '\u4e2d',
  '\u754c',
  '\u65e5\u672c\u8a9e',
  '\ud55c',
  '\uff48',
  '\u{1F600}',
  '\u{1F44D}\u{1F3FD}',
  '\u{1F1E8}\u{1F1E6}',
  '\u{1F468}\u200d\u{1F469}\u200d\u{1F467}',
  '\u2764\ufe0f',
  '1\ufe0f\u20e3',
  '\t',
  '\u200b',
  '\u00ad',
  '\u0301',
  '\u200d',
  '\ufe0f',
  '\u20e3',
  '\u0007',
  '\u00e9\u754c\u{1F600}',
  '[x](y)',
  '`c`',
  '**b**',
];

const _composed = [
  'n',
  '\u306b',
  '\u306b\u307b',
  '\u65e5\u672c',
  '\u65e5\u672c\u8a9e',
  '\u314e',
  '\ud55c',
  '\ud55c\uae00',
  'e\u0301',
  '',
  ' ',
  '*',
];

const _pastes = [
  'plain text',
  '**bold** and *em* and `code` and ~~gone~~',
  '- one\n- two\n  - nested\n- [ ] task\n- [x] done\n',
  '1. first\n2. second\n10. tenth\n',
  '> quote\n> more **text**\n>\n> - list in quote\n',
  '```dart\nvoid main() {\n  print("hi");\n}\n```\n',
  '```\nno language\n\n```',
  '    indented code\n    more\n',
  '| a | b |\n| :- | -: |\n| 1 | 2 |\n| 3 |\n',
  '| \u4e2d | \u{1F600} |\n| - | - |\n| e\u0301 | x |',
  '# Heading\n\nSetext\n======\n\nOther\n------\n',
  '[link](https://example.com "title") and ![img](pic.png)',
  '---\n\n***\n',
  '<div>\nhtml\n</div>\n',
  'Footnote[^1]\n\n[^1]: the note\n',
  'line one\nline two\\\nhard break\n',
  '\u4e2d\u6587 **\u7c97\u4f53** \u{1F600} e\u0301 tabs\there\n',
  '~~struck~~ <https://auto.link> www.example.com',
  '\n\n\n',
  'a\tb\tc',
  '![a](x.png)\n\n![b](y.png)',
  '- a\n\n  para in item\n\n      code in item\n',
];

const _documents = [
  '',
  'plain paragraph',
  '# Title\n\nSome **bold**, *em*, `code` and [a link](https://dart.dev).\n\n'
      '- one\n- [x] two\n  > quoted\n\n1. first\n2. second\n\n'
      '```dart\nfinal x = 1;\n```\n\n| a | b |\n| - | - |\n| c | d |\n',
  'wide \u4e2d\u6587 and \u{1F600} emoji, \u00e9 and e\u0301 combining, '
      'a\ttab, and a long tail that wraps around several narrow rows\n',
  '> quote **bold\n> continued** text\n>\n> - item\n\n    indented code\n',
  'Setext\n======\n\npara\n\n***\n\n## ATX ##\n\n<div>\nhtml\n</div>\n',
  '| Left | Center | Right |\n| :--- | :---: | ---: |\n'
      '| alpha beta gamma | \u754c | 42 |\n| tail |\n\nafter\n',
  '![alt text](image.png)\n\ntext with ![inline](i.png) image\n\n'
      '- ![in a list](l.png)\n',
  '```ruby\ndef hello\n  puts "hi"\nend\n```\n\n```\n\n```\n',
  'a\\\nhard break\n\nsoft\nwrap\n\nfootnote[^1]\n\n[^1]: note\n',
  '- a\n  - b\n    - c\n      - d\n        - e very deep item text here\n',
  'CRLF\r\nlines\r\n\r\n- item\r\n- item two\r\n',
  'x[ab](https://a.example)c [cd](https://b.example) <https://c.example> e\n',
];

/// One random input. Every parameter is drawn when the session is generated,
/// so a step list replays identically with any of its steps removed.
final class _Step {
  _Step(this.kind, this.p, {required this.render, required this.probe});
  final String kind;
  final List<int> p;
  final bool render, probe;
  @override
  String toString() =>
      '$kind(${p.join(', ')})${render ? '' : ' no-frame'}'
      '${probe ? ' probe' : ''}';
}

List<_Step> _generate(Random r) => [
  for (var i = 20 + r.nextInt(100); i > 0; i--)
    _Step(
      switch (r.nextInt(100)) {
        < 22 => 'type',
        < 28 => 'paste',
        < 54 => 'key',
        < 62 => 'click',
        < 66 => 'drag',
        < 69 => 'wheel',
        < 72 => 'resize',
        < 74 => 'policy',
        < 77 => 'focus',
        < 83 => 'compose',
        < 91 => 'command',
        < 92 => 'sourceMode',
        < 93 => 'load',
        < 98 => 'semantic',
        _ => 'remount',
      },
      [for (var j = 0; j < 8; j++) r.nextInt(1 << 20)],
      render: r.nextInt(6) != 0,
      probe: r.nextInt(3) == 0,
    ),
];

const _keys = <(KeyCode, Set<KeyModifier>)>[
  (KeyCode.arrowLeft, {}),
  (KeyCode.arrowRight, {}),
  (KeyCode.arrowUp, {}),
  (KeyCode.arrowDown, {}),
  (KeyCode.arrowLeft, {KeyModifier.shift}),
  (KeyCode.arrowRight, {KeyModifier.shift}),
  (KeyCode.arrowUp, {KeyModifier.shift}),
  (KeyCode.arrowDown, {KeyModifier.shift}),
  (KeyCode.arrowLeft, {KeyModifier.alt}),
  (KeyCode.arrowRight, {KeyModifier.alt}),
  (KeyCode.arrowLeft, {KeyModifier.ctrl, KeyModifier.shift}),
  (KeyCode.arrowRight, {KeyModifier.ctrl, KeyModifier.shift}),
  (KeyCode.arrowUp, {KeyModifier.superKey}),
  (KeyCode.home, {}),
  (KeyCode.end, {}),
  (KeyCode.home, {KeyModifier.shift}),
  (KeyCode.end, {KeyModifier.shift}),
  (KeyCode.home, {KeyModifier.ctrl}),
  (KeyCode.end, {KeyModifier.ctrl, KeyModifier.shift}),
  (KeyCode.pageDown, {}),
  (KeyCode.backspace, {}),
  (KeyCode.backspace, {}),
  (KeyCode.delete, {}),
  (KeyCode.backspace, {KeyModifier.alt}),
  (KeyCode.backspace, {KeyModifier.ctrl}),
  (KeyCode.delete, {KeyModifier.alt}),
  (KeyCode.delete, {KeyModifier.ctrl}),
  (KeyCode.enter, {}),
  (KeyCode.enter, {}),
  (KeyCode.enter, {KeyModifier.shift}),
  (KeyCode.tab, {}),
  (KeyCode.tab, {KeyModifier.shift}),
  (KeyCode.escape, {}),
  (KeyCode.a, {KeyModifier.superKey}),
  (KeyCode.a, {KeyModifier.ctrl}),
  (KeyCode.b, {KeyModifier.superKey}),
  (KeyCode.i, {KeyModifier.ctrl}),
  (KeyCode.k, {KeyModifier.superKey}),
  (KeyCode.z, {KeyModifier.superKey}),
  (KeyCode.z, {KeyModifier.superKey, KeyModifier.shift}),
  (KeyCode.y, {KeyModifier.ctrl}),
  (KeyCode.c, {KeyModifier.superKey}),
  (KeyCode.x, {KeyModifier.superKey}),
];

const _sizes = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 60, 80, 120];
const _heights = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 24, 30];
const _policies = [
  CellWidthPolicy.spec,
  CellWidthPolicy.cjk,
  CellWidthPolicy(emojiVariationSequence: CellWidth.one),
  CellWidthPolicy(ambiguous: CellWidth.two, emojiPresentation: CellWidth.one),
];

FlarkCellTheme _theme(Random r) => FlarkCellTheme(
  body: r.nextBool()
      ? CellStyle.none
      : const CellStyle(
          foreground: RgbColor(220, 220, 220),
          background: RgbColor(12, 12, 12),
        ),
  code: r.nextBool()
      ? CellStyle.none
      : const CellStyle(background: RgbColor(40, 40, 40)),
  codePadding: const [0, 1, 2, 0.5][r.nextInt(4)],
  codePaddingRows: const [0, 0.5, 1][r.nextInt(3)],
  headingGutter: r.nextInt(4) == 0,
  headingStyles: r.nextBool()
      ? const {}
      : const {
          1: FlarkHeadingStyle(band: true, divider: true, showLevel: true),
          2: FlarkHeadingStyle(divider: true),
          3: FlarkHeadingStyle(showLevel: true),
        },
  listIndent: 2 + r.nextInt(4),
  quoteIndent: 1 + r.nextInt(3),
  imagePreviewRows: const [0, 1, 3, 8][r.nextInt(4)],
  syntax: const {
    CodeSyntaxRole.keyword: CellStyle(foreground: RgbColor(200, 0, 200)),
    CodeSyntaxRole.string: CellStyle(foreground: RgbColor(0, 200, 0)),
  },
  selection: const CellStyle(background: _selectionColor),
  caret: const CellStyle(background: _caretColor),
);

Widget _preview(BuildContext context, InlineResource resource, Uri? uri) =>
    Text('img ${resource.text}');

String _variant(String text, int which) => switch (which % 3) {
  1 => text.replaceAll('\n', '\r\n'),
  2 => text.replaceAll('\n', '\r'),
  _ => text,
};

bool _caretCell(Cell cell) => cell.style.background == _caretColor;

typedef _State = ({String source, FlarkSelection selection});

/// FLARK_FLEURY_TRACE=1 prints the state after every editor step.
final _trace = Platform.environment['FLARK_FLEURY_TRACE'] == '1';

/// A reader click, with the geometry of the frame it hit.
typedef _Click = ({
  int col,
  int row,
  CellDocumentLayout? layout,
  CellRect? region,
});

/// A broken invariant, with what the session knew when it broke.
final class _Failure implements Exception {
  _Failure(this.message);
  final String message;
  @override
  String toString() => message;
}

enum _Mount { bare, app, composer }

/// Where a frame shows a fresh editor's full-document paint, and the facts
/// the caret checks need.
typedef _Window = ({List<int> tops, CellDocumentLayout layout});

/// Paints [state] with a newly mounted editor, every line in view.
CellBuffer _paintFresh(
  FlarkParseBackend backend,
  _StateView state, {
  required int cols,
  required int rows,
  required FlarkCellTheme theme,
  required TextPresentationPolicy policy,
  required bool focused,
  CodeEditingDelegate? code,
}) {
  final copy = FlarkEditor(
    backend,
    text: state.source,
    caret: 0,
    codeEditing: code,
  );
  if (state.sourceMode && !copy.sourceMode) copy.setSourceMode(true);
  final selection = state.selection;
  copy.apply(
    selection.tableCell != null
        ? PlaceCaret(selection.tableCell!, 0)
        : SetSelection(selection.base, selection.extent),
  );
  if (copy.selection != selection) {
    throw _Failure(
      'a fresh editor cannot hold selection $selection (got ${copy.selection})',
    );
  }
  final controller = FlarkFleuryController(copy);
  final focus = FocusNode();
  final paint = FleuryTester(
    viewportSize: CellSize(cols, max(1, rows)),
    textPolicy: policy,
  );
  try {
    paint.pumpWidget(
      Theme(
        data: const ThemeData(),
        child: FlarkEditorView(
          controller: controller,
          theme: theme,
          focusNode: focus,
          autofocus: focused,
          imagePreviewBuilder: _preview,
        ),
      ),
    );
    return paint.render();
  } finally {
    paint.dispose();
    controller.dispose();
    focus.dispose();
  }
}

/// The source, selection and mode a check reads, from an editor or reader.
typedef _StateView = ({
  String source,
  FlarkSelection selection,
  bool sourceMode,
});

/// Finds which window of [page] (a fresh paint of every line of [layout])
/// [frame] shows inside [region], comparing graphemes, then styles outside
/// caret cells and image slots. Returns the window tops that match.
List<int> _matchWindow(
  CellBuffer frame,
  CellRect region,
  CellBuffer page,
  CellDocumentLayout layout,
) {
  final cols = region.size.cols, rows = region.size.rows;
  final left = region.offset.col, top0 = region.offset.row;
  bool masked(int x, int pageRow) => layout.images.any(
    (slot) =>
        pageRow >= slot.top &&
        pageRow < slot.top + slot.height &&
        x >= slot.left &&
        x < slot.left + slot.width,
  );
  final slotRows = {
    for (final slot in layout.images)
      for (var y = slot.top; y < slot.top + slot.height; y++) y,
  };
  String key(CellBuffer b, int x0, int y, int pageRow) => [
    for (var x = 0; x < cols; x++)
      if (!slotRows.contains(pageRow) || !masked(x, pageRow))
        '${b.atColRow(x0 + x, y).role.index}${b.atColRow(x0 + x, y).grapheme ?? ''}',
  ].join(',');
  final maxTop = max(0, layout.lines.length - rows);
  final tops = <int>[];
  var best = 0, bestRows = -1;
  for (var top = 0; top <= maxTop; top++) {
    var same = 0;
    for (var y = 0; y < rows; y++) {
      if (key(frame, left, top0 + y, top + y) ==
          key(page, 0, top + y, top + y)) {
        same++;
      }
    }
    if (same == rows) tops.add(top);
    if (same > bestRows) (best, bestRows) = (top, same);
  }
  String text(CellBuffer b, int x0, int y) => [
    for (var x = 0; x < cols; x++)
      if (b.atColRow(x0 + x, y).role != CellRole.continuation)
        b.atColRow(x0 + x, y).grapheme ?? '·',
  ].join();
  if (tops.isEmpty) {
    final y = [for (var y = 0; y < rows; y++) y].firstWhere(
      (y) =>
          key(frame, left, top0 + y, best + y) !=
          key(page, 0, best + y, best + y),
    );
    throw _Failure(
      'the frame is no window of a fresh paint of the same state; '
      'the closest starts at line $best, and its row $y differs:\n'
      '    painted ${jsonEncode(text(frame, left, top0 + y))}\n'
      '    fresh   ${jsonEncode(text(page, 0, best + y))}',
    );
  }
  String? styleDifference(int top) {
    for (var y = 0; y < rows; y++) {
      for (var x = 0; x < cols; x++) {
        final a = frame.atColRow(left + x, top0 + y),
            b = page.atColRow(x, top + y);
        if (_caretCell(a) || _caretCell(b) || masked(x, top + y)) continue;
        if (a.style != b.style) {
          return 'cell ($x,$y) ${jsonEncode(a.grapheme)} is painted '
              '${a.style} where a fresh paint gives ${b.style}';
        }
      }
    }
    return null;
  }

  final styled = tops.where((top) => styleDifference(top) == null).toList();
  if (styled.isEmpty) {
    throw _Failure(
      'a style differs from a fresh paint: '
      '${styleDifference(tops.first)}',
    );
  }
  return styled;
}

/// The fresh paint shows each layout glyph whole, at its cell.
void _checkPagePaint(
  CellBuffer page,
  CellDocumentLayout layout,
  FlarkSelection selection,
) {
  for (var y = 0; y < layout.lines.length && y < page.size.rows; y++) {
    for (final line in layout.lines[y].fragments) {
      if (line.image != null || !line.labelVisible(selection)) continue;
      for (final glyph in line.glyphs) {
        final cell = page.atColRow(glyph.col, y);
        final expected = glyph.text == ' ' ? ' ' : glyph.text.characters.first;
        if (cell.role != CellRole.leading || cell.grapheme != expected) {
          throw _Failure(
            'line $y paints ${jsonEncode(cell.grapheme)} (${cell.role.name}) '
            'at ${glyph.col} for glyph ${jsonEncode(glyph.text)}',
          );
        }
        if (glyph.text != ' ' &&
            glyph.width == 2 &&
            page.atColRow(glyph.col + 1, y).role != CellRole.continuation) {
          throw _Failure(
            'line $y splits wide glyph ${jsonEncode(glyph.text)} at '
            '${glyph.col}',
          );
        }
      }
    }
  }
}

/// The geometry contract: glyphs inside the grid, one projected grapheme
/// each, and a row's lines partitioning its display text with a glyph for
/// every grapheme but its line breaks.
void _checkLayout(
  CellDocumentLayout layout,
  String source,
  List<ProjectedRow>? rows,
  int cols,
  CellWidthPolicy policy,
) {
  const widths = DefaultWidthResolver();
  for (var i = 0; i < layout.lines.length; i++) {
    for (final line in layout.lines[i].fragments) {
      var col = 0, offset = line.start;
      final text = line.row?.text ?? source;
      final shift = line.row == null ? line.sourceStart : 0;
      for (final glyph in line.glyphs) {
        final where = 'line $i glyph ${jsonEncode(glyph.text)} at ${glyph.col}';
        if (glyph.col < col || glyph.width <= 0) {
          throw _Failure('$where overlaps the glyph before it');
        }
        if (glyph.col + glyph.width > cols) {
          throw _Failure('$where (width ${glyph.width}) overflows $cols cells');
        }
        final start = glyph.start + shift, end = glyph.end + shift;
        if (glyph.start != offset || end <= start || end > text.length) {
          throw _Failure("$where does not continue the line's text");
        }
        final grapheme = text.substring(start, end);
        if (grapheme.characters.length != 1) {
          throw _Failure('$where covers ${jsonEncode(grapheme)}');
        }
        final safe = sanitizeForDisplay(grapheme);
        final width = widths.widthOfText(safe, policy);
        final shown = width == 0 ? '\u25cc$safe' : safe;
        final ok = grapheme == '\t'
            ? glyph.text == ' '
            : glyph.text == shown &&
                      glyph.width == widths.widthOfText(shown, policy) ||
                  (glyph.text == '\ufffd' || glyph.text == '?') &&
                      glyph.width == 1;
        if (!ok) {
          throw _Failure(
            '$where shows ${jsonEncode(grapheme)} as ${jsonEncode(glyph.text)}'
            ' in ${glyph.width} cells',
          );
        }
        col = glyph.col + glyph.width;
        offset = glyph.end;
      }
    }
  }
  if (rows == null) return;
  final editable = [
    for (final line in layout.lines)
      for (final fragment in line.fragments)
        if (fragment.editable && fragment.caretLine && fragment.row != null)
          fragment,
  ];
  for (final row in rows) {
    final owned = editable.where((l) => identical(l.row, row)).toList();
    if (owned.isEmpty) throw _Failure('row ${row.index} has no line');
    var shown = '';
    for (var i = 0; i < owned.length; i++) {
      final skipped = row.text.substring(
        i == 0 ? 0 : owned[i - 1].end,
        owned[i].start,
      );
      if (skipped != '' && skipped != '\n' && skipped != '\r\n') {
        throw _Failure(
          'row ${row.index} skips ${jsonEncode(skipped)} between lines',
        );
      }
      shown += row.text.substring(owned[i].start, owned[i].end);
    }
    String flat(String text) =>
        text.replaceAll('\r\n', '').replaceAll('\n', '');
    if (owned.last.end != row.text.length || flat(shown) != flat(row.text)) {
      throw _Failure(
        'row ${row.index} lines do not show ${jsonEncode(row.text)}',
      );
    }
    final glyphs = {
      for (final glyph in owned.expand((line) => line.glyphs))
        (glyph.start, glyph.end),
    };
    // Delimiter padding at the end of a table cell's text is not painted.
    final padded = row.kind == RowKind.tableCell
        ? row.text.trimRight().length
        : row.text.length;
    var at = 0;
    for (final grapheme in row.text.characters) {
      final range = (at, at + grapheme.length);
      at += grapheme.length;
      if (grapheme == '\n' || grapheme == '\r\n' || range.$1 >= padded) {
        continue;
      }
      if (!glyphs.contains(range)) {
        throw _Failure(
          'row ${row.index} paints no glyph for ${jsonEncode(grapheme)} '
          'at $range of ${jsonEncode(row.text)}',
        );
      }
    }
  }
}

final class _EditorSession {
  _EditorSession(this.backend, this.seed, this.steps) : r = Random(seed);
  final FlarkParseBackend backend;
  final int seed;
  final List<_Step> steps;
  final Random r;
  late final _Mount mount = _Mount.values[r.nextInt(_Mount.values.length)];
  late final FlarkCellTheme theme = _theme(r);
  late final CodeEditingDelegate? code = r.nextBool()
      ? FlarkCodeMirror()
      : null;
  late final FleuryTester tester;
  late FocusNode focus;
  late FlarkEditor editor;

  /// The view's controller, or, for the composer, one for layout checks.
  late FlarkFleuryController controller;
  public.FlarkController? owner;
  var readOnly = false;
  var now = Duration.zero;
  var pasteId = 0;
  final notices = <String>[];
  final opened = <Uri>[];

  /// The caret cell of the last checked frame, for the next step's checks.
  CellOffset? lastCaret;

  CellWidthPolicy get policy => tester.textPolicy.widths;

  /// History groups follow the session clock except in the composer, whose
  /// editor keeps its own: there Undo may join typing across steps.
  bool get exactUndo => mount != _Mount.composer;

  Future<void> run() async {
    tester = FleuryTester(
      viewportSize: CellSize(
        _sizes[r.nextInt(_sizes.length)],
        _heights[r.nextInt(_heights.length)],
      ),
      textPolicy: TextPresentationPolicy(
        widths: _policies[r.nextInt(_policies.length)],
      ),
    );
    focus = FocusNode();
    final initial = _documents[r.nextInt(_documents.length)];
    final caret = r.nextInt(initial.length + 1);
    try {
      if (mount == _Mount.composer) {
        final owner = this.owner = public.FlarkController(markdown: initial);
        await owner.ready;
        editor = owner.session.engine!;
      } else {
        editor = FlarkEditor(
          backend,
          text: initial,
          caret: caret,
          codeEditing: code,
          clock: () => now,
        );
      }
      controller = FlarkFleuryController(editor);
      tester.pumpWidget(build());
      await check('mount', null);
      for (var i = 0; i < steps.length; i++) {
        await perform(i, steps[i]);
      }
    } finally {
      tester.dispose();
      controller.dispose();
      owner?.dispose();
      focus.dispose();
    }
  }

  Widget build() {
    final view = FlarkEditorView(
      controller: controller,
      theme: theme,
      focusNode: focus,
      autofocus: true,
      readOnly: readOnly,
      showToolbar: mount == _Mount.app,
      imagePreviewBuilder: _preview,
      onOpenLink: opened.add,
      onNotice: notices.add,
    );
    return switch (mount) {
      _Mount.bare => Theme(data: const ThemeData(), child: view),
      _Mount.app => FleuryApp(title: 'Fuzz', home: view),
      _Mount.composer => FleuryApp(
        title: 'Fuzz',
        home: public.FlarkEditor(
          controller: owner,
          theme: theme,
          focusNode: focus,
          autofocus: true,
          readOnly: readOnly,
          imagePreviewBuilder: _preview,
          onOpenLink: opened.add,
        ),
      ),
    };
  }

  _State get state => (source: editor.source, selection: editor.selection);

  void key(KeyCode code, [Set<KeyModifier> modifiers = const {}]) =>
      tester.sendKey(KeyEvent(code, modifiers: modifiers));

  void mouse(MouseEventKind kind, int col, int row, [Set<KeyModifier>? mods]) =>
      tester.sendMouse(
        MouseEvent(
          kind: kind,
          button:
              kind == MouseEventKind.scrollUp ||
                  kind == MouseEventKind.scrollDown
              ? MouseButton.none
              : MouseButton.left,
          col: col,
          row: row,
          modifiers: mods ?? const {},
        ),
      );

  void paste(String text, List<int> p) {
    if (p[2] % 3 == 0 || text.length < 2) {
      tester.paste(text);
      return;
    }
    final cuts = {1, p[3] % text.length, p[4] % text.length}.toList()..sort();
    final parts = [
      for (var i = 0; i < cuts.length; i++)
        text.substring(i == 0 ? 0 : cuts[i - 1], cuts[i]),
      text.substring(cuts.last),
    ].where((part) => part.isNotEmpty).toList();
    final id = ++pasteId;
    for (var i = 0; i < parts.length; i++) {
      tester.dispatcher.dispatch(
        PasteEvent.segment(
          parts[i],
          pasteId: id,
          phase: i == 0
              ? PasteEventPhase.start
              : i == parts.length - 1
              ? PasteEventPhase.end
              : PasteEventPhase.continuation,
        ),
      );
    }
    if (parts.length == 1) {
      tester.dispatcher.dispatch(
        PasteEvent.segment('', pasteId: id, phase: PasteEventPhase.end),
      );
    }
  }

  void compose(List<String> updates, String? end) {
    for (final text in updates) {
      tester.dispatcher.dispatch(TextCompositionEvent.update(text));
    }
    if (end == 'cancel') {
      tester.dispatcher.dispatch(const TextCompositionEvent.cancel());
    } else if (end != null) {
      tester.dispatcher.dispatch(
        TextCompositionEvent.commit(end.isEmpty ? null : end),
      );
    }
  }

  /// Lets clipboard writes, dialogs and other host futures finish.
  Future<void> settle() async {
    for (var i = 0; i < 3; i++) {
      await Future<void>.delayed(Duration.zero);
    }
  }

  Future<void> perform(int index, _Step step) async {
    final p = step.p;
    // Each step is its own history group: typing never coalesces across
    // steps, so one Undo after a step returns the state before it.
    now += const Duration(seconds: 5);
    final before = state;
    final composing = editor.composing;
    final size = tester.viewportSize;
    final host = focus.hasFocus && !readOnly;
    final caretBefore = lastCaret;
    var exactSelection = true, history = false, cancelled = false;
    String? typed;
    switch (step.kind) {
      case 'type':
        typed = _typed[p[0] % _typed.length];
        tester.type(typed);
      case 'paste':
        final text = _variant(_pastes[p[0] % _pastes.length], p[1]);
        paste(text, p);
        if (p[2] % 3 != 0 && host && before.source != editor.source) {
          // A segmented paste is the paste of its whole text.
          final segmented = state;
          editor.apply(const Undo());
          tester.paste(text);
          if (editor.source != segmented.source ||
              editor.selection != segmented.selection) {
            throw _Failure(
              'step $index: a segmented paste of ${jsonEncode(text)} gives '
              '${jsonEncode(segmented.source)} ${segmented.selection}, a '
              'whole one ${jsonEncode(editor.source)} ${editor.selection}',
            );
          }
        }
      case 'key':
        final (code, modifiers) = _keys[p[0] % _keys.length];
        history = code == KeyCode.z || code == KeyCode.y;
        key(code, modifiers);
        await settle();
      case 'click':
        exactSelection = false;
        final mods = const <Set<KeyModifier>>[
          {},
          {},
          {KeyModifier.shift},
          {KeyModifier.ctrl},
          {KeyModifier.superKey},
        ][p[2] % 5];
        final col = p[0] % size.cols, row = p[1] % size.rows;
        mouse(MouseEventKind.down, col, row, mods);
        mouse(MouseEventKind.up, col, row, mods);
        await settle();
      case 'drag':
        exactSelection = false;
        var col = p[0] % size.cols, row = p[1] % size.rows;
        mouse(MouseEventKind.down, col, row);
        for (var i = 0; i <= p[4] % 3; i++) {
          col = p[2 + i % 2] % (size.cols + 4) - 2;
          row = p[3 - i % 2] % (size.rows + 4) - 2;
          mouse(MouseEventKind.drag, col, row);
        }
        mouse(MouseEventKind.up, col, row);
      case 'wheel':
        mouse(
          p[0].isEven ? MouseEventKind.scrollDown : MouseEventKind.scrollUp,
          p[1] % size.cols,
          p[2] % size.rows,
        );
      case 'resize':
        tester.viewportSize = CellSize(
          _sizes[p[0] % _sizes.length],
          _heights[p[1] % _heights.length],
        );
      case 'policy':
        tester.textPolicy = TextPresentationPolicy(
          widths: _policies[p[0] % _policies.length],
        );
      case 'focus':
        p[0].isEven ? focus.requestFocus() : focus.unfocus();
      case 'compose':
        final updates = [
          for (var i = 0; i <= p[0] % 4; i++)
            _composed[p[1 + i] % _composed.length],
        ];
        final end = switch (p[5] % 6) {
          0 || 1 || 2 => '',
          3 => _composed[p[6] % _composed.length],
          4 => 'cancel',
          _ => null,
        };
        compose(updates, end);
        cancelled = end == 'cancel' && !composing && host;
        if (end == '' &&
            !composing &&
            host &&
            updates.length > 1 &&
            editor.source != before.source) {
          // Only the last preedit survives a commit.
          final committed = state;
          editor.apply(const Undo());
          if (editor.source != before.source) {
            throw _Failure(
              'step $index: Undo of a committed composition gives '
              '${jsonEncode(editor.source)}, not ${jsonEncode(before.source)}',
            );
          }
          compose([updates.last], '');
          if (editor.source != committed.source) {
            throw _Failure(
              'step $index: preedits ${jsonEncode(updates)} commit '
              '${jsonEncode(committed.source)}, but the last alone commits '
              '${jsonEncode(editor.source)}',
            );
          }
        }
      case 'command':
        history = command(p);
      case 'sourceMode':
        final owner = this.owner;
        owner == null
            ? editor.setSourceMode(!editor.sourceMode)
            : owner.setSourceMode(!editor.sourceMode);
      case 'load':
        final text = _documents[p[0] % _documents.length];
        final owner = this.owner;
        if (owner == null) {
          editor.loadMarkdown(text);
        } else {
          p[1].isEven ? owner.loadMarkdown(text) : owner.replaceMarkdown(text);
        }
      case 'semantic':
        exactSelection = false;
        final targets = tester
            .semantics()
            .nodes
            .where(
              (node) =>
                  node.enabled != false &&
                  node.actions.contains(SemanticAction.activate),
            )
            .toList();
        if (targets.isNotEmpty) {
          final target = targets[p[0] % targets.length];
          history = target.label == 'Undo' || target.label == 'Redo';
          await tester.invokeSemanticAction(
            SemanticAction.activate,
            id: target.id,
          );
          await settle();
        }
      case 'remount':
        await remount(p);
    }
    if (mount != _Mount.bare) {
      // A toolbar picker that a change replaces while open must be disposed
      // by frames before more input: fleury_widgets' Select otherwise closes
      // itself from its deactivated or disposed state and throws (open
      // "Paragraph style", apply a command, click outside). Bare editors,
      // without pickers, keep input between frames.
      tester.render();
      if (tester.semantics().byRole(SemanticRole.menuItem).isNotEmpty) {
        tester.render();
      }
    }
    if (_trace) {
      print('$index $step: ${jsonEncode(editor.source)} ${editor.selection}');
    }
    if (!step.render) {
      lastCaret = null;
      return;
    }
    await check('step $index $step', before);
    final source = editor.source;
    if (readOnly &&
        const {
          'type',
          'paste',
          'key',
          'click',
          'drag',
          'compose',
        }.contains(step.kind) &&
        source != before.source) {
      throw _Failure('step $index $step: a read-only editor changed');
    }
    if (cancelled &&
        (source != before.source || editor.selection != before.selection)) {
      throw _Failure(
        'step $index: a cancelled composition left '
        '${jsonEncode(source)} ${editor.selection}, not '
        '${jsonEncode(before.source)} ${before.selection}',
      );
    }
    if (typed != null && host) checkTyped(typed, before);
    if ((step.kind == 'resize' || step.kind == 'policy') &&
        caretBefore != null &&
        focus.hasFocus &&
        editor.selection.isCollapsed &&
        tester.viewportSize.cols > 2 &&
        (region?.size.rows ?? 0) > 0 &&
        focus.caretRect == null) {
      throw _Failure(
        'step $index $step: the caret shown at $caretBefore left the view',
      );
    }
    if (step.probe &&
        before.source != source &&
        !editor.composing &&
        step.kind != 'load' &&
        step.kind != 'remount' &&
        !history &&
        // Only Undo changes the source and leaves something to redo: a
        // click or action on the toolbar's Undo.
        !editor.history.canRedo &&
        // A composition continued from an earlier step undoes to its start.
        !(composing && step.kind == 'compose') &&
        focus.hasFocus &&
        !readOnly) {
      final after = state;
      key(KeyCode.z, {KeyModifier.superKey});
      await check('Undo after step $index', null);
      if (exactUndo &&
          (editor.source != before.source ||
              (exactSelection && editor.selection != before.selection))) {
        throw _Failure(
          'Undo after step $index $step did not restore the state before it\n'
          '  expected ${jsonEncode(before.source)} ${before.selection}\n'
          '  actual   ${jsonEncode(editor.source)} ${editor.selection}',
        );
      }
      key(KeyCode.z, {KeyModifier.superKey, KeyModifier.shift});
      await check('Redo after step $index', null);
      if (editor.source != after.source ||
          editor.selection != after.selection) {
        throw _Failure(
          'Redo after step $index $step did not return the state after it\n'
          '  expected ${jsonEncode(after.source)} ${after.selection}\n'
          '  actual   ${jsonEncode(editor.source)} ${editor.selection}',
        );
      }
    }
  }

  /// A single typed character appears in the cell the caret was painted in
  /// when the caret moved one cell on along the same line.
  void checkTyped(String typed, _State before) {
    final was = lastCaret, rect = focus.caretRect;
    if (was == null ||
        rect == null ||
        typed.length != 1 ||
        !RegExp('[a-zA-Z0-9]').hasMatch(typed) ||
        editor.sourceMode ||
        editor.source !=
            before.source.replaceRange(
              before.selection.start,
              before.selection.end,
              typed,
            ) ||
        rect.offset != CellOffset(was.col + 1, was.row)) {
      return;
    }
    final cell = tester.render().atColRow(was.col, was.row);
    if (cell.grapheme != typed) {
      throw _Failure(
        'typed ${jsonEncode(typed)} with the caret at $was, but that cell '
        'shows ${jsonEncode(cell.grapheme)}',
      );
    }
  }

  /// Applies a random command the way an application would: to the kernel,
  /// or through the composer's public controller. Returns whether it was a
  /// history command.
  bool command(List<int> p) {
    final command = _command(p);
    final owner = this.owner;
    if (owner == null) {
      p[7].isEven
          ? editor.apply(command)
          : editor.applyAfterComposition(command);
    } else {
      switch (command) {
        case ToggleStyle(:final style):
          owner.toggleStyle(
            public.FlarkStyle.values.firstWhere((s) => s.kernelStyle == style),
          );
        case SetSelection(:final base, :final extent):
          owner.setSelection(base, extent);
        case InsertText(:final text):
          owner.insertText(text);
        case Undo():
          owner.undo();
        case Redo():
          owner.redo();
        case SelectAll():
          owner.selectAll();
        case ReplaceRange(:final start, :final end, :final text):
          owner.replaceSourceRange(start: start, end: end, markdown: text);
        default:
          owner.session.command(command);
      }
    }
    return command is Undo || command is Redo;
  }

  /// Kernel commands, resolved against the current document.
  FlarkCommand _command(List<int> p) {
    final length = editor.source.length;
    int at(int value) => value % (length + 1);
    final rows = editor.sourceMode ? 1 : editor.projection.rows.length;
    return switch (p[0] % 100) {
      < 10 => InsertText(_typed[p[1] % _typed.length]),
      < 18 => SetSelection(at(p[1]), at(p[2])),
      < 24 => SetSelection.caret(at(p[1])),
      < 32 => ToggleStyle(
        const [
          Style.strong,
          Style.emphasis,
          Style.code,
          Style.strikethrough,
        ][p[1] % 4],
      ),
      < 38 => SetStyle(
        const [Style.strong, Style.emphasis][p[1] % 2],
        enabled: p[2].isEven,
      ),
      < 46 => SetHeadingLevel(p[1] % 7),
      < 50 => const ToggleTask(),
      < 54 => p[1].isEven ? const Indent() : const Outdent(),
      < 58 => ReplaceRange(at(p[1]), at(p[1]), _typed[p[2] % _typed.length]),
      < 62 => ReplaceRange(at(p[1]), min(length, at(p[1]) + p[2] % 6), 'r'),
      < 66 => SetLink('https://example.com/${p[1] % 9}'),
      < 69 => SetImage('pic${p[1] % 9}.png', alt: 'alt'),
      < 71 => const RemoveLink(),
      < 73 => const RemoveImage(),
      < 76 => SetCodeLanguage(const ['', 'text', 'ruby', 'dart'][p[1] % 4]),
      < 80 => PlaceCaret(
        p[1] % rows,
        editor.sourceMode
            ? 0
            : p[2] % (editor.projection.rows[p[1] % rows].text.length + 1),
        leadingHalf: p[3].isEven,
        extend: p[4] % 4 == 0,
      ),
      < 84 => Paste(_pastes[p[1] % _pastes.length]),
      < 87 => MoveCaret(
        p[1].isEven ? MoveDirection.forward : MoveDirection.backward,
        unit: MoveUnit.values[p[2] % MoveUnit.values.length],
        extend: p[3].isEven,
      ),
      < 90 => DeleteBackward(word: p[1].isEven),
      < 93 => const Undo(),
      < 96 => const Redo(),
      < 98 => const SelectAll(),
      _ => Newline(paragraph: p[1].isEven),
    };
  }

  Future<void> remount(List<int> p) async {
    switch (p[0] % 3) {
      case 0:
        readOnly = !readOnly;
        tester.pumpWidget(build());
      case 1:
        final old = focus;
        focus = FocusNode();
        tester.pumpWidget(build());
        old.dispose();
      default:
        final old = controller, oldOwner = owner;
        if (oldOwner == null) {
          controller = FlarkFleuryController(editor);
          tester.pumpWidget(build());
        } else {
          final next = owner = public.FlarkController(markdown: editor.source);
          await next.ready;
          editor = next.session.engine!;
          controller = FlarkFleuryController(editor);
          tester.pumpWidget(build());
          oldOwner.dispose();
        }
        old.dispose();
    }
  }

  /// Every invariant of the frame just painted.
  Future<void> check(String label, _State? before) async {
    final frame = tester.render();
    try {
      _checkState();
      _checkFrame(frame);
    } on _Failure catch (failure) {
      throw _Failure(
        '$label: ${failure.message}\n'
        '  source ${jsonEncode(editor.source)} ${editor.selection}'
        '${editor.sourceMode ? ' (source mode)' : ''}\n'
        '  ${mount.name}${readOnly ? ' read-only' : ''} viewport '
        '${tester.viewportSize} ${policy.ambiguous.name}/'
        '${policy.emojiPresentation.name}/'
        '${policy.emojiVariationSequence.name} focused ${focus.hasFocus}',
      );
    }
  }

  void _checkState() {
    // Nothing interleaves with a paste's segments here: none may be dropped.
    if (notices.isNotEmpty) throw _Failure('input was dropped: $notices');
    final source = editor.source, selection = editor.selection;
    for (var i = 0; i < source.length; i++) {
      final unit = source.codeUnitAt(i);
      final next = i + 1 < source.length ? source.codeUnitAt(i + 1) : -1;
      if (unit == 0x0D && next != 0x0A ||
          unit & 0xFC00 == 0xD800 && next & 0xFC00 != 0xDC00 ||
          unit & 0xFC00 == 0xDC00 &&
              (i == 0 || source.codeUnitAt(i - 1) & 0xFC00 != 0xD800)) {
        throw _Failure('invalid source at $i');
      }
    }
    for (final offset in [selection.base, selection.extent]) {
      if (offset < 0 || offset > source.length) {
        throw _Failure('selection $selection outside 0..${source.length}');
      }
      if (offset > 0 &&
          offset < source.length &&
          (source.codeUnitAt(offset) & 0xFC00 == 0xDC00 ||
              source.startsWith('\r\n', offset - 1))) {
        throw _Failure('selection endpoint $offset splits a code point');
      }
    }
    if (owner != null && owner!.markdown != source) {
      throw _Failure('the controller publishes a different source');
    }
    if (editor.sourceMode) return;
    final whole =
        !selection.isCollapsed &&
        selection.start == 0 &&
        selection.end == source.length;
    final cell = editor.projection.isMissingCell(selection.tableCell);
    if (!whole &&
        !cell &&
        (!editor.document.isLegal(selection.base) ||
            !editor.document.isLegal(selection.extent))) {
      throw _Failure('illegal selection $selection');
    }
  }

  /// The editor's surface in the last checked frame.
  CellRect? region;

  void _checkFrame(CellBuffer frame) {
    lastCaret = null;
    final semantics = tester.semantics();
    final region = this.region = semantics
        .byRole(SemanticRole.textArea)
        .where((n) => n.label == 'Markdown editor')
        .firstOrNull
        ?.bounds;
    if (region == null || region.size.isEmpty) return;
    if (semantics.byRole(SemanticRole.dialog).isNotEmpty ||
        semantics.byRole(SemanticRole.menuItem).isNotEmpty ||
        semantics.byLabel('Link actions').isNotEmpty ||
        semantics.byLabel('Image actions').isNotEmpty) {
      return;
    }
    final cols = region.size.cols, rows = region.size.rows;
    final layout = CellDocumentLayout(controller, cols, theme, policy);
    _checkLayout(
      layout,
      editor.source,
      editor.sourceMode ? null : editor.projection.rows,
      cols,
      policy,
    );
    if (editor.sourceMode && editor.source.length > 8192) return;
    final page = _paintFresh(
      backend,
      (
        source: editor.source,
        selection: editor.selection,
        sourceMode: editor.sourceMode,
      ),
      cols: cols,
      rows: layout.lines.length + rows,
      theme: theme,
      policy: tester.textPolicy,
      focused: focus.hasFocus,
      code: editor.codeEditing,
    );
    _checkPagePaint(page, layout, editor.selection);
    final tops = _matchWindow(frame, region, page, layout);
    _checkCaret(frame, region, (tops: tops, layout: layout));
  }

  void _checkCaret(CellBuffer frame, CellRect region, _Window window) {
    final cols = region.size.cols, rows = region.size.rows;
    final painted = <CellOffset>[
      for (var y = 0; y < rows; y++)
        for (var x = 0; x < cols; x++)
          if (_caretCell(
                frame.atColRow(region.offset.col + x, region.offset.row + y),
              ) &&
              frame
                      .atColRow(region.offset.col + x, region.offset.row + y)
                      .role !=
                  CellRole.continuation)
            CellOffset(x, y),
    ];
    final selection = editor.selection;
    if (!focus.hasFocus || !selection.isCollapsed) {
      if (painted.isNotEmpty) {
        throw _Failure('a caret is painted at ${painted.first} without one');
      }
      return;
    }
    // A caret on a tab covers the cells the tab shows.
    final tab = painted.indexed.every(
      (cell) =>
          cell.$2.row == painted.first.row &&
          cell.$2.col == painted.first.col + cell.$1 &&
          frame
                  .atColRow(
                    region.offset.col + cell.$2.col,
                    region.offset.row + cell.$2.row,
                  )
                  .grapheme ==
              ' ',
    );
    if (painted.length > 1 && !tab) {
      throw _Failure('${painted.length} caret cells are painted: $painted');
    }
    final rect = focus.caretRect;
    final reported = rect == null ? null : rect.offset - region.offset;
    final shown = painted.firstOrNull;
    if (reported != shown) {
      throw _Failure(
        'the caret is painted at $shown but reported at $reported',
      );
    }
    if (shown == null) return;
    // The caret cell is where the projection puts the caret: at its glyph or
    // after the last glyph of its line.
    final candidates = {
      for (final lineEnd in [false, true])
        window.layout.positionFor(
          selection.extent,
          tableCell: selection.tableCell,
          lineEnd: lineEnd,
        ),
    };
    if (!window.tops.any(
      (top) => candidates.contains(CellOffset(shown.col, shown.row + top)),
    )) {
      throw _Failure(
        'the caret is painted at $shown, but the projection puts it at '
        '$candidates (the window starts at ${window.tops.join('/')})',
      );
    }
    lastCaret = rect!.offset;
  }
}

/// The reader: random Markdown updates, selection gestures, copies and link
/// clicks. Its selection is replayed with the reader's own rules (a press
/// places it, a drag extends it, Command+A takes everything, new text resets
/// it), and every frame must equal a fresh read-only paint of that state.
final class _ReaderSession {
  _ReaderSession(this.backend, this.seed, this.steps) : r = Random(seed);
  final FlarkParseBackend backend;
  final int seed;
  final List<_Step> steps;
  final Random r;
  late final FlarkCellTheme theme = _theme(r);
  late final bool selectable = r.nextInt(4) != 0;
  late final FleuryTester tester;
  late String markdown;
  final opened = <Uri>[];

  /// What the reader should hold: its selection, whether it has focus, and
  /// the geometry of the frame it last painted, which input hits.
  var selection = const FlarkSelection.collapsed(0);
  var focused = false;
  CellDocumentLayout? painted;
  CellRect? region;

  CellWidthPolicy get policy => tester.textPolicy.widths;

  Widget build() => Theme(
    data: const ThemeData(),
    child: FlarkMarkdown(
      markdown: markdown,
      selectable: selectable,
      theme: theme,
      imagePreviewBuilder: _preview,
      onOpenLink: opened.add,
    ),
  );

  Future<void> run() async {
    tester = FleuryTester(
      viewportSize: CellSize(
        _sizes[r.nextInt(_sizes.length)],
        _heights[r.nextInt(_heights.length)],
      ),
      textPolicy: TextPresentationPolicy(
        widths: _policies[r.nextInt(_policies.length)],
      ),
    );
    markdown = _documents[r.nextInt(_documents.length)];
    reset();
    try {
      tester.pumpWidget(build());
      for (var i = 0; i < 100 && loading; i++) {
        await Future<void>.delayed(Duration.zero);
      }
      check('mount', null);
      for (var i = 0; i < steps.length; i++) {
        await perform(i, steps[i]);
      }
    } finally {
      tester.dispose();
    }
  }

  bool get loading {
    tester.render();
    return tester
        .semantics()
        .byRole(SemanticRole.textArea)
        .where((n) => n.label == 'Markdown')
        .isEmpty;
  }

  /// A live editor over the reader's text, or null where it shows source.
  FlarkEditor? editable() {
    if (markdown.contains(RegExp('\r(?!\n)'))) return null;
    final editor = FlarkEditor(backend, text: markdown, caret: 0);
    return editor.sourceMode ? null : editor;
  }

  /// The reader's press, at a cell of the frame it last painted: only one
  /// inside that frame reaches it. A drag goes on reporting [anywhere].
  int? sourceAt(int col, int row, {bool anywhere = false}) {
    final layout = painted, at = region;
    if (layout == null || at == null) return null;
    if (!anywhere && !at.contains(CellOffset(col, row))) return null;
    final x = col - at.offset.col, y = row - at.offset.row;
    if (y < 0 || y >= layout.lines.length) return null;
    final line = layout.lineAt(y, x);
    return line.sourceAt(line.hit(x).$1);
  }

  void select(int base, int extent) {
    final editor = editable();
    if (editor == null) return;
    editor.apply(SetSelection(base, extent));
    selection = editor.selection;
  }

  void mouse(MouseEventKind kind, int col, int row) {
    tester.sendMouse(
      MouseEvent(kind: kind, button: MouseButton.left, col: col, row: row),
    );
  }

  /// New text starts with the legal caret nearest its start. Pumping it
  /// paints a frame, whose geometry the next press hits.
  void update(String next) {
    final changed = next != markdown;
    markdown = next;
    if (changed) reset();
    tester.pumpWidget(build());
    remember();
  }

  /// Records the region and layout of the frame just painted.
  FlarkEditor? remember() {
    painted = null;
    final region = this.region = tester
        .semantics()
        .byRole(SemanticRole.textArea)
        .where((n) => n.label == 'Markdown')
        .firstOrNull
        ?.bounds;
    final editor = editable();
    if (region == null || region.size.isEmpty || editor == null) return null;
    final controller = FlarkFleuryController(editor);
    painted = CellDocumentLayout(controller, region.size.cols, theme, policy);
    controller.dispose();
    return editor;
  }

  void reset() {
    selection = const FlarkSelection.collapsed(0);
    select(0, 0);
  }

  Future<void> perform(int index, _Step step) async {
    final p = step.p;
    final size = tester.viewportSize;
    opened.clear();
    _Click? clicked;
    String? copied;
    switch (step.kind) {
      case 'click' || 'semantic':
        final col = p[0] % size.cols, row = p[1] % size.rows;
        clicked = (col: col, row: row, layout: painted, region: region);
        mouse(MouseEventKind.down, col, row);
        final source = sourceAt(col, row);
        if (selectable && source != null) {
          focused = true;
          select(source, source);
        }
        mouse(MouseEventKind.up, col, row);
      case 'drag':
        final col = p[0] % size.cols, row = p[1] % size.rows;
        mouse(MouseEventKind.down, col, row);
        final source = sourceAt(col, row);
        if (selectable && source != null) {
          focused = true;
          select(source, source);
        }
        // Only a press on the reader's painted rows starts its drag.
        final pressed = region?.contains(CellOffset(col, row)) ?? false;
        final x = p[2] % (size.cols + 2) - 1, y = p[3] % (size.rows + 2) - 1;
        mouse(MouseEventKind.drag, x, y);
        final extent = sourceAt(x, y, anywhere: true);
        if (selectable && pressed && extent != null && (x, y) != (col, row)) {
          focused = true;
          select(selection.base, extent);
        }
        mouse(MouseEventKind.up, x, y);
      case 'key':
        final copy = p[0].isEven;
        tester.sendKey(
          KeyEvent(
            copy ? KeyCode.c : KeyCode.a,
            modifiers: {p[1].isEven ? KeyModifier.superKey : KeyModifier.ctrl},
          ),
        );
        await Future<void>.delayed(Duration.zero);
        if (selectable && focused) {
          final clipboard = tester.clipboard as InProcessClipboard;
          if (!copy) {
            selection = FlarkSelection(0, markdown.length);
          } else if (editable() case final editor?) {
            copied = clipboard.lastWritten;
            editor.apply(SetSelection(selection.base, selection.extent));
            final expected = editor.document.visibleText(
              selection.start,
              selection.end,
            );
            if (copied != expected) {
              throw _Failure(
                'step $index: Copy wrote ${jsonEncode(copied)}, but the '
                'selection $selection shows ${jsonEncode(expected)}',
              );
            }
          }
        }
      case 'resize':
        tester.viewportSize = CellSize(
          _sizes[p[0] % _sizes.length],
          _heights[p[1] % _heights.length],
        );
      case 'policy':
        tester.textPolicy = TextPresentationPolicy(
          widths: _policies[p[0] % _policies.length],
        );
      case 'type':
        tester.type(_typed[p[0] % _typed.length]);
      case 'paste':
        tester.paste(_pastes[p[0] % _pastes.length]);
      case 'load':
        update(_documents[p[0] % _documents.length]);
      default:
        // An edit by the page owner: insert, delete or replace some text.
        var at = p[0] % (markdown.length + 1);
        var end = min(markdown.length, at + p[1] % 8);
        // Never split a surrogate pair: that edit is the page's mistake.
        bool low(int i) =>
            i > 0 &&
            i < markdown.length &&
            markdown.codeUnitAt(i) & 0xFC00 == 0xDC00;
        if (low(at)) at--;
        if (low(end)) end++;
        final text = p[2] % 3 == 0
            ? ''
            : p[2] % 3 == 1
            ? _typed[p[3] % _typed.length]
            : _variant(_pastes[p[3] % _pastes.length], p[4]);
        update(markdown.replaceRange(at, end, text));
    }
    if (editable() == null) focused = false;
    if (!step.render) return;
    check('step $index $step', clicked);
  }

  /// The frame equals a fresh read-only paint of the same text and
  /// selection; a click opened exactly the link painted there.
  void check(String label, _Click? clicked) {
    final frame = tester.render();
    try {
      _checkFrame(frame, clicked);
    } on _Failure catch (failure) {
      throw _Failure(
        '$label: ${failure.message}\n'
        '  markdown ${jsonEncode(markdown)} $selection selectable '
        '$selectable\n'
        '  viewport ${tester.viewportSize} ${policy.ambiguous.name}/'
        '${policy.emojiPresentation.name}/'
        '${policy.emojiVariationSequence.name}\n'
        '${_trace ? tester.renderToString() : ''}',
      );
    }
  }

  void _checkFrame(CellBuffer frame, _Click? clicked) {
    final editor = remember(), region = this.region, layout = painted;
    if (editor == null || region == null || layout == null) return;
    {
      final cols = region.size.cols;
      _checkLayout(layout, markdown, editor.projection.rows, cols, policy);
      final page = _paintFresh(
        backend,
        (source: markdown, selection: selection, sourceMode: false),
        cols: cols,
        rows: layout.lines.length + region.size.rows,
        theme: theme,
        policy: tester.textPolicy,
        focused: false,
      );
      _checkPagePaint(page, layout, selection);
      final tops = _matchWindow(frame, region, page, layout);
      if (!tops.contains(0)) {
        throw _Failure('the reader scrolled its own content to ${tops.first}');
      }
      if (clicked == null || !selection.isCollapsed) return;
      // Only a link's painted text, or an image inside it, opens it; the
      // press hit the frame painted before it.
      Uri? expected;
      final hit = clicked.layout, at = clicked.region;
      final (col, row) = (
        clicked.col - (at?.offset.col ?? 0),
        clicked.row - (at?.offset.row ?? 0),
      );
      if (hit != null &&
          at != null &&
          at.contains(CellOffset(clicked.col, clicked.row)) &&
          row < hit.lines.length) {
        final line = hit.lines[row].cellAt(col);
        if (line.image case final image?) {
          expected = Uri.tryParse(
            editor.document
                    .resourceAt(
                      FlarkSelection(image.start, image.end),
                      image: false,
                    )
                    ?.destination ??
                '',
          );
        }
        for (final glyph in line.glyphs) {
          if (col < glyph.col || col >= glyph.col + glyph.width) continue;
          final at = line.sourceAt(glyph.start);
          for (final resource in editor.document.resources) {
            if (!resource.isImage &&
                resource.contentStart <= at &&
                at < resource.contentEnd) {
              expected ??= Uri.tryParse(resource.destination);
            }
          }
        }
      }
      if (!const {'http', 'https', 'mailto'}.contains(expected?.scheme)) {
        expected = null;
      }
      if (opened.lastOrNull != expected) {
        throw _Failure(
          'a click at ($col,$row) opened ${opened.lastOrNull}; the cell '
          'paints a glyph of ${expected ?? 'no link'}',
        );
      }
    }
  }
}

/// Whether [stack] failed inside Fleury code this package does not own, in
/// a known Fleury defect, to be fixed there: a toolbar picker, or the app's
/// text selection over toolbar labels, outliving the state or text it was
/// built for (open "Paragraph style", change the document, click outside or
/// pick by key; drag across labels, change the document, press
/// Shift+Right). Such a session stops without failing; the test prints how
/// many did.
bool _upstream(StackTrace stack) {
  final frames = stack.toString().split('\n');
  final own = frames.indexWhere(
    (frame) =>
        frame.contains('package:flark_fleury/') ||
        frame.contains('terminal_fuzz_test.dart'),
  );
  return frames
      .take(own < 0 ? frames.length : own)
      .any(
        (frame) =>
            frame.contains('package:fleury_widgets/src/select.dart') ||
            frame.contains('package:fleury/src/widgets/selection/'),
      );
}

/// Greedily drops steps while [fails] still fails, for a readable repro.
Future<List<_Step>> _minimize(
  List<_Step> steps,
  Future<bool> Function(List<_Step>) fails,
) async {
  var current = steps;
  for (var chunk = current.length ~/ 2; chunk >= 1; chunk ~/= 2) {
    for (var start = 0; start < current.length;) {
      final candidate = [
        ...current.take(start),
        ...current.skip(start + chunk),
      ];
      if (await fails(candidate)) {
        current = candidate;
      } else {
        start += chunk;
      }
    }
  }
  return current;
}

void main() {
  final backend = createParseBackend();
  final environment = Platform.environment;
  final seed = int.tryParse(environment['FLARK_FLEURY_SEED'] ?? '') ?? 2026;
  final iterations =
      int.tryParse(environment['FLARK_FLEURY_ITERATIONS'] ?? '') ?? 40;
  final minimize = environment['FLARK_FLEURY_MINIMIZE'] == '1';

  final upstream = <String>{};
  tearDownAll(() {
    if (upstream.isEmpty) return;
    // ignore: avoid_print
    print(
      '${upstream.length} sessions stopped at known Fleury defects:\n'
      '  ${upstream.join('\n  ')}',
    );
  });

  Future<void> fuzz(
    String what,
    int sessions,
    Future<void> Function(int session, List<_Step> steps) run,
  ) async {
    final master = Random(seed ^ what.hashCode);
    final only = int.tryParse(environment['FLARK_FLEURY_SESSION'] ?? '');
    for (var i = 0; i < (only == null ? sessions : 1); i++) {
      final session = only ?? master.nextInt(1 << 30);
      final steps = _generate(Random(session));
      Future<String?> attempt(List<_Step> steps) async {
        try {
          await run(session, steps);
          return null;
        } on _Failure catch (failure) {
          return failure.message;
        } catch (error, stack) {
          if (!_upstream(stack)) return 'threw $error\n$stack';
          upstream.add('$what session $session: $error');
          return null;
        }
      }

      final failure = await attempt(steps);
      if (failure == null) continue;
      var shown = steps;
      if (minimize) {
        // The same failure, wherever it now happens.
        String kind(String message) => message
            .split('\n')
            .first
            .replaceAll(RegExp(r'^step \d+ \S+\([^)]*\)[^:]*: '), '')
            .replaceAll(RegExp(r'"(?:[^"\\]|\\.)*"|\d+'), '#');
        final expected = kind(failure);
        shown = await _minimize(steps, (candidate) async {
          final again = await attempt(candidate);
          return again != null && kind(again) == expected;
        });
      }
      fail(
        '$what session $session failed:\n${await attempt(shown)}\n'
        'steps:\n  ${shown.indexed.map((e) => '${e.$1}: ${e.$2}').join('\n  ')}',
      );
    }
  }

  test(
    'random terminal input keeps editors and their frames consistent '
    '(seed $seed, $iterations sessions)',
    () => fuzz(
      'editor',
      iterations,
      (session, steps) => _EditorSession(backend, session, steps).run(),
    ),
    timeout: const Timeout(Duration(hours: 2)),
  );

  test(
    'random updates and gestures keep the reader consistent '
    '(seed $seed, ${iterations ~/ 2} sessions)',
    () => fuzz(
      'reader',
      iterations ~/ 2,
      (session, steps) => _ReaderSession(backend, session, steps).run(),
    ),
    timeout: const Timeout(Duration(hours: 2)),
  );
}
