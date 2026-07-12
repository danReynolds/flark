import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Pumps the live-rendered editor for a widget test with a real Comrak parse.
///
/// Wraps the editor in [DefaultTextEditingShortcuts] (the mapping
/// `WidgetsApp` installs in real apps) so hardware Enter/Backspace/Delete keys
/// route through the production intents — pass `wrapShortcuts: false` for
/// tests that drive the platform text-input channel directly instead.
Future<FlarkFlutterController> pumpLiveEditor(
  WidgetTester tester,
  String markdown, {
  int? caret,
  bool wrapShortcuts = true,
  bool autofocus = true,
}) async {
  final controller = FlarkFlutterController.fromMarkdown(markdown);
  addTearDown(controller.dispose);
  if (caret != null) {
    controller.applySelection(FlarkSelection.collapsed(caret));
  }
  await controller.parseNow();

  Widget editor = FlarkLiveRenderedEditableText(
    controller: controller,
    style: const TextStyle(fontSize: 14),
    autofocus: autofocus,
  );
  editor = Directionality(textDirection: TextDirection.ltr, child: editor);
  if (wrapShortcuts) {
    editor = DefaultTextEditingShortcuts(child: editor);
  }
  await tester.pumpWidget(editor);
  await tester.pumpAndSettle();
  return controller;
}

/// The widget-layer analog of `InlineSequence` (the headless source-model
/// harness): it drives real key/text input through the live-rendered editor
/// and, after every step, snapshots the **rendered structure** — the ordered
/// editable rows, which block each belongs to, and where the cursor is — not
/// just the source string.
///
/// This is the tier that catches rendering/layout regressions a headless gate
/// cannot: a document whose source and projected display are both correct but
/// which lays out into the wrong number of visible rows or focuses the wrong
/// one (e.g. a blockquote-exit leaving a phantom empty row). Every step also
/// re-runs the headless source round-trip gate, so it is a strict superset of
/// the `InlineSequence` gates for the flows it covers.
final class LiveRenderSequence {
  LiveRenderSequence(this.tester, this.controller);

  static Future<LiveRenderSequence> start(
    WidgetTester tester,
    String markdown, {
    int? caret,
  }) async {
    final controller = await pumpLiveEditor(tester, markdown, caret: caret);
    final sequence = LiveRenderSequence(tester, controller);
    sequence._expectSourceRoundTrips();
    return sequence;
  }

  final WidgetTester tester;
  final FlarkFlutterController controller;

  String get source => controller.markdown;

  String get display => controller.projection.projectText(controller.markdown);

  /// The rendered editable rows, top to bottom — one entry per visible
  /// `EditableText`, its content the row's text. This is the primary digest:
  /// a spurious or missing row (the row-count bug class) shows up directly
  /// here.
  List<String> get rows {
    final finder = find.byType(EditableText);
    final count = finder.evaluate().length;
    final entries = <({double y, String text})>[];
    for (var index = 0; index < count; index += 1) {
      final editable = finder.at(index);
      entries.add((
        y: tester.getTopLeft(editable).dy,
        text: tester.widget<EditableText>(editable).controller.text,
      ));
    }
    entries.sort((a, b) => a.y.compareTo(b.y));
    return [for (final entry in entries) entry.text];
  }

  /// The index (into [rows]) of the row whose editable currently holds focus,
  /// or null when none does.
  int? get focusedRow {
    final finder = find.byType(EditableText);
    final count = finder.evaluate().length;
    final entries = <({double y, bool focused})>[];
    for (var index = 0; index < count; index += 1) {
      final editable = finder.at(index);
      entries.add((
        y: tester.getTopLeft(editable).dy,
        focused: tester.widget<EditableText>(editable).focusNode.hasFocus,
      ));
    }
    entries.sort((a, b) => a.y.compareTo(b.y));
    for (var index = 0; index < entries.length; index += 1) {
      if (entries[index].focused) return index;
    }
    return null;
  }

  Future<void> type(String text) async {
    final focused = _focusedEditable();
    final controllerForField = focused.controller;
    final selection = controllerForField.selection;
    final base = controllerForField.text;
    // A real keystroke replaces the current selection; when it is collapsed
    // (start == end) this is an insertion at the caret. Modelling it as a
    // pure insertion at the anchor would misreport a keystroke over a
    // non-collapsed selection as inserting instead of replacing.
    final start = selection.isValid ? selection.start : base.length;
    final end = selection.isValid ? selection.end : base.length;
    final next = base.replaceRange(start, end, text);
    await tester.enterText(_focusedEditableFinder(), next);
    await _settle();
  }

  Future<void> enter() async {
    await _sendKey(LogicalKeyboardKey.enter);
  }

  Future<void> backspace() async {
    await _sendKey(LogicalKeyboardKey.backspace);
  }

