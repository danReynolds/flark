// RFC 024 Gate G4 — the surface contract both variants implement.
//
// The acceptance suite talks to this and nothing else, so Variant B can be
// dropped in without editing a line of the suite or this file.
//
// Also holds everything that must be *identical* between the two variants so
// the comparison is honest: layout metrics, text metrics/hit-testing, the
// painted (non-focused) block, and the autoscroll policy.
//
// ---------------------------------------------------------------------------
// To add Variant B (own-painted):
//   1. `class G4VariantB extends G4Surface` with
//      `class G4VariantBState extends G4SurfaceState<G4VariantB>`.
//   2. Implement `selection`, `setSelection`, `focusedBlock` (may return null
//      forever), `copySelection`, `replaceSelection`, `composingRegion`.
//   3. Stamp `g4BlockKey(index)` on every row widget. The suite proves
//      virtualization through that key, so a variant that forgets it will look
//      like it built nothing.
//   4. Use `g4PositionForViewportOffset` and `g4AutoscrollDelta` for gestures,
//      and `G4PaintedBlock` for rows, so any behavioural difference between the
//      variants is attributable to the input surface and not to the harness.
//   5. Add one line to `_variants` in g4_acceptance_test.dart. Change nothing
//      else in the suite or in this file.
// ---------------------------------------------------------------------------

import 'package:flutter/material.dart';

import 'g4_model.dart';

/// Layout constants. Shared so both variants have byte-identical geometry.
abstract final class G4Layout {
  /// Fixed row height. `itemExtent` on the ListView makes scroll offsets exact,
  /// which in turn makes "block N is at scroll offset N * itemExtent" a fact
  /// the surface can rely on for hit-testing blocks that were never built.
  static const double itemExtent = 40;

  static const EdgeInsets padding = EdgeInsets.symmetric(horizontal: 8, vertical: 10);

  static const double viewportWidth = 600;

  /// Roughly 8 rows visible, per the gate brief.
  static const double viewportHeight = itemExtent * 8;

  static const TextStyle textStyle = TextStyle(fontSize: 14, height: 1.2, color: Color(0xFF101010));

  static const Color selectionColor = Color(0x554285F4);

  static const Color cursorColor = Color(0xFF1A73E8);

  static double get textMaxWidth => viewportWidth - padding.horizontal;

  /// Distance from the viewport edge at which a drag starts autoscrolling.
  static const double autoscrollMargin = 24;

  static const double autoscrollPixelsPerTick = 12;

  static const Duration autoscrollTick = Duration(milliseconds: 16);
}

/// Key stamped on every block row by both variants. The suite uses it to prove
/// that blocks outside the viewport are genuinely absent from the tree.
Key g4BlockKey(int index) => ValueKey<String>('g4-block-$index');

/// Shared text measurement. Both variants hit-test through this, so a tap at a
/// given pixel resolves to the same model offset in either.
///
/// Crucially this can measure a block that has never been built: it needs only
/// the string. That is what lets a drag keep extending into rows the viewport
/// has not created yet.
abstract final class G4TextMetrics {
  static final Map<String, TextPainter> _cache = <String, TextPainter>{};

  static TextPainter painterFor(String text) {
    return _cache.putIfAbsent(text, () {
      final TextPainter p = TextPainter(
        text: TextSpan(text: text, style: G4Layout.textStyle),
        textDirection: TextDirection.ltr,
      )..layout(maxWidth: G4Layout.textMaxWidth);
      return p;
    });
  }

  /// Local offset (relative to the row box) -> UTF-16 offset in [text].
  static int offsetForLocal(String text, Offset local) {
    final Offset inText = local - Offset(G4Layout.padding.left, G4Layout.padding.top);
    return painterFor(text).getPositionForOffset(inText).offset.clamp(0, text.length);
  }

  /// UTF-16 offset in [text] -> local offset (relative to the row box),
  /// pointing at the vertical centre of the glyph so taps are unambiguous.
  static Offset localForOffset(String text, int charOffset) {
    final TextPainter p = painterFor(text);
    final Offset caret = p.getOffsetForCaret(
      TextPosition(offset: charOffset.clamp(0, text.length)),
      Rect.zero,
    );
    return caret +
        Offset(G4Layout.padding.left, G4Layout.padding.top + p.preferredLineHeight / 2);
  }

  static List<TextBox> boxesFor(String text, int start, int end) {
    if (start >= end) {
      return const <TextBox>[];
    }
    return painterFor(text).getBoxesForSelection(
      TextSelection(baseOffset: start, extentOffset: end),
    );
  }
}

/// The widget contract. Both variants extend this.
abstract class G4Surface extends StatefulWidget {
  const G4Surface({
    super.key,
    required this.document,
    required this.scrollController,
    this.onSelectionChanged,
  });

  final G4Document document;

  /// Owned by the caller so the suite can scroll the surface directly.
  final ScrollController scrollController;

  final ValueChanged<G4Selection?>? onSelectionChanged;

  @override
  G4SurfaceState<G4Surface> createState();
}

