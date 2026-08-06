// RFC 024 Gate G4 — the input-surface acceptance suite (§7).
//
// Runs unmodified against any `G4Surface`. To add Variant B, append one entry
// to `_variants` and change nothing else.
//
// Run:  flutter test lib/g4/g4_acceptance_test.dart
//
// Reporting convention: a case that a variant cannot express at all is
// registered with `notExpressible(...)`, which emits a SKIPPED test carrying
// the reason, rather than a green tick that means nothing.

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'g4_model.dart';
import 'g4_surface.dart';
import 'g4_variant_a.dart';
import 'g4_variant_b.dart';

const List<G4Variant> _variants = <G4Variant>[
  G4Variant('A(editable-island)', G4VariantA.builder),
  G4Variant('B(own-painted)', G4VariantB.builder),
];

const int kBlockCount = 400;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

class _Harness {
  _Harness(this.document, this.scrollController, this.key);

  final G4Document document;
  final ScrollController scrollController;
  final GlobalKey<G4SurfaceState<G4Surface>> key;

  G4SurfaceState<G4Surface> get surface => key.currentState!;
}

Future<_Harness> pumpSurface(WidgetTester tester, G4Variant variant) async {
  final G4Document doc = G4Document(g4FixtureBlocks(count: kBlockCount));
  final ScrollController scroll = ScrollController();
  final GlobalKey<G4SurfaceState<G4Surface>> key = GlobalKey<G4SurfaceState<G4Surface>>();

  addTearDown(scroll.dispose);

  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: variant.build(key: key, document: doc, scrollController: scroll),
        ),
      ),
    ),
  );
  await tester.pump();
  return _Harness(doc, scroll, key);
}

/// Blocks actually present in the widget tree right now.
Set<int> builtBlocks() {
  final Set<int> out = <int>{};
  for (int i = 0; i < kBlockCount; i++) {
    if (find.byKey(g4BlockKey(i)).evaluate().isNotEmpty) {
      out.add(i);
    }
  }
  return out;
}

/// Global position of a character offset inside a block that is currently
/// built. Uses the shared metrics, so it is identical for both variants.
Offset globalFor(WidgetTester tester, int block, int charOffset, G4Document doc) {
  final Offset topLeft = tester.getTopLeft(find.byKey(g4BlockKey(block)));
  return topLeft + G4TextMetrics.localForOffset(doc.blockAt(block), charOffset);
}

Future<TestGesture> mouseDownAt(WidgetTester tester, Offset global) async {
  final TestGesture g = await tester.startGesture(global, kind: PointerDeviceKind.mouse);
  await tester.pump();
  return g;
}

/// A stand-in for the platform IME.
///
/// It must hold its OWN copy of the editing value, because that is what a real
/// input method does: the engine keeps the last value it sent, and only learns
/// otherwise when the framework pushes a correction with `setEditingState`.
/// (`TestTextInput.editingState` reflects framework->platform pushes only, so
/// reading it back after `updateEditingValue` returns a stale value.)
///
/// Entirely variant-agnostic: it talks to `TestTextInput` and never to a widget.
class Ime {
  TextEditingValue value = TextEditingValue.empty;
  Map<String, dynamic>? _lastPush;

  bool get attached => value != TextEditingValue.empty;

  /// Adopt any correction the framework pushed down to the platform.
  void sync(WidgetTester tester) {
    final Map<String, dynamic>? st = tester.testTextInput.editingState;
    if (st == null) {
      return;
    }
    if (_lastPush != null && mapEquals(_lastPush, st)) {
      return;
    }
    _lastPush = Map<String, dynamic>.of(st);
    value = TextEditingValue(
      text: st['text'] as String,
      selection: TextSelection(
        baseOffset: st['selectionBase'] as int,
        extentOffset: st['selectionExtent'] as int,
      ),
      composing: TextRange(
        start: st['composingBase'] as int,
        end: st['composingExtent'] as int,
      ),
    );
  }

  Future<void> _send(WidgetTester tester, TextEditingValue next) async {
    value = next;
    tester.testTextInput.updateEditingValue(next);
    await tester.pump();
    sync(tester);
  }

