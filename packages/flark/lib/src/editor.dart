import 'dart:async';
import 'dart:math' as math;

import 'package:flark_core/flark_core.dart' show FlarkViewportRow;
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart' as material;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';
import 'render_surface.dart';

const _maximumQueuedVerticalMoves = 32;

final class _SecondaryTapGestureRecognizer extends TapGestureRecognizer {
  _SecondaryTapGestureRecognizer({super.debugOwner})
    : super(supportedDevices: const {PointerDeviceKind.mouse});
}

/// App-relative geometry exposed only to integration harnesses.
final class FlarkEditorDebugGeometry {
  const FlarkEditorDebugGeometry({
    required this.globalPosition,
    required this.rootLogicalSize,
  });

  final Offset globalPosition;
  final Size rootLogicalSize;
}

/// Opt-in bridge from source offsets to the currently painted Flutter surface.
///
/// Product input never depends on this handle. Native integration runners use
/// it to drive real pointer events without freezing tests to pixel guesses.
final class FlarkEditorDebugHandle {
  RenderFlarkSurface? _surface;

  FlarkEditorDebugGeometry? geometryForSourceUtf16(int offset) {
    final surface = _surface;
    if (surface == null || !surface.attached || !surface.hasSize) return null;
    final local = surface.debugLocalPositionForSourceUtf16(offset);
    if (local == null) return null;
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    return FlarkEditorDebugGeometry(
      globalPosition: surface.localToGlobal(local),
      rootLogicalSize: view.physicalSize / view.devicePixelRatio,
    );
  }

  FlarkEditorDebugGeometry? geometryForTaskCheckboxOrdinal(int ordinal) {
    final surface = _surface;
    if (surface == null || !surface.attached || !surface.hasSize) return null;
    final local = surface.debugLocalPositionForTaskCheckbox(ordinal);
    if (local == null) return null;
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    return FlarkEditorDebugGeometry(
      globalPosition: surface.localToGlobal(local),
      rootLogicalSize: view.physicalSize / view.devicePixelRatio,
    );
  }

  void _attach(RenderFlarkSurface? surface) => _surface = surface;

  void _detach(RenderFlarkSurface? surface) {
    if (identical(_surface, surface)) _surface = null;
  }
}

/// A real custom Flutter render surface backed by [FlarkEditorController].
///
/// It intentionally does not compose TextField, EditableText, ListView, or a
/// widget per Markdown block. Text input uses Flutter's delta model and a
/// bounded active window; painting visits only rows that fit this RenderBox.
final class FlarkEditor extends StatefulWidget {
  const FlarkEditor({
    required this.controller,
    this.autofocus = false,
    this.focusNode,
    this.textStyle = const TextStyle(
      color: Color(0xff202124),
      fontSize: 17,
      height: 1.45,
    ),
    this.padding = const EdgeInsets.symmetric(horizontal: 32, vertical: 28),
    this.caretColor = const Color(0xff246bfd),
    this.selectionColor = const Color(0x40246bfd),
    this.contentInsertionConfiguration,
    this.onAppPrivateCommand,
    this.enableTextDrop = true,
    this.debugInputEventObserver,
    this.debugPaintObserver,
    this.debugHandle,
    super.key,
  });

  final FlarkEditorController controller;
  final bool autofocus;
  final FocusNode? focusNode;
  final TextStyle textStyle;
  final EdgeInsets padding;
  final Color caretColor;
  final Color selectionColor;
  final ContentInsertionConfiguration? contentInsertionConfiguration;
  final AppPrivateCommandCallback? onAppPrivateCommand;
  final bool enableTextDrop;

  /// Opt-in adapter trace used by native scenario runners. It is never called
  /// unless supplied and does not participate in editing behavior.
  final ValueChanged<String>? debugInputEventObserver;

  /// Opt-in observations and geometry used only by integration harnesses.
  final ValueChanged<FlarkSurfacePaintObservation>? debugPaintObserver;
  final FlarkEditorDebugHandle? debugHandle;

  @override
  State<FlarkEditor> createState() => _FlarkEditorState();
}

