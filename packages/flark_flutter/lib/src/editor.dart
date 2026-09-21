import 'package:flark/session.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
  double? _goalX;
  bool _dragging = false;
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

  @override
  void didUpdateWidget(FlarkEditorWidget old) {
    super.didUpdateWidget(old);
    if (old.controller != c || old.readOnly != widget.readOnly) {
      _dismissLink();
      _resourceSession?.close();
      _resourceSession = null;
    }
    if (old.controller != c) {
      old.controller.removeListener(_changed);
      old.controller.finishComposition();
      c.addListener(_changed);
      _close();
    }
    if (old.focusNode != widget.focusNode) {
      _focus.removeListener(_focusChanged);
      _close();
      if (old.focusNode == null) _focus.dispose();
      _focus = widget.focusNode ?? FocusNode(debugLabel: 'Flark editor');
      _focus.addListener(_focusChanged);
    }
    if (widget.readOnly) {
      c.finishComposition();
      _close();
    } else if (_focus.hasFocus) {
      _attach();
    }
  }

  void _scrolled() {
    _dismissLink();
    if (mounted) {
      setState(() {});
      _scheduleGeometry();
    }
  }

  void _focusChanged() {
    if (_focus.hasFocus && !widget.readOnly) {
      _attach();
    } else {
      c.finishComposition();
      _close();
    }
    if (mounted) setState(() {});
  }

  void _attach() {
    if (_connection?.attached == true || widget.readOnly) return;
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

  void _receiveInput(TextEditingValue value) {
    final context = _inputContext;
    if (context == null) return;
    _sentValue = value;
    final full = context.expand(value);
    if (full != null) c.receive(full);
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
    _goalX = null;
    return c.command(command);
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
    if (_popover.isShowing) _popover.hide();
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
    _goalX ??= s.caretRect.left;
    s.vertical(down, extend: extend, goalX: _goalX);
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (const bool.fromEnvironment('FLARK_TRACE_INPUT')) {
      debugPrint(
        'flark key ${event.runtimeType} ${event.logicalKey.keyId} focused=${_focus.hasFocus}',
      );
    }
    if (event is KeyUpEvent || widget.readOnly) return KeyEventResult.ignored;
    final keyboard = HardwareKeyboard.instance;
    final shift = keyboard.isShiftPressed,
        primary = keyboard.isMetaPressed || keyboard.isControlPressed;
    final key = event.logicalKey;
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
      } else if (key == LogicalKeyboardKey.arrowLeft ||
          key == LogicalKeyboardKey.arrowRight) {
        _surface?.lineEdge(key == LogicalKeyboardKey.arrowRight, extend: shift);
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
    } else if (key == LogicalKeyboardKey.arrowLeft ||
        key == LogicalKeyboardKey.arrowRight) {
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
      _surface?.lineEdge(key == LogicalKeyboardKey.end, extend: shift);
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
    _resourceSession?.close();
    _popoverFocus.dispose();
    _clipboardBinding.dispose();
    c.removeListener(_changed);
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
    final resolvedTheme = FlarkThemeData.resolve(
      context,
      overrides: widget.theme,
      bodyStyle: widget.style,
    );
    final codeRow = !e.sourceMode ? e.document.rowAt(e.selection.extent) : null;
    final info = codeRow?.fenced == true
        ? e.source.substring(codeRow!.codeInfoStart, codeRow.codeInfoEnd)
        : '';
    final codeLanguage = codeLanguageName(info);
    final detected = codeRow?.fenced == true && codeLanguage.isEmpty
        ? e.codeEditing?.resolveLanguage(codeRow!.text, info)
        : null;
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
                          Text(
                            codeLanguage.isEmpty
                                ? 'Auto${detectedLabel == null ? '' : ' · $detectedLabel'}'
                                : codeLanguages[codeLanguage] ?? codeLanguage,
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
                    onDoubleTapDown: widget.readOnly
                        ? null
                        : (details) {
                            _dragging = false;
                            final surface = _surface;
                            if (surface != null) {
                              surface.selectWordAt(
                                surface.globalToLocal(details.globalPosition),
                              );
                            }
                          },
                    child: Listener(
                      onPointerDown: (event) {
                        _dismissLink();
                        _pressedLink = null;
                        final surface = _surface;
                        if (surface == null) return;
                        if (widget.readOnly ||
                            HardwareKeyboard.instance.isMetaPressed ||
                            HardwareKeyboard.instance.isControlPressed) {
                          final link = surface.linkAt(
                            surface.globalToLocal(event.position),
                          );
                          final uri = link == null
                              ? null
                              : flarkOpenableUri(
                                  link.destination,
                                  widget.baseUri,
                                );
                          if (uri != null && widget.onOpenLink != null) {
                            widget.onOpenLink!(uri);
                            return;
                          }
                        }
                        if (widget.readOnly) return;
                        _focus.requestFocus();
                        _attach();
                        _goalX = null;
                        final image = surface.imageAt(
                          surface.globalToLocal(event.position),
                        );
                        if (image != null) {
                          _dragging = false;
                          _pressedImage = (
                            image: image,
                            point: event.position,
                            revision: c.editor.revision,
                          );
                          return;
                        }
                        if (surface.toggleTaskAt(
                          surface.globalToLocal(event.position),
                        )) {
                          _dragging = false;
                          return;
                        }
                        _dragging =
                            event.kind == PointerDeviceKind.mouse &&
                            event.buttons == kPrimaryButton;
                        surface.place(
                          surface.globalToLocal(event.position),
                          extend: HardwareKeyboard.instance.isShiftPressed,
                        );
                        final link = surface.linkAt(
                          surface.globalToLocal(event.position),
                        );
                        if (link != null &&
                            !HardwareKeyboard.instance.isShiftPressed &&
                            event.buttons == kPrimaryButton) {
                          _pressedLink = (
                            resource: link,
                            point: event.position,
                          );
                        }
                      },
                      onPointerMove: (event) {
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
                          }
                        }
                      },
                      onPointerUp: (_) {
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
                        _pressedLink = null;
                        _dragging = false;
                        _pressedImage = null;
                      },
                      child: Scrollbar(
                        controller: _scroll,
                        child: SingleChildScrollView(
                          controller: _scroll,
                          child: OverlayPortal.overlayChildLayoutBuilder(
                            controller: _popover,
                            overlayChildBuilder: _buildLinkPopover,
                            child: FlarkSurface(
                              key: _surfaceKey,
                              controller: c,
                              theme: resolvedTheme,
                              textScaler: MediaQuery.textScalerOf(context),
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
    return TapRegion(
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
    );
  }
}

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
      state._receiveInput(next);
    }
  }

  @override
  void performAction(TextInputAction action) {
    if (active && action == TextInputAction.newline) {
      state._command(const Newline());
    }
  }

  @override
  void performSelector(String selectorName) {
    if (active) state._selector(selectorName);
  }

  @override
  void connectionClosed() {
    if (active) {
      state.c.finishComposition();
      state._connection = null;
      state._epoch++;
    }
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