/// The behaviour contract the acceptance suite drives.
abstract class G4SurfaceState<T extends G4Surface> extends State<T> {
  /// Current document-level selection, in model coordinates. Never derived
  /// from rendered text.
  G4Selection? get selection;

  /// Move the selection programmatically (shift-click, select-all, tests).
  void setSelection(G4Selection? selection);

  /// The block that currently owns the input connection, if any.
  /// Variant B is allowed to return null forever.
  int? get focusedBlock;

  /// Exact source for the current selection, including blocks never built.
  String copySelection();

  /// Replace the current selection with [text] through the document, and put
  /// the caret after it.
  void replaceSelection(String text);

  /// The IME composing region currently live in the surface, in MODEL
  /// coordinates, or null when there is no composition.
  ///
  /// Both variants must be able to answer this: it is the only way to check
  /// from the outside that a composition survived a scroll, a block move or a
  /// cross-block replacement, rather than being silently dropped.
  G4Selection? get composingRegion;

  G4Document get document => widget.document;

  ScrollController get scrollController => widget.scrollController;
}

/// Builder signature so the suite can be parameterised over variants.
typedef G4SurfaceBuilder =
    G4Surface Function({
      required Key key,
      required G4Document document,
      required ScrollController scrollController,
    });

@immutable
class G4Variant {
  const G4Variant(this.name, this.build);
  final String name;
  final G4SurfaceBuilder build;
}

// ---------------------------------------------------------------------------
// Shared painted block. Used by both variants for every non-focused row, and
// by Variant B for every row. Paints the block's *share* of a document-level
// selection it does not own.
// ---------------------------------------------------------------------------

class G4PaintedBlock extends StatelessWidget {
  const G4PaintedBlock({
    super.key,
    required this.text,
    required this.selectionStart,
    required this.selectionEnd,
    this.caretOffset,
  });

  final String text;

  /// Block-local UTF-16 range to highlight; equal values mean no highlight.
  final int selectionStart;
  final int selectionEnd;

  /// Block-local caret position, or null. Only Variant B paints this.
  final int? caretOffset;

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      painter: _G4BlockPainter(
        text: text,
        selectionStart: selectionStart,
        selectionEnd: selectionEnd,
        caretOffset: caretOffset,
      ),
      child: const SizedBox.expand(),
    );
  }
}

class _G4BlockPainter extends CustomPainter {
  _G4BlockPainter({
    required this.text,
    required this.selectionStart,
    required this.selectionEnd,
    required this.caretOffset,
  });

  final String text;
  final int selectionStart;
  final int selectionEnd;
  final int? caretOffset;

  @override
  void paint(Canvas canvas, Size size) {
    final TextPainter p = G4TextMetrics.painterFor(text);
    final Offset origin = Offset(G4Layout.padding.left, G4Layout.padding.top);

    if (selectionEnd > selectionStart) {
      final Paint paint = Paint()..color = G4Layout.selectionColor;
      for (final TextBox box in G4TextMetrics.boxesFor(text, selectionStart, selectionEnd)) {
        canvas.drawRect(box.toRect().shift(origin), paint);
      }
    }

    p.paint(canvas, origin);

    final int? caret = caretOffset;
    if (caret != null) {
      final Offset c = p.getOffsetForCaret(TextPosition(offset: caret), Rect.zero) + origin;
      canvas.drawRect(
        Rect.fromLTWH(c.dx, c.dy, 2, p.preferredLineHeight),
        Paint()..color = G4Layout.cursorColor,
      );
    }
  }

  @override
  bool shouldRepaint(_G4BlockPainter old) =>
      old.text != text ||
      old.selectionStart != selectionStart ||
      old.selectionEnd != selectionEnd ||
      old.caretOffset != caretOffset;
}

// ---------------------------------------------------------------------------
// Shared pointer -> model resolution and autoscroll. Identical in both
// variants, so any difference in gesture results is attributable to the input
// surface and not to the harness.
// ---------------------------------------------------------------------------

/// Resolves a pointer position inside the scrollable to a model position,
/// using only the scroll offset and the document strings.
///
/// Works for rows that are not built. This is the whole reason model-range
/// selection is viable under virtualization.
G4Position g4PositionForViewportOffset({
  required G4Document document,
  required double scrollOffset,
  required Offset local,
}) {
  final double y = scrollOffset + local.dy;
  final int block = (y / G4Layout.itemExtent).floor().clamp(0, document.blockCount - 1);
  final double rowTop = block * G4Layout.itemExtent;
  final Offset inRow = Offset(local.dx, y - rowTop);
  final int offset = G4TextMetrics.offsetForLocal(document.blockAt(block), inRow);
  return G4Position(block, offset);
}

/// Autoscroll amount for a pointer at [local] inside a viewport of
/// [viewportHeight]. Returns 0 when the pointer is not near an edge.
double g4AutoscrollDelta(Offset local, double viewportHeight) {
  if (local.dy < G4Layout.autoscrollMargin) {
    return -G4Layout.autoscrollPixelsPerTick;
  }
  if (local.dy > viewportHeight - G4Layout.autoscrollMargin) {
    return G4Layout.autoscrollPixelsPerTick;
  }
  return 0;
}
