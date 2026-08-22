// RFC 024 Gate G4 — Variant A: the editable island.
//
// The block containing the caret is a real `EditableText`. Every other visible
// block is painted. Selection lives in `G4Document` model coordinates, never in
// the editable.
//
// The bulk of this file is not the editor. It is the INTERCEPT SURFACE: the
// list of places `EditableText` will otherwise mutate its own controller and
// silently become the source of truth. Every one is tagged `INTERCEPT n` so the
// list can be counted. That list is the permanent maintenance obligation this
// variant carries.

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'g4_model.dart';
import 'g4_surface.dart';

/// Records every intercept that actually fired at runtime, so the suite can
/// assert the interception happened rather than assuming it.
class G4InterceptLog {
  final List<String> events = <String>[];
  void record(String name) => events.add(name);
  bool sawEvent(String name) => events.contains(name);
  void clear() => events.clear();
}

class G4VariantA extends G4Surface {
  const G4VariantA({
    super.key,
    required super.document,
    required super.scrollController,
    super.onSelectionChanged,
    this.interceptLog,
  });

  final G4InterceptLog? interceptLog;

  static G4Surface builder({
    required Key key,
    required G4Document document,
    required ScrollController scrollController,
  }) => G4VariantA(key: key, document: document, scrollController: scrollController);

  @override
  G4VariantAState createState() => G4VariantAState();
}

class G4VariantAState extends G4SurfaceState<G4VariantA> {
  final GlobalKey _viewportKey = GlobalKey();
  final FocusNode _focusNode = FocusNode(debugLabel: 'g4-variant-a-island');
  late final _G4AuthorityController _controller = _G4AuthorityController(this);

  G4Selection? _selection;
  int? _focusedBlock;

  // Drag state.
  bool _dragging = false;
  G4Position? _dragAnchor;
  Offset? _lastDragLocal;
  Timer? _autoscrollTimer;

  // Multi-tap state.
  int _tapCount = 0;
  Offset? _lastTapGlobal;
  DateTime _lastTapAt = DateTime.fromMillisecondsSinceEpoch(0);

  /// Set while we are pushing authoritative state into the editable, so the
  /// controller intercept knows the write is ours and not the platform's.
  bool _reflecting = false;

  G4InterceptLog get log => widget.interceptLog ?? _fallbackLog;
  final G4InterceptLog _fallbackLog = G4InterceptLog();

  @override
  void initState() {
    super.initState();
    document.addListener(_onDocumentChanged);
  }