  Future<void> forwardDelete() async {
    await _sendKey(LogicalKeyboardKey.delete);
  }

  /// Sends Tab (or Shift+Tab when [shift]) to the focused editable — cell
  /// navigation in tables, indentation elsewhere.
  Future<void> tab({bool shift = false}) async {
    await _sendKey(LogicalKeyboardKey.tab, shift: shift);
  }

  /// Sends an arrow key to the focused editable, holding Shift when [shift] to
  /// extend the selection.
  Future<void> arrow(LogicalKeyboardKey key, {bool shift = false}) async {
    await _sendKey(key, shift: shift);
  }

  /// Places the source caret at [sourceOffset] and lets the editor route focus
  /// to the block that owns it.
  Future<void> moveCaret(int sourceOffset) async {
    controller.applySelection(FlarkSelection.collapsed(sourceOffset));
    await _settle();
  }

  /// Selects the source range `[sourceStart, sourceEnd)`. Source-space, so it
  /// can span blocks; focus routing is the editor's job.
  Future<void> select(int sourceStart, int sourceEnd) async {
    controller.applySelection(
      FlarkSelection(baseOffset: sourceStart, extentOffset: sourceEnd),
    );
    await _settle();
  }

  /// Toggles an inline [style] over the current selection (or arms it for a
  /// collapsed caret) through the same command path a toolbar button uses.
  Future<void> toggleStyle(FlarkMarkdownInlineStyle style) async {
    controller.commands.toggleInlineStyle(style);
    await _parseAndSettle();
  }

  /// Inserts [text] at the current caret as one atomic edit — a paste or IME
  /// commit. Pasted Markdown converts per markdown semantics, so only the
  /// source round-trip gate runs (not display fidelity).
  ///
  /// When a table cell is focused, the paste is delivered through that cell's
  /// real [EditableText] (the platform-value path a genuine paste uses), so the
  /// cell input formatter and the cell→source write-back run — the same path
  /// typing takes, escaping `|`→`\|` and newlines→spaces. Routing a cell paste
  /// through the document-level projected edit instead would insert the pipe
  /// raw and split the row, which is a harness artifact, not real cell
  /// behavior. Other blocks keep the projected-edit path.
  Future<void> paste(String text) async {
    final focused = find.byWidgetPredicate(
      (widget) => widget is EditableText && widget.focusNode.hasFocus,
    );
    final inTableCell =
        focused.evaluate().isNotEmpty &&
        find
            .ancestor(
              of: focused.first,
              matching: find.byKey(LiveBlockKeys.table),
            )
            .evaluate()
            .isNotEmpty;
    if (inTableCell) {
      final state = tester.state<EditableTextState>(focused.first);
      final value = state.textEditingValue;
      final selection = value.selection.isValid
          ? value.selection
          : TextSelection.collapsed(offset: value.text.length);
      state.userUpdateTextEditingValue(
        TextEditingValue(
          text: value.text.replaceRange(selection.start, selection.end, text),
          selection: TextSelection.collapsed(
            offset: selection.start + text.length,
          ),
        ),
        SelectionChangedCause.keyboard,
      );
      await _parseAndSettle();
      return;
    }

    final display = controller.projection.projectText(controller.markdown);
    final caret = controller.projection.sourceToDisplayOffset(
      controller.selection.extentOffset,
    );
    final next = display.substring(0, caret) + text + display.substring(caret);
    final applied = controller.applyProjectedTextEdit(
      oldDisplayText: display,
      newDisplayText: next,
    );
    expect(applied, isTrue, reason: 'paste("$text") was not applied');
    await _parseAndSettle();
  }

  /// The screen-space caret rectangle for [sourceOffset], resolved against the
  /// editable that owns it. Returns null when no editable currently renders
  /// that offset. Exposed for gesture-level suites that must tap or drag at a
  /// real text position.
  Rect? caretRectForSource(int sourceOffset) {
    final displayOffset = controller.projection.sourceToDisplayOffset(
      sourceOffset,
    );
    final ordered = _orderedEditableFinders();
    var consumed = 0;
    for (final editableFinder in ordered) {
      final editable = tester.widget<EditableText>(editableFinder);
      final text = editable.controller.text;
      final localOffset = displayOffset - consumed;
      if (localOffset >= 0 && localOffset <= text.length) {
        final renderEditable = tester
            .state<EditableTextState>(editableFinder)
            .renderEditable;
        final endpoints = renderEditable.getEndpointsForSelection(
          TextSelection.collapsed(offset: localOffset),
        );
        if (endpoints.isEmpty) return null;
        final local = endpoints.first.point;
        return Rect.fromCenter(
          center: renderEditable.localToGlobal(local),
          width: 1,
          height: 1,
        );
      }
      consumed += text.length + 1; // +1 for the inter-row newline.
    }
    return null;
  }

