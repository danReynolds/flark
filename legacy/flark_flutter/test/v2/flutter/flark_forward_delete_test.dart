// Widget-level forward Delete (the Delete key) tests.
//
// Every scenario sends a real hardware Delete key event through the editing
// surface [FlarkMarkdownEditor] mounts in live mode
// ([FlarkLiveRenderedEditableText]): plain paragraphs with inline styling
// mount the whole-document projected host, quote documents mount per-block
// editables. The tree is wrapped in [DefaultTextEditingShortcuts] — the
// widget `WidgetsApp` installs in real apps — so the Delete key arrives as
// `DeleteCharacterIntent(forward: true)`, exactly the production route. The
// policy's boundary-aware resolver
// (`FlarkProjection.resolveForwardDeleteSelection`, the mirror of the
// Backspace resolver) intercepts marker-adjacent deletions; everything else
// falls through to Flutter's display-space default.
//
// After every step the tests assert the package invariant from
// docs/architecture/v2/inline_delimiter_validity_2026-07-10.md:
//   (a) the source markdown equals the pinned expectation, and
//   (b) export round-trip — a fresh, caret-free controller parsing
//       `controller.markdown` projects the identical display, proving the
//       source never depends on editor-local state.
// See [_expectCommitted].

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark_flutter/src/v2/flutter/flutter.dart';

