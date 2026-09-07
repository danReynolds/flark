import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'controller.dart';
import 'clipboard_binding_stub.dart'
    if (dart.library.js_interop) 'clipboard_binding_web.dart';
import 'input_context.dart';
import 'surface.dart';
import 'source_window.dart';

/// Scrollable editing surface. All Markdown commands belong to the kernel;
/// this widget owns platform input, focus, viewport and glyph geometry.
class FlarkEditorWidget extends StatefulWidget {
  const FlarkEditorWidget({
    super.key,
    required this.controller,
    this.autofocus = false,
    this.readOnly = false,
    this.showToolbar = true,
    this.onPaint,
    this.focusNode,
    this.style = defaultStyle,
  });
  static const defaultStyle = TextStyle(
    fontSize: 17,
    height: 1.45,
    color: Color(0xff253047),
  );
  final FlarkController controller;
  final bool autofocus, readOnly, showToolbar;
  final FocusNode? focusNode;
  final TextStyle style;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  State<FlarkEditorWidget> createState() => _FlarkEditorWidgetState();
}

class _FlarkEditorWidgetState extends State<FlarkEditorWidget> {
  final _surfaceKey = GlobalKey();
  final _scroll = ScrollController();
  late FocusNode _focus;
  late final EditorClipboardBinding _clipboardBinding;
  TextInputConnection? _connection;
  _InputClient? _client;
  TextEditingValue? _sentValue;
  InputContext? _inputContext;
  int _epoch = 0;
  double? _goalX;
  bool _dragging = false;
  bool _scheduled = false;
  int? _sourceWindowStart;
  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;
  FlarkController get c => widget.controller;

  @override
  void initState() {
    super.initState();
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
  }

  @override
  void didUpdateWidget(FlarkEditorWidget old) {
    super.didUpdateWidget(old);
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

  void _tab(bool backward) {
    if (!c.editor.sourceMode) {
      final row = c.editor.document.rowAt(c.editor.selection.extent);
      if (row.kind == RowKind.tableCell) {
        final index = row.index + (backward ? -1 : 1);
        final rows = c.editor.projection.rows;
        if (index >= 0 &&
            index < rows.length &&
            rows[index].tableBlock == row.tableBlock) {
          _command(SetSelection.caret(rows[index].sourceStart));
        } else if (!backward) {
          _command(const Newline());
        }
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
    if (c.editor.composing) {
      if (key == LogicalKeyboardKey.escape) {
        c.finishComposition(cancel: true);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
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
        _command(const Undo());
      case 'redo:':
        _command(const Redo());
    }
  }

  @override
  void dispose() {
    _clipboardBinding.dispose();
    c.removeListener(_changed);
    _focus.removeListener(_focusChanged);
    _close();
    if (widget.focusNode == null) _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final e = c.editor;
    final menuController = c, menuRevision = e.revision;
    final codeRow = !e.sourceMode ? e.document.rowAt(e.selection.extent) : null;
    final info = codeRow?.fenced == true
        ? e.source.substring(codeRow!.codeInfoStart, codeRow.codeInfoEnd)
        : '';
    final codeLanguage = codeLanguageName(info);
    final detected = codeRow?.fenced == true && codeLanguage.isEmpty
        ? e.codeEditing?.resolveLanguage(codeRow!.text, info)
        : null;
    final detectedLabel = codeLanguages[detected];
    final window = e.sourceMode
        ? SourceWindow.at(c.text, e.selection.extent)
        : null;
    _sourceWindowStart = window?.start;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.showToolbar && !widget.readOnly)
          Material(
            color: const Color(0xfff7f8fb),
            child: Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                if (codeRow?.fenced == true)
                  PopupMenuButton<String>(
                    tooltip: 'Code language',
                    initialValue: codeLanguage,
                    onSelected: (language) {
                      if (identical(c, menuController) &&
                          language != codeLanguage) {
                        menuController.command(
                          SetCodeLanguage(language),
                          expectedRevision: menuRevision,
                        );
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
                IconButton(
                  tooltip: 'Bold',
                  isSelected: e.typingContext & Style.strong != 0,
                  onPressed: () {
                    _command(const ToggleStyle(Style.strong));
                    _focus.requestFocus();
                  },
                  icon: const Icon(Icons.format_bold, size: 20),
                ),
                IconButton(
                  tooltip: 'Italic',
                  isSelected: e.typingContext & Style.emphasis != 0,
                  onPressed: () {
                    _command(const ToggleStyle(Style.emphasis));
                    _focus.requestFocus();
                  },
                  icon: const Icon(Icons.format_italic, size: 20),
                ),
                IconButton(
                  tooltip: 'Undo',
                  onPressed: e.history.canUndo
                      ? () {
                          _command(const Undo());
                          _focus.requestFocus();
                        }
                      : null,
                  icon: const Icon(Icons.undo, size: 20),
                ),
                IconButton(
                  tooltip: 'Redo',
                  onPressed: e.history.canRedo
                      ? () {
                          _command(const Redo());
                          _focus.requestFocus();
                        }
                      : null,
                  icon: const Icon(Icons.redo, size: 20),
                ),
                TextButton(
                  onPressed: () {
                    c.sourceMode(!e.sourceMode);
                    _focus.requestFocus();
                  },
                  child: Text(e.sourceMode ? 'Rendered' : 'Source'),
                ),
              ],
            ),
          ),
        if (e.sourceMode || c.notice != null)
          Material(
            color: const Color(0xfffff3d6),
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
                      onPointerDown: widget.readOnly
                          ? null
                          : (event) {
                              _focus.requestFocus();
                              _attach();
                              _goalX = null;
                              final surface = _surface;
                              if (surface == null) return;
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
                                extend:
                                    HardwareKeyboard.instance.isShiftPressed,
                              );
                            },
                      onPointerMove: (event) {
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
                      onPointerUp: (_) => _dragging = false,
                      child: Scrollbar(
                        controller: _scroll,
                        child: SingleChildScrollView(
                          controller: _scroll,
                          child: FlarkSurface(
                            key: _surfaceKey,
                            controller: c,
                            style: FlarkEditorWidget.defaultStyle.merge(
                              widget.style.copyWith(inherit: true),
                            ),
                            focused: _focus.hasFocus,
                            readOnly: widget.readOnly,
                            viewportHeight: constraints.maxHeight,
                            scrollOffset: _scroll.hasClients
                                ? _scroll.offset
                                : 0,
                            onPaint: widget.onPaint,
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
      ],
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
  const FlarkMarkdownView({super.key, required this.controller, this.onPaint});
  final FlarkController controller;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  Widget build(BuildContext context) => FlarkEditorWidget(
    controller: controller,
    readOnly: true,
    showToolbar: false,
    onPaint: onPaint,
  );
}
