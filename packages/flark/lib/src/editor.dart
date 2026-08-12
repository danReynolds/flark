import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';
import 'render_surface.dart';

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

  @override
  State<FlarkEditor> createState() => _FlarkEditorState();
}

final class _FlarkEditorState extends State<FlarkEditor>
    with DeltaTextInputClient {
  final GlobalKey _surfaceKey = GlobalKey();
  FocusNode? _ownedFocusNode;
  TextInputConnection? _connection;
  TextEditingValue? _lastSentValue;

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
  }

  @override
  void dispose() {
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

  void _activate(Offset localPosition, {bool extend = false}) {
    final hit = _surface?.positionForOffset(localPosition);
    if (hit == null) return;
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

  @override
  Widget build(BuildContext context) {
    return Focus(
      focusNode: _focusNode,
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
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            supportedDevices: const {PointerDeviceKind.mouse},
            onTapDown: (details) => _activate(details.localPosition),
            onPanStart: (details) => _activate(details.localPosition),
            onPanUpdate: (details) =>
                _activate(details.localPosition, extend: true),
            child: FlarkRenderSurfaceWidget(
              key: _surfaceKey,
              controller: widget.controller,
              textStyle: widget.textStyle,
              padding: widget.padding,
              caretColor: widget.caretColor,
              selectionColor: widget.selectionColor,
              includeEditingState: true,
            ),
          ),
        ),
      ),
    );
  }

  @override
  TextEditingValue? get currentTextEditingValue => widget.controller.inputValue;

  @override
  AutofillScope? get currentAutofillScope => null;

  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> textEditingDeltas) {
    widget.debugInputEventObserver?.call(
      'deltas:${textEditingDeltas.map((delta) => delta.runtimeType).join(',')}'
      ':old=${textEditingDeltas.isEmpty ? -1 : textEditingDeltas.first.oldText.length}',
    );
    widget.controller.applyDeltas(textEditingDeltas);
  }

  @override
  void updateEditingValue(TextEditingValue value) {
    widget.debugInputEventObserver?.call(
      'full-value:length=${value.text.length}:selection=${value.selection}'
      ':composing=${value.composing}',
    );
    widget.controller.updateEditingValue(value);
  }

  @override
  void performAction(TextInputAction action) {
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
        widget.controller.deleteBackward();
      case 'deleteForward:':
        widget.controller.deleteForward();
      case 'insertNewline:':
        widget.controller.insertNewline();
      case 'undo:':
        unawaited(widget.controller.undo());
      case 'redo:':
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
