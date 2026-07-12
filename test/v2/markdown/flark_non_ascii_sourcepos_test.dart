import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/flark_test_paths.dart';

/// Regression: comrak sourcepos columns are BYTE-based. Treating them as
/// character counts shifted every range after a multi-byte character by its
/// UTF-8 surplus, so any line containing a smart quote, accent, or emoji
/// lost inline styling entirely (marker ranges no longer landed on the
/// markers and were dropped).
void main() {
  // This is a native-bridge regression suite, so — like every other
  // parser-backed suite (transition matrix, upstream contracts, parser-backed
  // fuzz) — it skips cleanly when the bridge is not built rather than hard
  // failing. That keeps the documented `verify_package_confidence.sh
  // --skip-native` fast path green for contributors and CI lanes without a
  // Rust toolchain; the native lanes build the bridge and run it for real.
  final libPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libPath.isEmpty || !File(libPath).existsSync()) {
    test('native bridge not built; non-ascii sourcepos suite skipped', () {
      expect(true, isTrue);
    });
    return;
  }

  final backend = FlarkNativeComrakParseBackend.withNativeBridge(
    overrideLibraryPath: libPath,
  );

  Future<FlarkMarkdownParseResult> parse(String markdown) {
    return backend.parse(
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
