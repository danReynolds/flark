// RFC 024 Gate G4 — Variant B: own-painted, one document-level IME connection.
//
// There is no `EditableText` anywhere in this file. Every block is painted with
// the shared `G4PaintedBlock`. The surface owns:
//   * one `TextInputConnection` for the whole document, via `DeltaTextInputClient`;
//   * the caret (painted, blinking);
//   * selection painting;
//   * hit-testing, multi-tap, drag, autoscroll;
//   * every text-editing Intent the framework can send.
//
// The recipe is not invented. `super_editor` (DocumentImeInputClient +
// DocumentImeSerializer) and `appflowy_editor` (DeltaTextInputService) both do
// exactly this, and both scope the IME buffer the same way: serialise only the
// blocks the selection touches, and prepend an invisible character so the
// platform reports a backspace when the caret sits at the start of a block.
//
// ===========================================================================
// THE IME WINDOW — how it is scoped, and what breaks outside it
// ===========================================================================
//
// The platform IME understands one flat string. A document-level connection
// therefore needs a *window*: a bounded slice of the document handed to the
// platform, plus a bijection between window offsets and model coordinates.
//
// Scoping rule, in order:
//
//   1. The window covers exactly the blocks the current selection touches
//      (`[selection.start.block .. selection.end.block]`), serialised with the
//      model's own separator `\n\n`. Using the model's separator — rather than
//      super_editor's `\n` — makes window offsets an exact affine map of model
//      coordinates, so an IME edit and the corresponding model edit are the
//      same edit. Nothing is re-encoded, so nothing can be lost in re-encoding.
//
//   2. A fixed 2-character prefix `'. '` is ALWAYS prepended. Without it the
//      platform believes the caret is at offset 0 of the whole input and never
//      reports a backspace, so backspace-at-block-start silently does nothing.
//      That is the exact hole Variant A could not close. A deletion that eats
//      into the prefix is read as "delete backward across the window start" and
//      becomes a block merge.
//
//   3. If the serialisation would exceed `kImeWindowMaxChars`, the window
//      COLLAPSES to the caret block alone and is marked `clipped`. Select-all on
//      a 1 MB document must not push 1 MB to the platform IME. super_editor does
//      not cap, which is precisely why `Cmd+A` on a large document is a problem
//      there.
//
// What breaks outside the window:
//
//   * While `clipped`, the platform can only see the caret block. The IME
//     selection it is shown is that block's *share* of the document selection.
//     A text-replacing delta arriving in this state is WIDENED back out to the
//     full model selection before it reaches the document (`_maybeWiden`), the
//     same correction Variant A needs permanently. The difference is that here
//     it is a consequence of a cap we chose, applies only above 2 KB, and is
//     four lines of our own code rather than a fight with a foreign widget.
//   * Autocorrect/predictive text can only reason about text inside the window.
//     With the default cap that is the surrounding blocks, which is more context
//     than Variant A's single block ever has.
//   * A *single* block larger than the cap is still pushed whole: there is no
//     sub-block windowing. Splitting inside a block would put a word boundary in
//     the middle of the buffer and break autocorrect's view of the word being
//     typed. Cost is bounded by the largest block, not the document.
//   * The platform's Enter key produces a single `\n`. Our separator is `\n\n`,
//     so a lone `\n` is normalised to a block split (`_normaliseInsertedText`).
//     Whether a lone newline should be a block split or a hard line break is an
//     engine policy question, not an input-surface one; the hook is here.
//
// ===========================================================================

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'g4_model.dart';
import 'g4_surface.dart';

/// Invisible-to-the-user leading characters in the IME buffer. Exists only so
/// the platform will report a backspace at the start of the window.
/// (`super_editor` uses '. '; `appflowy_editor` uses ' '.)
const String kImePrefix = '. ';

/// Above this many UTF-16 units the window collapses to the caret block.
const int kImeWindowMaxChars = 2048;

const Duration kCaretBlinkPeriod = Duration(milliseconds: 500);

class G4VariantB extends G4Surface {
  const G4VariantB({
    super.key,
    required super.document,
    required super.scrollController,
    super.onSelectionChanged,
  });

  static G4Surface builder({
    required Key key,
    required G4Document document,
    required ScrollController scrollController,
  }) => G4VariantB(key: key, document: document, scrollController: scrollController);

  @override
  G4VariantBState createState() => G4VariantBState();
}

class G4VariantBState extends G4SurfaceState<G4VariantB> with DeltaTextInputClient {
  final GlobalKey _viewportKey = GlobalKey();
  final FocusNode _focusNode = FocusNode(debugLabel: 'g4-variant-b-document');

  G4Selection? _selection;

  /// Live composition, in MODEL coordinates.
  G4Selection? _composing;

  // ---- IME state -------------------------------------------------------
  TextInputConnection? _connection;

  /// The window the platform is currently looking at.
  _ImeWindow? _window;

  /// What the platform believes the buffer contains. Mirrors super_editor's
  /// `_platformTextEditingValue`: we only push when our canonical value differs
  /// from this, otherwise every edit ping-pongs.
  TextEditingValue _platformValue = TextEditingValue.empty;

  /// True while deltas are being folded into the document, so document/selection
  /// notifications do not re-enter and push a half-applied window.
  bool _applying = false;

