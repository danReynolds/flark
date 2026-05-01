import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

// CANARY TEST for RFC 007 (Fenced Code Architecture)
//
// Verifies that:
// 1. Geometry is derived SYNCHRONOUSLY from text changes.
// 2. The "Trailing Empty Line Rule" is active (instant background expansion).
void main() {
  group('SovereignGeometryScanner Canary', () {
    test('Enter inside code block updates geometry synchronously', () {
      // 1. Setup: Open fenced block
      // Line 0: ```
      // Line 1: def foo
      // Line 2: ```
      final initialText = "```\ndef foo\n```";
      final controller = SovereignController(text: initialText);

      // Expected Initial Geometry:
      // Start: 0
      // End: 16
      // Lines: 0 -> 3 (Lines 0, 1, 2 painted)
      expect(controller.geometry.codeBlocks.length, 1);
      expect(controller.geometry.codeBlocks.first.endLine, 3);

      // 2. Action: Simulate Enter at end of "def foo" (Line 1)
      // New Text:
      // Line 0: ```
      // Line 1: def foo
      // Line 2:
      // Line 3: ```
      // Insert \n at index of first \n + length of "def foo"
      // "```\n" is 4 chars. "def foo" is 7 chars.
      // Cursor check: 4 + 7 = 11.
      // initialText[11] is \n. We insert before it.
      // Actually simpler: Insert \n after "foo".

      final newText = "```\ndef foo\n\n```";

      // Simulate TextField behavior: calls value setter (which calls _applyOp)
      controller.value = controller.value.copyWith(
        text: newText,
        selection: const TextSelection.collapsed(offset: 12),
        composing: TextRange.empty,
      );

      // 3. Asset: Geometry MUST be updated in the SAME tick.
      // We do not await anything. We check immediately.

      final blocks = controller.geometry.codeBlocks;
      expect(blocks.length, 1, reason: "Block count preserved");

      final block = blocks.first;

      // Expected Lines: 0, 1, 2, 3 painted.
      // StartLine: 0
      // EndLine: 4 (Exclusive)
      expect(block.startLine, 0);
      expect(
        block.endLine,
        4,
        reason: "EndLine must expand instantly to include new empty line",
      );

      // Verification of specific fix logic (Trailing Empty Line Rule)
      // The block ends with "```". Wait.
      // "```\ndef foo\n" -> Block Part 1
      // "\n" -> The inserted newline
      // "```" -> Closing fence

      // RFC 007: "End" is after closing fence.
      // "```" at line start closes it.
      // So text structure:
      // 0: ```
      // 1: def foo
      // 2: (empty)
      // 3: ```

      // EndOffset includes the closing fence "```" and the newline before it?
      // Parser Rule: Matches `\n` + ` ``` `
      // So end includes line 3.
      // lineAtOffset of end should be 4 (start of next line or EOF).
      // So endLine 4 is correct.
    });

    test('Closed fence paint extent excludes hidden marker lines', () {
      const text = '```\nint main() {\n}\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      expect(controller.geometry.codeBlocks.length, 1);
      final block = controller.geometry.codeBlocks.first;
      // Source geometry still includes opener/closer lines.
      expect(block.startLine, 0);
      expect(block.endLine, 4);
      // Paint extent should include only body lines.
      expect(block.paintStartLine, 1);
      expect(block.paintEndLine, 3);
    });

    test('Visible opener tail remains inside painted fence extent', () {
      const text = '```int main() {\n}\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      expect(controller.geometry.codeBlocks.length, 1);
      final block = controller.geometry.codeBlocks.first;
      // Visible content on opener line should be painted as part of the block.
      expect(block.paintStartLine, 0);
      expect(block.paintEndLine, 2);
    });

    test('Unclosed fence paint extent keeps active EOF body line', () {
      const text = '```\nint main() {\n';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      expect(controller.geometry.codeBlocks.length, 1);
      final block = controller.geometry.codeBlocks.first;
      expect(block.paintStartLine, 1);
      expect(block.paintEndLine, greaterThan(block.paintStartLine));
    });

    test(
      'Bare opener line paints at caret row while fence body is not created',
      () {
        const text = '```';
        final controller = SovereignController(text: text);
        addTearDown(controller.dispose);

        expect(controller.geometry.codeBlocks.length, 1);
        final block = controller.geometry.codeBlocks.first;
        expect(block.startLine, 0);
        expect(block.endLine, 1);
        expect(block.paintStartLine, 0);
        expect(block.paintEndLine, 1);
      },
    );

    test(
      'Trailing Empty Line Rule: Typing Enter at very end of file inside block',
      () {
        // Setup: Unclosed block at EOF
        // ```
        // abc|
        final text1 = "```\nabc";
        final controller = SovereignController(text: text1);

        // Initial:
        // Start 0. End 7 (EOF).
        // lineAtOffset(7) -> 2 (Line 0, 1). Wait. "abc" is on line 1.
        // offset 0-3: ```\n (Line 0)
        // offset 4-7: abc (Line 1)
        // lineAtOffset(7) is line 1?? No, it is start of line.
        // If no newline at end, point 7 is on line 1.
        // So line index returns 1.
        // Block loops startLine(0) -> endLine(1).
        // Paints line 0.
        // DOES NOT Paint line 1 ("abc").
        // Bug in logic?
        // RFC 007 says endLine is EXCLUSIVE in loop.
        // If we want to paint line 1, endLine must be 2.
        // lineIndex.lineAtOffset(7) returns 1.
        // WE NEED TO FIX SCANNER LOGIC FOR EOF CASE if this fails.

        // Let's see what scanner does.

        // Action: Press Enter
        // ```
        // abc
        // |
        final text2 = "```\nabc\n";

        controller.value = controller.value.copyWith(
          text: text2,
          selection: const TextSelection.collapsed(offset: 8),
        );
        // Expectation:
        // Lines: 0, 1, 2 (Empty line).
        // Should paint line 0, 1, AND 2.
        // So endLine must be 3.

        final block = controller.geometry.codeBlocks.first;

        // If logic is correct:
        // endOffset = 8.
        // lineAtOffset(8) -> 2.
        // text[7] is \n.
        // Rule: if text[end-1] == \n, increment.
        // So 2 -> 3.
        // Correct.
        expect(
          block.endLine,
          3,
          reason: "Must cover the new empty line at EOF",
        );
      },
    );

    test(
      'Blockquote geometry tracks contiguous quote lines outside fences',
      () {
        const text = '> a\n> b\n\n```\n> not quote\n```\n> c';
        final controller = SovereignController(text: text);
        addTearDown(controller.dispose);

        final quoteBlocks = controller.geometry.quoteBlocks;
        expect(quoteBlocks.length, 2);

        expect(quoteBlocks[0].startLine, 0);
        expect(quoteBlocks[0].endLine, 2);

        expect(quoteBlocks[1].startLine, 6);
        expect(quoteBlocks[1].endLine, 7);
      },
    );
  });
}
