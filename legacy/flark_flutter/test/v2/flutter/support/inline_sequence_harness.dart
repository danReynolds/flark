import 'package:flark/src/v2/core/core.dart';
import 'package:flark_flutter/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';
import 'package:flutter_test/flutter_test.dart';

/// Drives a [FlarkFlutterController] through user-level editing sequences and
/// enforces the package's inline-validity invariant after every step:
///
/// 1. **Display fidelity** — the projected text equals exactly what the user
///    has typed; editor-authored delimiters never leak into the display.
/// 2. **Export round-trip** — parsing `controller.markdown` from scratch, with
///    no caret context, renders the same display. What you save is what anyone
///    else will see.
/// 3. **Selection sanity** — the selection stays within the document.
///
/// Gate 2 is the load-bearing one: it fails for any state whose styling
/// depends on editor-local compensation instead of the source itself.
///
/// [typeSource] types raw markdown characters instead: display fidelity is
/// deliberately not asserted there (typing the closing `*` of `*world*`
/// legitimately converts the run and hides its markers), while the round-trip
/// gate still runs on every keystroke.
final class InlineSequence {
  InlineSequence(this.controller);

  static Future<InlineSequence> start(String markdown) async {
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    final sequence = InlineSequence(controller);
    await controller.parseNow();
    return sequence;
  }

  final FlarkFlutterController controller;

  String get display => controller.projection.projectText(controller.markdown);

  int get displayCaret => controller.projection.sourceToDisplayOffset(
    controller.selection.extentOffset,
  );

