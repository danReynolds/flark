import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_style_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';

void main() {
  group('SovereignStyleScanner Level 1 (Logic + Performance Invariants)', () {
    // -------------------------------------------------------------------------
    // INVARIANT HELPERS
    // -------------------------------------------------------------------------
    void assertInvariants(ScannerResult result, int spanBudget) {
      final runs = result.runs;

      // 1. Valid Range
      // Runs must be sorted and non-overlapping?
      // V1 implementation produces sorted runs by definition (linear scan).
      int lastEnd = -1;
      for (int i = 0; i < runs.length; i++) {
        final run = runs[i];
        expect(run.start, lessThan(run.end), reason: 'Empty run');
        if (lastEnd != -1) {
          expect(
            run.start,
            greaterThanOrEqualTo(lastEnd),
            reason: 'Overlap or unsorted',
          );

          // Coalescing Check (No adjacent identical styles)
          if (run.start == lastEnd) {
            final prev = runs[i - 1];
            expect(
              run.style,
              isNot(prev.style),
              reason: 'Adjacent identical style (Coalescing failure)',
            );
          }
        }
        lastEnd = run.end;
      }

      // 2. Budget Compliance
      expect(
        runs.length,
        lessThanOrEqualTo(spanBudget),
        reason: 'Exceeded Span Budget',
      );

      // 3. Completeness consistency
      if (result.complete) {
        // expect(result.validTo, ...); // Ideally text.length, but we don't pass text here to assert.
      } else {
        // If incomplete, validTo is the safe prefix.
        expect(result.validTo, greaterThanOrEqualTo(0));
        if (runs.isNotEmpty) {
          expect(result.validTo, greaterThanOrEqualTo(runs.last.end));
        }
      }
    }

    // -------------------------------------------------------------------------
    // BASIC LOGIC
    // -------------------------------------------------------------------------

    test('Styles: Bold, Italic, Code', () {
      final text = 'Plain **Bold** _Italic_ `Code`';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      expect(result.runs.length, 3);
      // Bold
      expect(
        text.substring(result.runs[0].start, result.runs[0].end),
        '**Bold**',
      );
      expect(result.runs[0].style, SovereignStyle.bold);
      // Italic
      expect(
        text.substring(result.runs[1].start, result.runs[1].end),
        '_Italic_',
      );
      expect(result.runs[1].style, SovereignStyle.italic);
      // Code
      expect(
        text.substring(result.runs[2].start, result.runs[2].end),
        '`Code`',
      );
      expect(result.runs[2].style, SovereignStyle.code);
    });

    test('Prefix Stability (Unclosed Partial)', () {
      final text = 'foo **bar';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      expect(result.runs, isEmpty); // No runs found (safe prefix)
      expect(
        result.complete,
        true,
      ); // It completed parsing, just found nothing.
      expect(result.validTo, text.length);
    });

    test('Code Priority (No bold inside code)', () {
      final text = '`**foo**`';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      expect(result.runs.length, 1);
      final run = result.runs.first;
      expect(run.style, SovereignStyle.code);
      expect(text.substring(run.start, run.end), '`**foo**`');
    });

    test('Styles: Markdown links and autolinks', () {
      const text =
          'Go to [OpenAI](https://openai.com) and https://example.com or <https://dune.ai>';
      final result = SovereignStyleScanner.scan(text, timeBudgetMicros: 200000);
      assertInvariants(result, 250);

      final linkRuns = result.runs
          .where((run) => run.style == SovereignStyle.link)
          .toList(growable: false);
      expect(linkRuns.length, 3);
      expect(text.substring(linkRuns[0].start, linkRuns[0].end), 'OpenAI');
      expect(
        text.substring(linkRuns[1].start, linkRuns[1].end),
        'https://example.com',
      );
      expect(
        text.substring(linkRuns[2].start, linkRuns[2].end),
        'https://dune.ai',
      );
    });

    test('linkAtCaret resolves markdown link label + url', () {
      const text = 'A [test](https://google.com) B';
      final caret = text.indexOf('test') + 2;
      final match = SovereignStyleScanner.linkAtCaret(text, caret);
      expect(match, isNotNull);
      expect(match!.kind, SovereignLinkMatchKind.markdown);
      expect(match.labelText(text), 'test');
      expect(match.urlText(text), 'https://google.com');
      expect(
        text.substring(match.fullStart, match.fullEnd),
        '[test](https://google.com)',
      );
    });

    test('Links: markdown link hidden ranges hide wrapper and URL', () {
      const text = 'A [test](https://google.com) B';
      final result = SovereignStyleScanner.scan(text, timeBudgetMicros: 200000);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(hidden.any((r) => text.substring(r.start, r.end) == '['), isTrue);
      expect(
        hidden.any(
          (r) => text.substring(r.start, r.end) == '](https://google.com)',
        ),
        isTrue,
      );
    });

    test('Links: angle autolink hidden ranges hide angle brackets only', () {
      const text = '<https://dune.ai>';
      final result = SovereignStyleScanner.scan(text, timeBudgetMicros: 200000);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(hidden, contains(TextRange(start: 0, end: 1)));
      expect(
        hidden,
        contains(TextRange(start: text.length - 1, end: text.length)),
      );
      expect(
        hidden.any((r) => text.substring(r.start, r.end).contains('https://')),
        isFalse,
      );
    });

    test('linkAtCaret resolves bare URL and angle autolink', () {
      const text = 'Visit https://example.com and <https://dune.ai>.';
      final bare = SovereignStyleScanner.linkAtCaret(
        text,
        text.indexOf('example') + 2,
      );
      expect(bare, isNotNull);
      expect(bare!.kind, SovereignLinkMatchKind.bare);
      expect(bare.urlText(text), 'https://example.com');

      final angle = SovereignStyleScanner.linkAtCaret(
        text,
        text.indexOf('dune.ai') + 2,
      );
      expect(angle, isNotNull);
      expect(angle!.kind, SovereignLinkMatchKind.autolink);
      expect(angle.urlText(text), 'https://dune.ai');
    });

    test('linkAtCaret resolves reference-style link and definition URL', () {
      const text = '[Docs][api]\n\n[api]: https://dune.ai/docs';
      final caret = text.indexOf('Docs') + 2;
      final match = SovereignStyleScanner.linkAtCaret(text, caret);
      expect(match, isNotNull);
      expect(match!.kind, SovereignLinkMatchKind.reference);
      expect(match.labelText(text), 'Docs');
      expect(match.referenceLabelText(text), 'api');
      expect(
        SovereignStyleScanner.resolveReferenceLinkUrl(text, match),
        'https://dune.ai/docs',
      );
    });

    test('referenceDefinitionForLink returns definition span metadata', () {
      const text = '[Docs][api]\n\n[api]: https://dune.ai/docs';
      final match = SovereignStyleScanner.linkAtCaret(
        text,
        text.indexOf('Docs') + 1,
      );
      expect(match, isNotNull);
      final def = SovereignStyleScanner.referenceDefinitionForLink(
        text,
        match!,
      );
      expect(def, isNotNull);
      expect(def!.labelText(text), 'api');
      expect(def.urlText(text), 'https://dune.ai/docs');
      expect(
        text.substring(def.lineStart, def.lineEnd),
        '[api]: https://dune.ai/docs',
      );
    });

    test('Links: reference link hidden ranges hide wrappers and ref label', () {
      const text = '[Docs][api]\n[api]: https://dune.ai/docs';
      final result = SovereignStyleScanner.scan(text, timeBudgetMicros: 200000);
      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );

      expect(hidden.any((r) => text.substring(r.start, r.end) == '['), isTrue);
      expect(
        hidden.any((r) => text.substring(r.start, r.end) == '][api]'),
        isTrue,
      );
    });

    test('Links: markdown image syntax does not style alt text as a link', () {
      const text = '![alt](https://image.cdn/foo.png)';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      final linkRuns = result.runs
          .where((run) => run.style == SovereignStyle.link)
          .toList(growable: false);
      expect(
        linkRuns.any((run) => text.substring(run.start, run.end) == 'alt'),
        isFalse,
      );
      final imageRuns = result.runs
          .where((run) => run.style == SovereignStyle.image)
          .toList(growable: false);
      expect(imageRuns, hasLength(1));
    });

    test('Images: markdown image syntax produces image placeholder run', () {
      const text = 'Before ![alt text](https://image.cdn/foo.png) after';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      final imageRuns = result.runs
          .where((run) => run.style == SovereignStyle.image)
          .toList(growable: false);
      expect(imageRuns, hasLength(1));
      final run = imageRuns.single;
      expect(
        text.substring(run.start, run.end),
        '![alt text](https://image.cdn/foo.png)',
      );

      final hidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        result.runs,
      );
      expect(hidden, contains(TextRange(start: run.start, end: run.start + 2)));
      expect(
        hidden.any(
          (r) =>
              text.substring(r.start, r.end) == '](https://image.cdn/foo.png)',
        ),
        isTrue,
      );
    });

    test('imageAtCaret resolves markdown image alt text + url', () {
      const text = 'Before ![diagram](https://cdn.example/diagram.png) after';
      final caret = text.indexOf('diagram') + 3;
      final match = SovereignStyleScanner.imageAtCaret(text, caret);

      expect(match, isNotNull);
      expect(match!.altText(text), 'diagram');
      expect(match.urlText(text), 'https://cdn.example/diagram.png');
      expect(
        text.substring(match.fullStart, match.fullEnd),
        '![diagram](https://cdn.example/diagram.png)',
      );
    });

    test('imageAtCaret does not match caret after image syntax', () {
      const text = '![diagram](https://cdn.example/diagram.png)';
      expect(SovereignStyleScanner.imageAtCaret(text, text.length), isNull);
    });

    test('imageAtCaret does not match markdown link or url text', () {
      const text = '[link](https://dune.ai) and https://example.com';
      expect(
        SovereignStyleScanner.imageAtCaret(text, text.indexOf('link') + 1),
        isNull,
      );
      expect(
        SovereignStyleScanner.imageAtCaret(text, text.indexOf('example') + 1),
        isNull,
      );
    });

    test('Links: code spans suppress autolink styling', () {
      const text = '`https://example.com`';
      final result = SovereignStyleScanner.scan(text);
      assertInvariants(result, 250);

      expect(result.runs.length, 1);
      expect(result.runs.first.style, SovereignStyle.code);
      expect(
        text.substring(result.runs.first.start, result.runs.first.end),
        '`https://example.com`',
      );
    });

    // -------------------------------------------------------------------------
    // PERFORMANCE BARRIERS & INVARIANTS
    // -------------------------------------------------------------------------

    test('Budget: Rollback Stability (Mid-Token)', () {
      final text = 'Prefix **BoldToken** Suffix';
      // We want to force a stop in the middle of "BoldToken".
      // Offset of ** is 7. Offset of B is 9. Offset of T is 13.
      // Let's stop at char 13 ('T').

      final result = SovereignStyleScanner.scan(text, charLimit: 13);
      assertInvariants(result, 250);

      expect(result.complete, false);
      expect(result.runs, isEmpty); // Should roll back the open bold

      // Last Safe Offset should be 7 (start of **) or 0?
      // The scanner updates lastSafeOffset when a run closes or when no styles are open.
      // Before **, lastSafeOffset depends on where it updates.
      // Line 189: if (codeStart == null && boldStart == null...) lastSafeOffset = currentOffset.
      // At index 6 (space), no styles open -> lastSafeOffset = 7.
      // At index 7 (*), boldStart opens.
      // So at 13, boldStart is open.
      // It returns runs (invalid) and validTo (lastSafeOffset).

      expect(result.validTo, 7); // Safe up to the **
    });

    test('Budget: Nested Rollback', () {
      // **a _b_ c**
      final text = 'Start **Outer _Inner_ End**';
      // Indices:
      // 012345678901234567890123456
      // Start **Outer _I
      // _ opens at 14.
      // Force stop at 16 ('n').

      final result = SovereignStyleScanner.scan(text, charLimit: 16);

      expect(result.complete, false);
      expect(result.runs, isEmpty);
      // Safe point:
      // Before **, safe=6.
      // Inside **, boldOpen=true. safe isn't updated.
      // Inside _, italicOpen=true.
      // So validTo should be 6.
      expect(result.validTo, 6);
    });

    test('Budget: Span Limit (No Explosion)', () {
      // Create text with 300 bold items
      // **a** **b** ...
      final buffer = StringBuffer();
      for (int i = 0; i < 300; i++) {
        buffer.write('**$i** ');
      }
      final text = buffer.toString();

      final result = SovereignStyleScanner.scan(
        text,
        spanBudget: 250,
        timeBudgetMicros: 100000,
      );
      assertInvariants(result, 250);

      expect(result.complete, false);
      expect(result.runs.length, 250); // Hard cap

      // Check last run is valid
      final lastRun = result.runs.last;
      expect(lastRun.style, SovereignStyle.bold);
    });

    test('Monotonicity (Strict)', () {
      // Compare Budget A vs Budget B (B > A)
      final text = 'Start **A** **B** **C** End';

      // Scan with 1 run limit
      final resultA = SovereignStyleScanner.scan(text, spanBudget: 1);
      // Scan with 2 runs limit
      final resultB = SovereignStyleScanner.scan(text, spanBudget: 2);

      assertInvariants(resultA, 1);
      assertInvariants(resultB, 2);

      // 1. processedOffset (validTo) check
      expect(resultB.validTo, greaterThanOrEqualTo(resultA.validTo));

      // 2. Prefix check
      // resultA runs should be exactly the start of resultB runs
      expect(resultA.runs.length, 1);
      expect(resultB.runs.length, 2);
      expect(resultB.runs[0].start, resultA.runs[0].start);
      expect(resultB.runs[0].end, resultA.runs[0].end);
    });

    test('Stress: Marker Density', () {
      // _0_ **1** _2_ **3** ... alternating markers to prevent coalescing
      final text = List.generate(1000, (i) {
        return i % 2 == 0 ? '_${i}_' : '**$i**';
      }).join();
      // 1000 items (each is a run). Budget 250.

      final result = SovereignStyleScanner.scan(
        text,
        spanBudget: 250,
        timeBudgetMicros: 100000, // Ensure we hit span budget, not time
      );

      expect(result.complete, false);
      expect(result.runs.length, 250);

      // Verify alternated styles to prove they exist
      expect(result.runs[0].style, SovereignStyle.italic);
      expect(result.runs[1].style, SovereignStyle.bold);

      assertInvariants(result, 250);
    });
  });
}
