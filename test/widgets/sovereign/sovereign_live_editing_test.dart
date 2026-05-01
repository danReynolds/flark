import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';

void main() {
  group('SovereignController Live Editing', () {
    late SovereignController controller;

    setUp(() {
      controller = SovereignController();
    });

    tearDown(() {
      controller.dispose();
    });

    test('Typing "> " triggers Blockquote', () async {
      controller.text = ''; // Start empty

      // Simulate typing '>'
      controller.value = TextEditingValue(
        text: '>',
        selection: TextSelection.collapsed(offset: 1),
      );

      // Should still be Paragraph/Implicit
      expect(controller.decoration.tree.blocks.length, 0); // Empty or Paragraph

      // Simulate typing ' '
      controller.value = TextEditingValue(
        text: '> ',
        selection: TextSelection.collapsed(offset: 2),
      );

      // Wait for async parser
      await Future.delayed(const Duration(milliseconds: 50));

      // Now it should be a Blockquote
      expect(controller.decoration.tree.blocks.length, 1);
      expect(
        controller.decoration.tree.blocks.first.type,
        BlockType.blockquote,
      );
    });

    test('Typing "```" + Enter triggers Fenced Code', () async {
      controller.text = '';

      // Type ```
      String text = '```';
      controller.value = TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: 3),
      );

      // Wait for async parser
      await Future.delayed(const Duration(milliseconds: 50));

      // Should allow fencing if rules V1 match
      expect(controller.decoration.tree.blocks.length, 1);
      expect(
        controller.decoration.tree.blocks.first.type,
        BlockType.fencedCode,
      );
    });

    test(
      'SovereignController Live Editing Active Formatting: Typing "**" triggers Hidden Range',
      () async {
        // Step 1: Type "**Bold**"
        final initial = '**Bold**';
        controller.text = initial;

        // Current Selection: At end
        controller.selection = TextSelection.collapsed(offset: initial.length);

        // Because we are "touching" the range (adjacency), it should remain visible (Pop Scope).
        // Logic: [start-1, end+1].
        // Range: [0, 2] and [6, 8].
        // Cursor: 8.
        // 8 is adjacent to [6, 8]'s end (7... wait range is exclusive end?).
        // Range [6, 8] is char 6 and 7.
        // Adjacent zone: [5, 9].
        // Cursor 8 is inside. So Visibile.

        // Snap Hiding (Model C): [start, end-1].
        // Cursor 8. Range [6, 8].
        // 8 <= 7 is False.
        // Expected: HIDDEN.
        // Expected: HIDDEN.

        // Async Wait for parser
        await Future.delayed(Duration.zero);

        expect(controller.decoration.hiddenRanges, isNotEmpty);

        // Step 2: Move cursor away
        // Move to 0. Adjacent to [0, 2]?
        // Range [0, 2] -> 0, 1. Zone [-1, 3].
        // Cursor 0 is inside. Visible.

        // Add text " suffix" -> cursor at end.
        // "**Bold** suffix"
        // Cursor at 15.
        // Far from [0, 2] and [6, 8].
        // Should be hidden.

        controller.value = TextEditingValue(
          text: '**Bold** suffix',
          selection: TextSelection.collapsed(offset: 15),
        );

        // Expect hidden ranges
        expect(controller.decoration.hiddenRanges.length, 2);
      },
    );

    // -------------------------------------------------------------------------
    // BUG REPRODUCTION (Phase 7)
    // -------------------------------------------------------------------------

    test('Bug: Fenced Code ticks should be hidden', () async {
      // User Report: "code fencing still shows the original ticks inside the generated region"
      // Expected: The ``` markers should be in hiddenRanges.

      final text = '```\ncode\n```';
      controller.text = text;
      // Cursor away to ensure Pop Scope doesn't show them
      controller.selection = TextSelection.collapsed(
        offset: 5,
      ); // inside "code"

      // Wait for async/microtask parser to update BlockTree
      await Future.delayed(const Duration(milliseconds: 50));

      // We expect the blocks to be parsed
      expect(controller.decoration.tree.blocks.length, 1);
      expect(
        controller.decoration.tree.blocks.first.type,
        BlockType.fencedCode,
      );

      // We expect the ``` markers to be hidden.
      // Start fence: 0-3 (+newline?) -> [0, 4] usually? Or just [0, 3] ticks.
      // End fence: last 3 chars.

      // If this fails (isEmpty), it confirms the bug.
      expect(
        controller.decoration.hiddenRanges,
        isNotEmpty,
        reason: 'Fenced code ticks are NOT hidden',
      );

      // Detailed check
      // Should find logic that covers the start/end ticks
      // ...
    });

    test(
      'SovereignController Live Editing Bug: Italic Closing Behavior',
      () async {
        // User Report: "Closing italics with asterisks also don't immediately get rid of the closing asterisk"
        // Testing the exact sequence:
        controller.text = '';

        // 1. Type "*italic*"
        controller.value = TextEditingValue(
          text: '*italic*',
          selection: TextSelection.collapsed(offset: 8), // After last *
        );

        // At this point, valid markdown logic says it IS italics options.
        // But Cursor is at 8, adjacent to 7 (last char).
        // Range of style: [0, 8].
        // Hidden parts: [0, 1] and [7, 8].
        // Snap Hiding (Model C): [start, end-1].
        // Range [0, 8]. Markers [0, 1] and [7, 8].
        // Last Marker [7, 8]. Zone [7, 7].
        // Cursor 8.
        // 8 <= 7 is False.
        // Expected: HIDDEN (Snap).
        // 8 <= 7 is False.
        // Expected: HIDDEN (Snap).

        // Async Wait for parser
        await Future.delayed(Duration.zero);

        expect(
          controller.decoration.hiddenRanges,
          isNotEmpty,
          reason: 'Should SNAP HIDE at end (Model C)',
        );

        // 2. Hit Space (User says "you have to hit space first")
        controller.value = TextEditingValue(
          text: '*italic* ',
          selection: TextSelection.collapsed(offset: 9), // After space
        );

        // Now cursor is at 9.
        // Hysteresis Zone for [7, 8] is [6, 9]. (Wait, range.end+1).
        // If range ends at 8 (exclusive, char 7).
        // zoneEnd = 8 + 1 = 9.
        // Cursor is AT 9.
        // Intersects: selection.start (9) <= zoneEnd (9). Yes.
        // So it is STILL Intersecting?
        // If I type ONE space, I am at index 9. Dist from 7 is 2.
        // 7, 8 (space), 9 (cursor).
        // If zone is [start-1, end+1].
        // Range is [7, 8].
        // Start-1 = 6. End+1 = 9.
        // Zone [6, 9].
        // Cursor 9 is Inside/Touching.

        // So yes, one space keeps it visible.
        // Is this "Too Sticky"?
        // If I type "*italic* is..."
        // "*italic* i". Cursor at 10.
        // Zone [6, 9]. Cursor 10. Outside.
        // Should hide.

        expect(
          controller.decoration.hiddenRanges,
          isNotEmpty,
          reason: 'Should HIDE after moving away',
        );
      },
    );

    test('Empty Code Ticks do not hide (Triple Tick Flow)', () async {
      // Case: `` (Double tick)
      // Should NOT hide, because it has no content.
      // This allows typing ``` without the first two snapping hidden.
      controller.text = '``';
      controller.selection = TextSelection.collapsed(offset: 2);

      // Async Wait for parser
      await Future.delayed(Duration.zero);

      expect(
        controller.decoration.hiddenRanges,
        isEmpty,
        reason: 'Empty inline code should not hide markers',
      );

      // Case: `a` (Content)
      // Should hide.
      controller.text = '`a`';
      controller.selection = TextSelection.collapsed(offset: 3);

      // [Phase 9] Anti-Flicker Architecture:
      // Dirty State (Sync) uses "Shifted" ranges. Previous ranges were empty.
      // So Sync result is empty (Visible).
      // We must wait for Authoritative Parse (Async/Microtask) to see hiding.
      await Future.delayed(const Duration(milliseconds: 50));

      expect(controller.decoration.hiddenRanges, isNotEmpty);
    });

    test(
      'SovereignController Anti-Flicker: Shifting Downstream Ranges (Sync)',
      () async {
        // Setup: Fenced code block
        // ``` (0-3)
        // code
        // ``` (8-11)
        final text = '```\ncode\n```';
        controller.text = text;

        // Wait for authoritative parse (Establish baseline)
        await Future.delayed(Duration.zero);

        // Baseline Verification
        // Ranges: [0, 3], [8, 11]
        expect(controller.decoration.hiddenRanges.length, 2);
        expect(
          controller.decoration.hiddenRanges[0],
          const TextRange(start: 0, end: 3),
        );

        // Action: Insert 'a' at 0.
        // Op: Insert 'a' at 0. Delta +1.
        // Expected: Ranges shifted to [1, 4], [9, 12].
        // CRITICAL: This must be SYNC (Immediate), no await.

        controller.value = TextEditingValue(
          text: 'a$text',
          selection: const TextSelection.collapsed(offset: 0),
        );

        // Current: Dirty State.
        // _emitDecoration called logic "Shift + Verify".

        final currentRanges = controller.decoration.hiddenRanges;
        expect(
          currentRanges.length,
          1,
          reason:
              "Broken opening fence should not stay hidden; only real fences remain hidden during dirty state",
        );
        expect(
          currentRanges[0],
          const TextRange(start: 10, end: 13),
          reason: "closing fence becomes a new opening fence at column 0",
        );

        // Wait for Async Parse (Result should be same/authoritative)
        await Future.delayed(const Duration(milliseconds: 50));

        final authoritativeRanges = controller.decoration.hiddenRanges;
        // Parser runs and detects the fence is broken ('a```' is not a fence).
        // So it removes it. This is correct behavior (Eventual Consistency).
        expect(authoritativeRanges.length, 1);
      },
    );

    test(
      'SovereignController Bug: Typing inside Fenced Code Block (Flicker/Disappear)',
      () async {
        final text = '```\n```';
        controller.text = text;

        await Future.delayed(Duration.zero);
        expect(controller.decoration.hiddenRanges.length, 2);

        // Step 1: Type 'a' inside
        controller.value = TextEditingValue(
          text: '```\na```',
          selection: const TextSelection.collapsed(offset: 5),
        );

        var hidden = controller.decoration.hiddenRanges;
        // print('Step 1 Hidden: $hidden');
        // Note: Because cursor is at 5, and bottom fence is [5, 8],
        // The cursor TOUCHES the fence.
        // Pop Scope (Adjacency) reveals it.
        // So we expect 1 range (Top Fence only).
        expect(
          hidden.length,
          1,
          reason: "Step 1: Top fence preserved, Bottom popped",
        );
        expect(hidden[0], const TextRange(start: 0, end: 3));

        // Step 2: Type 'b' inside
        controller.value = TextEditingValue(
          text: '```\nab```',
          selection: const TextSelection.collapsed(offset: 6),
        );

        hidden = controller.decoration.hiddenRanges;
        // Note: Cursor at 6. Bottom fence shifted to [6, 9].
        // Cursor touches fence start. Popped.
        expect(
          hidden.length,
          1,
          reason: "Step 2: Top fence preserved, Bottom popped",
        );
        expect(hidden[0], const TextRange(start: 0, end: 3));

        // Step 3: Type 'c' inside
        controller.value = TextEditingValue(
          text: '```\nabc```',
          selection: const TextSelection.collapsed(offset: 7),
        );

        hidden = controller.decoration.hiddenRanges;
        // Note: Cursor at 7. Bottom fence shifted to [7, 10].
        // Cursor touches fence start. Popped.
        expect(
          hidden.length,
          1,
          reason: "Step 3: Top fence preserved, Bottom popped",
        );
        expect(hidden[0], const TextRange(start: 0, end: 3));

        // Final Async Check
        await Future.delayed(Duration.zero);
        // Cursor is still at 7. Range [8, 11]. Adjacent.
        // So it remains Popped (Visible).
        expect(controller.decoration.hiddenRanges.length, 1);
      },
    );
    test(
      'SovereignController Bug: Triple Tick Linear Typing (Disappearance)',
      () async {
        // Setup: Two ticks
        final text = '``';
        controller.text = text;

        // Step 1: Type 3rd tick
        controller.value = TextEditingValue(
          text: '```',
          selection: const TextSelection.collapsed(offset: 3),
        );

        // Wait for parser to recognize fence
        await Future.delayed(Duration.zero);
        // Expect finding [0, 3]
        expect(controller.decoration.hiddenRanges.length, 1);

        // Step 2: Type 'a' (info string or content?)
        controller.value = TextEditingValue(
          text: '```a',
          selection: const TextSelection.collapsed(offset: 4),
        );

        // Sync check (Shift)
        // Old: [0, 3]. Op at 3.
        // Upstream. Stays [0, 3].
        // In V1 we treat text after ``` on the opening line as a fence info
        // string (language tag). It is hidden by default, but we "pop" it while
        // the caret is at the end boundary so typing doesn't feel broken.
        expect(controller.decoration.hiddenRanges.length, 1);
        expect(
          controller.decoration.hiddenRanges[0],
          const TextRange(start: 0, end: 3),
        );

        await Future.delayed(Duration.zero);
        // Parser authoritative check (should preserve the same behavior).
        expect(controller.decoration.hiddenRanges.length, 1);
        expect(
          controller.decoration.hiddenRanges[0],
          const TextRange(start: 0, end: 3),
        );

        // Step 3: Type 'b'
        controller.value = TextEditingValue(
          text: '```ab',
          selection: const TextSelection.collapsed(offset: 5),
        );

        // Sync check
        expect(controller.decoration.hiddenRanges.length, 1);
        expect(
          controller.decoration.hiddenRanges[0],
          const TextRange(start: 0, end: 3),
        );

        // Step 4: Type 'c'
        controller.value = TextEditingValue(
          text: '```abc',
          selection: const TextSelection.collapsed(offset: 6),
        );

        // Sync check
        // User reports disappearance here.
        expect(controller.decoration.hiddenRanges.length, 1);
        expect(
          controller.decoration.hiddenRanges[0],
          const TextRange(start: 0, end: 3),
        );

        // Wait for parser (CRITICAL for reproduction)
        await Future.delayed(const Duration(milliseconds: 50));

        expect(
          controller.decoration.hiddenRanges.length,
          1,
          reason:
              "Fence info string should be popped while caret is at end boundary",
        );
      },
    );
    test('SovereignController Bug: Fenced Code Extension (Enter Key)',
        () async {
      // Setup: Valid fenced block
      // ```
      // abc
      // ```
      final text = '```\nabc\n```';
      controller.text = text;
      await Future.delayed(const Duration(milliseconds: 300));

      // Verify initial state
      // Blocks: 1 fenced block covering entire text?
      // Length: 3 + 1 + 3 + 1 + 3 = 11?
      // '`' '`' '`' '\n' 'a' 'b' 'c' '\n' '`' '`' '`'
      // Indices: 0-11
      // Check first block
      // We can't easily check blocks via controller.decoration
      // But we can check hiddenRanges? The closing fence [8, 11] should be hidden.
      expect(controller.decoration.hiddenRanges.length, 2); // Top and Bottom

      // Step: Hit Enter after 'abc' -> '```\nabc\n\n```'
      // Cursor was at 7 (after c), INSERT \n.
      controller.value = TextEditingValue(
        text: '```\nabc\n\n```',
        selection: const TextSelection.collapsed(offset: 8),
      );

      // Wait for parser
      await Future.delayed(const Duration(milliseconds: 300));

      // The bug report says "background doesn't extend".
      // This implies the BlockNode ended early.
      // If the parser is robust, it should see:
      // ``` (start)
      // abc
      //
      // ``` (end)
      // So one block covering 0 to 12.

      // We can inspect the internal state if we expose it, or infer via hidden ranges.
      // If the block broke, maybe we lost the bottom fence?
      // Or maybe the bottom fence [9, 12] is NOT hidden?

      // Let's assert we still have 2 hidden ranges (Top and Bottom).
      expect(
        controller.decoration.hiddenRanges.length,
        2,
        reason: "Should maintain block structure with valid bottom fence",
      );

      // Also, check the bottom fence range.
      // It was [8, 11]. Shifted to [9, 12].
      final bottomFence = controller.decoration.hiddenRanges.last;
      expect(bottomFence, const TextRange(start: 9, end: 12));

      // Phase 6: Verify Visual Projection (Synchronous Background Extension)
      // The block should encompass the new text length (12).
      // Stale Tree: [0, 11]. Shifted: [0, 12].
      //
      // [RFC 007 UPDATE]: projectedBlocks is removed.
      // Geometry is now authoritative via GeometryModel.
      // We can assert on controller.geometry if exposed for testing,
      // but internal visual projection is no longer a public API to test here.
      // The 'geometry_canary_test.dart' covers the sync geometry update.
    });
    testWidgets('SovereignController Bug: Range Error on Delete in Code Block',
        (
      tester,
    ) async {
      // Setup: Large code block
      final text = '```\n1234567890\n```';
      controller.text = text;
      await tester.pump();

      // Verify initial state
      // Ranges based on text length ~15.

      // Step: Select all and delete "1234567890" -> "```\n\n```"
      // or just delete a chunk.
      // Let's delete "12345".
      controller.value = TextEditingValue(
        text: '```\n67890\n```',
        selection: const TextSelection.collapsed(offset: 4),
      );

      // This triggers buildTextSpan synchronously with STALE decoration (old length 15)
      // applied to NEW text (length 10).
      // If code injection doesn't check bounds, it will try to style validTo=15.
      // Crash expectation.

      // Just accessing controller.buildTextSpan is enough?
      // WidgetTester pumps frames which calls buildTextSpan.
      // We can call it manually.
      // Use a proper widget pump to get context and trigger build
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) {
              controller.buildTextSpan(context: context, withComposing: false);
              return const SizedBox();
            },
          ),
        ),
      );
    });
  });
}