  // ---- gesture state ---------------------------------------------------
  bool _dragging = false;
  G4Position? _dragAnchor;
  Offset? _lastDragLocal;
  Timer? _autoscrollTimer;

  int _tapCount = 0;
  Offset? _lastTapGlobal;
  DateTime _lastTapAt = DateTime.fromMillisecondsSinceEpoch(0);

  // ---- caret -----------------------------------------------------------
  Timer? _blinkTimer;
  bool _caretOn = true;

  @override
  void initState() {
    super.initState();
    document.addListener(_onDocumentChanged);
    _focusNode.addListener(_onFocusChanged);
  }

  @override
  void dispose() {
    _autoscrollTimer?.cancel();
    _blinkTimer?.cancel();
    _detachIme();
    document.removeListener(_onDocumentChanged);
    _focusNode.removeListener(_onFocusChanged);
    _focusNode.dispose();
    super.dispose();
  }

  void _onDocumentChanged() {
    if (!mounted) {
      return;
    }
    setState(() {});
    if (!_applying) {
      _syncImeToModel();
    }
  }

  void _onFocusChanged() {
    if (_focusNode.hasFocus) {
      _attachIme();
    }
    _restartBlink();
  }

  // =====================================================================
  // G4Surface contract
  // =====================================================================

  @override
  G4Selection? get selection => _selection;

  /// The block the IME window is anchored on. There is no editable widget to
  /// "focus", so this is the caret block as of the last window push. It is
  /// deliberately NOT recomputed mid-drag: re-serialising and re-pushing the
  /// buffer on every pointer move would churn the platform connection sixty
  /// times a second for no benefit.
  @override
  int? get focusedBlock => _window?.anchorBlock ?? _selection?.extent.block;

  @override
  G4Selection? get composingRegion => _composing;

  @override
  void setSelection(G4Selection? next) => _applySelection(next);

  @override
  String copySelection() {
    final G4Selection? s = _selection;
    if (s == null || s.isCollapsed) {
      return '';
    }
    final String out = document.extractRange(s);
    unawaited(Clipboard.setData(ClipboardData(text: out)));
    return out;
  }

  @override
  void replaceSelection(String text) {
    final G4Selection s = _selection ?? G4Selection.collapsed(document.documentStart);
    final G4Position caret = document.replaceRange(s, text);
    _composing = null;
    _applySelection(G4Selection.collapsed(caret));
  }

  // =====================================================================
  // Selection
  // =====================================================================

  void _applySelection(G4Selection? next) {
    final G4Selection? clamped = next == null ? null : document.clampSelection(next);
    setState(() => _selection = clamped);
    if (clamped != null && !_focusNode.hasFocus && _focusNode.canRequestFocus) {
      // Own-painted has no editable to focus, but the surface must still hold
      // focus or `DefaultTextEditingShortcuts` resolves its Intents against an
      // ancestor that has no text actions and every key silently does nothing.
      _focusNode.requestFocus();
    }
    _restartBlink();
    if (clamped != null) {
      _attachIme();
    }
    _syncImeToModel();
    widget.onSelectionChanged?.call(clamped);
  }

  void _restartBlink() {
    _blinkTimer?.cancel();
    _blinkTimer = null;
    final bool wantCaret = _focusNode.hasFocus && _selection != null;
    if (!wantCaret) {
      if (_caretOn && mounted) {
        setState(() => _caretOn = false);
      }
      return;
    }
    _caretOn = true;
    _blinkTimer = Timer.periodic(kCaretBlinkPeriod, (_) {
      if (!mounted) {
        return;
      }
      setState(() => _caretOn = !_caretOn);
    });
  }

  // =====================================================================
  // IME: connection lifecycle
  // =====================================================================

  void _attachIme() {
    if (_connection != null && _connection!.attached) {
      return;
    }
    _connection = TextInput.attach(
      this,
      const TextInputConfiguration(
        inputType: TextInputType.multiline,
        inputAction: TextInputAction.newline,
        // The whole point of Variant B: edits arrive as deltas, which are
        // already model edits, instead of two opaque values to diff.
        enableDeltaModel: true,
        autocorrect: true,
        enableSuggestions: true,
        enableIMEPersonalizedLearning: true,
      ),
    );
    _platformValue = TextEditingValue.empty;
    _window = null;
    _connection!.show();
    _syncImeToModel(force: true);
  }

  void _detachIme() {
    _connection?.close();
    _connection = null;
    _window = null;
    _platformValue = TextEditingValue.empty;
  }

  /// Recompute the canonical window from the model and push it to the platform
  /// if the platform's belief differs.
  void _syncImeToModel({bool force = false}) {
    final TextInputConnection? c = _connection;
    final G4Selection? sel = _selection;
    if (c == null || !c.attached || sel == null) {
      return;
    }
    // Mid-gesture the window is frozen: the platform has no business composing
    // while the user is dragging a selection, and re-pushing per pointer move is
    // pure churn.
    if (_dragging && !force) {
      return;
    }
    final _ImeWindow w = _ImeWindow.around(
      document,
      sel,
      anchorBlock: sel.extent.block,
      maxChars: kImeWindowMaxChars,
    );
    final TextEditingValue value = w.valueFor(sel, _composing);
    _window = w;
    if (force || value != _platformValue) {
      _platformValue = value;
      c.setEditingState(value);
    }
    _updateImeGeometry();
  }