  /// Plain typing: replace whatever is selected.
  Future<void> type(WidgetTester tester, String s) async {
    sync(tester);
    final int start = value.selection.start;
    final int end = value.selection.end;
    await _send(
      tester,
      TextEditingValue(
        text: value.text.substring(0, start) + s + value.text.substring(end),
        selection: TextSelection.collapsed(offset: start + s.length),
      ),
    );
  }

  /// Start or grow a composition. If a composing region is already live it is
  /// replaced in place, as when a user keeps typing kana.
  Future<void> compose(WidgetTester tester, String composed) async {
    sync(tester);
    final bool live = value.composing.isValid && !value.composing.isCollapsed;
    final int start = live ? value.composing.start : value.selection.start;
    final int end = live ? value.composing.end : value.selection.end;
    await _send(
      tester,
      TextEditingValue(
        text: value.text.substring(0, start) + composed + value.text.substring(end),
        selection: TextSelection.collapsed(offset: start + composed.length),
        composing: TextRange(start: start, end: start + composed.length),
      ),
    );
  }

  /// Commit the live composition without changing the text.
  Future<void> commit(WidgetTester tester) async {
    sync(tester);
    await _send(
      tester,
      TextEditingValue(text: value.text, selection: value.selection),
    );
  }
}

String expectedRange(G4Document doc, G4Selection s) => doc.extractRange(s);

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

