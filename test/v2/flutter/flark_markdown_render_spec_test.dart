import 'package:flutter_test/flutter_test.dart';

import 'support/render_spec.dart';

/// One-string render specs: markdown source on the left, the rendered display
/// on the right with styled spans wrapped in tags. `expectRendered` parses
/// the source as a document; `expectTypedRendered` types it into a live
/// editor one keystroke at a time (round-trip gated per keystroke) and
/// additionally proves the typed document equals the loaded one.
///
/// Authoring a new spec: add the source with your best-guess annotation and
/// run — on mismatch the failure prints the actual annotated render, so a
/// verified expectation is one copy-paste away. Semantic claims (styled vs
/// literal) must never be weakened to match reality: if the parse disagrees
/// with CommonMark, that is a defect to file, not a spec to adjust.
void main() {
  void render(List<(String, String)> specs) {
    for (final (source, rendered) in specs) {
      test('renders ${_label(source)}', () async {
        await expectRendered(source, rendered);
      });
    }
  }

  group('emphasis family', () {
    render(const [
      ('*em*', '<em>em</em>'),
      ('_em_', '<em>em</em>'),
      ('**strong**', '<strong>strong</strong>'),
      ('__strong__', '<strong>strong</strong>'),
      ('~~strike~~', '<del>strike</del>'),
      ('***both***', '<em><strong>both</strong></em>'),
      ('___both___', '<em><strong>both</strong></em>'),
      (
        'hello *world* how **are** you',
        'hello <em>world</em> how <strong>are</strong> you',
      ),
      ('_em_ and __strong__', '<em>em</em> and <strong>strong</strong>'),
      ('**outer *inner* text**', '<strong>outer <em>inner</em> text</strong>'),
      ('*em **nested** em*', '<em>em <strong>nested</strong> em</em>'),
      ('~~strike **bold**~~', '<del>strike <strong>bold</strong></del>'),
      ('**bold ~~strike~~**', '<strong>bold <del>strike</del></strong>'),
      ('*~~wrapped~~*', '<em><del>wrapped</del></em>'),
      ('foo*bar*baz', 'foo<em>bar</em>baz'),
      ('foo**bar**baz', 'foo<strong>bar</strong>baz'),
      ('(*em*)', '(<em>em</em>)'),
      ('a _em_, right', 'a <em>em</em>, right'),
      ('._x_.', '.<em>x</em>.'),
      ('*a* *b*', '<em>a</em> <em>b</em>'),
      ('**a**_b_', '<strong>a</strong><em>b</em>'),
    ]);
  });

  group('invalid delimiters stay literal', () {
    // Every shape the editor's write paths can no longer produce, pinned as
    // parser-literal too — the write invariant and the parse agree.
    render(const [
      ('**foo **bar', '**foo **bar'),
      ('** foo**', '** foo**'),
      ('x * y * z', 'x * y * z'),
      ('** **', '** **'),
      ('a ** b ** c', 'a ** b ** c'),
      ('foo_bar_', 'foo_bar_'),
      ('foo__bar__baz', 'foo__bar__baz'),
      ('lonely *', 'lonely *'),
      ('*unclosed', '*unclosed'),
      ('unopened*', 'unopened*'),
    ]);
  });

  group('delimiter ambiguity (pinned resolution)', () {
    render(const [
      // cmark resolves `**foo*` as `*<em>foo</em>`; flark deliberately drops
      // partial-delimiter matches so mid-typing states stay source-visible
      // (see docs/production_readiness — "Partial Strong Delimiter Styling").
      ('**mismatch*', '**mismatch*'),
      ('*mismatch**', '*mismatch**'),
      // GFM single-tilde strike hides its markers like every styled run
      // (synthesized Dart-side; the bridge only emits `~~` marker ranges).
      ('~single~ tilde', '<del>single</del> tilde'),
      ('~a~ and ~~b~~', '<del>a</del> and <del>b</del>'),
      ('*a**b*', '<em>a**b</em>'),
      ('****x****', '<strong><strong>x</strong></strong>'),
    ]);
  });

  group('escapes', () {
    render(const [
      (r'\*not em\*', '*not em*'),
      // cmark would emphasize the tail (`*<em>almost</em>`); flark keeps the
      // escaped-star-adjacent run literal — the partial-delimiter deviation
      // applied to the run of three stars.
      (r'\**almost*', '**almost*'),
      (r'**bold \* star**', '<strong>bold * star</strong>'),
      (r'\~\~not strike\~\~', '~~not strike~~'),
    ]);
  });

  group('code spans', () {
    render(const [
      ('`x` code', '<code>x</code> code'),
      ('`code ` span', '<code>code </code> span'),
      // cmark strips one space from each side in HTML output; the editing
      // projection is source-faithful, so the display keeps them.
      ('` x ` stripped', '<code> x </code> stripped'),
      ('`a *not em* b`', '<code>a *not em* b</code>'),
      ('``a ` b`` span', '<code>a ` b</code> span'),
      ('`unclosed span', '`unclosed span'),
    ]);
  });

  group('links', () {
    render(const [
      ('see [docs](https://example.com)', 'see <a>docs</a>'),
      ('[*styled* label](u)', '<a><em>styled</em> label</a>'),
      // Bridge protocol v2: the autolink's angle brackets are markup and
      // hide like every other link form (previously they rendered raw).
      ('<https://example.com>', '<a>https://example.com</a>'),
      ('[a](u "title")', '<a>a</a>'),
    ]);
  });

  group('blocks and inline styling', () {
    render(const [
      // `* ` at line start is a bullet list marker, not emphasis: the marker
      // hides and the item's lone `*` is the content.
      ('* *', '*'),
      ('# Heading **bold**', 'Heading <strong>bold</strong>'),
      ('- item with *em*', 'item with <em>em</em>'),
      ('> quoted ~~s~~', 'quoted <del>s</del>'),
      ('```\n**not bold**\n```', '**not bold**'),
      ('    indented **code**', '    indented **code**'),
      (
        'first *a*\n\nsecond **b**',
        'first <em>a</em>\n\nsecond <strong>b</strong>',
      ),
      ('**foo\nbar**', '<strong>foo\nbar</strong>'),
      ('line one  \nline two', 'line one  \nline two'),
    ]);
  });

  group('unicode', () {
    render(const [
      ('**héllo wörld**', '<strong>héllo wörld</strong>'),
      ('**emoji 🎉 run**', '<strong>emoji 🎉 run</strong>'),
      ('**日本語**', '<strong>日本語</strong>'),
      // NBSP before the close is Unicode whitespace per CommonMark, so the
      // pair must stay literal (parity with FlarkInlineFlanking).
      ('**a **', '**a **'),
    ]);
  });

  group('typed specs (keystroke-by-keystroke, gated)', () {
    const specs = <(String, String)>[
      (
        'hello *world* how **are** you',
        'hello <em>world</em> how <strong>are</strong> you',
      ),
      ('*world*', '<em>world</em>'),
      ('*em*', '<em>em</em>'),
      ('_em_', '<em>em</em>'),
      ('__strong__', '<strong>strong</strong>'),
      ('~~done~~ plain', '<del>done</del> plain'),
      ('`x` code', '<code>x</code> code'),
      ('foo*bar*baz', 'foo<em>bar</em>baz'),
      ('**outer *inner* text**', '<strong>outer <em>inner</em> text</strong>'),
      (r'\*not em\*', '*not em*'),
      ('`a *not em* b`', '<code>a *not em* b</code>'),
      ('**foo **bar', '**foo **bar'),
      ('** foo**', '** foo**'),
      ('a ** b ** c', 'a ** b ** c'),
      ('*unclosed', '*unclosed'),
      ('foo_bar_', 'foo_bar_'),
      (
        'first *a*\n\nsecond **b**',
        'first <em>a</em>\n\nsecond <strong>b</strong>',
      ),
      ('# Heading **bold**', 'Heading <strong>bold</strong>'),
      ('- item with *em*', 'item with <em>em</em>'),
      ('> quoted ~~s~~', 'quoted <del>s</del>'),
      // Task-list sources (`- [ ]`) are deliberately absent: completing a task
      // marker auto-inserts its trailing space, so the typed source is not the
      // literal keystrokes — by design.
    ];
    for (final (source, rendered) in specs) {
      test('typing ${_label(source)}', () async {
        await expectTypedRendered(source, rendered);
      });
    }
  });
}

String _label(String source) => '"${source.replaceAll('\n', r'\n')}"';