final class _FlarkEditorState extends State<FlarkEditor>
    with DeltaTextInputClient {
  final GlobalKey _surfaceKey = GlobalKey();
  FocusNode? _ownedFocusNode;
  TextInputConnection? _connection;
  TextEditingValue? _lastKnownPlatformValue;
  bool _platformNewlineObservationAwaitingAction = false;
  FlarkSurfaceHit? _pendingTapHit;
  double? _preferredVerticalNavigationX;
  bool _verticalPageNavigationPending = false;
  bool _verticalMoveDrainScheduled = false;
  final List<({bool forward, bool modify})> _queuedVerticalMoves = [];
  final ContextMenuController _contextMenuController = ContextMenuController();

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
    _focusNode.addListener(_focusChanged);
    widget.controller.addListener(_controllerChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.controller.continueParsing();
    });
    if (widget.autofocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _focusNode.requestFocus();
      });
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _attachDebugHandle());
  }

  @override
  void didUpdateWidget(FlarkEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_controllerChanged);
      widget.controller.addListener(_controllerChanged);
      _lastKnownPlatformValue = null;
      _platformNewlineObservationAwaitingAction = false;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) widget.controller.continueParsing();
      });
    }
    if (oldWidget.focusNode != widget.focusNode) {
      (oldWidget.focusNode ?? _ownedFocusNode)?.removeListener(_focusChanged);
      _ownedFocusNode?.dispose();
      _ownedFocusNode = widget.focusNode == null ? FocusNode() : null;
      _focusNode.addListener(_focusChanged);
    }
    if (!identical(oldWidget.debugHandle, widget.debugHandle)) {
      oldWidget.debugHandle?._detach(_surface);
      WidgetsBinding.instance.addPostFrameCallback((_) => _attachDebugHandle());
    }
    if (!listEquals(
          oldWidget.contentInsertionConfiguration?.allowedMimeTypes,
          widget.contentInsertionConfiguration?.allowedMimeTypes,
        ) &&
        _focusNode.hasFocus) {
      _connection?.close();
      _connection = null;
      _lastKnownPlatformValue = null;
      _platformNewlineObservationAwaitingAction = false;
      _openConnection();
    }
  }

  @override
  void dispose() {
    widget.debugHandle?._detach(_surface);
    widget.controller.removeListener(_controllerChanged);
    _focusNode.removeListener(_focusChanged);
    widget.controller.commitActiveComposition();
    _contextMenuController.remove();
    _connection?.close();
    _ownedFocusNode?.dispose();
    super.dispose();
  }

  void _focusChanged() {
    if (_focusNode.hasFocus) {
      _openConnection();
    } else {
      widget.controller.commitActiveComposition();
      _connection?.close();
      _connection = null;
      _lastKnownPlatformValue = null;
      _platformNewlineObservationAwaitingAction = false;
    }
    setState(() {});
  }

  void _openConnection() {
    if (_connection?.attached ?? false) {
      // A platform keyboard can be dismissed while Flutter keeps the input
      // connection attached. A subsequent user activation must ask the
      // platform to show it again instead of treating the connection itself as
      // proof that the keyboard is visible.
      _connection!.show();
      return;
    }
    _connection = TextInput.attach(
      this,
      TextInputConfiguration(
        inputType: TextInputType.multiline,
        inputAction: TextInputAction.newline,
        autocorrect: true,
        enableSuggestions: true,
        enableDeltaModel: true,
        allowedMimeTypes:
            widget.contentInsertionConfiguration?.allowedMimeTypes ?? const [],
      ),
    );
    _sendEditingState(force: true);
    _connection!.show();
  }

  void _controllerChanged() => _sendEditingState();

  void _sendEditingState({bool force = false}) {
    final connection = _connection;
    if (connection == null || !connection.attached) return;
    final value = widget.controller.inputValue;
    if (!force && value == _lastKnownPlatformValue) return;
    connection.setEditingState(value);
    _lastKnownPlatformValue = value;
  }

  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;

  void _attachDebugHandle() {
    if (mounted) widget.debugHandle?._attach(_surface);
  }

  void _activate(Offset localPosition, {bool extend = false}) {
    final hit = _surface?.positionForOffset(localPosition);
    if (hit == null) return;
    _activateHit(hit, extend: extend);
  }

  void _activateHit(FlarkSurfaceHit hit, {bool extend = false}) {
    _preferredVerticalNavigationX = null;
    if (extend) {
      widget.controller.extendSelectionTo(
        hit.globalUtf16Offset,
        activeOrdinal: hit.row?.ordinal ?? -hit.ordinal - 1,
      );
    } else if (hit.row case final row?) {
      widget.controller.activateRow(
        row,
        hit.globalUtf16Offset,
        affinity: hit.affinity,
      );
    } else {
      widget.controller.activateNeutralLine(
        text: hit.neutralText ?? '',
        globalUtf16Start: hit.neutralUtf16Start ?? 0,
        globalUtf16Offset: hit.globalUtf16Offset,
        ordinal: hit.ordinal,
        affinity: hit.affinity,
      );
    }
    _focusNode.requestFocus();
    _openConnection();
    _sendEditingState(force: true);
  }

  void _handleTapDown(TapDownDetails details) {
    final touchLike = switch (details.kind) {
      PointerDeviceKind.touch ||
      PointerDeviceKind.stylus ||
      PointerDeviceKind.invertedStylus => true,
      _ => false,
    };
    if (touchLike && _focusNode.hasFocus && (_connection?.attached ?? false)) {
      // Android can hide the IME without closing Flutter's input connection.
      // Treat a fresh touch activation as an explicit request to bring it
      // back, even before the gesture resolves to a caret position.
      _connection!.show();
    }
    _pendingTapHit = _surface?.positionForOffset(
      details.localPosition,
      minimumActionExtent: touchLike ? 48 : 24,
    );
  }

  void _adoptNavigationHit(FlarkSurfaceHit hit, {required bool modify}) {
    if (!modify) {
      _activateHit(hit);
      return;
    }
    widget.controller.extendSelectionTo(
      hit.globalUtf16Offset,
      activeOrdinal: hit.row?.ordinal ?? -hit.ordinal - 1,
    );
    _focusNode.requestFocus();
    _openConnection();
    _sendEditingState(force: true);
  }

  void _setSemanticsSelection(
    FlarkViewportRow row,
    int baseUtf16,
    int extentUtf16,
  ) {
    widget.controller.activateRow(row, baseUtf16, selectionExtent: extentUtf16);
    _focusNode.requestFocus();
    _openConnection();
    _sendEditingState(force: true);
  }

  void _moveSemanticsCursor({
    required bool forward,
    required bool byWord,
    required bool extendSelection,
  }) {
    if (byWord) {
      _moveWord(forward: forward, modify: extendSelection);
    } else {
      _moveCharacter(forward: forward, modify: extendSelection);
    }
  }

  void _moveCharacter({required bool forward, required bool modify}) {
    final surface = _surface;
    if (surface == null) return;
    final controller = widget.controller;
    final base = controller.globalSelectionBase;
    final extent = controller.globalSelectionExtent;
    _preferredVerticalNavigationX = null;
    if (!modify && base != extent) {
      final boundary = forward
          ? math.max(base, extent)
          : math.min(base, extent);
      final hit = surface.hitForSourceUtf16(
        boundary,
        affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
      );
      if (hit != null) _adoptNavigationHit(hit, modify: false);
      return;
    }
    final hit = surface.adjacentCharacterHit(extent, forward: forward);
    if (hit != null) _adoptNavigationHit(hit, modify: modify);
  }

  void _moveVertically({
    required bool forward,
    required bool modify,
    bool fromQueue = false,
  }) {
    if (!fromQueue &&
        (_verticalPageNavigationPending ||
            _verticalMoveDrainScheduled ||
            _queuedVerticalMoves.isNotEmpty)) {
      _queueVerticalMove(forward: forward, modify: modify);
      return;
    }
    final surface = _surface;
    if (surface == null) return;
    final controller = widget.controller;
    final extent = controller.globalSelectionExtent;
    _preferredVerticalNavigationX ??= surface.localXForSourceUtf16(extent);
    final preferredX = _preferredVerticalNavigationX;
    final hit = surface.verticalHit(
      extent,
      forward: forward,
      preferredX: preferredX,
    );
    if (hit != null) {
      surface.ensureSourceUtf16Visible(hit.globalUtf16Offset);
      _adoptNavigationHit(hit, modify: modify);
      _preferredVerticalNavigationX = preferredX;
      return;
    }
    if (preferredX == null ||
        !surface.isAtViewportPageEdge(extent, forward: forward) ||
        (forward ? !controller.canPageForward : !controller.canPageBackward)) {
      return;
    }
    _verticalPageNavigationPending = true;
    unawaited(
      _continueVerticalNavigationAcrossPage(
        controller: controller,
        forward: forward,
        modify: modify,
        preferredX: preferredX,
        selectionBase: controller.globalSelectionBase,
        selectionExtent: extent,
      ),
    );
  }

  void _queueVerticalMove({required bool forward, required bool modify}) {
    if (_queuedVerticalMoves.length >= _maximumQueuedVerticalMoves) return;
    _queuedVerticalMoves.add((forward: forward, modify: modify));
  }

  void _clearQueuedVerticalMoves() {
    _queuedVerticalMoves.clear();
  }

  void _scheduleQueuedVerticalMove() {
    if (!mounted ||
        _verticalPageNavigationPending ||
        _verticalMoveDrainScheduled ||
        _queuedVerticalMoves.isEmpty) {
      return;
    }
    final controller = widget.controller;
    final expectedBase = controller.globalSelectionBase;
    final expectedExtent = controller.globalSelectionExtent;
    _verticalMoveDrainScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _verticalMoveDrainScheduled = false;
      if (!mounted ||
          !identical(controller, widget.controller) ||
          controller.globalSelectionBase != expectedBase ||
          controller.globalSelectionExtent != expectedExtent ||
          _queuedVerticalMoves.isEmpty) {
        _clearQueuedVerticalMoves();
        return;
      }
      final move = _queuedVerticalMoves.removeAt(0);
      _moveVertically(
        forward: move.forward,
        modify: move.modify,
        fromQueue: true,
      );
      _scheduleQueuedVerticalMove();
    });
  }

  Future<void> _continueVerticalNavigationAcrossPage({
    required FlarkEditorController controller,
    required bool forward,
    required bool modify,
    required double preferredX,
    required int selectionBase,
    required int selectionExtent,
  }) async {
    final moved = forward
        ? await controller.nextViewportPage()
        : await controller.previousViewportPage();
    final selectionChanged =
        controller.globalSelectionBase != selectionBase ||
        controller.globalSelectionExtent != selectionExtent;
    if (moved &&
        selectionChanged &&
        mounted &&
        identical(controller, widget.controller)) {
      await _restorePageAfterStaleVerticalNavigation(
        controller: controller,
        forward: forward,
      );
    }
    if (!moved ||
        !mounted ||
        !identical(controller, widget.controller) ||
        selectionChanged) {
      _verticalPageNavigationPending = false;
      _clearQueuedVerticalMoves();
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          !identical(controller, widget.controller) ||
          controller.globalSelectionBase != selectionBase ||
          controller.globalSelectionExtent != selectionExtent) {
        _clearQueuedVerticalMoves();
        if (mounted && identical(controller, widget.controller)) {
          unawaited(
            _restorePageAfterStaleVerticalNavigation(
              controller: controller,
              forward: forward,
            ).whenComplete(() => _verticalPageNavigationPending = false),
          );
        } else {
          _verticalPageNavigationPending = false;
        }
        return;
      }
      _verticalPageNavigationPending = false;
      final surface = _surface;
      final hit = surface?.verticalPageEdgeHit(
        forward: forward,
        preferredX: preferredX,
      );
      if (hit == null) {
        _clearQueuedVerticalMoves();
        return;
      }
      surface?.ensureSourceUtf16Visible(hit.globalUtf16Offset);
      _adoptNavigationHit(hit, modify: modify);
      _preferredVerticalNavigationX = preferredX;
      _scheduleQueuedVerticalMove();
    });
  }

  Future<void> _restorePageAfterStaleVerticalNavigation({
    required FlarkEditorController controller,
    required bool forward,
  }) async {
    if (forward) {
      await controller.previousViewportPage();
    } else {
      await controller.nextViewportPage();
    }
  }

  Future<void> _selectAll() async {
    _preferredVerticalNavigationX = null;
    final controller = widget.controller;
    await controller.selectOversizedRangeUtf16(0, controller.sourceUtf16Length);
    if (!mounted || !identical(controller, widget.controller)) return;
    _sendEditingState(force: true);
    widget.debugInputEventObserver?.call(
      'completed-select-all:generation=${controller.sourceGeneration}',
    );
  }

  void _moveToLineBoundary({required bool forward, required bool modify}) {
    _preferredVerticalNavigationX = null;
    final hit = _surface?.lineBoundaryHit(
      widget.controller.globalSelectionExtent,
      forward: forward,
    );
    if (hit != null) _adoptNavigationHit(hit, modify: modify);
  }

  void _moveToParagraphBoundary({required bool forward, required bool modify}) {
    _preferredVerticalNavigationX = null;
    final hit = _surface?.paragraphBoundaryHit(
      widget.controller.globalSelectionExtent,
      forward: forward,
    );
    if (hit != null) _adoptNavigationHit(hit, modify: modify);
  }

  void _moveWord({required bool forward, required bool modify}) {
    _preferredVerticalNavigationX = null;
    final hit = _surface?.wordBoundaryHit(
      widget.controller.globalSelectionExtent,
      forward: forward,
    );
    if (hit != null) _adoptNavigationHit(hit, modify: modify);
  }

  bool _moveTableCell({required bool forward}) {
    final surface = _surface;
    if (surface == null) return false;
    final offset = widget.controller.globalSelectionExtent;
    if (!surface.isTableCellPosition(offset)) return false;
    _preferredVerticalNavigationX = null;
    final hit = surface.adjacentTableCellHit(offset, forward: forward);
    if (hit != null) _adoptNavigationHit(hit, modify: false);
    return true;
  }

  bool _handleTab({required bool reverse}) {
    if (widget.controller.pendingTableNavigationLocked) {
      widget.debugInputEventObserver?.call('shortcut:pending-table-cell');
      return true;
    }
    if (widget.controller.handleListIndent(outdent: reverse)) {
      widget.debugInputEventObserver?.call(
        reverse ? 'shortcut:outdent-list' : 'shortcut:indent-list',
      );
      return true;
    }
    if (_moveTableCell(forward: !reverse)) {
      widget.debugInputEventObserver?.call(
        reverse ? 'shortcut:previous-table-cell' : 'shortcut:next-table-cell',
      );
      return true;
    }
    return false;
  }

  void _handleTap() {
    final hit = _pendingTapHit;
    _pendingTapHit = null;
    if (hit == null) return;
    if (hit.action == null) {
      _activateHit(hit);
      return;
    }
    if (hit.action != FlarkSurfaceAction.toggleTaskChecked || hit.row == null) {
      return;
    }
    _focusNode.requestFocus();
    _openConnection();
    unawaited(widget.controller.toggleTaskChecked(hit.row!));
  }

  void _handleSecondaryTapDown(TapDownDetails details) {
    final hit = _surface?.positionForOffset(details.localPosition);
    if (hit == null) return;
    final base = widget.controller.globalSelectionBase;
    final extent = widget.controller.globalSelectionExtent;
    final start = math.min(base, extent);
    final end = math.max(base, extent);
    if (base == extent ||
        hit.globalUtf16Offset < start ||
        hit.globalUtf16Offset > end) {
      _activateHit(hit);
    } else {
      _focusNode.requestFocus();
      _openConnection();
    }
  }

  void _showToolbar() {
    final surface = _surface;
    if (surface == null || !mounted) return;
    final controller = widget.controller;
    final base = controller.globalSelectionBase;
    final extent = controller.globalSelectionExtent;
    final anchors = surface.selectionToolbarAnchors(base, extent);
    if (anchors == null) return;
    final hasSelection = base != extent;
    final allSelected =
        math.min(base, extent) == 0 &&
        math.max(base, extent) == controller.sourceUtf16Length;
    void run(Future<void> Function() action) {
      _contextMenuController.remove();
      unawaited(action());
    }

    final items = <ContextMenuButtonItem>[
      if (hasSelection)
        ContextMenuButtonItem(
          type: ContextMenuButtonType.copy,
          onPressed: () => run(_copySelection),
        ),
      if (hasSelection)
        ContextMenuButtonItem(
          type: ContextMenuButtonType.cut,
          onPressed: () => run(_cutSelection),
        ),
      ContextMenuButtonItem(
        type: ContextMenuButtonType.paste,
        onPressed: () => run(_pasteClipboard),
      ),
      if (!allSelected)
        ContextMenuButtonItem(
          type: ContextMenuButtonType.selectAll,
          onPressed: () => run(_selectAll),
        ),
    ];
    _contextMenuController.show(
      context: context,
      debugRequiredFor: widget,
      contextMenuBuilder: (_) =>
          material.AdaptiveTextSelectionToolbar.buttonItems(
            anchors: anchors,
            buttonItems: items,
          ),
    );
    widget.debugInputEventObserver?.call('context-menu:show');
  }

  void _acceptTextDrop(DragTargetDetails<String> details) {
    if (!widget.enableTextDrop || details.data.isEmpty) return;
    final surface = _surface;
    if (surface == null) return;
    _activate(surface.globalToLocal(details.offset));
    widget.controller.replaceSelection(details.data);
    widget.debugInputEventObserver?.call('drop:text');
  }

  void _selectWordAt(Offset localPosition) {
    _pendingTapHit = null;
    final selection = _surface?.wordSelectionForOffset(localPosition);
    if (selection == null) return;
    final base = selection.base;
    final extent = selection.extent;
    if (base.row case final row?) {
      widget.controller.activateRow(
        row,
        base.globalUtf16Offset,
        selectionExtent: extent.globalUtf16Offset,
        affinity: base.affinity,
      );
    } else {
      widget.controller.activateNeutralLine(
        text: base.neutralText ?? '',
        globalUtf16Start: base.neutralUtf16Start ?? 0,
        globalUtf16Offset: base.globalUtf16Offset,
        selectionExtent: extent.globalUtf16Offset,
        ordinal: base.ordinal,
        affinity: base.affinity,
      );
    }
    _focusNode.requestFocus();
    _openConnection();
    _sendEditingState(force: true);
  }

  Map<Type, GestureRecognizerFactory> get _gestureRecognizers => {
    _SecondaryTapGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<_SecondaryTapGestureRecognizer>(
          () => _SecondaryTapGestureRecognizer(debugOwner: this),
          (recognizer) {
            recognizer
              ..onSecondaryTapDown = _handleSecondaryTapDown
              ..onSecondaryTap = _showToolbar;
          },
        ),
    TapGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<TapGestureRecognizer>(
          () => TapGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {
              PointerDeviceKind.touch,
              PointerDeviceKind.stylus,
              PointerDeviceKind.invertedStylus,
            },
          ),
          (recognizer) {
            recognizer
              ..onTapDown = _handleTapDown
              ..onTap = _handleTap
              ..onTapCancel = () => _pendingTapHit = null;
          },
        ),
    TapAndPanGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<TapAndPanGestureRecognizer>(
          () => TapAndPanGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {PointerDeviceKind.mouse},
          ),
          (recognizer) {
            recognizer
              ..dragStartBehavior = DragStartBehavior.down
              ..onTapDown = (details) {
                _pendingTapHit = _surface?.positionForOffset(
                  details.localPosition,
                );
                if (details.consecutiveTapCount == 2) {
                  _selectWordAt(details.localPosition);
                }
              }
              ..onTapUp = (details) {
                if (details.consecutiveTapCount == 1) _handleTap();
              }
              ..onCancel = () {
                _pendingTapHit = null;
              }
              ..onDragStart = (details) {
                _pendingTapHit = null;
                if (details.consecutiveTapCount == 1) {
                  _activate(details.localPosition);
                }
              }
              ..onDragUpdate = (details) {
                if (details.consecutiveTapCount == 1) {
                  _activate(details.localPosition, extend: true);
                }
              };
          },
        ),
    VerticalDragGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<VerticalDragGestureRecognizer>(
          () => VerticalDragGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {
              PointerDeviceKind.touch,
              PointerDeviceKind.stylus,
              PointerDeviceKind.invertedStylus,
            },
          ),
          (recognizer) {
            recognizer
              ..onStart = (_) {
                _pendingTapHit = null;
              }
              ..onUpdate = (details) {
                _surface?.scrollBy(-details.delta.dy);
              };
          },
        ),
    LongPressGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<LongPressGestureRecognizer>(
          () => LongPressGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {
              PointerDeviceKind.touch,
              PointerDeviceKind.stylus,
              PointerDeviceKind.invertedStylus,
            },
          ),
          (recognizer) {
            recognizer.onLongPressStart = (details) {
              _selectWordAt(details.localPosition);
              WidgetsBinding.instance.addPostFrameCallback((_) {
                if (mounted) _showToolbar();
              });
            };
          },
        ),
  };

  @override
  Widget build(BuildContext context) {
    final editor = CallbackShortcuts(
      bindings: _desktopShortcutBindings,
      child: Focus(
        focusNode: _focusNode,
        onKeyEvent: (node, event) {
          widget.debugInputEventObserver?.call(
            'key:${event.runtimeType}:${event.logicalKey.keyLabel}'
            ':meta=${HardwareKeyboard.instance.isMetaPressed}'
            ':control=${HardwareKeyboard.instance.isControlPressed}'
            ':shift=${HardwareKeyboard.instance.isShiftPressed}',
          );
          if ((event is KeyDownEvent || event is KeyRepeatEvent) &&
              event.logicalKey == LogicalKeyboardKey.tab &&
              !HardwareKeyboard.instance.isMetaPressed &&
              !HardwareKeyboard.instance.isControlPressed &&
              !HardwareKeyboard.instance.isAltPressed) {
            if (_handleTab(reverse: HardwareKeyboard.instance.isShiftPressed)) {
              return KeyEventResult.handled;
            }
          }
          return KeyEventResult.ignored;
        },
        child: MouseRegion(
          cursor: SystemMouseCursors.text,
          child: Listener(
            onPointerSignal: (event) {
              if (event is PointerScrollEvent) {
                _surface?.scrollBy(event.scrollDelta.dy);
              }
            },
            onPointerPanZoomUpdate: (event) {
              _surface?.scrollBy(-event.localPanDelta.dy);
            },
            child: RawGestureDetector(
              behavior: HitTestBehavior.opaque,
              gestures: _gestureRecognizers,
              child: FlarkRenderSurfaceWidget(
                key: _surfaceKey,
                controller: widget.controller,
                textStyle: widget.textStyle,
                padding: widget.padding,
                caretColor: widget.caretColor,
                selectionColor: widget.selectionColor,
                includeEditingState: true,
                semanticsActions: FlarkSurfaceSemanticsActions(
                  onSetSelection: _setSemanticsSelection,
                  onMoveCursor: _moveSemanticsCursor,
                  onCopy: () => unawaited(_copySelection()),
                  onCut: () => unawaited(_cutSelection()),
                  onPaste: () => unawaited(_pasteClipboard()),
                  onShowToolbar: _showToolbar,
                ),
                debugPaintObserver: widget.debugPaintObserver,
              ),
            ),
          ),
        ),
      ),
    );
    if (!widget.enableTextDrop) return editor;
    return DragTarget<String>(
      onWillAcceptWithDetails: (details) => details.data.isNotEmpty,
      onAcceptWithDetails: _acceptTextDrop,
      builder: (context, candidateData, rejectedData) => editor,
    );
  }

  bool get _usesAppleNavigationModifiers => switch (defaultTargetPlatform) {
    TargetPlatform.iOS || TargetPlatform.macOS => true,
    _ => false,
  };

  Map<ShortcutActivator, VoidCallback> get _desktopShortcutBindings => {
    SingleActivator(LogicalKeyboardKey.arrowLeft): () =>
        _moveCharacter(forward: false, modify: false),
    SingleActivator(LogicalKeyboardKey.arrowRight): () =>
        _moveCharacter(forward: true, modify: false),
    SingleActivator(LogicalKeyboardKey.arrowLeft, shift: true): () =>
        _moveCharacter(forward: false, modify: true),
    SingleActivator(LogicalKeyboardKey.arrowRight, shift: true): () =>
        _moveCharacter(forward: true, modify: true),
    SingleActivator(LogicalKeyboardKey.arrowUp): () =>
        _moveVertically(forward: false, modify: false),
    SingleActivator(LogicalKeyboardKey.arrowDown): () =>
        _moveVertically(forward: true, modify: false),
    SingleActivator(LogicalKeyboardKey.arrowUp, shift: true): () =>
        _moveVertically(forward: false, modify: true),
    SingleActivator(LogicalKeyboardKey.arrowDown, shift: true): () =>
        _moveVertically(forward: true, modify: true),
    SingleActivator(LogicalKeyboardKey.home): () =>
        _moveToLineBoundary(forward: false, modify: false),
    SingleActivator(LogicalKeyboardKey.end): () =>
        _moveToLineBoundary(forward: true, modify: false),
    SingleActivator(LogicalKeyboardKey.home, shift: true): () =>
        _moveToLineBoundary(forward: false, modify: true),
    SingleActivator(LogicalKeyboardKey.end, shift: true): () =>
        _moveToLineBoundary(forward: true, modify: true),
    SingleActivator(
      LogicalKeyboardKey.arrowUp,
      meta: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
    ): () =>
        _moveToParagraphBoundary(forward: false, modify: false),
    SingleActivator(
      LogicalKeyboardKey.arrowDown,
      meta: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
    ): () =>
        _moveToParagraphBoundary(forward: true, modify: false),
    SingleActivator(
      LogicalKeyboardKey.arrowUp,
      meta: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
      shift: true,
    ): () =>
        _moveToParagraphBoundary(forward: false, modify: true),
    SingleActivator(
      LogicalKeyboardKey.arrowDown,
      meta: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
      shift: true,
    ): () =>
        _moveToParagraphBoundary(forward: true, modify: true),
    SingleActivator(
      LogicalKeyboardKey.arrowLeft,
      alt: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
    ): () =>
        _moveWord(forward: false, modify: false),
    SingleActivator(
      LogicalKeyboardKey.arrowRight,
      alt: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
    ): () =>
        _moveWord(forward: true, modify: false),
    SingleActivator(
      LogicalKeyboardKey.arrowLeft,
      alt: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
      shift: true,
    ): () =>
        _moveWord(forward: false, modify: true),
    SingleActivator(
      LogicalKeyboardKey.arrowRight,
      alt: _usesAppleNavigationModifiers,
      control: !_usesAppleNavigationModifiers,
      shift: true,
    ): () =>
        _moveWord(forward: true, modify: true),
    if (_usesAppleNavigationModifiers) ...{
      SingleActivator(LogicalKeyboardKey.arrowLeft, meta: true): () =>
          _moveToLineBoundary(forward: false, modify: false),
      SingleActivator(LogicalKeyboardKey.arrowRight, meta: true): () =>
          _moveToLineBoundary(forward: true, modify: false),
      SingleActivator(
        LogicalKeyboardKey.arrowLeft,
        meta: true,
        shift: true,
      ): () =>
          _moveToLineBoundary(forward: false, modify: true),
      SingleActivator(
        LogicalKeyboardKey.arrowRight,
        meta: true,
        shift: true,
      ): () =>
          _moveToLineBoundary(forward: true, modify: true),
    },
    for (final meta in [true, false]) ...{
      SingleActivator(LogicalKeyboardKey.keyA, meta: meta, control: !meta): () {
        widget.debugInputEventObserver?.call('shortcut:select-all');
        unawaited(_selectAll());
      },
      SingleActivator(LogicalKeyboardKey.keyC, meta: meta, control: !meta): () {
        widget.debugInputEventObserver?.call('shortcut:copy');
        unawaited(_copySelection());
      },
      SingleActivator(LogicalKeyboardKey.keyX, meta: meta, control: !meta): () {
        widget.debugInputEventObserver?.call('shortcut:cut');
        unawaited(_cutSelection());
      },
      SingleActivator(LogicalKeyboardKey.keyV, meta: meta, control: !meta): () {
        widget.debugInputEventObserver?.call('shortcut:paste');
        unawaited(_pasteClipboard());
      },
      SingleActivator(LogicalKeyboardKey.keyZ, meta: meta, control: !meta): () {
        widget.debugInputEventObserver?.call('shortcut:undo');
        unawaited(_undo());
      },
      SingleActivator(
        LogicalKeyboardKey.keyZ,
        meta: meta,
        control: !meta,
        shift: true,
      ): () {
        widget.debugInputEventObserver?.call('shortcut:redo');
        unawaited(_redo());
      },
    },
    SingleActivator(LogicalKeyboardKey.keyY, control: true): () {
      widget.debugInputEventObserver?.call('shortcut:redo');
      unawaited(_redo());
    },
  };

  @override
  TextEditingValue? get currentTextEditingValue => widget.controller.inputValue;

  @override
  AutofillScope? get currentAutofillScope => null;

  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> textEditingDeltas) {
    _preferredVerticalNavigationX = null;
    if (textEditingDeltas.isNotEmpty) {
      var platformValue = TextEditingValue(
        text: textEditingDeltas.first.oldText,
      );
      for (final delta in textEditingDeltas) {
        platformValue = delta.apply(platformValue);
      }
      // A delta is the platform's declaration of its newly installed local
      // buffer. Remember it before the controller publishes so an
      // authoritative semantic receipt can send a corrective state even when
      // that state happens to equal the value Flutter sent before the key.
      _lastKnownPlatformValue = platformValue;
      _platformNewlineObservationAwaitingAction = _isPlatformNewlineObservation(
        widget.controller.inputValue,
        platformValue,
      );
    }
    final observer = widget.debugInputEventObserver;
    final stopwatch = observer == null ? null : (Stopwatch()..start());
    observer?.call(
      'deltas:${textEditingDeltas.map(_debugTextEditingDelta).join('|')}',
    );
    widget.controller.applyDeltas(textEditingDeltas);
    stopwatch?.stop();
    observer?.call(
      'accepted-deltas:generation=${widget.controller.sourceGeneration}'
      ':elapsedMicros=${stopwatch!.elapsedMicroseconds}',
    );
  }

  String _debugTextEditingDelta(TextEditingDelta delta) {
    final mutation = switch (delta) {
      TextEditingDeltaInsertion insertion =>
        '${insertion.insertionOffset}..${insertion.insertionOffset}'
            '=${insertion.textInserted}',
      TextEditingDeltaDeletion deletion =>
        '${deletion.deletedRange.start}..${deletion.deletedRange.end}=',
      TextEditingDeltaReplacement replacement =>
        '${replacement.replacedRange.start}..${replacement.replacedRange.end}'
            '=${replacement.replacementText}',
      TextEditingDeltaNonTextUpdate() => 'none',
      _ => 'unknown',
    };
    final selection = delta.selection;
    final composing = delta.composing;
    return '${delta.runtimeType}:old=${delta.oldText.length}:mutation=$mutation'
        ':selection=${selection.baseOffset}..${selection.extentOffset}'
        ':selectionValid=${selection.isValid}'
        ':composing=${composing.start}..${composing.end}'
        ':composingValid=${composing.isValid}';
  }

  @override
  void updateEditingValue(TextEditingValue value) {
    _preferredVerticalNavigationX = null;
    _platformNewlineObservationAwaitingAction = _isPlatformNewlineObservation(
      widget.controller.inputValue,
      value,
    );
    // Full-value clients have already adopted this value locally. Controller
    // publications must compare against that platform truth, not a stale
    // framework-send cache.
    _lastKnownPlatformValue = value;
    final observer = widget.debugInputEventObserver;
    final stopwatch = observer == null ? null : (Stopwatch()..start());
    observer?.call(
      'full-value:length=${value.text.length}:selection=${value.selection}'
      ':composing=${value.composing}',
    );
    widget.controller.updateEditingValue(value);
    stopwatch?.stop();
    observer?.call(
      'accepted-full-value:generation=${widget.controller.sourceGeneration}'
      ':elapsedMicros=${stopwatch!.elapsedMicroseconds}',
    );
  }

  @override
  void performAction(TextInputAction action) {
    _preferredVerticalNavigationX = null;
    widget.debugInputEventObserver?.call('action:$action');
    if (action == TextInputAction.newline) {
      final textObservationAlreadyApplied =
          _platformNewlineObservationAwaitingAction;
      _platformNewlineObservationAwaitingAction = false;
      widget.controller.observePlatformNewlineAction(
        textObservationAlreadyApplied: textObservationAlreadyApplied,
      );
    }
  }

  bool _isPlatformNewlineObservation(
    TextEditingValue before,
    TextEditingValue after,
  ) {
    if (before.composing != TextRange.empty ||
        after.composing != TextRange.empty ||
        !before.selection.isValid) {
      return false;
    }
    final start = math.min(
      before.selection.baseOffset,
      before.selection.extentOffset,
    );
    final end = math.max(
      before.selection.baseOffset,
      before.selection.extentOffset,
    );
    return before.text.replaceRange(start, end, '\n') == after.text;
  }

  @override
  void performSelector(String selectorName) {
    widget.debugInputEventObserver?.call('selector:$selectorName');
    switch (selectorName) {
      case 'copy:':
        unawaited(_copySelection());
      case 'cut:':
        unawaited(_cutSelection());
      case 'paste:':
        unawaited(_pasteClipboard());
      case 'selectAll:':
        unawaited(_selectAll());
      case 'deleteBackward:':
        _preferredVerticalNavigationX = null;
        widget.controller.observePlatformDeleteBackwardAction();
        widget.debugInputEventObserver?.call(
          'accepted-selector:delete-backward:'
          'generation=${widget.controller.sourceGeneration}',
        );
      case 'deleteForward:':
        _preferredVerticalNavigationX = null;
        widget.controller.deleteForward();
        widget.debugInputEventObserver?.call(
          'accepted-selector:delete-forward:'
          'generation=${widget.controller.sourceGeneration}',
        );
      case 'moveLeft:' || 'moveBackward:':
        _moveCharacter(forward: false, modify: false);
        widget.debugInputEventObserver?.call(
          'completed-navigation:generation='
          '${widget.controller.sourceGeneration}',
        );
      case 'moveRight:' || 'moveForward:':
        _moveCharacter(forward: true, modify: false);
        widget.debugInputEventObserver?.call(
          'completed-navigation:generation='
          '${widget.controller.sourceGeneration}',
        );
      case 'moveLeftAndModifySelection:':
        _moveCharacter(forward: false, modify: true);
      case 'moveRightAndModifySelection:':
        _moveCharacter(forward: true, modify: true);
      case 'moveUp:':
        _moveVertically(forward: false, modify: false);
      case 'moveDown:':
        _moveVertically(forward: true, modify: false);
      case 'moveUpAndModifySelection:':
        _moveVertically(forward: false, modify: true);
      case 'moveDownAndModifySelection:':
        _moveVertically(forward: true, modify: true);
      case 'moveToLeftEndOfLine:':
        _moveToLineBoundary(forward: false, modify: false);
      case 'moveToRightEndOfLine:':
        _moveToLineBoundary(forward: true, modify: false);
      case 'moveToLeftEndOfLineAndModifySelection:':
        _moveToLineBoundary(forward: false, modify: true);
      case 'moveToRightEndOfLineAndModifySelection:':
        _moveToLineBoundary(forward: true, modify: true);
      case 'moveToBeginningOfParagraph:':
        _moveToParagraphBoundary(forward: false, modify: false);
      case 'moveToEndOfParagraph:':
        _moveToParagraphBoundary(forward: true, modify: false);
      case 'moveToBeginningOfParagraphAndModifySelection:':
        _moveToParagraphBoundary(forward: false, modify: true);
      case 'moveToEndOfParagraphAndModifySelection:':
        _moveToParagraphBoundary(forward: true, modify: true);
      case 'moveWordLeft:':
        _moveWord(forward: false, modify: false);
      case 'moveWordRight:':
        _moveWord(forward: true, modify: false);
      case 'moveWordLeftAndModifySelection:':
        _moveWord(forward: false, modify: true);
      case 'moveWordRightAndModifySelection:':
        _moveWord(forward: true, modify: true);
      case 'insertTab:':
        _handleTab(reverse: false);
      case 'insertBacktab:':
        _handleTab(reverse: true);
      case 'insertNewline:':
        _preferredVerticalNavigationX = null;
        widget.controller.insertNewline();
        widget.debugInputEventObserver?.call(
          'accepted-selector:insert-newline:'
          'generation=${widget.controller.sourceGeneration}',
        );
      case 'undo:':
        _preferredVerticalNavigationX = null;
        unawaited(_undo());
      case 'redo:':
        _preferredVerticalNavigationX = null;
        unawaited(_redo());
      case 'cancelOperation:':
        _preferredVerticalNavigationX = null;
        unawaited(widget.controller.cancelComposition());
      default:
        break;
    }
  }

  Future<void> _copySelection() async {
    final text = await widget.controller.readSelectedText();
    if (text == null) return;
    await Clipboard.setData(ClipboardData(text: text));
    widget.debugInputEventObserver?.call(
      'completed-copy:generation=${widget.controller.sourceGeneration}',
    );
  }

  Future<void> _cutSelection() async {
    _preferredVerticalNavigationX = null;
    final controller = widget.controller;
    final value = controller.inputValue;
    final text = await controller.readSelectedText();
    if (text == null) return;
    final selectionGeneration = controller.canonicalSelectionGeneration;
    await Clipboard.setData(ClipboardData(text: text));
    if (!mounted || !identical(controller, widget.controller)) return;
    if (controller.inputValue != value) return;
    if (controller.canonicalSelectionGeneration != selectionGeneration) return;
    controller.replaceSelection('');
    widget.debugInputEventObserver?.call(
      'completed-cut:generation=${controller.sourceGeneration}',
    );
  }

  Future<void> _pasteClipboard() async {
    _preferredVerticalNavigationX = null;
    final observer = widget.debugInputEventObserver;
    final acceptedAtEpochMicros = observer == null
        ? null
        : DateTime.now().microsecondsSinceEpoch;
    final stopwatch = observer == null ? null : (Stopwatch()..start());
    final controller = widget.controller;
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (!mounted || !identical(controller, widget.controller)) return;
    final text = data?.text;
    if (text == null || text.isEmpty) return;
    controller.replaceSelection(text);
    stopwatch?.stop();
    observer?.call(
      'completed-paste:generation=${controller.sourceGeneration}'
      ':acceptedAtEpochMicros=$acceptedAtEpochMicros'
      ':elapsedMicros=${stopwatch!.elapsedMicroseconds}',
    );
  }

  Future<void> _undo() async {
    final controller = widget.controller;
    await controller.undo();
    if (!mounted || !identical(controller, widget.controller)) return;
    widget.debugInputEventObserver?.call(
      'completed-undo:generation=${controller.sourceGeneration}',
    );
  }

  Future<void> _redo() async {
    final controller = widget.controller;
    await controller.redo();
    if (!mounted || !identical(controller, widget.controller)) return;
    widget.debugInputEventObserver?.call(
      'completed-redo:generation=${controller.sourceGeneration}',
    );
  }

  @override
  void performPrivateCommand(String action, Map<String, dynamic> data) {
    widget.onAppPrivateCommand?.call(action, data);
  }

  @override
  void insertContent(KeyboardInsertedContent content) {
    final configuration = widget.contentInsertionConfiguration;
    if (configuration == null ||
        !configuration.allowedMimeTypes.contains(content.mimeType)) {
      return;
    }
    configuration.onContentInserted(content);
  }

  @override
  void didChangeInputControl(
    TextInputControl? oldControl,
    TextInputControl? newControl,
  ) {}

  @override
  void showToolbar() => _showToolbar();

  @override
  void insertTextPlaceholder(Size size) {}

  @override
  void removeTextPlaceholder() {}

  @override
  void updateFloatingCursor(RawFloatingCursorPoint point) {}

  @override
  void showAutocorrectionPromptRect(int start, int end) {}

  // Newer Flutter SDKs add this optional TextInputClient callback. Keep the
  // annotation there while allowing the 3.41 qualification lane.
  @override
  // ignore: override_on_non_overriding_member
  bool onFocusReceived() {
    _focusNode.requestFocus();
    return true;
  }

  @override
  void connectionClosed() {
    widget.debugInputEventObserver?.call('connection-closed');
    final connection = _connection;
    if (connection?.attached ?? false) {
      connection!.connectionClosedReceived();
    }
    _connection = null;
    _lastKnownPlatformValue = null;
    if (_focusNode.hasFocus) _focusNode.unfocus();
  }
}
