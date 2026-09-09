import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flark/resources.dart';
import 'package:fleury/fleury_core.dart';

import 'cell_layout.dart';
import 'controller.dart';
import 'theme.dart';
import 'resource_controls.dart';

part 'surface.dart';

/// A viewport over a borrowed controller. Give it bounded width and height.
/// Escape leaves editing focus; Tab indents in code/lists and traverses elsewhere.
class FlarkEditorView extends StatefulWidget {
  const FlarkEditorView({
    super.key,
    required this.controller,
    this.theme,
    this.focusNode,
    this.autofocus = false,
    this.readOnly = false,
    this.semanticLabel = 'Markdown editor',
    this.onOpenLink,
    this.baseUri,
    this.linkPopoverBuilder,
    this.presentResourceEditor,
  });

  final FlarkFleuryController controller;
  final FlarkCellTheme? theme;
  final FocusNode? focusNode;
  final bool autofocus, readOnly;
  final String semanticLabel;
  final void Function(Uri)? onOpenLink;
  final Uri? baseUri;
  final FlarkFleuryLinkPopoverBuilder? linkPopoverBuilder;
  final FlarkFleuryResourcePresenter? presentResourceEditor;

  @override
  State<FlarkEditorView> createState() => _EditorState();
}

/// The same projection and cell styles, with mutation routes disabled.
final class FlarkView extends FlarkEditorView {
  const FlarkView({
    super.key,
    required super.controller,
    super.theme,
    super.onOpenLink,
    super.baseUri,
    super.linkPopoverBuilder,
  }) : super(readOnly: true, semanticLabel: 'Markdown document');
}

