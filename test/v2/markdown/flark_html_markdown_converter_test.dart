import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkHtmlMarkdown', () {
    String md(String html) => FlarkHtmlMarkdown.convert(html);

    test('converts inline emphasis, strong, strike, and code', () {
      expect(md('<b>foo</b>'), '**foo**');
      expect(md('<strong>foo</strong>'), '**foo**');
      expect(md('<i>foo</i>'), '*foo*');
      expect(md('<em>foo</em>'), '*foo*');
      expect(md('<s>foo</s>'), '~~foo~~');
      expect(md('<del>foo</del>'), '~~foo~~');
      expect(md('<code>foo()</code>'), '`foo()`');
    });

    test('nests inline styles', () {
      expect(md('<b><i>x</i></b>'), '***x***');
    });

    test('converts links and images', () {
      expect(md('<a href="https://x.com">link</a>'), '[link](https://x.com)');
      expect(md('<img src="i.png" alt="pic">'), '![pic](i.png)');
    });

    test('converts headings and paragraphs', () {
      expect(md('<h2>Title</h2>'), '## Title');
      expect(md('<p>one</p><p>two</p>'), 'one\n\ntwo');
    });

    test('collapses HTML whitespace', () {
      expect(md('<p>  foo   bar  </p>'), 'foo bar');
      expect(md('<p>foo\n   bar</p>'), 'foo bar');
    });

    test('honours <br> as a line break and <hr> as a rule', () {
      expect(md('<p>a<br>b</p>'), 'a\nb');
      expect(md('<hr>'), '---');
    });

    test('converts unordered and ordered lists', () {
      expect(md('<ul><li>a</li><li>b</li></ul>'), '- a\n- b');
      expect(md('<ol><li>a</li><li>b</li></ol>'), '1. a\n2. b');
    });

    test('indents nested lists', () {
      expect(
        md('<ul><li>a<ul><li>b</li></ul></li></ul>'),
        '- a\n  - b',
      );
    });

    test('converts block quotes and code blocks', () {
      expect(md('<blockquote>quote</blockquote>'), '> quote');
      expect(md('<pre>line one\nline two</pre>'), '```\nline one\nline two\n```');
    });

    test('degrades unknown elements to their text', () {
      expect(md('<span class="x">plain</span>'), 'plain');
    });

    test('converts a realistic copied snippet', () {
      const html =
          '<h1>Heading</h1><p>Some <b>bold</b> and a '
          '<a href="https://x.com">link</a>.</p>'
          '<ul><li>first</li><li>second</li></ul>';
      expect(
        md(html),
        '# Heading\n\n'
        'Some **bold** and a [link](https://x.com).\n\n'
        '- first\n- second',
      );
    });

    test('returns empty for empty input', () {
      expect(md(''), '');
      expect(md('   '), '');
    });
  });
}