  Future<void> type(String text) async {
    final before = display;
    final caret = displayCaret;
    final expected =
        before.substring(0, caret) + text + before.substring(caret);
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: before,
        newDisplayText: expected,
      ),
      isTrue,
      reason: 'type("$text") was not applied (display "$before", caret $caret)',
    );
    await settle(expected);
  }

  /// Types [text] one character at a time as raw markdown source — the "user
  /// speaking markdown" flow. Each keystroke passes the round-trip and
  /// selection gates; display fidelity is not asserted because completing a
  /// marker pair legitimately re-renders (`*world` + `*` hides the markers).
  Future<void> typeSource(String text) async {
    for (final char in text.split('')) {
      final before = display;
      final caret = displayCaret;
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: before,
          newDisplayText:
              before.substring(0, caret) + char + before.substring(caret),
        ),
        isTrue,
        reason:
            'typeSource("$char") was not applied '
            '(display "$before", caret $caret)',
      );
      await settleSource();
    }
  }

  /// Simulates an IME composition at the caret: each stage *replaces* the
  /// previous stage's text (the composing region grows, shrinks, or converts
  /// wholesale, e.g. `['k', 'ka', 'かに']`), and the final stage is the
  /// commit. Composing text is ordinary visible display text, so every
  /// intermediate update passes the full gates.
  Future<void> compose(List<String> stages) async {
    var previous = '';
    for (final stage in stages) {
      final before = display;
      final caret = displayCaret;
      final regionStart = caret - previous.length;
      final expected =
          before.substring(0, regionStart) + stage + before.substring(caret);
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: before,
          newDisplayText: expected,
        ),
        isTrue,
        reason:
            'compose stage "$stage" was not applied '
            '(display "$before", region $regionStart..$caret)',
      );
      await settle(expected);
      previous = stage;
    }
  }

  /// Inserts [text] at the current display caret as one edit — a paste or an
  /// IME commit. Display follows markdown semantics (pasted markers may
  /// convert to styling), so only the round-trip and sanity gates run.
  Future<void> paste(String text) async {
    final before = display;
    final caret = displayCaret;
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: before,
        newDisplayText:
            before.substring(0, caret) + text + before.substring(caret),
      ),
      isTrue,
      reason: 'paste("$text") was not applied (display "$before")',
    );
    await settleSource();
  }

  /// Replaces the current selection with [text] as one edit. With
  /// [sourceSemantics] false (plain typing over a selection), the display
  /// must come out as the selection replaced by [text]; set it true when
  /// [text] contains markdown markers, whose conversion legitimately
  /// re-renders.
  Future<void> replaceSelection(
    String text, {
    bool sourceSemantics = false,
  }) async {
    final before = display;
    final selection = controller.selection;
    final displayStart = controller.projection.sourceToDisplayOffset(
      selection.start,
    );
    final displayEnd = controller.projection.sourceToDisplayOffset(
      selection.end,
    );
    final expected =
        before.substring(0, displayStart) + text + before.substring(displayEnd);
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: before,
        newDisplayText: expected,
      ),
      isTrue,
      reason:
          'replaceSelection("$text") was not applied '
          '(display "$before", range $displayStart..$displayEnd)',
    );
    if (sourceSemantics) {
      await settleSource();
    } else {
      await settle(expected);
    }
  }

  Future<void> backspace() async {
    final before = display;
    final caret = displayCaret;
    expect(caret, greaterThan(0), reason: 'backspace at display start');
    final expected = before.substring(0, caret - 1) + before.substring(caret);
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: before,
        newDisplayText: expected,
      ),
      isTrue,
      reason: 'backspace was not applied (display "$before", caret $caret)',
    );
    await settle(expected);
  }

  /// [backspace] gated like [typeSource]: deleting a character can complete
  /// or break a marker pair, legitimately re-rendering, so only the
  /// round-trip and sanity gates run.
  Future<void> backspaceSource() async {
    final before = display;
    final caret = displayCaret;
    expect(caret, greaterThan(0), reason: 'backspace at display start');
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: before,
        newDisplayText:
            before.substring(0, caret - 1) + before.substring(caret),
      ),
      isTrue,
      reason: 'backspace was not applied (display "$before", caret $caret)',
    );
    await settleSource();
  }

  Future<void> toggle(FlarkMarkdownInlineStyle style) async {
    // Captured before the toggle: a selection wrap edits the source, and its
    // fresh markers only hide after the parse — the visible text must come
    // out unchanged.
    final expected = display;
    controller.commands.toggleInlineStyle(style);
    await settle(expected);
  }

  Future<void> moveCaret(int displayOffset) async {
    controller.applyProjectedSelection(FlarkSelection.collapsed(displayOffset));
    await settle(display);
  }

  Future<void> select(int displayStart, int displayEnd) async {
    controller.applyProjectedSelection(
      FlarkSelection(baseOffset: displayStart, extentOffset: displayEnd),
    );
    await settle(display);
  }

  Future<void> pressEnter({String? expectedDisplay}) async {
    final before = display;
    final caret = displayCaret;
    final expected =
        expectedDisplay ??
        '${before.substring(0, caret)}\n${before.substring(caret)}';
    controller.dispatch(
      command: FlarkMarkdownInputCommands.handleEnter,
      payload: const FlarkHandleEnterPayload(),
    );
    await settle(expected);
  }

  Future<void> undoExpecting(String expectedDisplay) async {
    controller.commands.undo();
    await settle(expectedDisplay);
  }

  Future<void> redoExpecting(String expectedDisplay) async {
    controller.commands.redo();
    await settle(expectedDisplay);
  }

  Future<void> settle(String expectedDisplay) async {
    await controller.parseNow();
    // Display fidelity: the projected text equals what the user typed. The
    // one legitimate exception is a *block* reinterpretation — e.g. leading
    // whitespace accumulating into an indented code block — where markdown
    // semantics demand the line render raw. That is a structural property of
    // the format, not an inline-delimiter leak.
    if (display != expectedDisplay && !_hasCodeBlock()) {
      expect(
        display,
        expectedDisplay,
        reason:
            'display fidelity: source is "${controller.markdown}" but the '
            'display no longer matches what the user typed',
      );
    }
    _expectExportRoundTrip();
    _expectSelectionSanity();
  }

  /// The gate set for raw markdown typing: everything except display
  /// fidelity.
  Future<void> settleSource() async {
    await controller.parseNow();
    _expectExportRoundTrip();
    _expectSelectionSanity();
  }

  bool _hasCodeBlock() {
    var found = false;
    void visit(FlarkRenderBlock block) {
      if (block.kind == FlarkMarkdownBlockKind.codeBlock) found = true;
      block.children.forEach(visit);
    }

    controller.renderPlan.blocks.forEach(visit);
    return found;
  }

  /// The source must mean the same thing with no caret context: what the
  /// editor displays is exactly what any other consumer of the exported
  /// markdown will render. This is the invariant the sticky-run era violated.
  void _expectExportRoundTrip() {
    final export = controller.markdown;
    final fresh = FlarkFlutterController.fromMarkdown(export);
    try {
      expect(
        fresh.tryParseSync(),
        isTrue,
        reason: 'export round-trip needs a sync-capable parse backend',
      );
      expect(
        fresh.projection.projectText(export),
        display,
        reason:
            'export round-trip: "$export" renders differently with no '
            'caret context — the source depends on editor-local state',
      );
    } finally {
      fresh.dispose();
    }
  }

  void _expectSelectionSanity() {
    final selection = controller.selection;
    expect(
      selection.start >= 0 && selection.end <= controller.markdown.length,
      isTrue,
      reason: 'selection escaped the document',
    );
  }

  void expectSource(String source) {
    expect(controller.markdown, source);
  }

  void expectActive(FlarkMarkdownInlineStyle style, {required bool active}) {
    expect(
      controller.commands.isInlineActive(style),
      active,
      reason: 'toolbar honesty for $style',
    );
  }

  void dispose() {
    controller.dispose();
  }
}
