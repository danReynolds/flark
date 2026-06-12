import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

/// Regression: comrak sourcepos columns are BYTE-based. Treating them as
/// character counts shifted every range after a multi-byte character by its
/// UTF-8 surplus, so any line containing a smart quote, accent, or emoji
/// lost inline styling entirely (marker ranges no longer landed on the
/// markers and were dropped).
void main() {
  Future<FlarkMarkdownParseResult> parse(String markdown) {
    final backend = FlarkNativeComrakParseBackend.tryLoad();
    expect(backend, isNotNull, reason: 'native comrak bridge must load');
    return backend!.parse(
      FlarkMarkdownParseRequest(
        revision: 1,
        markdown: markdown,
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );
  }

  FlarkMarkdownInlineToken emphasisOf(FlarkMarkdownParseResult result) {
    return result.inlineTokens.singleWhere(
      (token) => token.type == 'emphasis',
    );
  }

  test('emphasis ranges stay aligned after a 3-byte smart quote', () async {
    const markdown = 'well isn’t that *interesting* here.';
    final result = await parse(markdown);

    final emphasis = emphasisOf(result);
    expect(
      markdown.substring(emphasis.sourceRange.start, emphasis.sourceRange.end),
      '*interesting*',
    );
    expect(result.hiddenRanges, hasLength(2));
    for (final hidden in result.hiddenRanges) {
      expect(
        markdown.substring(hidden.sourceRange.start, hidden.sourceRange.end),
        '*',
      );
    }
  });

  test('emphasis ranges stay aligned after a 4-byte emoji', () async {
    const markdown = 'launch 🚀 the *spice* now';
    final result = await parse(markdown);

    final emphasis = emphasisOf(result);
    expect(
      markdown.substring(emphasis.sourceRange.start, emphasis.sourceRange.end),
      '*spice*',
    );
    expect(result.hiddenRanges, hasLength(2));
  });

  test('multi-byte INSIDE the emphasis keeps the closing marker', () async {
    const markdown = 'a *café* b';
    final result = await parse(markdown);

    final emphasis = emphasisOf(result);
    expect(
      markdown.substring(emphasis.sourceRange.start, emphasis.sourceRange.end),
      '*café*',
    );
  });

  test('pure ascii is unchanged', () async {
    const markdown = 'well isnt that *interesting* here.';
    final result = await parse(markdown);
    final emphasis = emphasisOf(result);
    expect(
      markdown.substring(emphasis.sourceRange.start, emphasis.sourceRange.end),
      '*interesting*',
    );
  });
}