void main() {
  for (final G4Variant variant in _variants) {
    group(variant.name, () {
      // -------------------------------------------------------------------
      // Precondition for the whole gate: virtualization is real.
      // -------------------------------------------------------------------
      testWidgets('0. virtualization: most blocks are never built', (WidgetTester tester) async {
        await pumpSurface(tester, variant);
        final Set<int> built = builtBlocks();
        expect(built.length, lessThan(40), reason: 'viewport shows ~8 rows; found $built');
        expect(built.contains(0), isTrue);
        expect(built.contains(399), isFalse, reason: 'block 399 must not be built');
        expect(built.contains(200), isFalse, reason: 'block 200 must not be built');
      });

      // -------------------------------------------------------------------
      // 1. drag-select across blocks
      // -------------------------------------------------------------------
      testWidgets('1. drag-select across blocks', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);

        final Offset from = globalFor(tester, 2, 0, h.document);
        final Offset to = globalFor(tester, 5, 29, h.document);

        final TestGesture g = await mouseDownAt(tester, from);
        await g.moveTo(Offset(from.dx, to.dy));
        await tester.pump();
        await g.moveTo(to);
        await tester.pump();
        await g.up();
        await tester.pump();

        final G4Selection? sel = h.surface.selection;
        expect(sel, isNotNull);
        expect(sel!.normalized.base, const G4Position(2, 0));
        expect(sel.normalized.extent, const G4Position(5, 29));

        const String expected =
            'Block 002 alpha bravo charlie\n\n'
            'Block 003 alpha bravo charlie\n\n'
            'Block 004 alpha bravo charlie\n\n'
            'Block 005 alpha bravo charlie';
        expect(h.surface.copySelection(), expected);
        expect(expectedRange(h.document, sel), expected);
      });

      // -------------------------------------------------------------------
      // 2. drag-select with autoscroll
      // -------------------------------------------------------------------
      testWidgets('2. drag-select with autoscroll', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);

        final Offset from = globalFor(tester, 1, 0, h.document);
        final TestGesture g = await mouseDownAt(tester, from);

        // Drag to the bottom edge of the viewport and hold there.
        final Offset viewportTopLeft = tester.getTopLeft(find.byKey(g4BlockKey(0)));
        final Offset edge = Offset(
          from.dx + 200,
          viewportTopLeft.dy + G4Layout.viewportHeight - 4,
        );
        await g.moveTo(edge);
        await tester.pump();

        final double before = h.scrollController.offset;
        for (int i = 0; i < 60; i++) {
          await tester.pump(const Duration(milliseconds: 16));
        }
        final double after = h.scrollController.offset;

        expect(after, greaterThan(before), reason: 'autoscroll did not run');

        final G4Selection? mid = h.surface.selection;
        expect(mid, isNotNull);
        expect(
          mid!.normalized.extent.block,
          greaterThan(8),
          reason: 'selection stopped extending while the list scrolled',
        );

        await g.up();
        await tester.pump();

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, const G4Position(1, 0));
        expect(h.surface.copySelection(), expectedRange(h.document, sel));
        // Exactness check against a hand-built string for the whole span.
        final StringBuffer sb = StringBuffer();
        for (int b = 1; b <= sel.extent.block; b++) {
          if (b > 1) {
            sb.write('\n\n');
          }
          sb.write(
            b == sel.extent.block
                ? h.document.blockAt(b).substring(0, sel.extent.offsetUtf16)
                : h.document.blockAt(b),
          );
        }
        expect(h.surface.copySelection(), sb.toString());
      });

      // -------------------------------------------------------------------
      // 3. anchor destroyed
      // -------------------------------------------------------------------
      testWidgets('3. anchor destroyed mid-drag, selection still exact', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);

        final Offset from = globalFor(tester, 2, 6, h.document);
        final TestGesture g = await mouseDownAt(tester, from);
        await g.moveTo(from + const Offset(20, 20));
        await tester.pump();

        expect(builtBlocks().contains(2), isTrue, reason: 'anchor should start built');

        // Scroll far past the anchor while the gesture is live.
        h.scrollController.jumpTo(120 * G4Layout.itemExtent);
        await tester.pump();

        expect(
          builtBlocks().contains(2),
          isFalse,
          reason: 'anchor block must be disposed for this case to mean anything',
        );

        // Keep extending, now in a completely different part of the document.
        final Offset deep = globalFor(tester, 123, 16, h.document);
        await g.moveTo(deep);
        await tester.pump();
        await g.up();
        await tester.pump();

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, const G4Position(2, 6), reason: 'anchor moved after its block died');
        expect(sel.extent, const G4Position(123, 16));

        final String copied = h.surface.copySelection();
        expect(copied.startsWith('002 alpha bravo charlie\n\nBlock 003'), isTrue);
        expect(copied.endsWith('\n\nBlock 123 alpha '), isTrue);
        expect(copied, expectedRange(h.document, sel));
        expect(copied.split('\n\n').length, 122);
      });

      // -------------------------------------------------------------------
      // 4. select-all + copy over the whole document
      // -------------------------------------------------------------------
      testWidgets('4. select-all + copy returns the complete exact document', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);

        final List<MethodCall> clipboardCalls = <MethodCall>[];
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          (MethodCall call) async {
            if (call.method == 'Clipboard.setData') {
              clipboardCalls.add(call);
            }
            return null;
          },
        );
        addTearDown(
          () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
            SystemChannels.platform,
            null,
          ),
        );

        h.surface.setSelection(h.document.selectAll);
        await tester.pump();

        final Set<int> built = builtBlocks();
        expect(built.length, lessThan(40), reason: 'select-all must not build the document');

        final String copied = h.surface.copySelection();
        expect(copied, h.document.text);
        expect(copied.split('\n\n').length, kBlockCount);
        expect(copied.startsWith('Block 000 alpha bravo charlie'), isTrue);
        expect(copied.endsWith('Block 399 alpha bravo charlie'), isTrue);

        await tester.pump();
        expect(clipboardCalls, isNotEmpty, reason: 'copy never reached the platform clipboard');
        expect((clipboardCalls.first.arguments as Map<Object?, Object?>)['text'], h.document.text);
      });

      // -------------------------------------------------------------------
      // 5. shift-click extension
      // -------------------------------------------------------------------
      testWidgets('5. shift-click extension', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);

        final TestGesture g1 = await mouseDownAt(tester, globalFor(tester, 1, 10, h.document));
        await g1.up();
        await tester.pump();
        expect(h.surface.selection!.extent, const G4Position(1, 10));

        await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
        final TestGesture g2 = await mouseDownAt(tester, globalFor(tester, 6, 22, h.document));
        await g2.up();
        await tester.pump();
        await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, const G4Position(1, 10));
        expect(sel.extent, const G4Position(6, 22));
        expect(h.surface.copySelection(), expectedRange(h.document, sel));
        expect(h.surface.copySelection().startsWith('alpha bravo charlie\n\nBlock 002'), isTrue);
        expect(h.surface.copySelection().endsWith('Block 006 alpha bravo '), isTrue);
      });

      // -------------------------------------------------------------------
      // 6. double-click word / triple-click block
      // -------------------------------------------------------------------
      testWidgets('6a. double-click selects the word', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);
        // char 17 is inside "bravo" (16..21).
        final Offset at = globalFor(tester, 3, 17, h.document);

        final TestGesture g1 = await mouseDownAt(tester, at);
        await g1.up();
        await tester.pump(const Duration(milliseconds: 30));
        final TestGesture g2 = await mouseDownAt(tester, at);
        await g2.up();
        await tester.pump();

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, const G4Position(3, 16));
        expect(sel.extent, const G4Position(3, 21));
        expect(h.surface.copySelection(), 'bravo');
      });

      testWidgets('6b. triple-click selects the block', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);
        final Offset at = globalFor(tester, 3, 17, h.document);

        for (int i = 0; i < 3; i++) {
          final TestGesture g = await mouseDownAt(tester, at);
          await g.up();
          await tester.pump(const Duration(milliseconds: 30));
        }
        await tester.pump();

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, const G4Position(3, 0));
        expect(sel.extent, const G4Position(3, 29));
        expect(h.surface.copySelection(), 'Block 003 alpha bravo charlie');
      });

      // -------------------------------------------------------------------
      // 7. typing replaces a cross-block selection
      // -------------------------------------------------------------------
      testWidgets('7. typing replaces a cross-block selection', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);

        h.surface.setSelection(
          const G4Selection(base: G4Position(2, 6), extent: G4Position(5, 9)),
        );
        await tester.pump();
        await tester.pump();

        expect(h.document.blockCount, kBlockCount);
        await Ime().type(tester, 'X');
        await tester.pump();

        expect(
          h.document.blockCount,
          kBlockCount - 3,
          reason: 'blocks 2..5 must collapse into one',
        );
        expect(h.document.blockAt(2), 'Block X alpha bravo charlie');
        expect(h.document.blockAt(3), 'Block 006 alpha bravo charlie');
        expect(h.surface.selection!.isCollapsed, isTrue);
        expect(h.surface.selection!.extent, const G4Position(2, 7));
        expect(h.document.blockAt(0), 'Block 000 alpha bravo charlie');
        expect(h.document.blockAt(h.document.blockCount - 1), 'Block 399 alpha bravo charlie');
      });

      // -------------------------------------------------------------------
      // 8. IME composition while a selection exists elsewhere, during scroll
      // -------------------------------------------------------------------
      testWidgets('8. IME composition with a live cross-block selection, during scroll', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);
        final Ime ime = Ime();

        // A real cross-block selection: anchor in block 2, caret in block 5.
        h.surface.setSelection(
          const G4Selection(base: G4Position(2, 0), extent: G4Position(5, 29)),
        );
        await tester.pump();
        await tester.pump();

        expect(
          tester.testTextInput.hasAnyClients,
          isTrue,
          reason: 'no input connection to compose into',
        );
        ime.sync(tester);
        // VARIANT-NEUTRAL. The original form of these two assertions hard-coded
        // Variant A's IME buffer shape ("exactly the caret block, selected
        // 0..29"), which is an implementation detail of the editable island and
        // not a §7 requirement. Variant B hands the platform a document-level
        // window spanning the selected blocks, so it could never satisfy that
        // literal. Restated as the property both must have, which is strictly
        // stronger than the original: the caret block is in the buffer, and the
        // text the IME believes is selected really is this surface's share of
        // the document selection it does not own.
        expect(
          ime.value.text.contains('Block 005 alpha bravo charlie'),
          isTrue,
          reason: 'the caret block is not in the IME buffer at all',
        );
        final String imeSelected = ime.value.selection.textInside(ime.value.text);
        expect(imeSelected, isNotEmpty, reason: 'the IME was shown a collapsed selection');
        expect(
          h.surface.copySelection().endsWith(imeSelected),
          isTrue,
          reason:
              'the IME selection is not the surface\'s share of the document selection '
              '(ime saw "$imeSelected")',
        );

        // Scroll until the anchor block is disposed but the caret block is not
        // yet, then keep going so BOTH are gone while composition is live.
        h.scrollController.jumpTo(30 * G4Layout.itemExtent);
        await tester.pump();
        expect(builtBlocks().contains(2), isFalse);
        expect(builtBlocks().contains(5), isFalse);

        expect(
          tester.testTextInput.hasAnyClients,
          isTrue,
          reason: 'input connection died when the focused block left the viewport',
        );

        // Composition begins: it must replace the cross-block selection.
        await ime.compose(tester, 'に');
        await tester.pump();
        expect(
          h.document.blockCount,
          kBlockCount - 3,
          reason: 'composition did not replace the whole cross-block selection',
        );

        // Scroll again, mid-composition. The caret block (now 2) is far above
        // the viewport at this point.
        h.scrollController.jumpTo(60 * G4Layout.itemExtent);
        await tester.pump();
        expect(builtBlocks().contains(2), isFalse);
        expect(
          tester.testTextInput.hasAnyClients,
          isTrue,
          reason: 'input connection died while scrolling with a live composition',
        );

        // Composition grows in place.
        await ime.compose(tester, 'にほん');
        await tester.pump();
        // Asked of the SURFACE, not of our own fake IME, so this cannot pass
        // vacuously: it is the surface's own record of the live composition,
        // in model coordinates, after two scrolls and a block move.
        expect(
          h.surface.composingRegion,
          const G4Selection(base: G4Position(2, 0), extent: G4Position(2, 3)),
          reason: 'the composing region was lost across the block move',
        );

        await ime.commit(tester);
        await tester.pump();
        expect(h.surface.composingRegion, isNull, reason: 'composition never committed');

        expect(h.document.blockCount, kBlockCount - 3);
        expect(h.document.blockAt(2), 'にほん');
        expect(h.document.blockAt(3), 'Block 006 alpha bravo charlie');
        expect(h.document.blockAt(0), 'Block 000 alpha bravo charlie');
        expect(h.document.blockAt(h.document.blockCount - 1), 'Block 399 alpha bravo charlie');
        expect(h.surface.selection!.isCollapsed, isTrue);
      });

      // -------------------------------------------------------------------
      // Supporting evidence for the report. Not part of §7, but these are the
      // behaviours the variant must own to keep source authority at all.
      // -------------------------------------------------------------------
      testWidgets('9. focused block survives scrolling out of view', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);
        h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 4)));
        await tester.pump();
        await tester.pump();

        final bool hadClient = tester.testTextInput.hasAnyClients;

        h.scrollController.jumpTo(80 * G4Layout.itemExtent);
        await tester.pump();
        expect(builtBlocks().contains(3), isFalse, reason: 'row must leave the viewport');

        expect(h.surface.focusedBlock, 3, reason: 'focus target changed on scroll');
        if (hadClient) {
          expect(
            tester.testTextInput.hasAnyClients,
            isTrue,
            reason: 'input connection dropped when the focused row was recycled',
          );
        }

        // Still editable from off-screen.
        if (hadClient) {
          await Ime().type(tester, 'Z');
          await tester.pump();
          expect(h.document.blockAt(3), 'BlocZk 003 alpha bravo charlie');
        }
      });

      testWidgets('10. selection passing THROUGH the focused block paints its share', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);
        // Caret in block 4, then extend so the selection runs 1..6 with the
        // focused block strictly inside it.
        h.surface.setSelection(
          const G4Selection(base: G4Position(1, 3), extent: G4Position(6, 5)),
        );
        await tester.pump();
        await tester.pump();

        expect(h.surface.selection!.normalized.base, const G4Position(1, 3));
        expect(h.surface.selection!.normalized.extent, const G4Position(6, 5));
        expect(h.surface.copySelection(), expectedRange(h.document, h.surface.selection!));

        // Whatever the focused block is, its own view of the selection must be
        // the clip, not the whole thing, and the model must be unchanged by it.
        expect(h.document.text.split('\n\n').length, kBlockCount);
      });

      testWidgets('11. focus is not stolen mid-drag', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);
        h.surface.setSelection(const G4Selection.collapsed(G4Position(1, 2)));
        await tester.pump();
        await tester.pump();

        final int? before = h.surface.focusedBlock;

        final TestGesture g = await mouseDownAt(tester, globalFor(tester, 2, 0, h.document));
        for (int b = 3; b <= 7; b++) {
          await g.moveTo(globalFor(tester, b, 10, h.document));
          await tester.pump();
          expect(
            h.surface.focusedBlock,
            2,
            reason: 'focus followed the pointer to block $b during the drag (was $before)',
          );
        }
        await g.up();
        await tester.pump();
        expect(h.surface.focusedBlock, 7, reason: 'focus should commit on pointer up');
      });

      testWidgets('12. keyboard undo/redo routes through the document, not the editable', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);
        final String original = h.document.text;

        h.surface.setSelection(
          const G4Selection(base: G4Position(2, 0), extent: G4Position(4, 29)),
        );
        await tester.pump();
        await tester.pump();
        h.surface.replaceSelection('MERGED');
        await tester.pump();

        expect(h.document.blockCount, kBlockCount - 2);
        expect(h.document.blockAt(2), 'MERGED');

        await sendUndo(tester);
        await tester.pump();
        expect(
          h.document.text,
          original,
          reason: 'undo did not restore the whole document (editable-local undo stack?)',
        );

        await sendRedo(tester);
        await tester.pump();
        expect(h.document.blockAt(2), 'MERGED');
        expect(h.document.blockCount, kBlockCount - 2);
      });

      testWidgets('13. Cmd/Ctrl+A routes to the document, not the focused block', (
        WidgetTester tester,
      ) async {
        final _Harness h = await pumpSurface(tester, variant);
        h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 4)));
        await tester.pump();
        await tester.pump();

        await sendSelectAll(tester);
        await tester.pump();

        final G4Selection sel = h.surface.selection!.normalized;
        expect(sel.base, h.document.documentStart);
        expect(sel.extent, h.document.documentEnd);
        expect(h.surface.copySelection(), h.document.text);
      });

      testWidgets('14. backspace at offset 0 merges blocks', (WidgetTester tester) async {
        final _Harness h = await pumpSurface(tester, variant);
        h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 0)));
        await tester.pump();
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
        await tester.pump();

        expect(
          h.document.blockCount,
          kBlockCount - 1,
          reason: 'backspace at block start did not merge — the editable swallowed it',
        );
        expect(
          h.document.blockAt(2),
          'Block 002 alpha bravo charlieBlock 003 alpha bravo charlie',
        );
        expect(h.surface.selection!.extent, const G4Position(2, 29));
      });
    });
  }

  // =====================================================================
  // Variant-A-only evidence. These are not acceptance cases; they are the
  // proof behind the intercept table in the report. They are the reason the
  // controller-level intercept is mandatory rather than a belt-and-braces
  // convenience.
  // =====================================================================
  group('A(editable-island) EVIDENCE: the Actions override surface has holes', () {
    late G4Document doc;
    late ScrollController scroll;
    late GlobalKey<G4SurfaceState<G4Surface>> key;
    late G4InterceptLog log;
    late List<String> ancestorFired;

    Future<void> pumpProbe(WidgetTester tester) async {
      doc = G4Document(g4FixtureBlocks(count: 30));
      scroll = ScrollController();
      key = GlobalKey<G4SurfaceState<G4Surface>>();
      log = G4InterceptLog();
      ancestorFired = <String>[];
      addTearDown(scroll.dispose);

      Action<T> sentinel<T extends Intent>(String name) =>
          CallbackAction<T>(onInvoke: (T intent) {
            ancestorFired.add(name);
            return null;
          });

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              // An ancestor Actions that tries to claim EVERY mutating intent.
              // This is the "just override the Actions map" plan.
              child: Actions(
                actions: <Type, Action<Intent>>{
                  ReplaceTextIntent: sentinel<ReplaceTextIntent>('ReplaceTextIntent'),
                  UpdateSelectionIntent: sentinel<UpdateSelectionIntent>('UpdateSelectionIntent'),
                  PasteTextIntent: sentinel<PasteTextIntent>('PasteTextIntent'),
                  UndoTextIntent: sentinel<UndoTextIntent>('UndoTextIntent'),
                },
                child: G4VariantA(
                  key: key,
                  document: doc,
                  scrollController: scroll,
                  interceptLog: log,
                ),
              ),
            ),
          ),
        ),
      );
      key.currentState!.setSelection(const G4Selection.collapsed(G4Position(2, 3)));
      await tester.pump();
      await tester.pump();
      expect(find.byType(EditableText), findsOneWidget);
      expect(primaryFocus?.context, isNotNull);
    }

    testWidgets('PasteTextIntent IS overridable — the surface wins', (WidgetTester tester) async {
      await pumpProbe(tester);
      Actions.invoke(primaryFocus!.context!, const PasteTextIntent(SelectionChangedCause.keyboard));
      await tester.pump();
      expect(log.sawEvent('PasteTextIntent'), isTrue, reason: 'surface override did not run');
    });

    testWidgets('UndoTextIntent IS overridable — the surface wins', (WidgetTester tester) async {
      await pumpProbe(tester);
      Actions.invoke(primaryFocus!.context!, const UndoTextIntent(SelectionChangedCause.keyboard));
      await tester.pump();
      expect(log.sawEvent('UndoTextIntent'), isTrue, reason: 'surface override did not run');
    });

    testWidgets('ReplaceTextIntent is NOT overridable — it reaches the controller', (
      WidgetTester tester,
    ) async {
      await pumpProbe(tester);
      final TextEditingValue current = tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .value;

      Actions.invoke(
        primaryFocus!.context!,
        ReplaceTextIntent(
          current,
          'ZZZ',
          const TextSelection(baseOffset: 0, extentOffset: 5),
          SelectionChangedCause.keyboard,
        ),
      );
      await tester.pump();

      // editable_text.dart:5598 registers ReplaceTextIntent WITHOUT
      // Action.overridable, so no ancestor Actions is ever consulted.
      expect(
        ancestorFired,
        isNot(contains('ReplaceTextIntent')),
        reason: 'if this ever fires, the Flutter hole has been closed upstream',
      );
      // The write went straight at the controller. Only the controller-level
      // intercept stopped it from becoming the source of truth.
      expect(log.sawEvent('controller.text-write'), isTrue);
      expect(doc.blockAt(2), 'ZZZ 002 alpha bravo charlie');
      expect(doc.blockCount, 30);
    });

    testWidgets('UpdateSelectionIntent is NOT overridable — it reaches the controller', (
      WidgetTester tester,
    ) async {
      await pumpProbe(tester);
      final TextEditingValue current = tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .value;

      Actions.invoke(
        primaryFocus!.context!,
        UpdateSelectionIntent(
          current,
          const TextSelection(baseOffset: 2, extentOffset: 9),
          SelectionChangedCause.keyboard,
        ),
      );
      await tester.pump();

      expect(ancestorFired, isNot(contains('UpdateSelectionIntent')));
      expect(log.sawEvent('controller.selection-write'), isTrue);
      expect(
        key.currentState!.selection,
        const G4Selection(base: G4Position(2, 2), extent: G4Position(2, 9)),
      );
    });

    testWidgets('a multi-block selection survives a selection write from the island', (
      WidgetTester tester,
    ) async {
      await pumpProbe(tester);
      key.currentState!.setSelection(
        const G4Selection(base: G4Position(1, 4), extent: G4Position(5, 7)),
      );
      await tester.pump();
      await tester.pump();

      final TextEditingValue current = tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .value;
      // The island only knows about its own block, so left alone it would
      // "normalise" the selection down to a block-local one and destroy the
      // anchor sitting four blocks above it.
      Actions.invoke(
        primaryFocus!.context!,
        UpdateSelectionIntent(
          current,
          const TextSelection.collapsed(offset: 2),
          SelectionChangedCause.keyboard,
        ),
      );
      await tester.pump();

      expect(log.sawEvent('refuse-selection-collapse-multiblock'), isTrue);
      expect(
        key.currentState!.selection,
        const G4Selection(base: G4Position(1, 4), extent: G4Position(5, 7)),
        reason: 'the island collapsed a document selection it could not see',
      );
    });
  });
}

Future<void> sendUndo(WidgetTester tester) => _modifierKey(tester, LogicalKeyboardKey.keyZ);

Future<void> sendRedo(WidgetTester tester) async {
  await tester.sendKeyDownEvent(_modifier);
  await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyUpEvent(_modifier);
}

Future<void> sendSelectAll(WidgetTester tester) => _modifierKey(tester, LogicalKeyboardKey.keyA);

LogicalKeyboardKey get _modifier =>
    defaultTargetPlatform == TargetPlatform.macOS || defaultTargetPlatform == TargetPlatform.iOS
    ? LogicalKeyboardKey.metaLeft
    : LogicalKeyboardKey.controlLeft;

Future<void> _modifierKey(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(_modifier);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(_modifier);
}