  /// IME popups (candidate windows, autocorrect bubbles) are positioned by the
  /// platform from this. `EditableText` does it for free; own-painted must do it
  /// by hand, and must be able to answer it for a caret block that is not built.
  void _updateImeGeometry() {
    final TextInputConnection? c = _connection;
    final G4Selection? sel = _selection;
    final RenderObject? ro = _viewportKey.currentContext?.findRenderObject();
    if (c == null || !c.attached || sel == null || ro is! RenderBox || !ro.hasSize) {
      return;
    }
    final double scroll = scrollController.hasClients ? scrollController.offset : 0;
    final int block = sel.extent.block;
    final Offset inRow = G4TextMetrics.localForOffset(
      document.blockAt(block),
      sel.extent.offsetUtf16,
    );
    final double top = block * G4Layout.itemExtent - scroll + inRow.dy;
    c
      ..setEditableSizeAndTransform(ro.size, ro.getTransformTo(null))
      ..setCaretRect(Rect.fromLTWH(inRow.dx, top, 2, G4Layout.itemExtent));
  }

  // =====================================================================
  // IME: ingress
  // =====================================================================

  @override
  TextEditingValue? get currentTextEditingValue => _platformValue;

  @override
  AutofillScope? get currentAutofillScope => null;

  /// The delta path — what a real platform sends when `enableDeltaModel` is set.
  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> deltas) {
    if (deltas.isEmpty || _applying) {
      return;
    }
    for (final TextEditingDelta d in deltas) {
      _platformValue = d.apply(_platformValue);
    }
    _applyDeltas(deltas);
  }

  /// The non-delta path. Still reachable: the engine falls back to it, web has
  /// historically not supported deltas, and `TestTextInput.updateEditingValue`
  /// (which the acceptance suite drives) sends `TextInputClient.updateEditingState`
  /// regardless of `enableDeltaModel`. Both `super_editor` and `appflowy_editor`
  /// keep this path for the same reason. It synthesises the delta and then goes
  /// through exactly the same application code.
  @override
  void updateEditingValue(TextEditingValue value) {
    if (_applying || value == _platformValue) {
      return;
    }
    final TextEditingValue old = _platformValue;
    _platformValue = value;
    _applyDeltas(<TextEditingDelta>[_synthesiseDelta(old, value)]);
  }

  void _applyDeltas(List<TextEditingDelta> deltas) {
    _applying = true;
    try {
      for (final TextEditingDelta d in deltas) {
        _applyOneDelta(d);
      }
    } finally {
      _applying = false;
    }
    setState(() {});
    _restartBlink();
    _syncImeToModel();
    widget.onSelectionChanged?.call(_selection);
  }

  void _applyOneDelta(TextEditingDelta delta) {
    final _ImeWindow? w = _window;
    if (w == null) {
      return;
    }

    if (delta is TextEditingDeltaNonTextUpdate) {
      _selection = w.selectionFor(delta.selection) ?? _selection;
      _composing = w.selectionFor(delta.composing);
      return;
    }

    final int imeStart;
    final int imeEnd;
    final String rawInserted;
    switch (delta) {
      case TextEditingDeltaInsertion():
        imeStart = delta.insertionOffset;
        imeEnd = delta.insertionOffset;
        rawInserted = delta.textInserted;
      case TextEditingDeltaDeletion():
        imeStart = delta.deletedRange.start;
        imeEnd = delta.deletedRange.end;
        rawInserted = '';
      case TextEditingDeltaReplacement():
        imeStart = delta.replacedRange.start;
        imeEnd = delta.replacedRange.end;
        rawInserted = delta.replacementText;
      default:
        return;
    }
    final String inserted = _normaliseInsertedText(rawInserted);

    // Does the edit reach into the invisible prefix? That is the platform
    // saying "delete backward past the start of what you gave me".
    final bool touchesPrefix = imeStart < w.prefixLength;

    G4Position modelStart;
    final G4Position modelEnd = w.modelPositionFor(math.max(imeEnd, w.prefixLength));
    if (touchesPrefix) {
      modelStart = w.startBlock == 0
          ? const G4Position(0, 0)
          : G4Position(w.startBlock - 1, document.blockLength(w.startBlock - 1));
    } else {
      modelStart = w.modelPositionFor(imeStart);
    }

    G4Selection target = G4Selection(base: modelStart, extent: modelEnd);
    final bool widened = _maybeWiden(w, target);
    if (widened) {
      target = _selection!.normalized;
    }

    final int blocksBefore = document.blockCount;
    final G4Position caret = (target.isCollapsed && inserted.isEmpty)
        ? target.base
        : document.replaceRange(target, inserted);
    final int blocksDelta = document.blockCount - blocksBefore;

    // Clean case: the model edit was exactly the IME's edit, so the buffer the
    // platform now holds is the same window with the same edit applied. Its
    // offsets are still meaningful, so the delta's own post-edit selection and
    // composing region can be mapped straight back.
    final bool clean = !touchesPrefix && !widened && inserted == rawInserted;
    if (clean) {
      final _ImeWindow after = _ImeWindow.exact(
        document,
        w.startBlock,
        w.endBlock + blocksDelta,
        anchorBlock: caret.block,
        clipped: w.clipped,
      );
      _window = after;
      _selection = after.selectionFor(delta.selection) ?? G4Selection.collapsed(caret);
      _composing = after.selectionFor(delta.composing);
      return;
    }

    // Structural case (block merge across the prefix, or a widened cross-window
    // replacement). The buffer offsets the platform holds no longer describe the
    // document, so the caret comes from the model and the window is rebuilt from
    // scratch. `_syncImeToModel` then pushes it, because it will differ from
    // `_platformValue`.
    _selection = G4Selection.collapsed(caret);
    _composing = null;
    _window = _ImeWindow.exact(
      document,
      caret.block,
      caret.block,
      anchorBlock: caret.block,
      clipped: false,
    );
  }

  /// While the window is clipped the platform can only address the caret block,
  /// so a replacement of "this block's share" is really a replacement of the
  /// whole document selection.
  bool _maybeWiden(_ImeWindow w, G4Selection target) {
    final G4Selection? sel = _selection;
    if (!w.clipped || sel == null || sel.isCollapsed || !sel.normalized.isMultiBlock) {
      return false;
    }
    final int block = w.anchorBlock;
    final ({int start, int end})? clip = sel.normalized.clipToBlock(
      block,
      document.blockLength(block),
    );
    if (clip == null) {
      return false;
    }
    return target.base == G4Position(block, clip.start) &&
        target.extent == G4Position(block, clip.end);
  }

  /// Platforms send a single `\n` for Enter. The model's block separator is
  /// `\n\n`, so any run of newlines is promoted to exactly one block split.
  ///
  /// This is the one place where an engine policy leaks into the input surface:
  /// whether a lone newline should be a block split or a hard line break is a
  /// Markdown question, not an IME question. The hook is here and it is small.
  /// When it fires the edit is treated as structural (see `clean` below),
  /// because the buffer the platform holds and the text the model stored are no
  /// longer the same string.
  String _normaliseInsertedText(String s) =>
      s.contains('\n') ? s.replaceAll(RegExp(r'\n+'), kG4BlockSeparator) : s;

  static TextEditingDelta _synthesiseDelta(TextEditingValue old, TextEditingValue next) {
    if (old.text == next.text) {
      return TextEditingDeltaNonTextUpdate(
        oldText: old.text,
        selection: next.selection,
        composing: next.composing,
      );
    }
    final String a = old.text;
    final String b = next.text;

    int start = -1;
    int end = -1;
    String inserted = '';

    // Prefer what the platform believed it was replacing. A prefix/suffix diff
    // is ambiguous whenever the replacement shares characters with the text it
    // replaced, which is exactly the autocorrect case.
    if (old.selection.isValid && !old.selection.isCollapsed) {
      final int s = old.selection.start;
      final int e = old.selection.end;
      final int insertedLen = b.length - s - (a.length - e);
      if (insertedLen >= 0 &&
          s <= b.length &&
          s + insertedLen <= b.length &&
          e <= a.length &&
          a.substring(0, s) == b.substring(0, s) &&
          a.substring(e) == b.substring(s + insertedLen)) {
        start = s;
        end = e;
        inserted = b.substring(s, s + insertedLen);
      }
    }
    if (start < 0) {
      int p = 0;
      final int minLen = math.min(a.length, b.length);
      while (p < minLen && a.codeUnitAt(p) == b.codeUnitAt(p)) {
        p++;
      }
      int sa = a.length;
      int sb = b.length;
      while (sa > p && sb > p && a.codeUnitAt(sa - 1) == b.codeUnitAt(sb - 1)) {
        sa--;
        sb--;
      }
      start = p;
      end = sa;
      inserted = b.substring(p, sb);
    }

    if (start == end) {
      return TextEditingDeltaInsertion(
        oldText: a,
        textInserted: inserted,
        insertionOffset: start,
        selection: next.selection,
        composing: next.composing,
      );
    }
    if (inserted.isEmpty) {
      return TextEditingDeltaDeletion(
        oldText: a,
        deletedRange: TextRange(start: start, end: end),
        selection: next.selection,
        composing: next.composing,
      );
    }
    return TextEditingDeltaReplacement(
      oldText: a,
      replacementText: inserted,
      replacedRange: TextRange(start: start, end: end),
      selection: next.selection,
      composing: next.composing,
    );
  }

  // ---- the rest of the TextInputClient surface -------------------------

  @override
  void performAction(TextInputAction action) {
    if (action == TextInputAction.newline) {
      replaceSelection(kG4BlockSeparator);
    }
  }

  /// macOS routes editing keys through selectors as well as Intents.
  /// `appflowy_editor` implements exactly this one for the same reason: without
  /// it, backspace on macOS can arrive here and nowhere else.
  @override
  void performSelector(String selectorName) {
    switch (selectorName) {
      case 'deleteBackward:':
        _deleteBackward(document.positionBefore);
      case 'deleteForward:':
        _deleteForward(document.positionAfter);
      case 'deleteWordBackward:':
        _deleteBackward(document.wordBefore);
      case 'deleteWordForward:':
        _deleteForward(document.wordAfter);
      case 'moveLeft:':
        _moveOrExtend(document.positionBefore, collapse: true);
      case 'moveRight:':
        _moveOrExtend(document.positionAfter, collapse: true);
    }
  }

  @override
  void connectionClosed() {
    _connection = null;
    _window = null;
    _platformValue = TextEditingValue.empty;
  }

  /// Browser autofill blurs and refocuses the input; `EditableText` handles this
  /// (editable_text.dart:4129). Own-painted has to as well.
  @override
  bool onFocusReceived() {
    if (mounted && !_focusNode.hasFocus && _focusNode.canRequestFocus) {
      _focusNode.requestFocus();
      return true;
    }
    return false;
  }

  // Deliberately unimplemented; each one is a line in the "B must reimplement"
  // column of the report.
  @override
  void updateFloatingCursor(RawFloatingCursorPoint point) {}
  @override
  void showAutocorrectionPromptRect(int start, int end) {}
  @override
  void performPrivateCommand(String action, Map<String, dynamic> data) {}
  @override
  void insertContent(KeyboardInsertedContent content) {}
  @override
  void insertTextPlaceholder(Size size) {}
  @override
  void removeTextPlaceholder() {}
  @override
  void showToolbar() {}
  @override
  void didChangeInputControl(TextInputControl? oldControl, TextInputControl? newControl) {}

  // =====================================================================
  // Document editing primitives (driven by Intents below)
  // =====================================================================

  void _deleteBackward(G4Position Function(G4Position) boundary) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    if (!s.isCollapsed) {
      _composing = null;
      _applySelection(G4Selection.collapsed(document.delete(s)));
      return;
    }
    final G4Position from = boundary(s.extent);
    if (from == s.extent) {
      return;
    }
    _composing = null;
    _applySelection(
      G4Selection.collapsed(document.delete(G4Selection(base: from, extent: s.extent))),
    );
  }

  void _deleteForward(G4Position Function(G4Position) boundary) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    if (!s.isCollapsed) {
      _composing = null;
      _applySelection(G4Selection.collapsed(document.delete(s)));
      return;
    }
    final G4Position to = boundary(s.extent);
    if (to == s.extent) {
      return;
    }
    _composing = null;
    _applySelection(
      G4Selection.collapsed(document.delete(G4Selection(base: s.extent, extent: to))),
    );
  }

  void _moveOrExtend(G4Position Function(G4Position) boundary, {required bool collapse}) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    final G4Position next = boundary(s.extent);
    _applySelection(
      collapse ? G4Selection.collapsed(next) : G4Selection(base: s.base, extent: next),
    );
  }

  void _verticalMove(int lines, {required bool collapse}) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    final int block = (s.extent.block + lines).clamp(0, document.blockCount - 1);
    final G4Position next = G4Position(
      block,
      math.min(s.extent.offsetUtf16, document.blockLength(block)),
    );
    _applySelection(
      collapse ? G4Selection.collapsed(next) : G4Selection(base: s.base, extent: next),
    );
  }

  Future<void> _paste() async {
    final ClipboardData? data = await Clipboard.getData(Clipboard.kTextPlain);
    if (!mounted) {
      return;
    }
    replaceSelection(data?.text ?? '');
  }

  void _undo() {
    _composing = null;
    _applySelection(document.undo() ?? _selection);
  }

  void _redo() {
    _composing = null;
    _applySelection(document.redo() ?? _selection);
  }

  // =====================================================================
  // Pointer handling — identical policy to Variant A, because Variant A has
  // to hand-write it too (`rendererIgnoresPointer: true`). This is not a cost
  // B pays and A avoids.
  // =====================================================================

  Offset? _toViewportLocal(Offset global) {
    final RenderObject? ro = _viewportKey.currentContext?.findRenderObject();
    if (ro is! RenderBox) {
      return null;
    }
    return ro.globalToLocal(global);
  }

  G4Position? _positionForGlobal(Offset global) {
    final Offset? local = _toViewportLocal(global);
    if (local == null) {
      return null;
    }
    return g4PositionForViewportOffset(
      document: document,
      scrollOffset: scrollController.hasClients ? scrollController.offset : 0,
      local: local,
    );
  }

  void _onPointerDown(PointerDownEvent event) {
    if (event.kind != PointerDeviceKind.mouse && event.kind != PointerDeviceKind.stylus) {
      return;
    }
    final G4Position? pos = _positionForGlobal(event.position);
    if (pos == null) {
      return;
    }
    _focusNode.requestFocus();

    final DateTime now = DateTime.now();
    final bool sameSpot =
        _lastTapGlobal != null && (event.position - _lastTapGlobal!).distance < kDoubleTapSlop;
    final bool inTime = now.difference(_lastTapAt) < kDoubleTapTimeout;
    _tapCount = (sameSpot && inTime) ? _tapCount + 1 : 1;
    _lastTapGlobal = event.position;
    _lastTapAt = now;

    if (HardwareKeyboard.instance.isShiftPressed && _selection != null) {
      _dragAnchor = _selection!.base;
      _applySelection(G4Selection(base: _selection!.base, extent: pos));
      _dragging = true;
      return;
    }
    if (_tapCount == 2) {
      _dragging = false;
      _applySelection(document.wordAt(pos));
      return;
    }
    if (_tapCount >= 3) {
      _dragging = false;
      _applySelection(document.blockSelectionAt(pos));
      return;
    }

    _dragAnchor = pos;
    // Push the window once, for the press point, and then freeze it until
    // pointer-up (see `focusedBlock`).
    _applySelection(G4Selection.collapsed(pos));
    _dragging = true;
  }

  void _onPointerMove(PointerMoveEvent event) {
    if (!_dragging) {
      return;
    }
    final Offset? local = _toViewportLocal(event.position);
    if (local == null) {
      return;
    }
    _lastDragLocal = local;
    _extendDragTo(local);
    _updateAutoscroll(local);
  }

  void _extendDragTo(Offset local) {
    final G4Position? anchor = _dragAnchor;
    if (anchor == null) {
      return;
    }
    final G4Position pos = g4PositionForViewportOffset(
      document: document,
      scrollOffset: scrollController.hasClients ? scrollController.offset : 0,
      local: local,
    );
    _applySelection(G4Selection(base: anchor, extent: pos));
  }

  void _updateAutoscroll(Offset local) {
    final double delta = g4AutoscrollDelta(local, G4Layout.viewportHeight);
    if (delta == 0) {
      _autoscrollTimer?.cancel();
      _autoscrollTimer = null;
      return;
    }
    _autoscrollTimer ??= Timer.periodic(G4Layout.autoscrollTick, (_) {
      if (!mounted || !_dragging || !scrollController.hasClients) {
        return;
      }
      final double step = g4AutoscrollDelta(_lastDragLocal ?? local, G4Layout.viewportHeight);
      if (step == 0) {
        return;
      }
      final double next = (scrollController.offset + step).clamp(
        scrollController.position.minScrollExtent,
        scrollController.position.maxScrollExtent,
      );
      if (next != scrollController.offset) {
        scrollController.jumpTo(next);
      }
      final Offset? l = _lastDragLocal;
      if (l != null) {
        _extendDragTo(l);
      }
    });
  }

  void _onPointerUp(PointerUpEvent event) {
    _autoscrollTimer?.cancel();
    _autoscrollTimer = null;
    if (!_dragging) {
      return;
    }
    _dragging = false;
    _dragAnchor = null;
    _lastDragLocal = null;
    // The gesture is over: unfreeze and re-window on the caret.
    _syncImeToModel();
    setState(() {});
  }

  void _onPointerCancel(PointerCancelEvent event) {
    _autoscrollTimer?.cancel();
    _autoscrollTimer = null;
    _dragging = false;
    _dragAnchor = null;
  }

  // =====================================================================
  // Intents. Variant A OVERRIDES this list to stop EditableText's defaults
  // corrupting the document. Variant B IMPLEMENTS it, because nothing else
  // will. The list is the same size; the failure mode when an entry is
  // missing is not: A silently edits the wrong thing, B does nothing.
  // =====================================================================

  Map<Type, Action<Intent>> _buildActions() {
    Action<T> cb<T extends Intent>(void Function(T) run) =>
        CallbackAction<T>(onInvoke: (T intent) {
          run(intent);
          return null;
        });

    return <Type, Action<Intent>>{
      UndoTextIntent: cb<UndoTextIntent>((_) => _undo()),
      RedoTextIntent: cb<RedoTextIntent>((_) => _redo()),
      CopySelectionTextIntent: cb<CopySelectionTextIntent>((CopySelectionTextIntent i) {
        final String text = copySelection();
        if (i.collapseSelection && text.isNotEmpty) {
          _composing = null;
          _applySelection(G4Selection.collapsed(document.delete(_selection!)));
        }
      }),
      PasteTextIntent: cb<PasteTextIntent>((_) => unawaited(_paste())),
      SelectAllTextIntent: cb<SelectAllTextIntent>((_) => _applySelection(document.selectAll)),
      DeleteCharacterIntent: cb<DeleteCharacterIntent>((DeleteCharacterIntent i) {
        if (i.forward) {
          _deleteForward(document.positionAfter);
        } else {
          _deleteBackward(document.positionBefore);
        }
      }),
      DeleteToNextWordBoundaryIntent: cb<DeleteToNextWordBoundaryIntent>((
        DeleteToNextWordBoundaryIntent i,
      ) {
        if (i.forward) {
          _deleteForward(document.wordAfter);
        } else {
          _deleteBackward(document.wordBefore);
        }
      }),
      DeleteToLineBreakIntent: cb<DeleteToLineBreakIntent>((DeleteToLineBreakIntent i) {
        if (i.forward) {
          _deleteForward((G4Position p) => G4Position(p.block, document.blockLength(p.block)));
        } else {
          _deleteBackward((G4Position p) => G4Position(p.block, 0));
        }
      }),
      ExtendSelectionByCharacterIntent: cb<ExtendSelectionByCharacterIntent>(
        (ExtendSelectionByCharacterIntent i) => _moveOrExtend(
          i.forward ? document.positionAfter : document.positionBefore,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextWordBoundaryIntent: cb<ExtendSelectionToNextWordBoundaryIntent>(
        (ExtendSelectionToNextWordBoundaryIntent i) => _moveOrExtend(
          i.forward ? document.wordAfter : document.wordBefore,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextWordBoundaryOrCaretLocationIntent:
          cb<ExtendSelectionToNextWordBoundaryOrCaretLocationIntent>(
            (ExtendSelectionToNextWordBoundaryOrCaretLocationIntent i) => _moveOrExtend(
              i.forward ? document.wordAfter : document.wordBefore,
              collapse: false,
            ),
          ),
      ExtendSelectionToLineBreakIntent: cb<ExtendSelectionToLineBreakIntent>(
        (ExtendSelectionToLineBreakIntent i) => _moveOrExtend(
          (G4Position p) => i.forward
              ? G4Position(p.block, document.blockLength(p.block))
              : G4Position(p.block, 0),
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextParagraphBoundaryIntent:
          cb<ExtendSelectionToNextParagraphBoundaryIntent>(
            (ExtendSelectionToNextParagraphBoundaryIntent i) => _moveOrExtend(
              (G4Position p) => i.forward
                  ? G4Position(math.min(p.block + 1, document.blockCount - 1), 0)
                  : G4Position(math.max(p.block - 1, 0), 0),
              collapse: i.collapseSelection,
            ),
          ),
      ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent:
          cb<ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent>(
            (ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent i) => _moveOrExtend(
              (G4Position p) => i.forward
                  ? G4Position(math.min(p.block + 1, document.blockCount - 1), 0)
                  : G4Position(math.max(p.block - 1, 0), 0),
              collapse: false,
            ),
          ),
      ExtendSelectionToDocumentBoundaryIntent: cb<ExtendSelectionToDocumentBoundaryIntent>(
        (ExtendSelectionToDocumentBoundaryIntent i) => _moveOrExtend(
          (G4Position _) => i.forward ? document.documentEnd : document.documentStart,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionVerticallyToAdjacentLineIntent:
          cb<ExtendSelectionVerticallyToAdjacentLineIntent>(
            (ExtendSelectionVerticallyToAdjacentLineIntent i) =>
                _verticalMove(i.forward ? 1 : -1, collapse: i.collapseSelection),
          ),
      ExtendSelectionVerticallyToAdjacentPageIntent:
          cb<ExtendSelectionVerticallyToAdjacentPageIntent>(
            (ExtendSelectionVerticallyToAdjacentPageIntent i) =>
                _verticalMove(i.forward ? 8 : -8, collapse: i.collapseSelection),
          ),
      ExpandSelectionToLineBreakIntent: cb<ExpandSelectionToLineBreakIntent>(
        (ExpandSelectionToLineBreakIntent i) => _moveOrExtend(
          (G4Position p) => i.forward
              ? G4Position(p.block, document.blockLength(p.block))
              : G4Position(p.block, 0),
          collapse: false,
        ),
      ),
      ExpandSelectionToDocumentBoundaryIntent: cb<ExpandSelectionToDocumentBoundaryIntent>(
        (ExpandSelectionToDocumentBoundaryIntent i) => _moveOrExtend(
          (G4Position _) => i.forward ? document.documentEnd : document.documentStart,
          collapse: false,
        ),
      ),
      TransposeCharactersIntent: cb<TransposeCharactersIntent>((_) {
        final G4Selection? s = _selection;
        if (s == null || !s.isCollapsed || s.extent.offsetUtf16 < 2) {
          return;
        }
        final String b = document.blockAt(s.extent.block);
        final int o = s.extent.offsetUtf16;
        document.replaceRange(
          G4Selection(
            base: G4Position(s.extent.block, o - 2),
            extent: G4Position(s.extent.block, o),
          ),
          b.substring(o - 1, o) + b.substring(o - 2, o - 1),
        );
        _applySelection(s);
      }),
      ScrollToDocumentBoundaryIntent: cb<ScrollToDocumentBoundaryIntent>(
        (ScrollToDocumentBoundaryIntent i) {
          if (scrollController.hasClients) {
            scrollController.jumpTo(
              i.forward ? scrollController.position.maxScrollExtent : 0,
            );
          }
          _applySelection(
            G4Selection.collapsed(i.forward ? document.documentEnd : document.documentStart),
          );
        },
      ),
    };
  }

  // =====================================================================
  // Build
  // =====================================================================

  @override
  Widget build(BuildContext context) {
    return Actions(
      actions: _buildActions(),
      child: Focus(
        focusNode: _focusNode,
        child: Listener(
          onPointerDown: _onPointerDown,
          onPointerMove: _onPointerMove,
          onPointerUp: _onPointerUp,
          onPointerCancel: _onPointerCancel,
          behavior: HitTestBehavior.deferToChild,
          child: SizedBox(
            key: _viewportKey,
            width: G4Layout.viewportWidth,
            height: G4Layout.viewportHeight,
            // No Stack, no overlay, no hand-positioned island: every row is a
            // painted row and the input connection is not attached to any of
            // them. This is why nothing here depends on the caret block being
            // built, and why no layout oracle is needed.
            child: ListView.builder(
              controller: scrollController,
              itemExtent: G4Layout.itemExtent,
              itemCount: document.blockCount,
              itemBuilder: _buildRow,
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildRow(BuildContext context, int index) {
    final String text = document.blockAt(index);
    final G4Selection? sel = _selection;
    final ({int start, int end})? clip = sel?.normalized.clipToBlock(index, text.length);
    final bool showCaret =
        _caretOn && _focusNode.hasFocus && sel != null && sel.extent.block == index;
    return KeyedSubtree(
      key: g4BlockKey(index),
      child: G4PaintedBlock(
        text: text,
        selectionStart: clip?.start ?? 0,
        selectionEnd: clip?.end ?? 0,
        caretOffset: showCaret ? math.min(sel.extent.offsetUtf16, text.length) : null,
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// The IME window.
// ---------------------------------------------------------------------------

/// A bounded slice of the document handed to the platform IME, plus the
/// bijection between its UTF-16 offsets and model coordinates.
@immutable
class _ImeWindow {
  const _ImeWindow._({
    required this.startBlock,
    required this.endBlock,
    required this.anchorBlock,
    required this.clipped,
    required this.text,
    required this.blockStarts,
    required this.blockLengths,
  });

  /// First and last document block in the window, inclusive.
  final int startBlock;
  final int endBlock;

  /// The block the caret was on when the window was built. Frozen during a
  /// gesture; this is what `focusedBlock` reports.
  final int anchorBlock;

  /// True when the selection was too large to serialise and the window
  /// collapsed to [anchorBlock] alone.
  final bool clipped;

  final String text;
  final List<int> blockStarts;
  final List<int> blockLengths;

  int get prefixLength => kImePrefix.length;

  /// The window for [selection], capped at [maxChars].
  factory _ImeWindow.around(
    G4Document doc,
    G4Selection selection, {
    required int anchorBlock,
    required int maxChars,
  }) {
    final G4Selection n = doc.clampSelection(selection).normalized;
    final int anchor = anchorBlock.clamp(0, doc.blockCount - 1);
    int total = kImePrefix.length;
    for (int b = n.start.block; b <= n.end.block; b++) {
      total += doc.blockLength(b) + (b > n.start.block ? kG4BlockSeparator.length : 0);
    }
    if (total > maxChars) {
      return _ImeWindow.exact(doc, anchor, anchor, anchorBlock: anchor, clipped: true);
    }
    return _ImeWindow.exact(
      doc,
      n.start.block,
      n.end.block,
      anchorBlock: anchor,
      clipped: false,
    );
  }

  factory _ImeWindow.exact(
    G4Document doc,
    int first,
    int last, {
    required int anchorBlock,
    required bool clipped,
  }) {
    final int s = first.clamp(0, doc.blockCount - 1);
    final int e = last.clamp(s, doc.blockCount - 1);
    final StringBuffer sb = StringBuffer(kImePrefix);
    final List<int> starts = <int>[];
    final List<int> lengths = <int>[];
    int cursor = kImePrefix.length;
    for (int b = s; b <= e; b++) {
      if (b > s) {
        sb.write(kG4BlockSeparator);
        cursor += kG4BlockSeparator.length;
      }
      starts.add(cursor);
      lengths.add(doc.blockLength(b));
      sb.write(doc.blockAt(b));
      cursor += doc.blockLength(b);
    }
    return _ImeWindow._(
      startBlock: s,
      endBlock: e,
      anchorBlock: anchorBlock.clamp(s, e),
      clipped: clipped,
      text: sb.toString(),
      blockStarts: starts,
      blockLengths: lengths,
    );
  }

  /// Window offset -> model position. Offsets landing in the prefix or in a
  /// block separator clamp to the nearest real position.
  G4Position modelPositionFor(int imeOffset) {
    final int o = imeOffset.clamp(0, text.length);
    if (o <= prefixLength) {
      return G4Position(startBlock, 0);
    }
    for (int i = blockStarts.length - 1; i >= 0; i--) {
      if (o >= blockStarts[i]) {
        return G4Position(startBlock + i, math.min(o - blockStarts[i], blockLengths[i]));
      }
    }
    return G4Position(startBlock, 0);
  }

  /// Model position -> window offset. Positions before or after the window
  /// clamp to its edges, which is what makes a clipped window show the caret
  /// block's *share* of a larger document selection.
  int imeOffsetFor(G4Position p) {
    if (p.block < startBlock) {
      return prefixLength;
    }
    if (p.block > endBlock) {
      return text.length;
    }
    final int i = p.block - startBlock;
    return blockStarts[i] + p.offsetUtf16.clamp(0, blockLengths[i]);
  }

  /// A window range expressed back in model coordinates, or null if invalid.
  G4Selection? selectionFor(TextRange range) {
    if (!range.isValid) {
      return null;
    }
    if (range is TextSelection) {
      return G4Selection(
        base: modelPositionFor(range.baseOffset),
        extent: modelPositionFor(range.extentOffset),
      );
    }
    if (range.isCollapsed) {
      return null;
    }
    return G4Selection(
      base: modelPositionFor(range.start),
      extent: modelPositionFor(range.end),
    );
  }

  TextEditingValue valueFor(G4Selection? selection, G4Selection? composing) {
    final TextSelection sel = selection == null
        ? TextSelection.collapsed(offset: prefixLength)
        : TextSelection(
            baseOffset: imeOffsetFor(selection.base),
            extentOffset: imeOffsetFor(selection.extent),
          );
    final TextRange comp = composing == null
        ? TextRange.empty
        : TextRange(
            start: imeOffsetFor(composing.normalized.start),
            end: imeOffsetFor(composing.normalized.end),
          );
    return TextEditingValue(text: text, selection: sel, composing: comp);
  }
}