  void expectRows(List<String> expected, {String? reason}) {
    expect(
      rows,
      expected,
      reason: reason ?? 'rendered rows mismatch (source "${_escaped(source)}")',
    );
    _expectSourceRoundTrips();
  }

  void expectSource(String expected) {
    expect(source, expected);
  }

  void expectFocusedRow(int index) {
    expect(
      focusedRow,
      index,
      reason: 'expected the cursor in row $index (rows: $rows)',
    );
  }

  /// Whether the editable at row [index] renders inside a block carrying
  /// [blockKey] (e.g. `Key('FlarkLiveBlockBlockquote')`).
  bool rowIsInBlock(int index, Key blockKey) {
    final ordered = _orderedEditableFinders();
    if (index < 0 || index >= ordered.length) return false;
    return find
        .ancestor(of: ordered[index], matching: find.byKey(blockKey))
        .evaluate()
        .isNotEmpty;
  }

  void expectRowInBlock(int index, Key blockKey) {
    expect(
      rowIsInBlock(index, blockKey),
      isTrue,
      reason: 'expected row $index inside ${blockKey.toString()} (rows: $rows)',
    );
  }

  void expectRowNotInBlock(int index, Key blockKey) {
    expect(
      rowIsInBlock(index, blockKey),
      isFalse,
      reason:
          'expected row $index outside ${blockKey.toString()} (rows: $rows)',
    );
  }

  Future<void> _sendKey(LogicalKeyboardKey key, {bool shift = false}) async {
    await tester.showKeyboard(_focusedEditableFinder());
    await tester.pump();
    if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shift);
    await tester.sendKeyEvent(key);
    if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shift);
    await _settle();
  }

  Future<void> _settle() async {
    await tester.pumpAndSettle();
    _expectSourceRoundTrips();
  }

  /// Settle for a controller-driven mutation (command/paste), which — unlike a
  /// real key event routed through the widget — gets no widget-scheduled
  /// parse, so the authoritative parse is forced explicitly.
  Future<void> _parseAndSettle() async {
    await controller.parseNow();
    await tester.pumpAndSettle();
    _expectSourceRoundTrips();
  }

  /// Headless gate carried into the widget tier: the source must render the
  /// same with no editor state — a fresh parse of `controller.markdown`
  /// projects the identical display.
  void _expectSourceRoundTrips() {
    final export = source;
    final fresh = FlarkFlutterController.fromMarkdown(export);
    try {
      expect(
        fresh.tryParseSync(),
        isTrue,
        reason: 'round-trip needs a sync-capable parse backend',
      );
      expect(
        fresh.projection.projectText(export),
        display,
        reason:
            'export round-trip: "${_escaped(export)}" renders differently '
            'with no editor state',
      );
    } finally {
      fresh.dispose();
    }
  }

  List<Finder> _orderedEditableFinders() {
    final finder = find.byType(EditableText);
    final count = finder.evaluate().length;
    final entries = <({double y, Finder finder})>[];
    for (var index = 0; index < count; index += 1) {
      final editable = finder.at(index);
      entries.add((y: tester.getTopLeft(editable).dy, finder: editable));
    }
    entries.sort((a, b) => a.y.compareTo(b.y));
    return [for (final entry in entries) entry.finder];
  }

  Finder _focusedEditableFinder() {
    final focused = find.byWidgetPredicate(
      (widget) => widget is EditableText && widget.focusNode.hasFocus,
    );
    if (focused.evaluate().isNotEmpty) return focused.first;
    return find.byType(EditableText).first;
  }

  EditableText _focusedEditable() {
    return tester.widget<EditableText>(_focusedEditableFinder());
  }

  static String _escaped(String value) => value.replaceAll('\n', r'\n');
}

/// Common block keys, so tests read as intent rather than string literals.
///
/// [blockquote], [codeFence], and [table] key the block's wrapping container,
/// so they are ancestors of that block's row editables — use them with
/// [LiveRenderSequence.expectRowInBlock]. [listMarker] is different: it keys
/// the bullet/number *glyph*, a sibling of the row editable inside the same
/// `Row`, not an ancestor — `expectRowInBlock(_, listMarker)` is therefore
/// always false. Assert list structure by counting markers instead
/// (`find.byKey(LiveBlockKeys.listMarker)` → `findsNWidgets(n)`).
abstract final class LiveBlockKeys {
  static const blockquote = Key('FlarkLiveBlockBlockquote');
  static const codeFence = Key('FlarkLiveBlockCodeFence');

  /// Keys the list bullet/number glyph (a row sibling, not an ancestor). See
  /// the class doc: count these, don't pass to `expectRowInBlock`.
  static const listMarker = Key('FlarkLiveBlockListMarker');
  static const table = Key('FlarkLiveBlockTable');
}
