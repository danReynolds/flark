import 'package:flark/session.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flutter/cupertino.dart'
    show cupertinoTextSelectionHandleControls;
import 'package:flutter/gestures.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/semantics.dart';
import 'controller.dart';
import 'clipboard_binding_stub.dart'
    if (dart.library.js_interop) 'clipboard_binding_web.dart';
import 'input_context.dart';
import 'surface.dart';
import 'source_window.dart';
import 'resource_dialog.dart';
import 'image_previews.dart';
import 'theme.dart';
import 'resource_controls.dart';

/// Scrollable editing surface. All Markdown commands belong to the kernel;
/// this widget owns platform input, focus, viewport and glyph geometry.
class FlarkEditorWidget extends StatefulWidget {
  const FlarkEditorWidget({
    super.key,
    required this.controller,
    this.actions,
    this.session,
    this.autofocus = false,
    this.readOnly = false,
    this.showToolbar = true,
    this.onPaint,
    this.focusNode,
    this.style,
    this.theme,
    this.linkPopoverBuilder,
    this.presentResourceEditor,
    this.baseUri,
    this.imageProvider,
    this.showImagePreviews = true,
    this.onOpenLink,
  });
  static const defaultStyle = TextStyle(
    fontSize: 17,
    height: 1.45,
    color: Color(0xff253047),
  );
  final FlarkController controller;
  final FlarkActions? actions;
  final FlarkSession? session;
  final bool autofocus, readOnly, showToolbar;
  final FocusNode? focusNode;
  final TextStyle? style;
  final FlarkThemeData? theme;
  final FlarkLinkPopoverBuilder? linkPopoverBuilder;
  final FlarkResourcePresenter? presentResourceEditor;
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  final bool showImagePreviews;
  final ValueChanged<Uri>? onOpenLink;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  State<FlarkEditorWidget> createState() => _FlarkEditorWidgetState();
}

class _FlarkEditorWidgetState extends State<FlarkEditorWidget> {
  final _surfaceKey = GlobalKey();
  final _scroll = ScrollController();
  late FocusNode _focus;
  (FlarkController, int, FlarkSelection)? _toolbarMenuTarget;
  late final EditorClipboardBinding _clipboardBinding;
  TextInputConnection? _connection;
  _InputClient? _client;
  TextEditingValue? _sentValue;
  InputContext? _inputContext;
  int _epoch = 0;

  /// The x that a run of Up and Down keys aims for, and the controller and
  /// revision the last of them produced. Typing through the platform, line
  /// edges and other edits change the revision, so the next vertical move
  /// starts from wherever the caret then is.
  ({double x, FlarkController controller, int revision})? _goal;
  bool _dragging = false;

  /// A touch on the document that has not moved past the touch slop, and
  /// how far the document has scrolled under it. Touch presses when it
  /// lifts, so a scroll neither moves the caret nor raises the keyboard.
  ({int pointer, Offset position, double scrolled})? _touch;

  /// The platform's touch slop, which its scroll views also use.
  double get _touchSlop =>
      MediaQuery.maybeGestureSettingsOf(context)?.touchSlop ?? kTouchSlop;
  ({InlineResource image, Offset point, int revision})? _pressedImage;
  bool _scheduled = false;
  bool get _resourceDialogOpen => _resourceSession != null;
  FlarkResourceSession? _resourceSession;
  final _popover = OverlayPortalController();
  final _popoverFocus = FocusScopeNode(debugLabel: 'Link actions');
  InlineResource? _link;
  FlarkController? _linkController;
  TextSelection? _linkSelection;
  int _linkRevision = -1, _linkEpoch = 0;
  ({InlineResource resource, Offset point})? _pressedLink;

  int? _sourceWindowStart;
  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;
  FlarkController get c => widget.controller;

