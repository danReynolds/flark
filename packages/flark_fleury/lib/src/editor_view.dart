import 'package:flark/rendering.dart';
import 'package:flark/session.dart';
import 'package:flark/code.dart' show CodeHighlight;
import 'dart:async';
import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flark/resources.dart';
import 'package:fleury/fleury_core.dart';
import 'package:flark_codemirror/flark_codemirror.dart'
    show CodeMirrorLanguageMenu;

import 'cell_layout.dart';
import 'image_previews.dart';
import 'controller.dart';
import 'theme.dart';
import 'resource_controls.dart';

part 'surface.dart';
part 'markdown.dart';
part 'link_anchor.dart';
part 'interactive_semantics.dart';
part 'toolbar.dart';

/// A viewport over a borrowed controller. Give it bounded width and height.
/// Escape leaves editing focus; Tab indents in code/lists and traverses elsewhere.
class FlarkEditorView extends StatefulWidget {
  const FlarkEditorView({
    super.key,
    required this.controller,
    this.session,
    this.theme,
    this.focusNode,
    this.autofocus = false,
    this.readOnly = false,
    this.showToolbar = false,
    this.semanticLabel = 'Markdown editor',
    this.onOpenLink,
    this.baseUri,
    this.linkPopoverBuilder,
    this.presentResourceEditor,
    this.imagePreviewBuilder,
    this.onNotice,
  });

  final FlarkFleuryController controller;

  /// The consumer session this view edits, if any: the toolbar reads its
  /// published state, and its link and image editors open here.
  final FlarkSession? session;
  final FlarkCellTheme? theme;
  final FocusNode? focusNode;
  final bool autofocus, readOnly;

  /// Opt into the host's formatting and fenced-code language controls.
  final bool showToolbar;
  final String semanticLabel;
  final void Function(Uri)? onOpenLink;
  final Uri? baseUri;
  final FlarkFleuryLinkPopoverBuilder? linkPopoverBuilder;
  final FlarkFleuryResourcePresenter? presentResourceEditor;
  final FlarkFleuryImagePreviewBuilder? imagePreviewBuilder;

