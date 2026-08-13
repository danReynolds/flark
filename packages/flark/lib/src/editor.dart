import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';
import 'render_surface.dart';

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
  TextEditingValue? _lastSentValue;
  FlarkSurfaceHit? _pendingTapHit;
  double? _preferredVerticalNavigationX;

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
      _lastSentValue = null;
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
  }

  @override
  void dispose() {
    widget.debugHandle?._detach(_surface);
    widget.controller.removeListener(_controllerChanged);
    _focusNode.removeListener(_focusChanged);
    _connection?.close();
    _ownedFocusNode?.dispose();
    super.dispose();
  }

  void _focusChanged() {
    if (_focusNode.hasFocus) {
      _openConnection();
    } else {
      _connection?.close();
      _connection = null;
    }
    setState(() {});
  }

  void _openConnection() {
    if (_connection?.attached ?? false) return;
    _connection = TextInput.attach(
      this,
      const TextInputConfiguration(
        inputType: TextInputType.multiline,
        inputAction: TextInputAction.newline,
        autocorrect: true,
        enableSuggestions: true,
        enableDeltaModel: true,
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
    if (!force && value == _lastSentValue) return;
    connection.setEditingState(value);
    _lastSentValue = value;
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
    _pendingTapHit = _surface?.positionForOffset(details.localPosition);
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

  void _moveVertically({required bool forward, required bool modify}) {
    final surface = _surface;
    if (surface == null) return;
    final extent = widget.controller.globalSelectionExtent;
    _preferredVerticalNavigationX ??= surface.localXForSourceUtf16(extent);
    final hit = surface.verticalHit(
      extent,
      forward: forward,
      preferredX: _preferredVerticalNavigationX,
    );
    if (hit == null) return;
    final preferredX = _preferredVerticalNavigationX;
    _adoptNavigationHit(hit, modify: modify);
    _preferredVerticalNavigationX = preferredX;
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

  Map<Type, GestureRecognizerFactory> get _gestureRecognizers => {
    TapGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<TapGestureRecognizer>(
          () => TapGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {
              PointerDeviceKind.mouse,
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
    PanGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<PanGestureRecognizer>(
          () => PanGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {PointerDeviceKind.mouse},
          ),
          (recognizer) {
            recognizer
              ..onStart = (details) {
                _pendingTapHit = null;
                _activate(details.localPosition);
              }
              ..onUpdate = (details) =>
                  _activate(details.localPosition, extend: true);
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
  };

  @override
  Widget build(BuildContext context) {
    return CallbackShortcuts(
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
                debugPaintObserver: widget.debugPaintObserver,
              ),
            ),
          ),
        ),
      ),
    );
  }

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
    for (final meta in [true, false]) ...{
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
        unawaited(widget.controller.undo());
      },
      SingleActivator(
        LogicalKeyboardKey.keyZ,
        meta: meta,
        control: !meta,
        shift: true,
      ): () {
        widget.debugInputEventObserver?.call('shortcut:redo');
        unawaited(widget.controller.redo());
      },
    },
    SingleActivator(LogicalKeyboardKey.keyY, control: true): () {
      widget.debugInputEventObserver?.call('shortcut:redo');
      unawaited(widget.controller.redo());
    },
  };

  @override
  TextEditingValue? get currentTextEditingValue => widget.controller.inputValue;

  @override
  AutofillScope? get currentAutofillScope => null;

  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> textEditingDeltas) {
    _preferredVerticalNavigationX = null;
    widget.debugInputEventObserver?.call(
      'deltas:${textEditingDeltas.map((delta) => delta.runtimeType).join(',')}'
      ':old=${textEditingDeltas.isEmpty ? -1 : textEditingDeltas.first.oldText.length}',
    );
    widget.controller.applyDeltas(textEditingDeltas);
  }

  @override
  void updateEditingValue(TextEditingValue value) {
    _preferredVerticalNavigationX = null;
    widget.debugInputEventObserver?.call(
      'full-value:length=${value.text.length}:selection=${value.selection}'
      ':composing=${value.composing}',
    );
    widget.controller.updateEditingValue(value);
  }

  @override
  void performAction(TextInputAction action) {
    _preferredVerticalNavigationX = null;
    widget.debugInputEventObserver?.call('action:$action');
    if (action == TextInputAction.newline) {
      widget.controller.observePlatformNewlineAction();
    }
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
      case 'deleteBackward:':
        _preferredVerticalNavigationX = null;
        widget.controller.observePlatformDeleteBackwardAction();
      case 'deleteForward:':
        _preferredVerticalNavigationX = null;
        widget.controller.deleteForward();
      case 'moveLeft:' || 'moveBackward:':
        _moveCharacter(forward: false, modify: false);
      case 'moveRight:' || 'moveForward:':
        _moveCharacter(forward: true, modify: false);
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
      case 'insertNewline:':
        _preferredVerticalNavigationX = null;
        widget.controller.insertNewline();
      case 'undo:':
        _preferredVerticalNavigationX = null;
        unawaited(widget.controller.undo());
      case 'redo:':
        _preferredVerticalNavigationX = null;
        unawaited(widget.controller.redo());
      default:
        break;
    }
  }

  Future<void> _copySelection() async {
    final text = await widget.controller.readSelectedText();
    if (text == null) return;
    await Clipboard.setData(ClipboardData(text: text));
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
  }

  Future<void> _pasteClipboard() async {
    _preferredVerticalNavigationX = null;
    final controller = widget.controller;
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (!mounted || !identical(controller, widget.controller)) return;
    final text = data?.text;
    if (text == null || text.isEmpty) return;
    controller.replaceSelection(text);
  }

  @override
  void performPrivateCommand(String action, Map<String, dynamic> data) {}

  @override
  void insertContent(KeyboardInsertedContent content) {}

  @override
  void didChangeInputControl(
    TextInputControl? oldControl,
    TextInputControl? newControl,
  ) {}

  @override
  void showToolbar() {}

  @override
  void insertTextPlaceholder(Size size) {}

  @override
  void removeTextPlaceholder() {}

  @override
  void updateFloatingCursor(RawFloatingCursorPoint point) {}

  @override
  void showAutocorrectionPromptRect(int start, int end) {}

  @override
  bool onFocusReceived() {
    _focusNode.requestFocus();
    return true;
  }

  @override
  void connectionClosed() {
    widget.debugInputEventObserver?.call('connection-closed');
    _connection = null;
  }
}