  Future<FlarkEditResult> _presentResource(bool image) async {
    if (widget.readOnly) {
      return const FlarkEditResult.rejected(FlarkEditRejection.readOnly);
    }
    if (_resourceDialogOpen ||
        c.editor.sourceMode ||
        c.editor.document.caretRow.kind == RowKind.codeBlock) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unavailable);
    }
    return _editResource(image);
  }

  @override
  void initState() {
    super.initState();
    widget.session?.resourcePresenter = _presentResource;
    // Initialize optional decoration before the input connection is opened.
    _focus = widget.focusNode ?? FocusNode(debugLabel: 'Flark editor');
    _focus.addListener(_focusChanged);
    _clipboardBinding = EditorClipboardBinding(
      isActive: () =>
          !widget.readOnly && _focus.hasFocus && _connection?.attached == true,
      selectedText: () =>
          c.editor.selection.isCollapsed ? null : c.selectedText,
      cut: () => _command(const DeleteBackward()),
      paste: (text) => _command(Paste(text)),
    );
    c.addListener(_changed);
    _scroll.addListener(_scrolled);
    // A caller can request an external focus node while automatic loading is
    // still showing its placeholder. Attach input when that focused view mounts.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _focus.hasFocus && !widget.readOnly) _attach();
    });
  }

  // Resolved once per ambient theme or override change, not per edit: the
  // editor rebuilds for every keystroke to refresh its toolbar.
  FlarkThemeData? _theme;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _theme = null;
  }

  @override
  void didUpdateWidget(FlarkEditorWidget old) {
    super.didUpdateWidget(old);
    if (old.theme != widget.theme || old.style != widget.style) _theme = null;
    if (old.controller != c || old.readOnly != widget.readOnly) {
      _hideTouchSelection(rebuild: false);
      _dismissLink();
      _resourceSession?.close();
      _resourceSession = null;
    }
    if (old.controller != c) {
      old.controller.removeListener(_changed);
      _endComposition(old.controller);
      c.addListener(_changed);
      _close();
    }
    if (old.focusNode != widget.focusNode) {
      _focus.removeListener(_focusChanged);
      // Closing the connection ends the platform's composition, so the
      // kernel's must end with it, as it does when focus is lost.
      _endComposition(c);
      _close();
      if (old.focusNode == null) _focus.dispose();
      _focus = widget.focusNode ?? FocusNode(debugLabel: 'Flark editor');
      _focus.addListener(_focusChanged);
    }
    if (widget.readOnly) {
      _endComposition(c);
      _close();
    } else if (_focus.hasPrimaryFocus) {
      // Only while the editor itself has focus: a field in the link popover
      // holds the input connection while it is focused, and a host rebuild
      // must not take it.
      _attach();
    }
  }

  void _scrolled() {
    _dismissLink();
    if (!mounted) return;
    final overlay = _touchSelection;
    if (overlay != null && overlay.toolbarIsVisible && _handleDrag == null) {
      overlay.hideToolbar();
      _menuAfterScroll = true;
    }
    // Only the surface's viewport changed. Rebuilding this widget per scroll
    // frame also rebuilt the toolbar and re-resolved the theme.
    _surface?.scrolled(_scroll.hasClients ? _scroll.offset : 0);
    _scheduleGeometry();
  }

  void _focusChanged() {
    if (_focus.hasFocus && !widget.readOnly) {
      _attach();
    } else {
      c.finishComposition();
      _close();
      _hideTouchSelection(rebuild: false);
    }
    if (mounted) setState(() {});
  }

  void _attach() {
    if (_connection?.attached == true || widget.readOnly) return;
    // A new platform client starts with an empty buffer. Forget what an
    // earlier connection was sent: otherwise an unchanged document is never
    // sent to this one, and its first input would describe an empty field.
    _sentValue = null;
    _client = _InputClient(this, ++_epoch);
    _connection = TextInput.attach(
      _client!,
      const TextInputConfiguration(
        inputType: TextInputType.multiline,
        inputAction: TextInputAction.newline,
        // Web replacements (paste/autofill) can arrive without beforeinput
        // metadata. Read the complete bounded DOM value so those edits cannot
        // be mistaken for selection-only deltas by the engine.
        enableDeltaModel: !kIsWeb,
        autocorrect: true,
        enableSuggestions: true,
        // The text is Markdown source. iOS would turn `--` into a dash,
        // breaking a rule or a table's delimiter row, and straight quotes
        // into curly ones, which no longer delimit a link's title or stay
        // the code they were typed into.
        smartDashesType: SmartDashesType.disabled,
        smartQuotesType: SmartQuotesType.disabled,
      ),
    );
    _sync();
    _connection!.show();
    _scheduleGeometry();
  }

  void _close() {
    _sentValue = null;
    _inputContext = null;
    _epoch++;
    _connection?.close();
    _connection = null;
    _client = null;
  }

  /// Ends [controller]'s composition with the connection that held it. The
  /// controller then notifies its listeners, which may rebuild other widgets,
  /// and the framework refuses that while it builds or unmounts this one, so
  /// during a frame the composition ends as soon as the frame completes.
  static void _endComposition(FlarkController controller) {
    final scheduler = SchedulerBinding.instance;
    if (scheduler.schedulerPhase != SchedulerPhase.persistentCallbacks) {
      controller.finishComposition();
    } else if (controller.editor.composing) {
      scheduler.addPostFrameCallback(
        (_) => controller.finishComposition(),
        debugLabel: 'FlarkEditorWidget.endComposition',
      );
    }
  }

  /// The platform closed the connection: iOS when its input view resigns
  /// first responder (the iPad hide-keyboard key), the web engine on a blur
  /// without a related target. Focus and the caret remain, so release the
  /// connection and the mirror it held. Its composition ended with it. The
  /// next press or key reattaches. A window or tab that loses focus takes the
  /// editor's focus too until it returns, and regaining focus reattaches in
  /// _focusChanged, so the app resuming needs no handler of its own.
  void _connectionClosed() {
    if (_connection?.attached == true) _connection!.connectionClosedReceived();
    _close();
    c.finishComposition();
  }

  void _sync() {
    if (_connection?.attached != true) return;
    final context = InputContext.of(c.value, previous: _inputContext);
    if (context.rebased) {
      // A different slice has a different local coordinate system. Old queued
      // callbacks must retain their old client identity and become inert.
      _close();
      _attach();
      return;
    }
    _inputContext = context;
    if (_sentValue != context.value) {
      _sentValue = context.value;
      _connection!.setEditingState(_sentValue!);
    }
  }

  /// [authenticated] input was a delta batch whose old text matched the
  /// value the platform was sent.
  void _receiveInput(TextEditingValue value, {bool authenticated = false}) {
    final context = _inputContext;
    if (context == null) return;
    _sentValue = value;
    final full = context.expand(value);
    if (full != null) c.receive(full, authenticated: authenticated);
    _sync();
  }

  void _changed() {
    if (!mounted) return;
    if (!_linkActive) _dismissLink();
    if (_resourceSession?.active == false) _resourceSession!.close();
    final start = c.editor.sourceMode
        ? SourceWindow.at(c.text, c.editor.selection.extent).start
        : null;
    if (start != _sourceWindowStart && _scroll.hasClients) {
      _scroll.jumpTo(0);
    }
    _sourceWindowStart = start;
    _sync();
    _syncTouchSelection();
    setState(() {});
    _scheduleGeometry();
  }

  // Correct the single viewport during its child layout, before it computes
  // its paint transform. Post-frame reveal would leave one stale input frame.
  double _revealDuringLayout(Rect rect, double contentHeight, double height) {
    if (!_scroll.hasClients) return 0;
    final position = _scroll.position, current = position.pixels;
    final target = rect.top < current + 12
        ? rect.top - 12
        : rect.bottom > current + height - 12
        ? rect.bottom - height + 12
        : current;
    final maximum = contentHeight > height ? contentHeight - height : 0.0;
    final clamped = target.clamp(0.0, maximum);
    if ((clamped - current).abs() > .5) position.correctBy(clamped - current);
    return position.pixels;
  }

  void _scheduleGeometry() {
    if (_scheduled) return;
    _scheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scheduled = false;
      if (!mounted) return;
      final surface = _surface;
      if (surface == null || !surface.hasSize) return;
      if (_connection?.attached == true) {
        _connection!.setEditableSizeAndTransform(
          surface.size,
          surface.getTransformTo(null),
        );
        _connection!.setCaretRect(surface.caretRect);
        _connection!.setComposingRect(surface.caretRect);
      }
    });
  }

  bool _command(FlarkCommand command) {
    _goal = null;
    return c.command(command);
  }

  /// A press at the global [position]. In the reader, or with the platform
  /// modifier, a link opens; otherwise the editor takes focus and the press
  /// toggles a task, places the caret, or arms an image or link for the
  /// release. A [touch] also asks for the keyboard, which the platform can
  /// hide (Android's back gesture) while the connection stays open.
  void _press(
    Offset position, {
    bool primary = true,
    bool drag = false,
    bool touch = false,
  }) {
    final surface = _surface;
    if (surface == null) return;
    final point = surface.globalToLocal(position);
    if (widget.readOnly ||
        HardwareKeyboard.instance.isMetaPressed ||
        HardwareKeyboard.instance.isControlPressed) {
      final link = surface.linkAt(point);
      final uri = link == null
          ? null
          : flarkOpenableUri(link.destination, widget.baseUri);
      if (uri != null && widget.onOpenLink != null) {
        widget.onOpenLink!(uri);
        return;
      }
    }
    if (widget.readOnly) return;
    _hideTouchSelection();
    _focus.requestFocus();
    // A text field elsewhere that has focus unfocuses itself when a mouse
    // presses outside it, after this listener, and its scope kept the focus
    // while this editor held the input connection: typed text arrived, but
    // no caret was drawn and keys went nowhere. Text fields take focus when
    // a tap lifts, after that; ask again once the press has been dispatched.
    scheduleMicrotask(() {
      if (mounted && !widget.readOnly && !_focus.hasFocus) {
        _focus.requestFocus();
      }
    });
    if (touch && _connection?.attached == true) _connection!.show();
    _attach();
    _goal = null;
    final image = surface.imageAt(point);
    if (image != null) {
      _dragging = false;
      _pressedImage = (
        image: image,
        point: position,
        revision: c.editor.revision,
      );
      return;
    }
    if (surface.toggleTaskAt(point)) {
      _dragging = false;
      return;
    }
    _dragging = drag;
    surface.place(point, extend: HardwareKeyboard.instance.isShiftPressed);
    // A hit past a wrapped line's end can leave the caret where it was and
    // move only the line it is drawn on, which the input method follows.
    _scheduleGeometry();
    final link = surface.linkAt(point);
    if (link != null && !HardwareKeyboard.instance.isShiftPressed && primary) {
      _pressedLink = (resource: link, point: position);
    }
  }

  // Touch selection. A long press, or a double tap by touch, selects a word
  // and shows handles and a context menu over the document. The surface
  // paints the handles' leaders at the selection's ends, so they follow
  // scrolling; the menu follows the editor.
  final _startHandle = LayerLink(), _endHandle = LayerLink();
  final _menuLink = LayerLink();
  SelectionOverlay? _touchSelection;
  bool _touchHandles = false, _menuAfterScroll = false;

  /// The source the touch selection was made in; an edit ends it.
  String? _touchSource;

  /// A caret the menu was shown at; moving it ends the touch selection.
  FlarkSelection? _touchCaret;
  (int, int)? _longPressWord;
  ({Offset grab, int fixed})? _handleDrag;

  void _showTouchSelection({bool menu = true}) {
    final surface = _surface;
    if (surface == null || !mounted || widget.readOnly) return;
    final selection = c.editor.selection;
    _touchSource = c.text;
    _touchCaret = selection.isCollapsed ? selection : null;
    final overlay = _touchSelection ??= _createTouchSelection(surface);
    _updateTouchSelection();
    if (!selection.isCollapsed && !_touchHandles) {
      overlay.showHandles();
      setState(() => _touchHandles = true);
    }
    if (menu) {
      overlay.showToolbar(context: context, contextMenuBuilder: _touchMenu);
    }
  }

  void _hideTouchSelection({bool rebuild = true}) {
    final overlay = _touchSelection;
    if (overlay == null) return;
    _touchSelection = _touchSource = _touchCaret = null;
    _longPressWord = _handleDrag = null;
    _menuAfterScroll = false;
    overlay.dispose();
    if (_touchHandles) {
      _touchHandles = false;
      if (rebuild && mounted) setState(() {});
    }
  }

  /// Typing ends the touch selection, and so does a caret it was not shown
  /// at. An edit that keeps a selection (a style) keeps the handles without
  /// the menu. Otherwise the handles and menu follow the selection; a long
  /// press in progress owns it, including when it returns to a caret.
  void _syncTouchSelection() {
    final overlay = _touchSelection;
    if (overlay == null) return;
    if (_longPressWord != null) {
      _updateTouchSelection();
      return;
    }
    final selection = c.editor.selection;
    final edited = c.text != _touchSource;
    if (selection.isCollapsed && (edited || selection != _touchCaret)) {
      _hideTouchSelection();
      return;
    }
    if (edited) {
      _touchSource = c.text;
      overlay.hideToolbar();
    }
    _updateTouchSelection();
  }

  (Rect, Rect) _touchEnds(RenderFlarkSurface surface) {
    final selection = c.editor.selection;
    return (
      surface.caretRectAt(selection.start),
      surface.caretRectAt(selection.end),
    );
  }

  SelectionOverlay _createTouchSelection(RenderFlarkSurface surface) {
    final (start, end) = _touchEnds(surface);
    return SelectionOverlay(
      context: context,
      debugRequiredFor: widget,
      startHandleType: TextSelectionHandleType.left,
      lineHeightAtStart: start.height,
      onStartHandleDragStart: (details) => _startHandleDrag(details, true),
      onStartHandleDragUpdate: (details) => _updateHandleDrag(details, true),
      onStartHandleDragEnd: _endHandleDrag,
      endHandleType: TextSelectionHandleType.right,
      lineHeightAtEnd: end.height,
      onEndHandleDragStart: (details) => _startHandleDrag(details, false),
      onEndHandleDragUpdate: (details) => _updateHandleDrag(details, false),
      onEndHandleDragEnd: _endHandleDrag,
      selectionEndpoints: [
        TextSelectionPoint(start.bottomLeft, TextDirection.ltr),
        TextSelectionPoint(end.bottomLeft, TextDirection.ltr),
      ],
      selectionControls: Theme.of(context).platform == TargetPlatform.iOS
          ? cupertinoTextSelectionHandleControls
          : materialTextSelectionHandleControls,
      // Required, although deprecated; the menu is built by _touchMenu.
      // ignore: deprecated_member_use
      selectionDelegate: null,
      clipboardStatus: null,
      startHandleLayerLink: _startHandle,
      endHandleLayerLink: _endHandle,
      toolbarLayerLink: _menuLink,
      magnifierConfiguration: TextMagnifier.adaptiveMagnifierConfiguration,
    );
  }

  void _updateTouchSelection() {
    final overlay = _touchSelection, surface = _surface;
    if (overlay == null || surface == null) return;
    final (start, end) = _touchEnds(surface);
    overlay
      ..lineHeightAtStart = start.height
      ..lineHeightAtEnd = end.height
      ..selectionEndpoints = [
        TextSelectionPoint(start.bottomLeft, TextDirection.ltr),
        TextSelectionPoint(end.bottomLeft, TextDirection.ltr),
      ];
  }

  MagnifierInfo _magnifierAt(Offset gesture, int source) {
    final surface = _surface!;
    Rect global(Rect rect) =>
        MatrixUtils.transformRect(surface.getTransformTo(null), rect);
    return MagnifierInfo(
      globalGesturePosition: gesture,
      caretRect: global(surface.caretRectAt(source)),
      currentLineBoundaries: global(surface.lineRectAt(source)),
      fieldBounds: global(surface.visibleRect),
    );
  }

  void _longPressStart(LongPressStartDetails details) {
    // The lift ends the long press; it must not place a caret.
    _touch = null;
    final surface = _surface;
    if (surface == null || widget.readOnly) return;
    _dismissLink();
    _focus.requestFocus();
    if (_connection?.attached == true) _connection!.show();
    _attach();
    _goal = null;
    final word = surface.wordAt(surface.globalToLocal(details.globalPosition));
    _hideTouchSelection();
    _longPressWord = word;
    _command(SetSelection(word.$1, word.$2));
    unawaited(Feedback.forLongPress(context));
    _showTouchSelection(menu: false);
    _touchSelection?.showMagnifier(
      _magnifierAt(details.globalPosition, c.editor.selection.extent),
    );
  }

  /// Dragging a long press extends the selection by words.
  void _longPressMove(LongPressMoveUpdateDetails details) {
    final surface = _surface, origin = _longPressWord;
    if (surface == null || origin == null) return;
    final word = surface.wordAt(surface.globalToLocal(details.globalPosition));
    _command(
      word.$2 >= origin.$2
          ? SetSelection(origin.$1, word.$2)
          : SetSelection(origin.$2, word.$1),
    );
    _showTouchSelection(menu: false);
    _touchSelection?.updateMagnifier(
      _magnifierAt(details.globalPosition, c.editor.selection.extent),
    );
  }

  void _longPressEnd(LongPressEndDetails details) {
    if (_longPressWord == null) return;
    _longPressWord = null;
    _touchSelection?.hideMagnifier();
    _showTouchSelection();
  }

  /// A long press the platform cancels after it began keeps its selection
  /// and handles, without the magnifier or menu.
  void _longPressCancel() {
    if (_longPressWord == null) return;
    _longPressWord = null;
    _touchSelection?.hideMagnifier();
  }

  void _startHandleDrag(DragStartDetails details, bool start) {
    final surface = _surface;
    if (surface == null) return;
    final selection = c.editor.selection;
    final moving = start ? selection.start : selection.end;
    final line = surface.localToGlobal(surface.caretRectAt(moving).center);
    _handleDrag = (
      // A handle hangs below its line. The finger keeps its height from the
      // line's middle and moves the end with its own x, as Flutter's text
      // fields do.
      grab: Offset(0, line.dy - details.globalPosition.dy),
      fixed: start ? selection.end : selection.start,
    );
    _touchSelection
      ?..hideToolbar()
      ..showMagnifier(_magnifierAt(details.globalPosition, moving));
  }

  void _updateHandleDrag(DragUpdateDetails details, bool start) {
    final surface = _surface, drag = _handleDrag;
    if (surface == null || drag == null) return;
    final hit = surface.hitAt(
      surface.globalToLocal(details.globalPosition + drag.grab),
    );
    // The ends keep their order and never meet.
    if (start ? hit.source < drag.fixed : hit.source > drag.fixed) {
      // The fixed end stays on the line it is drawn on. An end dragged past
      // a wrapped line stays drawn at that line's end; a start there begins
      // the next line, as its selection does.
      final fixedAtLineEnd = surface.drawnAtLineEnd(drag.fixed);
      _command(SetSelection(drag.fixed, hit.source));
      if (start) {
        surface.placedAt(drag.fixed, lineEnd: fixedAtLineEnd);
      } else {
        surface.placedAt(hit.source, lineEnd: hit.lineEnd);
      }
      _scheduleGeometry();
    }
    _touchSelection?.updateMagnifier(
      _magnifierAt(details.globalPosition, c.editor.selection.extent),
    );
  }

  void _endHandleDrag(DragEndDetails details) {
    if (_handleDrag == null) return;
    _handleDrag = null;
    _touchSelection?.hideMagnifier();
    _showTouchSelection();
  }

  /// Over a selection on one line, or across the document's width, clamped
  /// to what is visible.
  TextSelectionToolbarAnchors _touchMenuAnchors() {
    final surface = _surface;
    if (surface == null) {
      return const TextSelectionToolbarAnchors(primaryAnchor: Offset.zero);
    }
    final (start, end) = _touchEnds(surface);
    final visible = surface.visibleRect;
    final oneLine = start.top == end.top;
    final x = oneLine
        ? (start.left + end.right) / 2
        : (visible.left + visible.right) / 2;
    return TextSelectionToolbarAnchors(
      primaryAnchor: surface.localToGlobal(
        Offset(x, start.top.clamp(visible.top, visible.bottom)),
      ),
      secondaryAnchor: surface.localToGlobal(
        Offset(x, end.bottom.clamp(visible.top, visible.bottom)),
      ),
    );
  }

  Widget _touchMenu(BuildContext context) {
    final selection = c.editor.selection;
    return AdaptiveTextSelectionToolbar.buttonItems(
      anchors: _touchMenuAnchors(),
      buttonItems: [
        if (!selection.isCollapsed) ...[
          ContextMenuButtonItem(
            type: ContextMenuButtonType.cut,
            onPressed: () {
              _hideTouchSelection();
              unawaited(_copy(cut: true));
            },
          ),
          ContextMenuButtonItem(
            type: ContextMenuButtonType.copy,
            onPressed: () {
              final end = c.editor.selection.end;
              unawaited(_copy());
              // Android collapses a copied selection; iOS keeps it.
              if (Theme.of(this.context).platform == TargetPlatform.iOS) {
                _touchSelection?.hideToolbar();
              } else {
                _command(SetSelection.caret(end));
              }
            },
          ),
        ],
        ContextMenuButtonItem(
          type: ContextMenuButtonType.paste,
          onPressed: () {
            _hideTouchSelection();
            unawaited(_paste());
          },
        ),
        ContextMenuButtonItem(
          type: ContextMenuButtonType.selectAll,
          onPressed: () {
            _selectAll();
            _showTouchSelection();
          },
        ),
      ],
    );
  }

  /// A touch under which the document scrolled past the touch slop is a
  /// scroll, not a tap, whatever the finger's own movement. A scroll hides the
  /// menu; it returns when a scroll ends with the selection in view.
  bool _scrollNotification(ScrollNotification notification) {
    final touch = _touch;
    if (touch != null && notification is ScrollUpdateNotification) {
      final scrolled = touch.scrolled + (notification.scrollDelta ?? 0).abs();
      _touch = scrolled > _touchSlop
          ? null
          : (
              pointer: touch.pointer,
              position: touch.position,
              scrolled: scrolled,
            );
    }
    final surface = _surface, overlay = _touchSelection;
    if (notification is ScrollEndNotification &&
        _menuAfterScroll &&
        surface != null &&
        overlay != null) {
      final (start, end) = _touchEnds(surface);
      if (start.expandToInclude(end).overlaps(surface.visibleRect)) {
        _menuAfterScroll = false;
        overlay.showToolbar(context: context, contextMenuBuilder: _touchMenu);
      }
    }
    return false;
  }

  Future<void> _copy({bool cut = false}) async {
    if (c.editor.selection.isCollapsed) return;
    final target = c, revision = c.editor.revision;
    await Clipboard.setData(ClipboardData(text: target.selectedText));
    if (cut && mounted && identical(target, c) && !widget.readOnly) {
      target.command(const DeleteBackward(), expectedRevision: revision);
    }
  }

  Future<void> _paste() async {
    final target = c, revision = c.editor.revision;
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (mounted &&
        identical(target, c) &&
        !widget.readOnly &&
        data?.text != null) {
      target.command(Paste(data!.text!), expectedRevision: revision);
    }
  }

  Future<FlarkEditResult> _editResource(bool image) async {
    if (_resourceDialogOpen ||
        widget.readOnly ||
        c.editor.sourceMode ||
        c.editor.document.rowAt(c.editor.selection.extent).kind ==
            RowKind.codeBlock) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unavailable);
    }
    _dismissLink();
    c.finishComposition();
    final target = c,
        revision = c.editor.revision,
        selection = c.value.selection;
    final resource = c.editor.document.resourceAt(
      c.editor.selection,
      image: image,
    );
    final uri = resource == null
        ? null
        : flarkOpenableUri(resource.destination, widget.baseUri);
    final previousFocus = FocusManager.instance.primaryFocus;
    final route = ModalRoute.of(context);
    bool active() =>
        mounted &&
        identical(c, target) &&
        !widget.readOnly &&
        !target.editor.sourceMode &&
        target.editor.revision == revision &&
        target.value.selection == selection;
    var applied = false;
    final session = FlarkResourceSession(
      image: image,
      resource: resource,
      selectedText: target.selectedText,
      isActive: active,
      apply: (command) => (applied =
          active() && target.command(command, expectedRevision: revision)),
      onOpen: !image && uri != null && widget.onOpenLink != null
          ? () {
              if (active()) widget.onOpenLink?.call(uri);
            }
          : null,
    );
    _resourceSession = session;
    try {
      final presenter = widget.presentResourceEditor;
      if (presenter != null) {
        await presenter(context, session);
      } else {
        await showDialog<void>(
          context: context,
          builder: (_) => ResourceDialog(session: session),
        );
      }
    } finally {
      session.close();
      // A document swap may already have opened a different presentation.
      // Only the current session can release the guard or restore focus.
      if (identical(_resourceSession, session)) {
        _resourceSession = null;
        final focus = FocusManager.instance.primaryFocus;
        if (mounted &&
            identical(c, target) &&
            !widget.readOnly &&
            route?.isCurrent != false &&
            (focus == previousFocus ||
                focus == null ||
                focus == _focus ||
                focus is FocusScopeNode)) {
          _focus.requestFocus();
        }
      }
    }
    if (applied) return const FlarkEditResult.changed();
    if (!mounted || target.editor.revision != revision) {
      return const FlarkEditResult.rejected(FlarkEditRejection.staleRevision);
    }
    return const FlarkEditResult.unchanged();
  }

  bool get _linkActive =>
      _link != null &&
      mounted &&
      identical(c, _linkController) &&
      !widget.readOnly &&
      !c.editor.sourceMode &&
      c.editor.revision == _linkRevision &&
      c.value.selection == _linkSelection;

  void _dismissLink({bool restoreFocus = false}) {
    _link = null;
    _linkEpoch++;
    if (_popover.isShowing) {
      // A rebuild with another controller, or read-only, dismisses it while
      // the framework builds this widget, when an overlay portal must not
      // change. Inactive, it already builds nothing; hide it after the frame.
      if (SchedulerBinding.instance.schedulerPhase ==
          SchedulerPhase.persistentCallbacks) {
        final epoch = _linkEpoch;
        SchedulerBinding.instance.addPostFrameCallback((_) {
          if (mounted && epoch == _linkEpoch && _popover.isShowing) {
            _popover.hide();
          }
        }, debugLabel: 'FlarkEditorWidget.dismissLink');
      } else {
        _popover.hide();
      }
    }
    if (restoreFocus && mounted && !widget.readOnly) _focus.requestFocus();
  }

  bool _showSelectionLink() {
    if (widget.readOnly ||
        c.editor.sourceMode ||
        !c.editor.selection.isCollapsed) {
      return false;
    }
    final link = c.editor.document.resourceAt(c.editor.selection, image: false);
    if (link == null) return false;
    _showLink(link, keyboard: true);
    return true;
  }

  void _showLink(InlineResource link, {bool keyboard = false}) {
    if (widget.readOnly ||
        _resourceDialogOpen ||
        !c.editor.selection.isCollapsed) {
      return;
    }
    _link = link;
    _linkController = c;
    _linkRevision = c.editor.revision;
    _linkSelection = c.value.selection;
    _linkEpoch++;
    _popover.show();
    if (keyboard) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _linkActive) _popoverFocus.nextFocus();
      });
    }
  }

  Widget _buildLinkPopover(
    BuildContext overlayContext,
    OverlayChildLayoutInfo info,
  ) {
    if (!_linkActive) return const SizedBox.shrink();
    final link = _link!, epoch = _linkEpoch;
    bool active() => epoch == _linkEpoch && _linkActive;
    final uri = flarkOpenableUri(link.destination, widget.baseUri);
    final actions = FlarkLinkActions(
      resource: link,
      dismiss: () {
        if (active()) _dismissLink(restoreFocus: _popoverFocus.hasFocus);
      },
      open: uri == null || widget.onOpenLink == null
          ? null
          : () {
              if (active()) widget.onOpenLink?.call(uri);
            },
      edit: () {
        if (active()) unawaited(_editResource(false));
      },
      remove: () {
        if (!active()) return;
        _dismissLink();
        _command(const RemoveLink());
        _focus.requestFocus();
      },
    );
    final media = MediaQuery.of(overlayContext);
    return CustomSingleChildLayout(
      delegate: LinkPopoverLayout(
        () {
          final surface = _surface;
          if (surface == null || !active()) return Rect.zero;
          final rect =
              surface.linkRect(
                link,
                nearSource: _linkSelection!.extentOffset,
              ) ??
              surface.caretRect;
          return MatrixUtils.transformRect(info.childPaintTransform, rect);
        },
        Rect.fromLTWH(
          0,
          media.padding.top,
          info.overlaySize.width,
          (info.overlaySize.height -
                  media.padding.top -
                  media.viewInsets.bottom -
                  media.padding.bottom)
              .clamp(0, double.infinity),
        ),
      ),
      child: TapRegion(
        groupId: this,
        child: FocusScope(
          node: _popoverFocus,
          onKeyEvent: (_, event) {
            if (event is KeyDownEvent &&
                event.logicalKey == LogicalKeyboardKey.escape) {
              _dismissLink(restoreFocus: true);
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: SingleChildScrollView(
            child:
                widget.linkPopoverBuilder?.call(overlayContext, actions) ??
                FlarkLinkPopover(actions: actions),
          ),
        ),
      ),
    );
  }

  void _tab(bool backward) {
    if (!c.editor.sourceMode) {
      final row = c.editor.document.caretRow;
      if (row.kind == RowKind.tableCell) {
        _command(MoveTableCell(backward: backward));
        return;
      }
    }
    _command(backward ? const Outdent() : const Indent());
  }

  void _selectAll() {
    _command(const SelectAll());
  }

  void _vertical(bool down, bool extend) {
    final s = _surface;
    if (s == null) return;
    final goal = _goal;
    final x =
        goal != null &&
            identical(goal.controller, c) &&
            goal.revision == c.editor.revision
        ? goal.x
        : s.caretRect.left;
    s.vertical(down, extend: extend, goalX: x);
    _goal = (x: x, controller: c, revision: c.editor.revision);
    _scheduleGeometry();
  }

  void _lineEdge(bool end, {required bool extend}) {
    _goal = null;
    _surface?.lineEdge(end, extend: extend);
    _scheduleGeometry();
  }

  /// Command-Backspace on Apple platforms deletes from the caret to the start
  /// of its visual line, and Command-Delete to its end, as one edit that Undo
  /// restores; Cocoa sends deleteToBeginningOfLine: and deleteToEndOfLine:
  /// for the same keys. A selection is deleted as it is, and a caret already
  /// at that edge deletes one grapheme, as in Cocoa.
  void _deleteToLineEdge({required bool forward}) {
    final selection = c.editor.selection;
    final caret = selection.extent;
    final edge = selection.isCollapsed
        ? _surface?.lineEdgeTarget(forward)
        : null;
    if (edge != null && (forward ? edge > caret : edge < caret)) {
      _command(
        forward ? ReplaceRange(caret, edge, '') : ReplaceRange(edge, caret, ''),
      );
    } else {
      _command(forward ? const DeleteForward() : const DeleteBackward());
    }
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (const bool.fromEnvironment('FLARK_TRACE_INPUT')) {
      debugPrint(
        'flark key ${event.runtimeType} ${event.logicalKey.keyId} focused=${_focus.hasFocus}',
      );
    }
    if (event is KeyUpEvent || widget.readOnly) return KeyEventResult.ignored;
    // Keys a focused descendant does not handle bubble here, such as those of
    // a field or button in the link popover. They are that widget's: taken
    // here, Enter would edit the document instead of pressing the button, Tab
    // would not move focus, and a reattach would take the field's input.
    if (!_focus.hasPrimaryFocus) return KeyEventResult.ignored;
    if (_connection?.attached != true) {
      // Keys still reach the focused editor after the platform closed its
      // connection, but typed characters cannot. A key means the user is
      // typing here, so it reopens input rather than drop every character
      // until a press. Chrome still delivers this key's character to the new
      // input element; elsewhere that one may be lost. The new connection is
      // sent the editor's geometry after a frame, which nothing else may
      // schedule.
      _attach();
      SchedulerBinding.instance.ensureVisualUpdate();
    }
    final keyboard = HardwareKeyboard.instance;
    final shift = keyboard.isShiftPressed,
        primary = keyboard.isMetaPressed || keyboard.isControlPressed;
    // Apple platforms reach line edges with Command and move by word with
    // Option. Windows and Linux move by word with Control and reach line
    // edges with Home and End. On the web this follows the browser's
    // operating system.
    final apple = switch (defaultTargetPlatform) {
      TargetPlatform.iOS || TargetPlatform.macOS => true,
      _ => false,
    };
    final key = event.logicalKey;
    final horizontal =
        key == LogicalKeyboardKey.arrowLeft ||
        key == LogicalKeyboardKey.arrowRight;
    if (key == LogicalKeyboardKey.escape && _popover.isShowing) {
      _dismissLink();
      return KeyEventResult.handled;
    }
    if (c.editor.composing) {
      if (key == LogicalKeyboardKey.escape) {
        c.finishComposition(cancel: true);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }
    if ((key == LogicalKeyboardKey.f10 && shift ||
            key == LogicalKeyboardKey.contextMenu) &&
        _showSelectionLink()) {
      return KeyEventResult.handled;
    }
    if ((keyboard.isAltPressed || keyboard.isControlPressed) &&
        (key == LogicalKeyboardKey.backspace ||
            key == LogicalKeyboardKey.delete)) {
      _command(
        key == LogicalKeyboardKey.backspace
            ? const DeleteBackward(word: true)
            : const DeleteForward(word: true),
      );
      return KeyEventResult.handled;
    }
    if (apple &&
        keyboard.isMetaPressed &&
        (key == LogicalKeyboardKey.backspace ||
            key == LogicalKeyboardKey.delete)) {
      _deleteToLineEdge(forward: key == LogicalKeyboardKey.delete);
      return KeyEventResult.handled;
    }
    if (!apple && keyboard.isControlPressed && horizontal) {
      _command(
        MoveCaret(
          key == LogicalKeyboardKey.arrowRight
              ? MoveDirection.forward
              : MoveDirection.backward,
          unit: MoveUnit.word,
          extend: shift,
        ),
      );
      return KeyEventResult.handled;
    }
    if (primary) {
      if (kIsWeb &&
          (key == LogicalKeyboardKey.keyC ||
              key == LogicalKeyboardKey.keyX ||
              key == LogicalKeyboardKey.keyV)) {
        // Let the browser deliver clipboard data under the user's gesture.
        // Merely ignoring this key lets Flutter's ancestor Shortcuts consume
        // it and attempt a separate, permission-dependent clipboard API call.
        return KeyEventResult.skipRemainingHandlers;
      }
      final documentStart =
          (keyboard.isMetaPressed && key == LogicalKeyboardKey.arrowUp) ||
          (keyboard.isControlPressed && key == LogicalKeyboardKey.home);
      final documentEnd =
          (keyboard.isMetaPressed && key == LogicalKeyboardKey.arrowDown) ||
          (keyboard.isControlPressed && key == LogicalKeyboardKey.end);
      if (documentStart || documentEnd) {
        final target = documentEnd ? c.text.length : 0;
        _command(
          SetSelection(shift ? c.editor.selection.base : target, target),
        );
      } else if (key == LogicalKeyboardKey.keyA) {
        _selectAll();
      } else if (key == LogicalKeyboardKey.keyC) {
        unawaited(_copy());
      } else if (key == LogicalKeyboardKey.keyX) {
        unawaited(_copy(cut: true));
      } else if (key == LogicalKeyboardKey.keyV) {
        unawaited(_paste());
      } else if (key == LogicalKeyboardKey.keyZ) {
        _command(shift ? const Redo() : const Undo());
      } else if (key == LogicalKeyboardKey.keyB) {
        _command(const ToggleStyle(Style.strong));
      } else if (key == LogicalKeyboardKey.keyI) {
        _command(const ToggleStyle(Style.emphasis));
      } else if (key == LogicalKeyboardKey.keyK) {
        unawaited(_editResource(false));
      } else if (horizontal) {
        _lineEdge(key == LogicalKeyboardKey.arrowRight, extend: shift);
      } else {
        return KeyEventResult.ignored;
      }
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      _command(Newline(paragraph: shift));
    } else if (key == LogicalKeyboardKey.backspace) {
      _command(const DeleteBackward());
    } else if (key == LogicalKeyboardKey.delete) {
      _command(const DeleteForward());
    } else if (horizontal) {
      _command(
        MoveCaret(
          key == LogicalKeyboardKey.arrowRight
              ? MoveDirection.forward
              : MoveDirection.backward,
          unit: keyboard.isAltPressed ? MoveUnit.word : MoveUnit.grapheme,
          extend: shift,
        ),
      );
    } else if (key == LogicalKeyboardKey.arrowUp ||
        key == LogicalKeyboardKey.arrowDown) {
      _vertical(key == LogicalKeyboardKey.arrowDown, shift);
    } else if (key == LogicalKeyboardKey.home ||
        key == LogicalKeyboardKey.end) {
      _lineEdge(key == LogicalKeyboardKey.end, extend: shift);
    } else if (key == LogicalKeyboardKey.tab) {
      _tab(shift);
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  void _selector(String selector) {
    if (const bool.fromEnvironment('FLARK_TRACE_INPUT')) {
      debugPrint('flark selector $selector');
    }
    switch (selector) {
      case 'insertNewline:':
        _command(const Newline());
      case 'deleteBackward:':
        _command(const DeleteBackward());
      case 'deleteForward:':
        _command(const DeleteForward());
      case 'deleteToBeginningOfLine:':
        _deleteToLineEdge(forward: false);
      case 'deleteToEndOfLine:':
        _deleteToLineEdge(forward: true);
      case 'moveLeft:':
        _command(const MoveCaret(MoveDirection.backward));
      case 'moveRight:':
        _command(const MoveCaret(MoveDirection.forward));
      case 'moveUp:':
        _vertical(false, false);
      case 'moveDown:':
        _vertical(true, false);
      case 'moveLeftAndModifySelection:':
        _command(const MoveCaret(MoveDirection.backward, extend: true));
      case 'moveRightAndModifySelection:':
        _command(const MoveCaret(MoveDirection.forward, extend: true));
      case 'selectAll:':
        _selectAll();
      case 'copy:':
        unawaited(_copy());
      case 'cut:':
        unawaited(_copy(cut: true));
      case 'paste:':
        unawaited(_paste());
      case 'undo:':
        widget.actions == null
            ? _command(const Undo())
            : widget.actions!.undo();
      case 'redo:':
        widget.actions == null
            ? _command(const Redo())
            : widget.actions!.redo();
    }
  }

  @override
  void dispose() {
    _touchSelection?.dispose();
    _resourceSession?.close();
    _popoverFocus.dispose();
    _clipboardBinding.dispose();
    c.removeListener(_changed);
    // The controller can outlive this view. Its composition belonged to the
    // connection closed here, and left open it would make a later view
    // resend a stale composing range and ignore editing keys.
    _endComposition(c);
    _focus.removeListener(_focusChanged);
    _close();
    if (widget.focusNode == null) _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Widget _styleButton(String label, IconData icon, int style) {
    final publicStyle = FlarkStyle.values.firstWhere(
      (value) => value.kernelStyle == style,
    );
    final state =
        widget.actions?.state.styles[publicStyle] ?? c.styleState(style);
    final colors = Theme.of(context).colorScheme;
    return MergeSemantics(
      child: Semantics(
        value: state.isMixed ? 'Mixed' : null,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 2),
          child: IconButton(
            tooltip: label,
            isSelected: state.isOn,
            style: ButtonStyle(
              shape: WidgetStatePropertyAll(
                RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
              ),
              backgroundColor: WidgetStateProperty.resolveWith((states) {
                if (states.contains(WidgetState.selected)) {
                  return colors.secondaryContainer;
                }
                if (state.isMixed) {
                  return colors.secondaryContainer.withValues(alpha: 0.45);
                }
                return null;
              }),
              foregroundColor: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.disabled)
                    ? null
                    : states.contains(WidgetState.selected)
                    ? colors.onSecondaryContainer
                    : null,
              ),
              side: WidgetStatePropertyAll(
                state.isMixed
                    ? BorderSide(color: colors.outline)
                    : BorderSide.none,
              ),
            ),
            onPressed: widget.readOnly || !state.canToggle
                ? null
                : () {
                    final actions = widget.actions;
                    if (actions != null) {
                      actions.setStyle(publicStyle, enabled: !state.isOn);
                    } else {
                      _command(SetStyle(style, enabled: !state.isOn));
                    }
                    _focus.requestFocus();
                  },
            icon: state.isMixed
                ? Stack(
                    clipBehavior: Clip.none,
                    children: [
                      Icon(icon, size: 20),
                      const Positioned(
                        right: -5,
                        bottom: -5,
                        child: Icon(Icons.remove, size: 10),
                      ),
                    ],
                  )
                : Icon(icon, size: 20),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final e = c.editor;
    final resolvedTheme = _theme ??= FlarkThemeData.resolve(
      context,
      overrides: widget.theme,
      bodyStyle: widget.style,
    );
    final codeRow = !e.sourceMode ? e.document.rowAt(e.selection.extent) : null;
    final info = codeRow?.fenced == true
        ? e.source.substring(codeRow!.codeInfoStart, codeRow.codeInfoEnd)
        : '';
    final codeLanguage = codeMirrorLanguageName(info);
    final detected = codeRow?.fenced == true && codeLanguage.isEmpty
        ? e.codeEditing?.resolveLanguage(codeRow!.text, info)
        : null;
    // The languages the editor's delegate highlights, or every ported one.
    final delegate = e.codeEditing;
    final codeLanguages = {
      for (final language
          in delegate is FlarkCodeMirror
              ? delegate.languages
              : CodeMirrorLanguages.all)
        language.name: language.label,
      'text': 'Plain text',
    };
    final detectedLabel = codeLanguages[detected];
    final headingFormatting =
        codeRow != null &&
        (codeRow.kind == RowKind.paragraph ||
            codeRow.kind == RowKind.heading ||
            (codeRow.kind == RowKind.blank && e.selection.isCollapsed));
    void captureMenu() => _toolbarMenuTarget = (c, e.revision, e.selection);
    bool menuActive() {
      final target = _toolbarMenuTarget;
      return mounted &&
          target != null &&
          identical(c, target.$1) &&
          !widget.readOnly &&
          !e.sourceMode &&
          e.revision == target.$2 &&
          e.selection == target.$3;
    }

    final window = e.sourceMode
        ? SourceWindow.at(c.text, e.selection.extent)
        : null;
    _sourceWindowStart = window?.start;
    final contents = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.showToolbar && !widget.readOnly)
          Material(
            color: Theme.of(context).colorScheme.surfaceContainerLow,
            child: Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                if (codeRow?.fenced == true)
                  PopupMenuButton<String>(
                    tooltip: 'Code language',
                    onOpened: captureMenu,
                    initialValue: codeLanguage,
                    onSelected: (language) {
                      if (menuActive() && language != codeLanguage) {
                        (widget.actions == null
                            ? _command(SetCodeLanguage(language))
                            : widget.actions!.setCodeLanguage(language));
                      }
                      _focus.requestFocus();
                    },
                    itemBuilder: (_) => [
                      const PopupMenuItem(value: '', child: Text('Automatic')),
                      const PopupMenuItem(
                        value: 'text',
                        child: Text('Plain text'),
                      ),
                      for (final entry in codeLanguages.entries)
                        if (entry.key != 'text')
                          PopupMenuItem(
                            value: entry.key,
                            child: Text(entry.value),
                          ),
                    ],
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 12,
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          // An unknown language shows its info string's
                          // first word, which can be any length.
                          Flexible(
                            child: Text(
                              codeLanguage.isEmpty
                                  ? 'Auto${detectedLabel == null ? '' : ' · $detectedLabel'}'
                                  : codeLanguages[codeLanguage] ?? codeLanguage,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          const Icon(Icons.arrow_drop_down, size: 18),
                        ],
                      ),
                    ),
                  ),
                PopupMenuButton<int>(
                  tooltip: 'Paragraph style',
                  onOpened: captureMenu,
                  enabled:
                      widget.actions?.state.heading.canSet ?? headingFormatting,
                  initialValue: codeRow?.headingLevel ?? 0,
                  onSelected: (level) {
                    if (menuActive()) {
                      if (widget.actions != null) {
                        widget.actions!.setHeading(level);
                      } else {
                        _command(SetHeadingLevel(level));
                      }
                    }
                    _focus.requestFocus();
                  },
                  itemBuilder: (_) => [
                    const PopupMenuItem(value: 0, child: Text('Paragraph')),
                    for (var level = 1; level <= 6; level++)
                      PopupMenuItem(
                        value: level,
                        child: Text('Heading $level'),
                      ),
                  ],
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 12,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          codeRow?.kind == RowKind.heading
                              ? 'Heading ${codeRow!.headingLevel}'
                              : 'Paragraph',
                        ),
                        const Icon(Icons.arrow_drop_down, size: 18),
                      ],
                    ),
                  ),
                ),
                _styleButton('Bold', Icons.format_bold, Style.strong),
                _styleButton('Italic', Icons.format_italic, Style.emphasis),
                _styleButton(
                  'Strikethrough',
                  Icons.format_strikethrough,
                  Style.strikethrough,
                ),
                _styleButton('Inline code', Icons.code, Style.code),
                IconButton(
                  tooltip: 'Link',
                  icon: const Icon(Icons.link, size: 20),
                  onPressed:
                      !(widget.actions?.state.link.canSet ?? e.canSetResource())
                      ? null
                      : () =>
                            widget.actions?.showLinkEditor() ??
                            _editResource(false),
                ),
                IconButton(
                  tooltip: 'Image',
                  icon: const Icon(Icons.image_outlined, size: 20),
                  onPressed: !e.canSetResource(image: true)
                      ? null
                      : () =>
                            widget.actions?.showImageEditor() ??
                            _editResource(true),
                ),
                IconButton(
                  tooltip: 'Undo',
                  onPressed:
                      (widget.actions?.state.canUndo ?? e.history.canUndo)
                      ? () {
                          widget.actions == null
                              ? _command(const Undo())
                              : widget.actions!.undo();
                          _focus.requestFocus();
                        }
                      : null,
                  icon: const Icon(Icons.undo, size: 20),
                ),
                IconButton(
                  tooltip: 'Redo',
                  onPressed:
                      (widget.actions?.state.canRedo ?? e.history.canRedo)
                      ? () {
                          widget.actions == null
                              ? _command(const Redo())
                              : widget.actions!.redo();
                          _focus.requestFocus();
                        }
                      : null,
                  icon: const Icon(Icons.redo, size: 20),
                ),
                TextButton(
                  onPressed: () {
                    widget.actions == null
                        ? c.sourceMode(!e.sourceMode)
                        : widget.actions!.setSourceMode(!e.sourceMode);
                    _focus.requestFocus();
                  },
                  child: Text(e.sourceMode ? 'Rendered' : 'Source'),
                ),
              ],
            ),
          ),
        if (e.sourceMode || c.notice != null)
          Material(
            color: Theme.of(context).colorScheme.errorContainer,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              child: Text(
                c.notice ?? 'Source mode · exact Markdown remains editable',
                style: const TextStyle(fontSize: 12),
              ),
            ),
          ),
        if (window != null && window.count > 1)
          Row(
            children: [
              IconButton(
                tooltip: 'Previous source page',
                onPressed: window.previous == null
                    ? null
                    : () {
                        _command(SetSelection.caret(window.previous!));
                        _focus.requestFocus();
                      },
                icon: const Icon(Icons.chevron_left),
              ),
              Text(
                'Source page ${window.index + 1} of ${window.count}',
                style: const TextStyle(fontSize: 12),
              ),
              IconButton(
                tooltip: 'Next source page',
                onPressed: window.next == null
                    ? null
                    : () {
                        _command(SetSelection.caret(window.next!));
                        _focus.requestFocus();
                      },
                icon: const Icon(Icons.chevron_right),
              ),
            ],
          ),
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) => Actions(
              actions: {
                SelectAllTextIntent: CallbackAction<SelectAllTextIntent>(
                  onInvoke: (_) {
                    _selectAll();
                    return null;
                  },
                ),
                CopySelectionTextIntent:
                    CallbackAction<CopySelectionTextIntent>(
                      onInvoke: (intent) => _copy(
                        cut: intent.collapseSelection && !widget.readOnly,
                      ),
                    ),
                PasteTextIntent: CallbackAction<PasteTextIntent>(
                  onInvoke: (_) => widget.readOnly ? null : _paste(),
                ),
                UndoTextIntent: CallbackAction<UndoTextIntent>(
                  onInvoke: (_) =>
                      widget.readOnly ? null : _command(const Undo()),
                ),
                RedoTextIntent: CallbackAction<RedoTextIntent>(
                  onInvoke: (_) =>
                      widget.readOnly ? null : _command(const Redo()),
                ),
              },
              child: Focus(
                focusNode: _focus,
                // The surface owns the text-field semantics. A second focused
                // ancestor makes Flutter web move DOM focus away from it.
                includeSemantics: false,
                autofocus: widget.autofocus,
                onKeyEvent: _key,
                child: MouseRegion(
                  cursor: widget.readOnly
                      ? SystemMouseCursors.basic
                      : SystemMouseCursors.text,
                  child: GestureDetector(
                    // The surface owns the text field's semantics; a long
                    // press action on an ancestor would select at the origin.
                    excludeFromSemantics: true,
                    // A mouse held still before a drag selects by dragging.
                    supportedDevices: const {
                      PointerDeviceKind.touch,
                      PointerDeviceKind.stylus,
                      PointerDeviceKind.invertedStylus,
                    },
                    onLongPressStart: widget.readOnly ? null : _longPressStart,
                    onLongPressMoveUpdate: widget.readOnly
                        ? null
                        : _longPressMove,
                    onLongPressEnd: widget.readOnly ? null : _longPressEnd,
                    onLongPressCancel: widget.readOnly
                        ? null
                        : _longPressCancel,
                    // With nothing else competing, the reader's scroll view
                    // would claim a touch at once and scroll by a finger's
                    // jitter, losing link taps. A tap recognizer makes it
                    // wait for the touch slop, as the editor's gestures do.
                    onTap: widget.readOnly ? () {} : null,
                    child: GestureDetector(
                      onDoubleTapDown: widget.readOnly
                          ? null
                          : (details) {
                              _dragging = false;
                              // The second touch must not collapse the word
                              // when it lifts.
                              _touch = null;
                              final surface = _surface;
                              if (surface != null) {
                                surface.selectWordAt(
                                  surface.globalToLocal(details.globalPosition),
                                );
                                if (_isTouch(details.kind!)) {
                                  _showTouchSelection();
                                }
                              }
                            },
                      child: Listener(
                        onPointerDown: (event) {
                          _dismissLink();
                          _pressedLink = null;
                          if (_isTouch(event.kind)) return;
                          _press(
                            event.position,
                            primary: event.buttons == kPrimaryButton,
                            drag:
                                event.kind == PointerDeviceKind.mouse &&
                                event.buttons == kPrimaryButton,
                          );
                        },
                        onPointerMove: (event) {
                          final touch = _touch;
                          if (touch != null &&
                              touch.pointer == event.pointer &&
                              (event.position - touch.position).distance >
                                  _touchSlop) {
                            _touch = null;
                          }
                          if (_pressedLink != null &&
                              (event.position - _pressedLink!.point).distance >
                                  8) {
                            _pressedLink = null;
                          }
                          if (_pressedImage != null &&
                              (event.position - _pressedImage!.point).distance >
                                  8) {
                            _pressedImage = null;
                          }
                          if (_dragging) {
                            final surface = _surface;
                            if (surface != null) {
                              surface.place(
                                surface.globalToLocal(event.position),
                                extend: true,
                              );
                              _scheduleGeometry();
                            }
                          }
                        },
                        onPointerUp: (event) {
                          final touch = _touch;
                          _touch = null;
                          if (touch != null && touch.pointer == event.pointer) {
                            _press(touch.position, touch: true);
                          }
                          _dragging = false;
                          final pressedLink = _pressedLink;
                          _pressedLink = null;
                          if (pressedLink != null) {
                            _showLink(pressedLink.resource);
                          }
                          final pressed = _pressedImage;
                          _pressedImage = null;
                          if (pressed != null &&
                              pressed.revision == c.editor.revision) {
                            _command(
                              SetSelection(
                                pressed.image.contentStart,
                                pressed.image.contentEnd,
                              ),
                            );
                            unawaited(_editResource(true));
                          }
                        },
                        onPointerCancel: (_) {
                          _touch = null;
                          _pressedLink = null;
                          _dragging = false;
                          _pressedImage = null;
                        },
                        child: NotificationListener<ScrollNotification>(
                          onNotification: _scrollNotification,
                          child: Scrollbar(
                            controller: _scroll,
                            child: SingleChildScrollView(
                              controller: _scroll,
                              // The scroll view ignores its content's pointers
                              // while it moves, so a touch that stops a fling is
                              // never recorded here.
                              child: Listener(
                                onPointerDown: (event) {
                                  if (_isTouch(event.kind)) {
                                    _touch = (
                                      pointer: event.pointer,
                                      position: event.position,
                                      scrolled: 0,
                                    );
                                  }
                                },
                                child: OverlayPortal.overlayChildLayoutBuilder(
                                  controller: _popover,
                                  overlayChildBuilder: _buildLinkPopover,
                                  child: FlarkSurface(
                                    key: _surfaceKey,
                                    controller: c,
                                    theme: resolvedTheme,
                                    textScaler: MediaQuery.textScalerOf(
                                      context,
                                    ),
                                    focused: _focus.hasFocus,
                                    readOnly: widget.readOnly,
                                    viewportHeight: constraints.maxHeight,
                                    scrollOffset: _scroll.hasClients
                                        ? _scroll.offset
                                        : 0,
                                    baseUri: widget.baseUri,
                                    imageProvider: widget.imageProvider,
                                    showImagePreviews: widget.showImagePreviews,
                                    onPaint: widget.onPaint,
                                    onFocus: _focus.requestFocus,
                                    revealDuringLayout: _revealDuringLayout,
                                    handles: _touchHandles
                                        ? (_startHandle, _endHandle)
                                        : null,
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
    // The touch selection menu follows the editor.
    return CompositedTransformTarget(
      link: _menuLink,
      child: TapRegion(
        groupId: this,
        onTapOutside: (_) => _dismissLink(),
        child: Semantics(
          customSemanticsActions: {
            if (!widget.readOnly)
              const CustomSemanticsAction(label: 'Link actions'): () {
                _showSelectionLink();
              },
          },
          child: contents,
        ),
      ),
    );
  }
}

/// Touch and stylus scroll the document when they move, so they press only
/// when they lift.
bool _isTouch(PointerDeviceKind kind) =>
    kind == PointerDeviceKind.touch ||
    kind == PointerDeviceKind.stylus ||
    kind == PointerDeviceKind.invertedStylus;

/// Each connection has its own client. A callback captured before focus or
/// document replacement cannot mutate the new editor through a stale closure.
class _InputClient with TextInputClient, DeltaTextInputClient {
  _InputClient(this.state, this.epoch);
  final _FlarkEditorWidgetState state;
  final int epoch;
  bool get active =>
      state.mounted && !state.widget.readOnly && state._epoch == epoch;
  @override
  TextEditingValue? get currentTextEditingValue =>
      active ? state._inputContext?.value : null;
  @override
  AutofillScope? get currentAutofillScope => null;
  @override
  void updateEditingValue(TextEditingValue value) {
    if (const bool.fromEnvironment('FLARK_TRACE_INPUT')) {
      debugPrint('flark value active=$active length=${value.text.length}');
    }
    if (active) {
      state._receiveInput(value);
    }
  }

  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> deltas) {
    if (const bool.fromEnvironment('FLARK_TRACE_INPUT')) {
      debugPrint('flark delta active=$active count=${deltas.length}');
    }
    if (active) {
      var next = currentTextEditingValue!;
      for (final delta in deltas) {
        if (delta.oldText != next.text) {
          state._sentValue = null;
          state._sync();
          return;
        }
        next = delta.apply(next);
      }
      state._receiveInput(next, authenticated: true);
    }
  }

  /// Return reaches a multiline client as a key the editor handles or as a
  /// line break in the text; the newline action only reports it, as Flutter's
  /// own multiline fields read it. The web engine sends the action from the
  /// textarea's keydown listener after Flutter handled the Return key, so a
  /// Newline here typed a browser's every Return twice. iOS sends it before
  /// the line break it inserts, which that made stale, with any
  /// autocorrection UIKit sent alongside.
  @override
  void performAction(TextInputAction action) {}

  @override
  void performSelector(String selectorName) {
    if (active) state._selector(selectorName);
  }

  @override
  void connectionClosed() {
    if (active) state._connectionClosed();
  }

  @override
  void performPrivateCommand(String action, Map<String, dynamic> data) {}
  @override
  void updateFloatingCursor(RawFloatingCursorPoint point) {}
  @override
  void showAutocorrectionPromptRect(int start, int end) {}
  @override
  bool onFocusReceived() {
    if (!active) return false;
    state._focus.requestFocus();
    return true;
  }
}

class FlarkMarkdownView extends StatelessWidget {
  const FlarkMarkdownView({
    super.key,
    required this.controller,
    this.onPaint,
    this.style,
    this.theme,
    this.baseUri,
    this.imageProvider,
    this.showImagePreviews = true,
    this.onOpenLink,
  });
  final FlarkController controller;
  final TextStyle? style;
  final FlarkThemeData? theme;
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  final bool showImagePreviews;
  final ValueChanged<Uri>? onOpenLink;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  Widget build(BuildContext context) => FlarkEditorWidget(
    controller: controller,
    style: style,
    theme: theme,
    readOnly: true,
    showToolbar: false,
    baseUri: baseUri,
    imageProvider: imageProvider,
    showImagePreviews: showImagePreviews,
    onOpenLink: onOpenLink,
    onPaint: onPaint,
  );
}
