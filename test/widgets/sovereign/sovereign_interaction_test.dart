import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_style_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';

void main() {
  group('Sovereign Interaction Tests (Interpolation)', () {
    // Helper to assert runs and hidden ranges
    void assertRunAndHidden(
      String text,
      ScannerResult result,
      int runIndex,
      String expectedText,
      SovereignStyle expectedStyle, {
      required int expectedHiddenCount,
    }) {
      // Run Assertions
      expect(
        runIndex,
        lessThan(result.runs.length),
        reason: 'Run index $runIndex out of bounds',
      );
      final run = result.runs[runIndex];
      expect(
        text.substring(run.start, run.end),
        expectedText,
        reason: 'Text content mismatch at index $runIndex',
      );
      expect(
        run.style,
        expectedStyle,
        reason: 'Style mismatch at index $runIndex',
      );

      // Hidden Range Assertions
      // We expect the *total* hidden ranges for this entire result to match the sum of expectations.
      // But for this helper, let's just assert the count if it's simple.
      // Or better: pass expected markers logic?
      // Let's just check if we found specific markers inside this run.

      // For V1 extraction logic:
      // Bold (4 chars): start, start+2, end-2, end
      // Italic/Code (2 chars): start, start+1, end-1, end

      // We can verify that for THIS run, appropriate ranges were added.
      // But extractHiddenRanges returns a flat list.
    }

    test('Bold inside Blockquote', () {
      final text = '> **Bold** Text';
      // Expected:
      // > is plain (rendered by block painter, but text exists)
      // **Bold** is bold run

      final result = SovereignStyleScanner.scan(text);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(result.runs.length, 1);
      assertRunAndHidden(
        text,
        result,
        0,
        '**Bold**',
        SovereignStyle.bold,
        expectedHiddenCount: 2,
      );

      // Verify markers are hidden
      // Run is [2, 10] (**Bold**)
      // Hidden should be [2, 4] (**) and [8, 10] (**)
      expect(hidden.length, 2);
      expect(text.substring(hidden[0].start, hidden[0].end), '**');
      expect(hidden[0].start, 2);
      expect(hidden[0].end, 4);
      expect(text.substring(hidden[1].start, hidden[1].end), '**');
      expect(hidden[1].start, 8);
      expect(hidden[1].end, 10);
    });

    test('Italic inside List', () {
      final text = '- _Italic_ Item';
      final result = SovereignStyleScanner.scan(text);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(result.runs.length, 1);
      assertRunAndHidden(
        text,
        result,
        0,
        '_Italic_',
        SovereignStyle.italic,
        expectedHiddenCount: 2,
      );

      // Hidden: [2, 3] (_) and [9, 10] (_)
      expect(hidden.length, 2);
      expect(text.substring(hidden[0].start, hidden[0].end), '_');
      expect(hidden[0].start, 2);
      expect(hidden[0].end, 3);
      expect(text.substring(hidden[1].start, hidden[1].end), '_');
      expect(hidden[1].start, 9);
      expect(hidden[1].end, 10);
    });

    test('Code inside Header', () {
      final text = '# Header `Code`';
      final result = SovereignStyleScanner.scan(text);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(result.runs.length, 1);
      assertRunAndHidden(
        text,
        result,
        0,
        '`Code`',
        SovereignStyle.code,
        expectedHiddenCount: 2,
      );

      // Hidden: ` and `
      expect(hidden.length, 2);
      expect(text.substring(hidden[0].start, hidden[0].end), '`');
      expect(hidden[0].start, 9);
      expect(hidden[0].end, 10);
      expect(text.substring(hidden[1].start, hidden[1].end), '`');
      expect(hidden[1].start, 14);
      expect(hidden[1].end, 15);
    });

    // -------------------------------------------------------------------------
    // BOUNDARY CROSSING (The likely culprit)
    // -------------------------------------------------------------------------

    test('Style crossing Newline', () {
      final text = '**Bold\nLine**';
      final result = SovereignStyleScanner.scan(text);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(result.runs.length, 1);
      assertRunAndHidden(
        text,
        result,
        0,
        '**Bold\nLine**',
        SovereignStyle.bold,
        expectedHiddenCount: 2,
      );

      expect(hidden.length, 2);
      expect(text.substring(hidden[0].start, hidden[0].end), '**');
      expect(hidden[0].start, 0);
      expect(hidden[0].end, 2);
      expect(text.substring(hidden[1].start, hidden[1].end), '**');
      expect(hidden[1].start, 11);
      expect(hidden[1].end, 13);
    });

    test('Style crossing Block Boundary', () {
      // # Header **Bold
      // End**
      // V1 Scanner is mostly block-agnostic, checking excluded ranges.
      // Headers are NOT excluded ranges.
      // So this should technically match as one bold run across the header boundary.
      // Whether that's desirable is a design choice, but it is "Correct" for the scanner.

      final text = '# H **Start\nEnd**';
      final result = SovereignStyleScanner.scan(text);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(result.runs.length, 1);
      assertRunAndHidden(
        text,
        result,
        0,
        '**Start\nEnd**',
        SovereignStyle.bold,
        expectedHiddenCount: 2,
      );

      expect(hidden.length, 2);
      expect(text.substring(hidden[0].start, hidden[0].end), '**');
      expect(hidden[0].start, 4);
      expect(hidden[0].end, 6);
      expect(text.substring(hidden[1].start, hidden[1].end), '**');
      expect(hidden[1].start, 15);
      expect(hidden[1].end, 17);
    });

    test('Style NOT crossing Exclusion (Code Block)', () {
      // **Start
      // ```
      // code
      // ```
      // End**

      // We must simulate the excluded ranges that SovereignController would pass.
      // Code block is [8, 20] (approx).
      final text = '**Start\n```\ncode\n```\nEnd**';
      final codeBlockStart = text.indexOf('```');
      final codeBlockEnd = text.indexOf('```', codeBlockStart + 3) + 3;
      final excluded = [TextRange(start: codeBlockStart, end: codeBlockEnd)];

      final result = SovereignStyleScanner.scan(text, excludedRanges: excluded);

      // Expectation:
      // 1. **Start opens before exclusion.
      // 2. Hit exclusion. Scanner RESETs state (according to code).
      // 3. Jump to end of exclusion.
      // 4. End** sees End and **.
      // Since state reset, the first ** is abandoned.
      // The second ** is seen as... opener? Or just plain?
      // Since it's at end of text, it opens but never closes.
      // So 0 runs expected.

      expect(result.runs, isEmpty);
    });

    test('Style inside "Plain" gap between styles', () {
      // **A** plain **B**
      final text = '**A** plain **B**';
      final result = SovereignStyleScanner.scan(text);

      expect(result.runs.length, 2);
      assertRunAndHidden(
        text,
        result,
        0,
        '**A**',
        SovereignStyle.bold,
        expectedHiddenCount: 4,
      );
      assertRunAndHidden(
        text,
        result,
        1,
        '**B**',
        SovereignStyle.bold,
        expectedHiddenCount: 4,
      );
    });

    test('Nested Styles (V1: Flat priority or simple nesting?)', () {
      // **Bold _Italic_**
      // Current Scanner:
      // Sees ** (Open Bold)
      // Sees _ (Open Italic check... but code checks CodeStart? logic?)
      // Let's check logic:
      // if (codeStart == null && char == 42) -> Bold check
      // else (codeStart == null && char == 95) -> Italic check

      // It ALLOWS nesting if you trace the ifs.
      // If inside bold (boldStart != null), it hits `else if` for `_`?
      // No, `else if` chains on `char`.
      // `char == 42` (Bold)
      // `char == 95` (Italic)
      // They are mutually exclusive characters.
      // So yes, it can find `_` while `boldStart` is set.

      final text = '**Bold _Italic_**';
      final result = SovereignStyleScanner.scan(text);

      // We expect runs for both?
      // **Bold _Italic_** is ONE run of Bold?
      // And _Italic_ is ONE run of Italic?
      // Overlapping runs?
      // Scanner logic:
      // `_addRun` adds to list.
      // It does NOT flatten.
      // So we get [4, 11] Italic, [0, 15] Bold.
      // List order depends on closing order.
      // _Italic_ closes first. So Run 0 is Italic.
      // **...** closes second. So Run 1 is Bold.

      expect(result.runs.length, 2);
      assertRunAndHidden(
        text,
        result,
        0,
        '_Italic_',
        SovereignStyle.italic,
        expectedHiddenCount: 4,
      );
      assertRunAndHidden(
        text,
        result,
        1,
        '**Bold _Italic_**',
        SovereignStyle.bold,
        expectedHiddenCount: 4,
      );
    });
  });
}