class _EditorState extends State<FlarkEditorView>
    implements TextInputClaimant, PasteEventClaimant, TextCompositionClaimant {
  late FocusNode _focus;
  final _viewport = _Viewport();
  FlarkEditor get _editor => widget.controller.editor;
  CellDocumentLayout? get _inputLayout {
    final previous = _viewport.layout;
    if (previous == null) return null;
    if (previous.source == _editor.source &&
        (previous.projection == null) == _editor.sourceMode &&
        (previous.sourceWindow == null ||
            (_editor.selection.extent >= previous.sourceWindow!.$1 &&
                _editor.selection.extent <= previous.sourceWindow!.$2))) {
      return previous;
    }
    // Several input events can arrive before a frame. Movement must never use
    // geometry from an earlier source, even though paint has not caught up yet.
    return _viewport.layout = CellDocumentLayout(
      widget.controller,
      previous.cols,
      previous.theme,
      previous.policy,
    );
  }

  int? _goalColumn, _compositionStart, _compositionEnd;
  int? _pasteId, _pasteRevision;
  StringBuffer? _paste;
  InlineResource? _link;
  FlarkEditor? _linkEditor;
  FlarkSelection? _linkSelection;
  int _linkRevision = -1, _linkEpoch = 0;
  ({InlineResource resource, int revision, bool open})? _pressedLink;
  FlarkResourceSession? _resourceSession;
  bool _dialogOpen = false;

  bool get _linkActive =>
      _link != null &&
      identical(_linkEditor, _editor) &&
      !_editor.sourceMode &&
      _linkRevision == _editor.revision &&
      _linkSelection == _editor.selection;

  void _dismissLink({bool restoreFocus = false}) {
    _link = null;
    _linkEpoch++;
    if (restoreFocus && mounted) _focus.requestFocus();
    if (mounted) setState(() {});
  }

  InlineResource? _linkAt(int col, int row) {
    final layout = _inputLayout;
    if (layout == null || _editor.sourceMode) return null;
    final index = row - _viewport.origin.row + _viewport.top;
    if (index < 0 || index >= layout.lines.length) return null;
    final line = layout.lines[index], x = col - _viewport.origin.col;
    for (final glyph in line.glyphs) {
      if (x < glyph.col || x >= glyph.col + glyph.width) continue;
      final source = line.sourceAt(glyph.start);
      for (final resource in _editor.document.resources) {
        if (!resource.isImage &&
            resource.contentStart <= source &&
            source < resource.contentEnd) {
          return resource;
        }
      }
    }
    return null;
  }

  void _pointerDown(int col, int row, Set<KeyModifier> modifiers) {
    final resource = _linkAt(col, row);
    _dismissLink();
    _point(
      col,
      row,
      extend: modifiers.contains(KeyModifier.shift),
      toggle: true,
    );
    _pressedLink = resource == null || modifiers.contains(KeyModifier.shift)
        ? null
        : (
            resource: resource,
            revision: _editor.revision,
            open:
                modifiers.contains(KeyModifier.ctrl) ||
                modifiers.contains(KeyModifier.superKey),
          );
  }

  void _pointerUp(int col, int row) {
    final pressed = _pressedLink;
    _pressedLink = null;
    if (pressed == null ||
        pressed.revision != _editor.revision ||
        !_editor.selection.isCollapsed ||
        _linkAt(col, row)?.start != pressed.resource.start) {
      return;
    }
    final uri = flarkOpenableUri(pressed.resource.destination, widget.baseUri);
    if (pressed.open && uri != null && widget.onOpenLink != null) {
      widget.onOpenLink!(uri);
    } else {
      _showLink(pressed.resource);
    }
  }

  void _showLink(InlineResource resource) {
    if (_dialogOpen) return;
    _link = resource;
    _linkEditor = _editor;
    _linkSelection = _editor.selection;
    _linkRevision = _editor.revision;
    _linkEpoch++;
    setState(() {});
  }

  Future<void> _editLink() async {
    if (_dialogOpen ||
        widget.readOnly ||
        _editor.sourceMode ||
        _editor.document.rowAt(_editor.selection.extent).kind ==
            RowKind.codeBlock) {
      return;
    }
    final navigator = Navigator.maybeOf(context);
    if (navigator == null && widget.presentResourceEditor == null) return;
    _dismissLink();
    _finishInput();
    final editor = _editor,
        revision = editor.revision,
        selection = editor.selection;
    final resource = editor.document.resourceAt(selection, image: false);
    bool active() =>
        mounted &&
        identical(editor, _editor) &&
        !widget.readOnly &&
        !editor.sourceMode &&
        editor.revision == revision &&
        editor.selection == selection;
    final session = FlarkResourceSession(
      image: false,
      resource: resource,
      selectedText: editor.document.visibleText(selection.start, selection.end),
      isActive: active,
      apply: (command) =>
          active() && editor.apply(command, expectedRevision: revision),
    );
    _dialogOpen = true;
    _resourceSession = session;
    try {
      final presenter = widget.presentResourceEditor;
      if (presenter != null) {
        await presenter(context, session);
      } else {
        await navigator!.present<void>(
          FlarkResourceDialog(session: session),
          transition: RouteTransition.none,
        );
      }
    } finally {
      session.close();
      _resourceSession = null;
      _dialogOpen = false;
      if (mounted && identical(editor, _editor)) _focus.requestFocus();
    }
  }

  @override
  void initState() {
    super.initState();
    _attachFocus();
    widget.controller.addListener(_changed);
  }

  void _attachFocus() {
    _focus = widget.focusNode ?? FocusNode(debugLabel: 'Flark');
    _focus.textInputClaimant = this;
    _focus.textCompositionClaimant = this;
  }

  void _detachFocus(FocusNode? supplied) {
    if (identical(_focus.textInputClaimant, this)) {
      _focus.textInputClaimant = null;
    }
    if (identical(_focus.textCompositionClaimant, this)) {
      _focus.textCompositionClaimant = null;
    }
    _focus.caretRect = null;
    if (supplied == null) _focus.dispose();
  }

  @override
  void didUpdateWidget(FlarkEditorView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      _resourceSession?.close();
      _link = null;
      oldWidget.controller.editor.cancelComposition();
      oldWidget.controller.removeListener(_changed);
      widget.controller.addListener(_changed);
      _resetInput();
      _viewport.top = 0;
    }
    if (oldWidget.focusNode != widget.focusNode) {
      _detachFocus(oldWidget.focusNode);
      _attachFocus();
    }
    if (oldWidget.readOnly != widget.readOnly) {
      _resourceSession?.close();
      _link = null;
      _editor.cancelComposition();
      _resetInput();
    }
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  void _resetInput() {
    _compositionStart = _compositionEnd = null;
    _paste = null;
    _pasteId = _pasteRevision = null;
    _goalColumn = null;
  }

  void _finishInput() {
    _editor.commitComposition();
    _resetInput();
  }

  void _apply(FlarkCommand command, {bool mutation = true}) {
    if (mutation && widget.readOnly) return;
    _editor.apply(command);
  }

  @override
  KeyEventResult onTextInput(String text) {
    _finishInput();
    _apply(InsertText(text));
    return KeyEventResult.handled;
  }

  @override
  KeyEventResult onPaste(String text) {
    _finishInput();
    _apply(Paste(text.replaceAll('\r\n', '\n').replaceAll('\r', '\n')));
    return KeyEventResult.handled;
  }

  @override
  KeyEventResult onPasteEvent(PasteEvent event) {
    if (widget.readOnly) return KeyEventResult.handled;
    if (event.isFirst) {
      _finishInput();
      _paste = StringBuffer();
      _pasteId = event.pasteId;
      _pasteRevision = _editor.revision;
    }
    if (_paste == null ||
        event.pasteId != _pasteId ||
        _pasteRevision != _editor.revision) {
      _paste = null;
      return KeyEventResult.handled;
    }
    if (_paste!.length + event.text.length > _editor.sourceLimit) {
      _paste = null;
      return KeyEventResult.handled;
    }
    _paste!.write(event.text);
    if (event.isFinal) onPaste(_paste.toString());
    return KeyEventResult.handled;
  }

  @override
  KeyEventResult onTextCompositionUpdate(String text) {
    if (widget.readOnly) return KeyEventResult.handled;
    if (_compositionStart == null) {
      _finishInput();
      _editor.beginComposition();
      _compositionStart = _editor.selection.start;
      _compositionEnd = _editor.selection.end;
    }
    if (_editor.apply(
      ReplaceRange(_compositionStart!, _compositionEnd!, text),
    )) {
      _compositionEnd = _compositionStart! + text.length;
    }
    return KeyEventResult.handled;
  }

  @override
  KeyEventResult onTextCompositionCommit(String? text) {
    if (text != null) onTextCompositionUpdate(text);
    _finishInput();
    return KeyEventResult.handled;
  }

  @override
  KeyEventResult onTextCompositionCancel() {
    _editor.cancelComposition();
    _resetInput();
    return KeyEventResult.handled;
  }

  void _select(int source, bool extend) => _apply(
    SetSelection(extend ? _editor.selection.base : source, source),
    mutation: false,
  );

  void _vertical(int delta, bool extend) {
    final layout = _inputLayout;
    if (layout == null) return;
    final caret = layout.positionFor(_editor.selection.extent);
    _goalColumn ??= caret.col;
    final index = (caret.row + delta).clamp(0, layout.lines.length - 1);
    final line = layout.lines[index];
    final hit = line.hit(_goalColumn!);
    _select(line.sourceAt(hit.$1), extend);
  }

  void _point(int col, int row, {bool extend = false, bool toggle = false}) {
    final layout = _inputLayout;
    if (layout == null) return;
    _finishInput();
    _focus.requestFocus();
    final line =
        layout.lines[(row - _viewport.origin.row + _viewport.top).clamp(
          0,
          layout.lines.length - 1,
        )];
    final localCol = col - _viewport.origin.col;
    final hit = line.hit(localCol);
    if (line.row case final projected?) {
      _apply(
        PlaceCaret(
          projected.index,
          hit.$1,
          leadingHalf: hit.$2,
          extend: extend,
        ),
        mutation: false,
      );
      final taskColumn = line.prefix.indexOf('['); // rendered shell, not source
      if (toggle &&
          !extend &&
          taskColumn >= 0 &&
          localCol >= taskColumn &&
          localCol < taskColumn + 3 &&
          projected.shells.any((s) => s.task)) {
        _apply(const ToggleTask());
      }
    } else {
      _select(line.sourceAt(hit.$1), extend);
    }
  }

  Future<void> _copy({bool cut = false}) async {
    if (_editor.selection.isCollapsed) return;
    final editor = _editor, revision = _editor.revision;
    final value = editor.source.substring(
      editor.selection.start,
      editor.selection.end,
    );
    await ClipboardScope.of(context).write(value);
    if (mounted && cut && !widget.readOnly && identical(editor, _editor)) {
      editor.apply(const DeleteBackward(), expectedRevision: revision);
    }
  }

  void _key(KeyEvent event) {
    if (event.type == KeyEventType.up) return;
    final primary = event.hasCtrl || event.hasSuper;
    final vertical =
        event.code == KeyCode.arrowUp || event.code == KeyCode.arrowDown;
    final goal = _goalColumn;
    _finishInput();
    if (vertical) _goalColumn = goal;
    FlarkCommand? command;
    var mutation = true;
    if (primary && event.code == KeyCode.a) {
      command = const SelectAll();
      mutation = false;
    } else if (primary && event.code == KeyCode.k) {
      unawaited(_editLink());
      event.consume();
      return;
    } else if (event.code == KeyCode.escape && _linkActive) {
      _dismissLink(restoreFocus: true);
      event.consume();
      return;
    } else if (primary &&
        (event.code == KeyCode.c || event.code == KeyCode.x)) {
      unawaited(_copy(cut: event.code == KeyCode.x));
      event.consume();
      return;
    } else if (primary && event.code == KeyCode.b) {
      command = const ToggleStyle(Style.strong);
    } else if (primary && event.code == KeyCode.i) {
      command = const ToggleStyle(Style.emphasis);
    } else if (primary && event.code == KeyCode.z) {
      command = event.hasShift ? const Redo() : const Undo();
    } else if (primary && event.code == KeyCode.y) {
      command = const Redo();
    } else if (event.code == KeyCode.tab) {
      if (widget.readOnly) return;
      final pos = _editor.sourceMode
          ? null
          : _editor.projection.displayForSource(_editor.selection.extent);
      final row = pos == null ? null : _editor.projection.rows[pos.row];
      if (row?.kind != RowKind.codeBlock &&
          row?.shells.any((s) => s.kind == ShellKind.item) != true) {
        return;
      }
      command = event.hasShift ? const Outdent() : const Indent();
    } else if (vertical && !primary && !event.hasAlt) {
      _vertical(event.code == KeyCode.arrowUp ? -1 : 1, event.hasShift);
      event.consume();
      return;
    } else if (event.code == KeyCode.escape) {
      _focus.unfocus();
      event.consume();
      return;
    } else {
      final action = TextEditingKeymap.defaultMultiline.resolve(event);
      switch (action) {
        case TextEditingKeyAction.moveLeft:
        case TextEditingKeyAction.moveRight:
        case TextEditingKeyAction.moveWordLeft:
        case TextEditingKeyAction.moveWordRight:
          final back =
              action == TextEditingKeyAction.moveLeft ||
              action == TextEditingKeyAction.moveWordLeft;
          command = MoveCaret(
            back ? MoveDirection.backward : MoveDirection.forward,
            unit:
                action == TextEditingKeyAction.moveWordLeft ||
                    action == TextEditingKeyAction.moveWordRight
                ? MoveUnit.word
                : MoveUnit.grapheme,
            extend: event.hasShift,
          );
          mutation = false;
        case TextEditingKeyAction.moveLineStart:
        case TextEditingKeyAction.moveLineEnd:
          final layout = _inputLayout;
          if (layout == null) return;
          final line =
              layout.lines[layout.positionFor(_editor.selection.extent).row];
          final end = action == TextEditingKeyAction.moveLineEnd;
          var target = line.sourceAt(
            end ? line.end : line.start,
            anchor: end ? Anchor.after : Anchor.before,
          );
          if (!_editor.sourceMode) {
            final anchors = _editor.document.anchorsAt(target);
            target = end ? anchors.last : anchors.first;
          }
          _select(target, event.hasShift);
          event.consume();
          return;
        case TextEditingKeyAction.moveDocumentStart:
        case TextEditingKeyAction.moveDocumentEnd:
          _select(
            action == TextEditingKeyAction.moveDocumentStart
                ? 0
                : _editor.source.length,
            event.hasShift,
          );
          event.consume();
          return;
        case TextEditingKeyAction.backspace:
          command = const DeleteBackward();
        case TextEditingKeyAction.deleteForward:
          command = const DeleteForward();
        case TextEditingKeyAction.insertNewline:
        case TextEditingKeyAction.submit:
          command = Newline(paragraph: event.hasShift);
        case TextEditingKeyAction.copy:
        case TextEditingKeyAction.cut:
          unawaited(_copy(cut: action == TextEditingKeyAction.cut));
          event.consume();
          return;
        default:
          return;
      }
    }
    _apply(command, mutation: mutation);
    event.consume();
  }

  @override
  Widget build(BuildContext context) => Stack(
    children: [
      _buildEditor(context),
      if (_linkActive) _buildLinkPopover(context),
    ],
  );

  Widget _buildLinkPopover(BuildContext context) {
    final layout = _viewport.layout!, epoch = _linkEpoch, resource = _link!;
    final caret = layout.positionFor(_editor.selection.extent);
    final width = layout.cols.clamp(1, 52);
    final height = width < 40 ? 8 : 6;
    final below = caret.row - _viewport.top + 1;
    final y = (below + height <= _viewport.rows ? below : below - height - 1)
        .clamp(0, (_viewport.rows - height).clamp(0, _viewport.rows));
    bool active() => mounted && epoch == _linkEpoch && _linkActive;
    final uri = flarkOpenableUri(resource.destination, widget.baseUri);
    final actions = FlarkLinkActions(
      resource: resource,
      dismiss: () {
        if (active()) _dismissLink(restoreFocus: true);
      },
      open: uri == null || widget.onOpenLink == null
          ? null
          : () {
              if (active()) {
                widget.onOpenLink!(uri);
                _dismissLink(restoreFocus: true);
              }
            },
      edit: widget.readOnly
          ? null
          : () {
              if (active()) unawaited(_editLink());
            },
      remove: widget.readOnly
          ? null
          : () {
              if (active()) {
                _editor.apply(
                  const RemoveLink(),
                  expectedRevision: _linkRevision,
                );
                _dismissLink(restoreFocus: true);
              }
            },
    );
    return Positioned(
      left: caret.col.clamp(0, layout.cols - width),
      top: y,
      width: width,
      child: KeyDetector(
        onKey: (event) {
          if (event.code == KeyCode.escape) {
            actions.dismiss();
            event.consume();
          }
        },
        child:
            widget.linkPopoverBuilder?.call(context, actions) ??
            FlarkLinkPopover(actions: actions),
      ),
    );
  }

  Widget _buildEditor(BuildContext context) => Semantics(
    role: SemanticRole.textArea,
    label: widget.semanticLabel,
    value: _editor.source,
    focused: _focus.hasFocus,
    state: SemanticState({
      'selectionBase': _editor.selection.base,
      'selectionExtent': _editor.selection.extent,
      'readOnly': widget.readOnly,
      'textEditable': true,
      'composingActive': _editor.composing,
    }),
    actions: {SemanticAction.focus, SemanticAction.copy},
    onAction: (action) {
      if (action == SemanticAction.focus) _focus.requestFocus();
      if (action == SemanticAction.copy) unawaited(_copy());
    },
    child: KeyDetector(
      onKey: _key,
      child: FocusDetector(
        onFocusChange: (focused) {
          if (!focused) _finishInput();
          _changed();
        },
        child: Focus(
          focusNode: _focus,
          autofocus: widget.autofocus,
          child: GestureDetector(
            onTapDownWithModifiers: _pointerDown,
            onTapUp: _pointerUp,
            onDragUpdate: (col, row) {
              _pressedLink = null;
              _point(col, row, extend: true);
            },
            onDragStart: (col, row) {
              _pressedLink = null;
              _point(col, row, extend: true);
            },
            child: PointerScrollListener(
              router: PointerRouterScope.maybeOf(context),
              onScrollUp: () => setState(() => _viewport.scroll(-3)),
              onScrollDown: () => setState(() => _viewport.scroll(3)),
              child: BoundsObserver(
                notifier: _viewport.bounds,
                child: _Surface(
                  widget.controller,
                  widget.theme ?? FlarkCellTheme.of(context),
                  _focus,
                  _viewport,
                  MediaQuery.textPolicyOf(context).widths,
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );

  @override
  void dispose() {
    _resourceSession?.close();
    widget.controller.removeListener(_changed);
    _editor.commitComposition();
    _detachFocus(widget.focusNode);
    _viewport.dispose();
    super.dispose();
  }
}