void main() {
  group('forward Delete at inline-run boundaries (host surface)', () {
    testWidgets('just before a run deletes its first content character', (
      tester,
    ) async {
      // The caret sits before the hidden opening `**`; a naive forward
      // delete would cut a marker char (`**bold**` → `*bold**`). The
      // resolver re-enters the run forward so 'b' goes instead.
      final controller = await _pumpLiveEditor(tester, '**bold**');
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(
        tester,
        controller,
        source: '**old**',
        display: 'old',
      );
      // The caret lands at the run's interior start, so typing continues
      // the style.
      expect(controller.selection, const FlarkSelection.collapsed(2));
    });

    testWidgets('deletes a whole emoji at a run leading edge, not half a '
        'surrogate pair', (tester) async {
      // The run's first content character is a surrogate pair. Re-entering the
      // run forward and deleting a single UTF-16 code unit would strand a
      // broken half-surrogate (`**\uD800bold**`); the deletion must cover the
      // whole grapheme. Regression test for the boundary-resolver building a
      // raw (anchor, anchor+1) range.
      final controller = await _pumpLiveEditor(tester, '**\u{1F600}bold**');
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(
        tester,
        controller,
        source: '**bold**',
        display: 'bold',
      );
    });

    testWidgets('just before a mid-text run deletes into the run', (
      tester,
    ) async {
      final controller = await _pumpLiveEditor(tester, 'x**bold**');
      controller.applySelection(
        const FlarkSelection.collapsed(1),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(
        tester,
        controller,
        source: 'x**old**',
        display: 'xold',
      );
    });

    testWidgets(
      'at the inside-end steps past the close and deletes the character '
      'after the run',
      (tester) async {
        // `**bold|** x`: the caret sits immediately before the hidden
        // closing `**`. The delete must target the space after the run —
        // never a marker character.
        final controller = await _pumpLiveEditor(tester, '**bold** x');
        controller.applySelection(
          const FlarkSelection.collapsed(6),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);

        await _expectCommitted(
          tester,
          controller,
          source: '**bold**x',
          display: 'boldx',
        );
      },
    );

    testWidgets(
      'inside-end delete stays source-anchored when the deleted character '
      'is ambiguous in display space',
      (tester) async {
        // `**aa**|a` in display is "aa|a": deleting forward and deleting
        // backward produce the same display text, so a display-space diff
        // re-anchors as a backspace and removes the run's second 'a'
        // (`**a**a`). Resolving in source space removes the trailing 'a'.
        final controller = await _pumpLiveEditor(tester, '**aa**a');
        controller.applySelection(
          const FlarkSelection.collapsed(4),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);

        await _expectCommitted(
          tester,
          controller,
          source: '**aa**',
          display: 'aa',
        );
      },
    );

    testWidgets('at the inside-end of the last run no-ops but exits the run', (
      tester,
    ) async {
      // Only hidden markers separate the caret from the document end;
      // there is nothing to delete. The caret steps past the markers so
      // its true position is visible to the (no-op) default.
      final controller = await _pumpLiveEditor(tester, '**bold**');
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(
        tester,
        controller,
        source: '**bold**',
        display: 'bold',
      );
      expect(controller.selection, const FlarkSelection.collapsed(8));
    });

    testWidgets('deleting a run\'s last content character removes the '
        'orphaned markers', (tester) async {
      // `**x**` forward-delete on 'x' → empty document, from both source
      // caret positions that render at the display start.
      for (final caret in const [0, 2]) {
        final controller = await _pumpLiveEditor(tester, '**x**');
        controller.applySelection(
          FlarkSelection.collapsed(caret),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);

        await _expectCommitted(
          tester,
          controller,
          source: '',
          display: '',
          reason: 'forward delete in "**x**" at caret $caret',
        );
      }
    });

    testWidgets('nested stacked markers resolve to the innermost content', (
      tester,
    ) async {
      // `*~~f~~*`: the opening chain `*` + `~~` is walked as a unit, so the
      // deletion targets 'f' and the orphan expansion removes every marker
      // — from the document start and from the innermost content position.
      for (final caret in const [0, 3]) {
        final controller = await _pumpLiveEditor(tester, '*~~f~~*');
        controller.applySelection(
          FlarkSelection.collapsed(caret),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);

        await _expectCommitted(
          tester,
          controller,
          source: '',
          display: '',
          reason: 'forward delete in "*~~f~~*" at caret $caret',
        );
      }
    });

    testWidgets('nested inner inside-end steps past the whole closing chain', (
      tester,
    ) async {
      // `*~~f|~~*`: the closing chain `~~` + `*` reaches the document end,
      // so nothing is deleted and the caret exits past both markers —
      // never landing between the two closers.
      final controller = await _pumpLiveEditor(tester, '*~~f~~*');
      controller.applySelection(
        const FlarkSelection.collapsed(4),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(
        tester,
        controller,
        source: '*~~f~~*',
        display: 'f',
      );
      expect(controller.selection, const FlarkSelection.collapsed(7));
    });

    testWidgets('code spans participate exactly like emphasis', (tester) async {
      // Before the opening backtick: re-enter and delete the first content
      // character.
      final before = await _pumpLiveEditor(tester, '`ab`');
      before.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, before, source: '`b`', display: 'b');

      // Last content character: the orphaned backticks go too.
      final last = await _pumpLiveEditor(tester, '`x`');
      last.applySelection(const FlarkSelection.collapsed(1), userEvent: 'test');
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, last, source: '', display: '');

      // Inside-end at the document end: no-op, caret exits the span.
      final atEnd = await _pumpLiveEditor(tester, '`ab`');
      atEnd.applySelection(
        const FlarkSelection.collapsed(3),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, atEnd, source: '`ab`', display: 'ab');
      expect(atEnd.selection, const FlarkSelection.collapsed(4));
    });

    testWidgets('mid-content code span deletion falls through to the default '
        '(already correct without the resolver)', (tester) async {
      // `` `a|b` ``: no marker is adjacent, so the resolver defers and
      // Flutter's display-space default deletes 'b' — pinned as a control
      // that the resolver never interferes mid-content.
      final controller = await _pumpLiveEditor(tester, '`ab`');
      controller.applySelection(
        const FlarkSelection.collapsed(2),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(tester, controller, source: '`a`', display: 'a');
    });

    testWidgets(
      'at a line end merges with a next line that starts with a styled run',
      (tester) async {
        // Step 1 (default path, already correct): the character after the
        // caret is the newline — no marker adjacent — so the display-space
        // default deletes it and the lines merge.
        final controller = await _pumpLiveEditor(tester, 'a\n**b**');
        controller.applySelection(
          const FlarkSelection.collapsed(1),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);
        await _expectCommitted(
          tester,
          controller,
          source: 'a**b**',
          display: 'ab',
        );

        // Step 2 (resolver path): the next character now belongs to the
        // run's hidden opening marker; deleting 'b' orphans the pair, so
        // the markers go too.
        await _sendForwardDelete(tester);
        await _expectCommitted(tester, controller, source: 'a', display: 'a');
      },
    );

    testWidgets('plain text is untouched by the resolver', (tester) async {
      // Controls proving the wiring never swallows the key when no marker
      // is adjacent (already correct without the resolver).
      final controller = await _pumpLiveEditor(tester, 'abc');
      controller.applySelection(
        const FlarkSelection.collapsed(1),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, controller, source: 'ac', display: 'ac');

      // At the document end forward delete is a no-op.
      controller.applySelection(
        const FlarkSelection.collapsed(2),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, controller, source: 'ac', display: 'ac');
    });

    testWidgets('a selection covering a run\'s whole content expands over '
        'the orphaned markers', (tester) async {
      // Identical to Backspace: select-all-of-the-content + Delete removes
      // the now-meaningless markers too.
      final controller = await _pumpLiveEditor(tester, '**bold**');
      controller.applySelection(
        const FlarkSelection(baseOffset: 2, extentOffset: 6),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);

      await _expectCommitted(tester, controller, source: '', display: '');
    });

    testWidgets('deleting the gap between two same-style runs merges them '
        '(both keys canonicalize the join)', (tester) async {
      // `**a|** **b**` + Delete and `**a** **|b**` + Backspace both remove the
      // gap space through the source-side keyboard path. That deletion routes
      // through the placement repairs (joiningDeletionRepair), so the two runs
      // merge into one — never the fused literal `**a****b**` that a plain
      // range delete would leave. The two keys stay symmetric, both valid.
      final forward = await _pumpLiveEditor(tester, '**a** **b**');
      forward.applySelection(
        const FlarkSelection.collapsed(3),
        userEvent: 'test',
      );
      await _sendForwardDelete(tester);
      await _expectCommitted(tester, forward, source: '**ab**', display: 'ab');

      final backward = await _pumpLiveEditor(tester, '**a** **b**');
      backward.applySelection(
        const FlarkSelection.collapsed(8),
        userEvent: 'test',
      );
      await tester.showKeyboard(find.byType(EditableText).first);
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      await _expectCommitted(tester, backward, source: '**ab**', display: 'ab');
    });

    testWidgets('backspacing a run\'s last word-char relocates the close '
        '(keyboard path stays valid)', (tester) async {
      // `**foo x**` backspacing `x` would plain-delete to the invalid
      // `**foo **`; the keyboard path canonicalizes it to `**foo** `.
      final controller = await _pumpLiveEditor(tester, '**foo x**');
      controller.applySelection(
        const FlarkSelection.collapsed(8),
        userEvent: 'test',
      );
      await tester.showKeyboard(find.byType(EditableText).first);
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      await _expectCommitted(
        tester,
        controller,
        source: '**foo** ',
        display: 'foo ',
      );
    });

    testWidgets(
      'forward-deleting a run\'s last word-char relocates the close',
      (tester) async {
        // `**foo x** y`, caret inside before `x` (source 6): forward-delete
        // removes `x`, which would strand the close against the run's internal
        // space (`**foo **`); the repair relocates it to `**foo** `, leaving
        // `**foo**  y` (the relocated space plus the pre-existing one).
        final controller = await _pumpLiveEditor(tester, '**foo x** y');
        controller.applySelection(
          const FlarkSelection.collapsed(6),
          userEvent: 'test',
        );
        await _sendForwardDelete(tester);
        await _expectCommitted(
          tester,
          controller,
          source: '**foo**  y',
          display: 'foo  y',
        );
      },
    );
  });

  group('forward Delete at inline-run boundaries (block surface)', () {
    testWidgets('inside-end delete in a blockquote block routes through the '
        'resolver', (tester) async {
      // Quote documents mount per-block editables; their Delete arrives at
      // the block's DeleteCharacterIntent override rather than the policy's.
      final controller = await _pumpLiveEditor(tester, '> **bold** x');
      expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
      final editable = find.descendant(
        of: find.byKey(const Key('FlarkLiveBlockBlockquote')),
        matching: find.byType(EditableText),
      );
      // Source `> **bold** x`: offset 8 is the run's inside-end.
      controller.applySelection(
        const FlarkSelection.collapsed(8),
        userEvent: 'test',
      );
      await tester.pump();
      await _sendForwardDelete(tester, editable: editable);

      await _expectCommitted(
        tester,
        controller,
        source: '> **bold**x',
        display: 'boldx',
      );
    });

    testWidgets('last-content-character delete in a blockquote removes the '
        'orphaned markers', (tester) async {
      final controller = await _pumpLiveEditor(tester, '> **x** y');
      final editable = find.descendant(
        of: find.byKey(const Key('FlarkLiveBlockBlockquote')),
        matching: find.byType(EditableText),
      );
      // Source `> **x** y`: offset 4 is the run's single content char.
      controller.applySelection(
        const FlarkSelection.collapsed(4),
        userEvent: 'test',
      );
      await tester.pump();
      await _sendForwardDelete(tester, editable: editable);

      await _expectCommitted(tester, controller, source: '>  y', display: ' y');
    });
  });
}

/// Pumps [FlarkLiveRenderedEditableText] — the surface [FlarkMarkdownEditor]
/// mounts in live mode — around a fresh controller for [markdown], with an
/// authoritative comrak parse adopted before the first frame.
///
/// The tree is wrapped in [DefaultTextEditingShortcuts] (installed by
/// `WidgetsApp` in real apps) so a hardware Delete key event reaches the
/// editor as `DeleteCharacterIntent(forward: true)` — the production route
/// this suite exercises.
Future<FlarkFlutterController> _pumpLiveEditor(
  WidgetTester tester,
  String markdown,
) async {
  final controller = FlarkFlutterController.fromMarkdown(markdown);
  addTearDown(controller.dispose);
  await controller.parseNow();
  await tester.pumpWidget(
    DefaultTextEditingShortcuts(
      child: Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    ),
  );
  await tester.pump();
  return controller;
}

/// Focuses [editable] (the host's single editable by default) and sends one
/// real hardware Delete key event.
Future<void> _sendForwardDelete(WidgetTester tester, {Finder? editable}) async {
  await tester.showKeyboard(editable ?? find.byType(EditableText).first);
  await tester.pump();
  await tester.sendKeyEvent(LogicalKeyboardKey.delete);
  await tester.pump();
}

/// Asserts the committed document after settling the authoritative parse:
///
/// 1. `controller.markdown` equals the pinned [source].
/// 2. **Display fidelity** — the projected display equals [display].
/// 3. **Export round-trip** — a fresh, caret-free controller parsing the
///    exported `controller.markdown` projects the identical display, so the
///    source never depends on editor-local state (armed styles, caret
///    affinity) — the invariant from
///    docs/architecture/v2/inline_delimiter_validity_2026-07-10.md.
Future<void> _expectCommitted(
  WidgetTester tester,
  FlarkFlutterController controller, {
  required String source,
  required String display,
  String? reason,
}) async {
  await controller.parseNow();
  await tester.pump();
  expect(controller.markdown, source, reason: reason ?? 'pinned source');
  expect(
    controller.projection.projectText(controller.markdown),
    display,
    reason:
        'display fidelity: source "${controller.markdown}" must project '
        'the expected display',
  );
  final export = controller.markdown;
  final fresh = FlarkFlutterController.fromMarkdown(export);
  try {
    expect(
      fresh.tryParseSync(),
      isTrue,
      reason: 'export round-trip needs the sync-capable comrak backend',
    );
    expect(
      fresh.projection.projectText(export),
      display,
      reason:
          'export round-trip: "$export" renders differently with no caret '
          'context — the source depends on editor-local state',
    );
  } finally {
    fresh.dispose();
  }
}
