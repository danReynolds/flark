import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import 'flark_command_actions.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_input_policy.dart';
import 'flark_text_selection_gestures.dart';

final class FlarkEditableText extends StatefulWidget {
  const FlarkEditableText({
    super.key,
    required this.controller,
    this.focusNode,
    this.style,
    this.cursorColor = const Color(0xFF006ADC),
    this.backgroundCursorColor = const Color(0x00000000),
    this.minLines,
    this.maxLines,
    this.expands = false,
    this.autofocus = false,
    this.shortcuts = const <ShortcutActivator, FlarkCommandIntent>{},
  });

  final FlarkFlutterController controller;
  final FocusNode? focusNode;
  final TextStyle? style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, FlarkCommandIntent> shortcuts;

  @override
  State<FlarkEditableText> createState() => _FlarkEditableTextState();
}

final class _FlarkEditableTextState extends State<FlarkEditableText> {
  static int _nextCompositionUndoGroupId = 1;

  final _editableStateKey = GlobalKey<EditableTextState>();
  late final TextEditingController _textController;
  FocusNode? _ownedFocusNode;
  bool _syncingFromRuntime = false;
  int? _compositionUndoGroupId;

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    _textController = TextEditingController();
    _textController.addListener(_handleTextEditingValueChanged);
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
    widget.controller.addListener(_syncFromRuntime);
    _syncFromRuntime();
  }

  @override
  void didUpdateWidget(FlarkEditableText oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_syncFromRuntime);
      widget.controller.addListener(_syncFromRuntime);
      _syncFromRuntime();
    }
    if (oldWidget.focusNode == null && widget.focusNode != null) {
      _ownedFocusNode?.dispose();
      _ownedFocusNode = null;
    } else if (oldWidget.focusNode != null && widget.focusNode == null) {
      _ownedFocusNode = FocusNode();
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_syncFromRuntime);
    _textController.removeListener(_handleTextEditingValueChanged);
    _ownedFocusNode?.dispose();
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style.merge(widget.style);
    Widget editor = EditableText(
      key: _editableStateKey,
      controller: _textController,
      focusNode: _focusNode,
      style: style,
      cursorColor: widget.cursorColor,
      selectionColor: _selectionColorForCursor(widget.cursorColor),
      selectionControls: flarkTextSelectionControlsForPlatform(context),
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      expands: widget.expands,
      autofocus: widget.autofocus,
      keyboardType: TextInputType.multiline,
      textInputAction: TextInputAction.newline,
      paintCursorAboveText: true,
      rendererIgnoresPointer: true,
    );
    editor = flarkEditableTextGestureDetector(
      editableTextKey: _editableStateKey,
      child: editor,
    );
    editor = _markdownInputPolicy.wrapKeyboardShortcuts(
      child: editor,
      currentSelection: () =>
          FlarkMarkdownInputPolicy.selectionFromTextSelection(
            _textController.selection,
          ),
      applySelection: (selection) {
        widget.controller.applySelection(
          selection,
          userEvent: 'selection.editableText.markdownPolicy',
        );
      },
    );
    editor = FlarkCommandActions(controller: widget.controller, child: editor);
    if (widget.shortcuts.isNotEmpty) {
      editor = Shortcuts(
        shortcuts: <ShortcutActivator, Intent>{...widget.shortcuts},
        child: editor,
      );
    }
    return editor;
  }

  void _handleTextEditingValueChanged() {
    if (_syncingFromRuntime) return;
    final value = _textController.value;
    final compositionUndoGroupId = _compositionUndoGroupIdFor(value);
    if (value.text != widget.controller.markdown) {
      if (compositionUndoGroupId == null &&
          _markdownInputPolicy.handlePlatformTextChange(
            oldText: widget.controller.markdown,
            newValue: value,
            oldTextSelection: widget.controller.selection,
            applyOldTextSelection: (selection) {
              widget.controller.applySelection(
                selection,
                userEvent: 'selection.editableText.markdownPolicy',
              );
            },
          )) {
        return;
      }
      final transaction = _transactionForTextChange(
        widget.controller.markdown,
        value.text,
        value.selection,
        undoGroupId: compositionUndoGroupId,
      );
      if (transaction == null) {
        _syncFromRuntime();
        return;
      }
      widget.controller.applyTransaction(transaction);
      _clearCompositionGroupIfCommitted(value);
      return;
    }

    final selection = _selectionFromTextSelection(value.selection);
    if (selection == null || selection == widget.controller.selection) {
      _clearCompositionGroupIfCommitted(value);
      return;
    }
    widget.controller.applySelection(
      selection,
      userEvent: 'selection.editableText',
    );
    _clearCompositionGroupIfCommitted(value);
  }

  void _syncFromRuntime() {
    final currentValue = _textController.value;
    final nextValue = TextEditingValue(
      text: widget.controller.markdown,
      selection: _textSelection(widget.controller.selection),
      composing: currentValue.text == widget.controller.markdown
          ? currentValue.composing
          : TextRange.empty,
    );
    if (_textController.value == nextValue) return;
    _syncingFromRuntime = true;
    _textController.value = nextValue;
    _syncingFromRuntime = false;
  }

  TextSelection _textSelection(FlarkSelection selection) {
    return TextSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }

  FlarkTransaction? _transactionForTextChange(
    String before,
    String after,
    TextSelection selection, {
    int? undoGroupId,
  }) {
    if (before == after) return null;

    var prefix = 0;
    final shortest = before.length < after.length
        ? before.length
        : after.length;
    while (prefix < shortest &&
        before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
      prefix++;
    }

    var beforeSuffix = before.length;
    var afterSuffix = after.length;
    while (beforeSuffix > prefix &&
        afterSuffix > prefix &&
        before.codeUnitAt(beforeSuffix - 1) ==
            after.codeUnitAt(afterSuffix - 1)) {
      beforeSuffix--;
      afterSuffix--;
    }

    final replacementText = after.substring(prefix, afterSuffix);
    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: FlarkSourceRange(prefix, beforeSuffix),
        replacementText: replacementText,
      ),
      selectionAfter: _selectionFromTextSelection(selection),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'input.editableText',
        undoGroupId: undoGroupId,
        parseInvalidationRange: FlarkSourceRange(prefix, beforeSuffix),
        projectionInvalidationRange: FlarkSourceRange(prefix, beforeSuffix),
      ),
    );
  }

  int? _compositionUndoGroupIdFor(TextEditingValue value) {
    if (!value.composing.isValid && _compositionUndoGroupId == null) {
      return null;
    }
    _compositionUndoGroupId ??= 0xC0000000 + _nextCompositionUndoGroupId++;
    return _compositionUndoGroupId;
  }

  void _clearCompositionGroupIfCommitted(TextEditingValue value) {
    if (!value.composing.isValid) {
      _compositionUndoGroupId = null;
    }
  }

  FlarkSelection? _selectionFromTextSelection(TextSelection selection) {
    return FlarkMarkdownInputPolicy.selectionFromTextSelection(selection);
  }

  FlarkMarkdownInputPolicy get _markdownInputPolicy {
    return FlarkMarkdownInputPolicy(
      controller: widget.controller,
      enterUserEvent: 'input.editableText.enter',
      backspaceUserEvent: 'input.editableText.backspace',
    );
  }
}

Color _selectionColorForCursor(Color cursorColor) {
  return cursorColor.withValues(alpha: 0.24);
}