  @override
  void dispose() {
    _autoscrollTimer?.cancel();
    document.removeListener(_onDocumentChanged);
    _focusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _onDocumentChanged() {
    if (mounted) {
      setState(() {});
    }
  }

  // =====================================================================
  // G4Surface contract
  // =====================================================================

  @override
  G4Selection? get selection => _selection;

  @override
  int? get focusedBlock => _focusedBlock;

  @override
  G4Selection? get composingRegion {
    final int? block = _focusedBlock;
    final TextRange composing = _controller.value.composing;
    if (block == null || !composing.isValid || composing.isCollapsed) {
      return null;
    }
    return G4Selection(
      base: G4Position(block, composing.start),
      extent: G4Position(block, composing.end),
    );
  }

  @override
  void setSelection(G4Selection? next) {
    _applySelection(next, moveFocus: !_dragging);
  }

  @override
  String copySelection() {
    final G4Selection? s = _selection;
    if (s == null || s.isCollapsed) {
      return '';
    }
    // Read from the model, never from rendered text. Blocks that were never
    // built are included because the model does not know about widgets.
    final String out = document.extractRange(s);
    unawaited(Clipboard.setData(ClipboardData(text: out)));
    return out;
  }

  @override
  void replaceSelection(String text) {
    final G4Selection s = _selection ?? G4Selection.collapsed(document.documentStart);
    final G4Position caret = document.replaceRange(s, text);
    _applySelection(G4Selection.collapsed(caret), moveFocus: true);
  }

  // =====================================================================
  // Selection application + editable reflection
  // =====================================================================

  void _applySelection(G4Selection? next, {required bool moveFocus}) {
    final G4Selection? clamped = next == null ? null : document.clampSelection(next);
    setState(() {
      _selection = clamped;
      if (moveFocus) {
        _focusedBlock = clamped?.extent.block;
      }
    });
    _reflectIntoEditable();
    widget.onSelectionChanged?.call(clamped);
  }

  /// Push the authoritative block text + the editable's *share* of the
  /// document selection into the island.
  void _reflectIntoEditable({TextRange composing = TextRange.empty}) {
    final int? block = _focusedBlock;
    if (block == null || block >= document.blockCount) {
      return;
    }
    final String text = document.blockAt(block);
    final G4Selection? s = _selection;
    TextSelection sel;
    if (s == null) {
      sel = const TextSelection.collapsed(offset: 0);
    } else {
      final ({int start, int end})? clip = s.normalized.clipToBlock(block, text.length);
      if (clip == null) {
        sel = TextSelection.collapsed(offset: text.length);
      } else if (s.base <= s.extent) {
        sel = TextSelection(baseOffset: clip.start, extentOffset: clip.end);
      } else {
        sel = TextSelection(baseOffset: clip.end, extentOffset: clip.start);
      }
    }
    _reflecting = true;
    _controller.reflect(TextEditingValue(text: text, selection: sel, composing: composing));
    _reflecting = false;

    if (!_focusNode.hasFocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _focusedBlock != null && !_focusNode.hasFocus) {
          _focusNode.requestFocus();
        }
      });
    }
  }

  // =====================================================================
  // INTERCEPT 1 — the controller value setter.
  //
  // This is the catch-all, and it is mandatory. `ReplaceTextIntent` and
  // `UpdateSelectionIntent` are the ONLY two entries in EditableText's action
  // map that are NOT wrapped in `Action.overridable` (editable_text.dart
  // lines 5598-5599), so an ancestor `Actions` widget can never override them.
  // Raw IME traffic (`updateEditingValue`) does not go through Actions at all.
  // Both land here.
  // =====================================================================

  void handleEditableWrite(TextEditingValue oldValue, TextEditingValue newValue) {
    final int? block = _focusedBlock;
    if (block == null) {
      return;
    }
    if (oldValue.text != newValue.text) {
      log.record('controller.text-write');
      _applyBlockLocalEdit(block, oldValue, newValue);
      return;
    }
    // Selection / composing only.
    log.record('controller.selection-write');
    _applyBlockLocalSelection(block, oldValue, newValue);
  }

  void _applyBlockLocalEdit(int block, TextEditingValue oldValue, TextEditingValue newValue) {
    final _BlockEdit edit = _BlockEdit.between(oldValue, newValue);

    // INTERCEPT 2 — cross-block selection replacement.
    // The island only ever sees its own block, so when a document selection
    // spans several blocks the editable believes it is replacing a range that
    // ends at its own boundary. Widen the edit back out to the real selection
    // before it reaches the document, or blocks 2..4 of a 2..5 selection would
    // silently survive.
    G4Selection target = G4Selection(
      base: G4Position(block, edit.start),
      extent: G4Position(block, edit.end),
    );
    final G4Selection? docSel = _selection?.normalized;
    if (docSel != null && docSel.isMultiBlock) {
      final ({int start, int end})? clip = docSel.clipToBlock(block, oldValue.text.length);
      if (clip != null && clip.start == edit.start && clip.end == edit.end) {
        log.record('widen-cross-block-replace');
        target = docSel;
      }
    }

    final G4Position caret = document.replaceRange(target, edit.inserted);
    final int newBlock = caret.block;

    // Composing offsets only survive if the resulting block's text is exactly
    // what the platform believes it is holding. The block INDEX is allowed to
    // change (a cross-block replace moves the island up the document), but its
    // content must match or the offsets no longer mean anything.
    final bool blockShapePreserved = document.blockAt(newBlock) == newValue.text;
    final TextRange composing = blockShapePreserved && newValue.composing.isValid
        ? newValue.composing
        : TextRange.empty;
    if (!blockShapePreserved && newValue.composing.isValid) {
      log.record('composing-dropped-on-reshape');
    }

    setState(() {
      _selection = G4Selection.collapsed(caret);
      _focusedBlock = newBlock;
    });
    _reflectIntoEditable(composing: composing);
    widget.onSelectionChanged?.call(_selection);
  }

  void _applyBlockLocalSelection(
    int block,
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    // INTERCEPT 3 — selection-only writes from the island.
    // Accept them when the document selection is confined to this block. Refuse
    // them when it is not: a multi-block selection's anchor lives in a block the
    // editable cannot see, and letting the editable "normalise" the selection
    // would destroy that anchor. (Observed: the editable clamps its selection
    // when its value is replaced.)
    final G4Selection? docSel = _selection;
    final bool composingOnly =
        oldValue.selection == newValue.selection && oldValue.composing != newValue.composing;
    if (composingOnly) {
      _reflectIntoEditable(composing: newValue.composing);
      return;
    }
    if (docSel != null && docSel.normalized.isMultiBlock) {
      log.record('refuse-selection-collapse-multiblock');
      _reflectIntoEditable(composing: newValue.composing);
      return;
    }
    if (!newValue.selection.isValid) {
      _reflectIntoEditable(composing: newValue.composing);
      return;
    }
    final G4Selection next = G4Selection(
      base: G4Position(block, newValue.selection.baseOffset),
      extent: G4Position(block, newValue.selection.extentOffset),
    );
    setState(() => _selection = document.clampSelection(next));
    _reflectIntoEditable(composing: newValue.composing);
    widget.onSelectionChanged?.call(_selection);
  }

  // =====================================================================
  // Document-level editing primitives, invoked by the intercepted Actions.
  // =====================================================================

  void _deleteBackward(G4Position Function(G4Position) boundary) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    if (!s.isCollapsed) {
      final G4Position caret = document.delete(s);
      _applySelection(G4Selection.collapsed(caret), moveFocus: true);
      return;
    }
    final G4Position from = boundary(s.extent);
    if (from == s.extent) {
      return;
    }
    final G4Position caret = document.delete(G4Selection(base: from, extent: s.extent));
    _applySelection(G4Selection.collapsed(caret), moveFocus: true);
  }

  void _deleteForward(G4Position Function(G4Position) boundary) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    if (!s.isCollapsed) {
      final G4Position caret = document.delete(s);
      _applySelection(G4Selection.collapsed(caret), moveFocus: true);
      return;
    }
    final G4Position to = boundary(s.extent);
    if (to == s.extent) {
      return;
    }
    final G4Position caret = document.delete(G4Selection(base: s.extent, extent: to));
    _applySelection(G4Selection.collapsed(caret), moveFocus: true);
  }

  void _moveOrExtend(G4Position Function(G4Position) boundary, {required bool collapse}) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    final G4Position next = boundary(s.extent);
    _applySelection(
      collapse ? G4Selection.collapsed(next) : G4Selection(base: s.base, extent: next),
      moveFocus: true,
    );
  }

  void _verticalMove(int lines, {required bool collapse}) {
    final G4Selection? s = _selection;
    if (s == null) {
      return;
    }
    // One block == one line in this fixture, which is enough to prove the
    // intent is intercepted and resolved in model coordinates.
    final int block = (s.extent.block + lines).clamp(0, document.blockCount - 1);
    final G4Position next = G4Position(
      block,
      math.min(s.extent.offsetUtf16, document.blockLength(block)),
    );
    _applySelection(
      collapse ? G4Selection.collapsed(next) : G4Selection(base: s.base, extent: next),
      moveFocus: true,
    );
  }

  Future<void> _paste() async {
    final ClipboardData? data = await Clipboard.getData(Clipboard.kTextPlain);
    final String text = data?.text ?? '';
    if (!mounted) {
      return;
    }
    replaceSelection(text);
  }

  void _undo() {
    final G4Selection? restored = document.undo();
    _applySelection(restored ?? _selection, moveFocus: true);
  }

  void _redo() {
    final G4Selection? restored = document.redo();
    _applySelection(restored ?? _selection, moveFocus: true);
  }

  // =====================================================================
  // Pointer handling. We own all of it; the renderer is told to ignore
  // pointers (INTERCEPT 12).
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
    // Touch drags belong to the scrollable (Flutter's default `dragDevices`
    // excludes the mouse, so there is no arena fight here). Touch selection on
    // a real device means handles + magnifier, which cannot be driven from a
    // widget test — see the report.
    if (event.kind != PointerDeviceKind.mouse && event.kind != PointerDeviceKind.stylus) {
      return;
    }
    final G4Position? pos = _positionForGlobal(event.position);
    if (pos == null) {
      return;
    }

    final DateTime now = DateTime.now();
    final bool sameSpot =
        _lastTapGlobal != null && (event.position - _lastTapGlobal!).distance < kDoubleTapSlop;
    final bool inTime = now.difference(_lastTapAt) < kDoubleTapTimeout;
    _tapCount = (sameSpot && inTime) ? _tapCount + 1 : 1;
    _lastTapGlobal = event.position;
    _lastTapAt = now;

    if (HardwareKeyboard.instance.isShiftPressed && _selection != null) {
      // Shift-click: keep the existing anchor, move the extent.
      _dragging = true;
      _dragAnchor = _selection!.base;
      _applySelection(G4Selection(base: _selection!.base, extent: pos), moveFocus: false);
      return;
    }

    if (_tapCount == 2) {
      _dragging = false;
      _applySelection(document.wordAt(pos), moveFocus: true);
      return;
    }
    if (_tapCount >= 3) {
      _dragging = false;
      _applySelection(document.blockSelectionAt(pos), moveFocus: true);
      return;
    }

    _dragging = true;
    _dragAnchor = pos;
    // Focus moves once, here, to the press point — and then not again until
    // pointer-up (INTERCEPT 13). Without the pin, every row the pointer crosses
    // would take focus, which rebuilds the island and restarts the platform
    // input connection dozens of times per drag.
    _applySelection(G4Selection.collapsed(pos), moveFocus: true);
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
    _applySelection(G4Selection(base: anchor, extent: pos), moveFocus: false);
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
    // Commit focus now that the gesture is over.
    setState(() => _focusedBlock = _selection?.extent.block);
    _reflectIntoEditable();
  }

  void _onPointerCancel(PointerCancelEvent event) {
    _autoscrollTimer?.cancel();
    _autoscrollTimer = null;
    _dragging = false;
    _dragAnchor = null;
  }

  // =====================================================================
  // The intercept map.
  // =====================================================================

  Map<Type, Action<Intent>> _buildActions() {
    Action<T> cb<T extends Intent>(String name, void Function(T) run) {
      return CallbackAction<T>(
        onInvoke: (T intent) {
          log.record(name);
          run(intent);
          return null;
        },
      );
    }

    return <Type, Action<Intent>>{
      // INTERCEPT 4 — undo. `UndoHistory` (an internal descendant of
      // EditableText) keeps its own stack of TextEditingValues for the focused
      // block only. Left alone it will restore stale text into a block whose
      // document may have changed underneath it, and it knows nothing about
      // the other 399 blocks.
      UndoTextIntent: cb<UndoTextIntent>('UndoTextIntent', (_) => _undo()),
      RedoTextIntent: cb<RedoTextIntent>('RedoTextIntent', (_) => _redo()),

      // INTERCEPT 5 — copy and cut. `CopySelectionTextIntent` carries
      // `collapseSelection: true` for cut. The default action copies the
      // *editable's* text, i.e. one block, and cut writes straight to the
      // controller.
      CopySelectionTextIntent: cb<CopySelectionTextIntent>('CopySelectionTextIntent', (
        CopySelectionTextIntent intent,
      ) {
        final String text = copySelection();
        if (intent.collapseSelection && text.isNotEmpty) {
          final G4Position caret = document.delete(_selection!);
          _applySelection(G4Selection.collapsed(caret), moveFocus: true);
        }
      }),

      // INTERCEPT 6 — paste. Default reads the clipboard and dispatches
      // ReplaceTextIntent (which is not overridable), so this must be stopped
      // one level up.
      PasteTextIntent: CallbackAction<PasteTextIntent>(
        onInvoke: (PasteTextIntent intent) {
          log.record('PasteTextIntent');
          unawaited(_paste());
          return null;
        },
      ),

      // INTERCEPT 7 — select all. Default selects the focused block only.
      SelectAllTextIntent: cb<SelectAllTextIntent>(
        'SelectAllTextIntent',
        (_) => _applySelection(document.selectAll, moveFocus: true),
      ),

      // INTERCEPT 8 — the three delete intents. Each one dispatches
      // ReplaceTextIntent internally; each must be caught here because deleting
      // at offset 0 of a block is a *block merge*, which the editable cannot
      // express at all.
      DeleteCharacterIntent: cb<DeleteCharacterIntent>('DeleteCharacterIntent', (
        DeleteCharacterIntent intent,
      ) {
        if (intent.forward) {
          _deleteForward(document.positionAfter);
        } else {
          _deleteBackward(document.positionBefore);
        }
      }),
      DeleteToNextWordBoundaryIntent: cb<DeleteToNextWordBoundaryIntent>(
        'DeleteToNextWordBoundaryIntent',
        (DeleteToNextWordBoundaryIntent intent) {
          if (intent.forward) {
            _deleteForward(document.wordAfter);
          } else {
            _deleteBackward(document.wordBefore);
          }
        },
      ),
      DeleteToLineBreakIntent: cb<DeleteToLineBreakIntent>('DeleteToLineBreakIntent', (
        DeleteToLineBreakIntent intent,
      ) {
        if (intent.forward) {
          _deleteForward((G4Position p) => G4Position(p.block, document.blockLength(p.block)));
        } else {
          _deleteBackward((G4Position p) => G4Position(p.block, 0));
        }
      }),

      // INTERCEPT 9 — every caret-motion and selection-extension intent.
      // Not optional: the default actions clamp to the focused block, so
      // shift-down-arrow at the last line of a block does nothing instead of
      // extending into the next block, and plain left-arrow at offset 0 stops
      // dead instead of crossing the boundary.
      ExtendSelectionByCharacterIntent: cb<ExtendSelectionByCharacterIntent>(
        'ExtendSelectionByCharacterIntent',
        (ExtendSelectionByCharacterIntent i) => _moveOrExtend(
          i.forward ? document.positionAfter : document.positionBefore,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextWordBoundaryIntent: cb<ExtendSelectionToNextWordBoundaryIntent>(
        'ExtendSelectionToNextWordBoundaryIntent',
        (ExtendSelectionToNextWordBoundaryIntent i) => _moveOrExtend(
          i.forward ? document.wordAfter : document.wordBefore,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextWordBoundaryOrCaretLocationIntent:
          cb<ExtendSelectionToNextWordBoundaryOrCaretLocationIntent>(
            'ExtendSelectionToNextWordBoundaryOrCaretLocationIntent',
            (ExtendSelectionToNextWordBoundaryOrCaretLocationIntent i) => _moveOrExtend(
              i.forward ? document.wordAfter : document.wordBefore,
              collapse: false,
            ),
          ),
      ExtendSelectionToLineBreakIntent: cb<ExtendSelectionToLineBreakIntent>(
        'ExtendSelectionToLineBreakIntent',
        (ExtendSelectionToLineBreakIntent i) => _moveOrExtend(
          (G4Position p) =>
              i.forward ? G4Position(p.block, document.blockLength(p.block)) : G4Position(p.block, 0),
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextParagraphBoundaryIntent: cb<ExtendSelectionToNextParagraphBoundaryIntent>(
        'ExtendSelectionToNextParagraphBoundaryIntent',
        (ExtendSelectionToNextParagraphBoundaryIntent i) => _moveOrExtend(
          (G4Position p) => i.forward
              ? G4Position(math.min(p.block + 1, document.blockCount - 1), 0)
              : G4Position(math.max(p.block - 1, 0), 0),
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent:
          cb<ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent>(
            'ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent',
            (ExtendSelectionToNextParagraphBoundaryOrCaretLocationIntent i) => _moveOrExtend(
              (G4Position p) => i.forward
                  ? G4Position(math.min(p.block + 1, document.blockCount - 1), 0)
                  : G4Position(math.max(p.block - 1, 0), 0),
              collapse: false,
            ),
          ),
      ExtendSelectionToDocumentBoundaryIntent: cb<ExtendSelectionToDocumentBoundaryIntent>(
        'ExtendSelectionToDocumentBoundaryIntent',
        (ExtendSelectionToDocumentBoundaryIntent i) => _moveOrExtend(
          (G4Position _) => i.forward ? document.documentEnd : document.documentStart,
          collapse: i.collapseSelection,
        ),
      ),
      ExtendSelectionVerticallyToAdjacentLineIntent:
          cb<ExtendSelectionVerticallyToAdjacentLineIntent>(
            'ExtendSelectionVerticallyToAdjacentLineIntent',
            (ExtendSelectionVerticallyToAdjacentLineIntent i) =>
                _verticalMove(i.forward ? 1 : -1, collapse: i.collapseSelection),
          ),
      ExtendSelectionVerticallyToAdjacentPageIntent:
          cb<ExtendSelectionVerticallyToAdjacentPageIntent>(
            'ExtendSelectionVerticallyToAdjacentPageIntent',
            (ExtendSelectionVerticallyToAdjacentPageIntent i) =>
                _verticalMove(i.forward ? 8 : -8, collapse: i.collapseSelection),
          ),
      ExpandSelectionToLineBreakIntent: cb<ExpandSelectionToLineBreakIntent>(
        'ExpandSelectionToLineBreakIntent',
        (ExpandSelectionToLineBreakIntent i) => _moveOrExtend(
          (G4Position p) =>
              i.forward ? G4Position(p.block, document.blockLength(p.block)) : G4Position(p.block, 0),
          collapse: false,
        ),
      ),
      ExpandSelectionToDocumentBoundaryIntent: cb<ExpandSelectionToDocumentBoundaryIntent>(
        'ExpandSelectionToDocumentBoundaryIntent',
        (ExpandSelectionToDocumentBoundaryIntent i) => _moveOrExtend(
          (G4Position _) => i.forward ? document.documentEnd : document.documentStart,
          collapse: false,
        ),
      ),

      // INTERCEPT 10 — transpose (Ctrl+T on macOS). Writes to the controller.
      TransposeCharactersIntent: cb<TransposeCharactersIntent>('TransposeCharactersIntent', (_) {
        final G4Selection? s = _selection;
        if (s == null || !s.isCollapsed || s.extent.offsetUtf16 < 2) {
          return;
        }
        final String b = document.blockAt(s.extent.block);
        final int o = s.extent.offsetUtf16;
        final String swapped = b.substring(o - 2, o - 1);
        document.replaceRange(
          G4Selection(
            base: G4Position(s.extent.block, o - 2),
            extent: G4Position(s.extent.block, o),
          ),
          b.substring(o - 1, o) + swapped,
        );
        _applySelection(s, moveFocus: true);
      }),

      // INTERCEPT 11 — tap-outside focus theft. The default actions unfocus the
      // editable when a pointer lands outside its tap region. Every drag that
      // leaves the focused row is such a pointer, so mid-drag the island would
      // lose focus and drop the IME connection.
      EditableTextTapOutsideIntent: cb<EditableTextTapOutsideIntent>(
        'EditableTextTapOutsideIntent',
        (_) {},
      ),
      EditableTextTapUpOutsideIntent: cb<EditableTextTapUpOutsideIntent>(
        'EditableTextTapUpOutsideIntent',
        (_) {},
      ),

      // INTERCEPT 14 — scroll-to-document-boundary (Cmd+Home/End). The default
      // scrolls the *editable's* internal Scrollable, not our list.
      ScrollToDocumentBoundaryIntent: cb<ScrollToDocumentBoundaryIntent>(
        'ScrollToDocumentBoundaryIntent',
        (ScrollToDocumentBoundaryIntent i) {
          if (!scrollController.hasClients) {
            return;
          }
          scrollController.jumpTo(
            i.forward ? scrollController.position.maxScrollExtent : 0,
          );
          _applySelection(
            G4Selection.collapsed(i.forward ? document.documentEnd : document.documentStart),
            moveFocus: true,
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
      // INTERCEPT 15 — one tap region for the whole surface, so that a pointer
      // landing on a *painted* block is not "outside" the island.
      child: TextFieldTapRegion(
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
            // INTERCEPT 16 — the island cannot be a child of the list.
            //
            // The obvious construction (build an EditableText for the focused
            // index inside `itemBuilder`) does not survive virtualization, and
            // `AutomaticKeepAliveClientMixin` is not enough to save it:
            //   * a kept-alive child is still rebuilt by the sliver on every
            //     setState, so the moment `focusedBlock` moves the old row is
            //     rebuilt as painted and its EditableText is disposed;
            //   * and the NEW focused index is not in the sliver's child map at
            //     all when it is outside the viewport, so no editable is built
            //     for it. The surface ends up with no input connection.
            // Both were observed. The fix is to host the island in a Stack
            // above the list and position it from the scroll offset by hand —
            // which means owning a layout oracle that can answer "where is
            // block N" for blocks the list has never laid out.
            child: Stack(
              clipBehavior: Clip.hardEdge,
              children: <Widget>[
                ListView.builder(
                  controller: scrollController,
                  itemExtent: G4Layout.itemExtent,
                  itemCount: document.blockCount,
                  itemBuilder: _buildRow,
                ),
                if (_focusedBlock != null)
                  AnimatedBuilder(
                    animation: scrollController,
                    builder: (BuildContext context, Widget? child) {
                      final double top =
                          (_focusedBlock ?? 0) * G4Layout.itemExtent -
                          (scrollController.hasClients ? scrollController.offset : 0);
                      return Positioned(
                        left: 0,
                        top: top,
                        width: G4Layout.viewportWidth,
                        height: G4Layout.itemExtent,
                        child: child!,
                      );
                    },
                    child: _G4EditableIsland(
                      // Stable key: the island must NOT be recreated when the
                      // focused block index changes, or the platform input
                      // connection restarts and any live IME composition dies.
                      key: const ValueKey<String>('g4-island'),
                      controller: _controller,
                      focusNode: _focusNode,
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildRow(BuildContext context, int index) {
    final String text = document.blockAt(index);
    final ({int start, int end})? clip = _selection?.normalized.clipToBlock(index, text.length);

    // The focused row is drawn by the island in the Stack above, so the list
    // slot for it is empty. It still carries the block key so hit-testing and
    // the acceptance suite see a consistent geometry for every row.
    return KeyedSubtree(
      key: g4BlockKey(index),
      child: index == _focusedBlock
          ? const SizedBox.expand()
          : G4PaintedBlock(
              text: text,
              selectionStart: clip?.start ?? 0,
              selectionEnd: clip?.end ?? 0,
            ),
    );
  }
}

// ---------------------------------------------------------------------------
// The island itself.
// ---------------------------------------------------------------------------

class _G4EditableIsland extends StatefulWidget {
  const _G4EditableIsland({super.key, required this.controller, required this.focusNode});

  final TextEditingController controller;
  final FocusNode focusNode;

  @override
  State<_G4EditableIsland> createState() => _G4EditableIslandState();
}

class _G4EditableIslandState extends State<_G4EditableIsland> {
  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: G4Layout.padding,
      child: EditableText(
        controller: widget.controller,
        focusNode: widget.focusNode,
        style: G4Layout.textStyle,
        cursorColor: G4Layout.cursorColor,
        backgroundCursorColor: const Color(0xFFBBBBBB),
        selectionColor: G4Layout.selectionColor,
        maxLines: null,
        // INTERCEPT 12 — RenderEditable ships its own TapGestureRecognizer and
        // LongPressGestureRecognizer (rendering/editable.dart:1646-1649) which
        // set `selection` directly on the render object. They must be switched
        // off or every tap races our model-level gesture handling.
        rendererIgnoresPointer: true,
        // INTERCEPT 17 — the selection toolbar. `TextSelectionToolbarAnchors`
        // handlers call `delegate.cutSelection/copySelection/pasteText/selectAll`
        // as DIRECT METHOD CALLS on EditableTextState
        // (widgets/text_selection.dart:223-267). They do not go through Actions
        // and therefore cannot be overridden. The only defence is to never let
        // the toolbar exist.
        contextMenuBuilder: null,
        // INTERCEPT 18 — selection handles. `TextSelectionControls` handle drags
        // also write selection directly onto the editable.
        selectionControls: null,
        // INTERCEPT 19 — stylus handwriting / Scribble. `TextInputClient`
        // scribble callbacks mutate the controller outside updateEditingValue.
        stylusHandwritingEnabled: false,
        // Deliberately left ON: this is what Variant A is supposed to buy.
        autocorrect: true,
        enableSuggestions: true,
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// INTERCEPT 1's implementation: a controller that refuses to be the source of
// truth.
// ---------------------------------------------------------------------------

class _G4AuthorityController extends TextEditingController {
  _G4AuthorityController(this._host);

  final G4VariantAState _host;

  /// Authoritative push from the model. The only write that is allowed to
  /// land unmodified.
  void reflect(TextEditingValue authoritative) {
    if (super.value == authoritative) {
      return;
    }
    super.value = authoritative;
  }

  @override
  set value(TextEditingValue newValue) {
    if (_host._reflecting) {
      super.value = newValue;
      return;
    }
    final TextEditingValue old = super.value;
    if (old == newValue) {
      return;
    }
    // Do NOT apply. Hand it to the document and let the model push back the
    // authoritative value. If we applied first, the controller would be the
    // source of truth for one frame — which is exactly the failure mode.
    _host.handleEditableWrite(old, newValue);
  }
}

/// A block-local text edit recovered from two consecutive editing values.
@immutable
class _BlockEdit {
  const _BlockEdit(this.start, this.end, this.inserted);

  /// Range in the OLD text that was replaced.
  final int start;
  final int end;
  final String inserted;

  /// Recover the edit. Prefer the old value's selection when it is a real
  /// range: that is what the platform believed it was replacing, and a plain
  /// prefix/suffix diff is ambiguous when the replacement shares characters
  /// with the replaced text.
  factory _BlockEdit.between(TextEditingValue oldValue, TextEditingValue newValue) {
    final String a = oldValue.text;
    final String b = newValue.text;

    if (oldValue.selection.isValid && !oldValue.selection.isCollapsed) {
      final int s = oldValue.selection.start;
      final int e = oldValue.selection.end;
      final int insertedLen = b.length - s - (a.length - e);
      if (insertedLen >= 0 &&
          s <= b.length &&
          s + insertedLen <= b.length &&
          a.substring(0, s) == b.substring(0, s) &&
          a.substring(e) == b.substring(s + insertedLen)) {
        return _BlockEdit(s, e, b.substring(s, s + insertedLen));
      }
    }

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
    return _BlockEdit(p, sa, b.substring(p, sb));
  }
}