  /// Told when input was accepted by the terminal but dropped here — a paste
  /// abandoned because the document moved under it, or one over the source
  /// limit. Without it those disappear with nothing to show the user.
  final void Function(String reason)? onNotice;

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
    super.imagePreviewBuilder,
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
    if (previous.describes(
      widget.controller,
      previous.cols,
      previous.theme,
      previous.policy,
    )) {
      return previous;
    }
    // Several input events can arrive before a frame. Movement must never use
    // geometry from an earlier source, even though paint has not caught up yet.
    return _viewport.layout = CellDocumentLayout(
      widget.controller,
      previous.cols,
      previous.theme,
      previous.policy,
      previous: previous,
    );
  }

  int? _goalColumn, _pasteId, _pasteRevision;
  var _composing = false;

  /// Where the last preedit is in the source, and the source it is a range
  /// of. Null when the next preedit cannot replace it there.
  ({int start, int end, String source})? _preedit;
  StringBuffer? _paste;
  InlineResource? _link;
  FlarkEditor? _linkEditor;
  FlarkSelection? _linkSelection;
  int _linkRevision = -1, _linkEpoch = 0;
  ({InlineResource resource, int revision, bool open})? _pressedLink;
  FlarkResourceSession? _resourceSession;
  bool get _dialogOpen => _resourceSession != null;

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

  InlineResource? _linkAt(int col, int row) =>
      _inputLayout == null ? null : _viewport.resourceAt(_editor, col, row);

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
    if ((pressed.open || widget.readOnly) &&
        uri != null &&
        widget.onOpenLink != null) {
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

  /// Opens the link or image editor where the kernel can set one: the same
  /// test that enables the toolbar's buttons, so Command-K and the session's
  /// presenter never open a form whose Save can only fail.
  Future<FlarkEditResult> _editLink({bool image = false}) async {
    if (_dialogOpen ||
        widget.readOnly ||
        !_editor.canSetResource(image: image)) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unavailable);
    }
    final navigator = Navigator.maybeOf(context);
    if (navigator == null && widget.presentResourceEditor == null) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unavailable);
    }
    _dismissLink();
    _finishInput();
    final editor = _editor,
        revision = editor.revision,
        selection = editor.selection;
    final resource = editor.document.resourceAt(selection, image: image);
    bool active() =>
        mounted &&
        identical(editor, _editor) &&
        !widget.readOnly &&
        !editor.sourceMode &&
        editor.revision == revision &&
        editor.selection == selection;
    var applied = false;
    final session = FlarkResourceSession(
      image: image,
      resource: resource,
      selectedText: editor.document.visibleText(selection.start, selection.end),
      isActive: active,
      apply: (command) => (applied =
          active() && editor.apply(command, expectedRevision: revision)),
    );
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
      // Completion belongs to this presentation, not whichever document or
      // presentation replaced it while its custom future was pending.
      if (identical(_resourceSession, session)) {
        _resourceSession = null;
        if (mounted && identical(editor, _editor)) _focus.requestFocus();
      }
    }
    if (applied) return const FlarkEditResult.changed();
    if (!mounted || editor.revision != revision) {
      return const FlarkEditResult.rejected(FlarkEditRejection.staleRevision);
    }
    return const FlarkEditResult.unchanged();
  }

  Future<FlarkEditResult> _presentResource(bool image) async {
    if (widget.readOnly) {
      return const FlarkEditResult.rejected(FlarkEditRejection.readOnly);
    }
    return _editLink(image: image);
  }

  @override
  void initState() {
    super.initState();
    widget.session?.resourcePresenter = _presentResource;
    _attachFocus();
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
    if (supplied == null) _focus.dispose();
  }

  @override
  void didUpdateWidget(FlarkEditorView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      _resourceSession?.close();
      _resourceSession = null;
      _link = null;
      oldWidget.controller.editor.cancelComposition();
      _resetInput();
      _viewport.top = 0;
      // Geometry and caret reporting are bound to the editor they were built
      // from; two documents with identical text would otherwise keep the old
      // one. A presenter that never completed must not wedge the new editor's
      // link editor either.
      _viewport.layout = null;
    }
    if (oldWidget.focusNode != widget.focusNode) {
      _detachFocus(oldWidget.focusNode);
      _attachFocus();
    }
    if (oldWidget.readOnly != widget.readOnly) {
      _resourceSession?.close();
      _resourceSession = null;
      _link = null;
      _editor.cancelComposition();
      _resetInput();
    }
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  void _resetInput() {
    _composing = false;
    _preedit = null;
    _paste = null;
    _pasteId = _pasteRevision = null;
    _goalColumn = null;
  }

  void _finishInput() {
    // A read-only view shares the editor; committing here would land another
    // view's in-flight composition in that editor's history.
    if (!widget.readOnly) _editor.commitComposition();
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
      // Deliberately the revision, not the source: a selection change between
      // chunks means the paste would land somewhere the user did not start it,
      // and abandoning it is safer than moving it.
      _pasteRevision = _editor.revision;
    }
    if (_paste == null ||
        event.pasteId != _pasteId ||
        _pasteRevision != _editor.revision) {
      if (_paste != null) {
        widget.onNotice?.call('Paste discarded: the document changed.');
      }
      _paste = null;
      return KeyEventResult.handled;
    }
    if (_paste!.length + event.text.length > _editor.sourceLimit) {
      widget.onNotice?.call('Paste discarded: too large for this document.');
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
    // Only the last preedit counts, as if typed where the composition began.
    // The kernel holds composed text as it is, so a preedit replaces the
    // last where it stands: cancelling and composing again from the start
    // rebuilt the document twice for every preedit.
    final last = _preedit;
    if (_composing &&
        _editor.composing &&
        last != null &&
        identical(last.source, _editor.source) &&
        (last.source.substring(last.start, last.end) == text ||
            (text.isNotEmpty &&
                _composePreedit(
                  ReplaceRange(last.start, last.end, text),
                  last.start,
                  last.end,
                  text,
                )))) {
      return KeyEventResult.handled;
    }
    // A first preedit is typed where the composition began, and so is one
    // that cannot replace the last: an empty one, which gives back a
    // selection the last replaced; one the kernel did not put in as it is;
    // one the kernel refused. One the kernel ended starts again.
    if (_composing && _editor.composing) {
      _editor.cancelComposition();
    } else {
      _finishInput();
      _composing = true;
    }
    _editor.beginComposition();
    _preedit = null;
    final at = _editor.selection;
    if (text.isNotEmpty) {
      _composePreedit(InsertText(text), at.start, at.end, text);
    }
    return KeyEventResult.handled;
  }

  /// Applies [command], which composes [text] over [start]..[end], and
  /// records where the text is when the kernel put it in as it is: in a live
  /// document, outside code it reshapes (an empty fence's first body line).
  /// In source mode a replacement widens to whole graphemes, and a preedit
  /// can join the text after it (a variation selector), so there the next
  /// preedit is not put over this one's range. False when the kernel refused
  /// the text.
  bool _composePreedit(FlarkCommand command, int start, int end, String text) {
    final before = _editor.source;
    if (!_editor.apply(command)) return false;
    final after = _editor.source;
    _preedit =
        !_editor.sourceMode &&
            after.length == before.length - (end - start) + text.length &&
            after.startsWith(text, start) &&
            after == before.replaceRange(start, end, text)
        ? (start: start, end: start + text.length, source: after)
        : null;
    return true;
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

  void _drag(int col, int row) {
    _pressedLink = null;
    _point(col, row, extend: true);
  }

  void _select(int source, bool extend) => _apply(
    SetSelection(extend ? _editor.selection.base : source, source),
    mutation: false,
  );

  void _vertical(int delta, bool extend) {
    final layout = _inputLayout;
    if (layout == null) return;
    final caret = _viewport.caret;
    _goalColumn ??= caret.col;
    final index = (caret.row + delta).clamp(0, layout.lines.length - 1);
    // Up on the first line or Down on the last has nowhere to go. It is no
    // press: a pending style and the typing's undo step outlast it.
    if (index == caret.row && !extend) return;
    final line = layout.lineAt(index, _goalColumn!, direction: delta);
    final hit = line.hit(_goalColumn!);
    if (line.row case final row?) {
      _apply(
        PlaceCaret(row.index, hit.$1, leadingHalf: hit.$2, extend: extend),
        mutation: false,
      );
    } else {
      _select(line.sourceAt(hit.$1), extend);
    }
    _viewport.placedCaret(_editor, atLineEnd: hit.$1 == line.end);
  }

  void _point(int col, int row, {bool extend = false, bool toggle = false}) {
    final layout = _inputLayout;
    if (layout == null) return;
    _finishInput();
    _focus.requestFocus();
    final index = (row - _viewport.origin.row + _viewport.top).clamp(
      0,
      layout.lines.length - 1,
    );
    final localCol = col - _viewport.origin.col;
    final painted = layout.lines[index].cellAt(localCol);
    if (painted.image case final image?) {
      _select(image.contentStart, extend);
      return;
    }
    final line = layout.lineAt(
      index,
      localCol,
      direction: painted.headingRule ? -1 : 1,
    );
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
      // A click past a wrapped line's end keeps the caret on that line.
      _viewport.placedCaret(_editor, atLineEnd: hit.$1 == line.end);
      final taskColumn = line.taskColumn;
      if (toggle &&
          !extend &&
          taskColumn >= 0 &&
          localCol >= taskColumn &&
          localCol < taskColumn + line.taskWidth &&
          projected.shells.any((s) => s.task)) {
        _apply(const ToggleTask());
      }
    } else {
      if (line.row case final row?) {
        _apply(
          PlaceCaret(row.index, hit.$1, leadingHalf: hit.$2, extend: extend),
          mutation: false,
        );
      } else {
        _select(line.sourceAt(hit.$1), extend);
      }
      _viewport.placedCaret(_editor, atLineEnd: hit.$1 == line.end);
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
    // Only a plain Up/Down keeps the goal column; Cmd+Up jumps to the document
    // start and the next Down must aim at that new column, not the old one.
    final vertical =
        (event.code == KeyCode.arrowUp || event.code == KeyCode.arrowDown) &&
        !primary &&
        !event.hasAlt;
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
    } else if (event.code == KeyCode.enter &&
        event.hasShift &&
        !primary &&
        !event.hasAlt) {
      // The default multiline keymap binds plain Enter only. Flark also uses
      // Shift+Enter to retain an intentional blank line inside a code fence.
      command = const Newline(paragraph: true);
    } else if (event.code == KeyCode.tab) {
      if (widget.readOnly) return;
      final row = _editor.sourceMode ? null : _editor.document.caretRow;
      if (row?.kind == RowKind.tableCell) {
        _apply(MoveTableCell(backward: event.hasShift), mutation: false);
        event.consume();
        return;
      }
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
          final caret = _viewport.caret;
          final line = layout.lineAt(caret.row, caret.col);
          final end = action == TextEditingKeyAction.moveLineEnd;
          if (_editor.selection.tableCell != null) {
            event.consume();
            return;
          }
          var target = line.sourceAt(
            end ? line.end : line.start,
            anchor: end ? Anchor.after : Anchor.before,
          );
          if (!_editor.sourceMode) {
            final anchors = _editor.document.anchorsAt(target);
            target = end ? anchors.last : anchors.first;
          }
          _select(target, event.hasShift);
          // A wrapped line ends where the next starts: End stays on this one.
          _viewport.placedCaret(_editor, atLineEnd: end);
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
        case TextEditingKeyAction.killWordLeft:
          command = DeleteBackward(word: event.hasAlt || event.hasCtrl);
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
          // A key the editor ignores, such as a lone Shift before Shift+Down.
          _goalColumn = goal;
          return;
      }
    }
    _apply(command, mutation: mutation);
    event.consume();
  }

  @override
  Widget build(BuildContext context) {
    // Rebuilt whenever the controller publishes a change, and resubscribed
    // when the widget is given another controller.
    context.listen(widget.controller);
    final surface = Stack(
      children: [
        _buildEditor(context),
        if (_linkActive) _buildLinkPopover(context),
      ],
    );
    return Column(
      children: [
        widget.showToolbar && !widget.readOnly
            ? _buildToolbar()
            : const SizedBox(height: 0),
        Expanded(key: const ValueKey('editor-surface'), child: surface),
      ],
    );
  }

  Widget _buildLinkPopover(BuildContext context) {
    final epoch = _linkEpoch, resource = _link!;
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
              if (active()) unawaited(_editLink(image: resource.isImage));
            },
      remove: widget.readOnly
          ? null
          : () {
              if (active()) {
                _editor.apply(
                  resource.isImage ? const RemoveImage() : const RemoveLink(),
                  expectedRevision: _linkRevision,
                );
                _dismissLink(restoreFocus: true);
              }
            },
    );
    return _LinkAnchor(
      viewport: _viewport,
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

  Widget _buildEditor(BuildContext context) => _InteractiveSemantics(
    controller: widget.controller,
    viewport: _viewport,
    readOnly: widget.readOnly,
    onActivate: (col, row) {
      _pointerDown(col, row, const {});
      _pointerUp(col, row);
    },
    child: Semantics(
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
              onTapDown: (event) => _pointerDown(
                event.globalPosition.col,
                event.globalPosition.row,
                event.modifiers,
              ),
              onTapUp: (event) => _pointerUp(
                event.globalPosition.col,
                event.globalPosition.row,
              ),
              onDragUpdate: (event) =>
                  _drag(event.globalPosition.col, event.globalPosition.row),
              onDragStart: (event) =>
                  _drag(event.globalPosition.col, event.globalPosition.row),
              child: MouseRegion(
                onScroll: (event) {
                  final before = _viewport.top;
                  setState(() => _viewport.scroll(event.delta.row * 3));
                  return before != _viewport.top;
                },
                child: BoundsObserver(
                  notifier: _viewport.bounds,
                  child: _buildSurface(context),
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );

  Widget _buildSurface(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final theme = widget.theme ?? FlarkCellTheme.of(context);
      final policy = MediaQuery.textPolicyOf(context).widths;
      final size = _viewport.prepare(
        constraints,
        widget.controller,
        theme,
        policy,
        _focus,
      );
      final slots = _viewport.layout!.images.indexed.where(
        (entry) =>
            entry.$2.top < _viewport.top + size.rows &&
            entry.$2.top + entry.$2.height > _viewport.top,
      );
      return SizedBox(
        width: size.cols,
        height: size.rows,
        child: _ClipViewport(
          child: Stack(
            children: [
              _Surface(widget.controller, theme, _focus, _viewport, policy),
              for (final (index, slot) in slots.take(8))
                Positioned(
                  key: ValueKey('image/$index/${slot.resource.destination}'),
                  left: slot.left,
                  // A preview scrolled partly above the viewport starts at
                  // its top, raised by the rows it has scrolled past.
                  top: math.max(0, slot.top - _viewport.top),
                  width: slot.width,
                  height: slot.height - math.max(0, _viewport.top - slot.top),
                  child: _RaisedPreview(
                    rows: math.max(0, _viewport.top - slot.top),
                    child:
                        widget.imagePreviewBuilder?.call(
                          context,
                          slot.resource,
                          flarkOpenableUri(
                            slot.resource.destination,
                            widget.baseUri ?? Uri.base,
                          ),
                        ) ??
                        FlarkImagePreview(
                          uri: flarkOpenableUri(
                            slot.resource.destination,
                            widget.baseUri ?? Uri.base,
                          ),
                          label: slot.resource.text,
                        ),
                  ),
                ),
            ],
          ),
        ),
      );
    },
  );

  @override
  void dispose() {
    _resourceSession?.close();
    if (!widget.readOnly) _editor.commitComposition();
    _detachFocus(widget.focusNode);
    _viewport.dispose();
    super.dispose();
  }
}
