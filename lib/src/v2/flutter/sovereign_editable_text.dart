import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import 'sovereign_command_actions.dart';
import 'sovereign_flutter_controller.dart';
import 'sovereign_markdown_input_policy.dart';
import 'sovereign_text_selection_gestures.dart';

final class SovereignEditableText extends StatefulWidget {
  const SovereignEditableText({
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
    this.shortcuts = const <ShortcutActivator, SovereignCommandIntent>{},
  });

  final SovereignFlutterController controller;
  final FocusNode? focusNode;
  final TextStyle? style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, SovereignCommandIntent> shortcuts;

  @override
  State<SovereignEditableText> createState() => _SovereignEditableTextState();
}

final class _SovereignEditableTextState extends State<SovereignEditableText> {
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
  void didUpdateWidget(SovereignEditableText oldWidget) {
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
      selectionControls: sovereignTextSelectionControlsForPlatform(context),
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
    editor = sovereignEditableTextGestureDetector(
      editableTextKey: _editableStateKey,
      child: editor,
    );
    editor = _markdownInputPolicy.wrapKeyboardShortcuts(
      child: editor,
      currentSelection: () =>
          SovereignMarkdownInputPolicy.selectionFromTextSelection(
        _textController.selection,
      ),
      applySelection: (selection) {
        widget.controller.applySelection(
          selection,
          userEvent: 'selection.editableText.markdownPolicy',
        );
      },
    );
    editor = SovereignCommandActions(
      controller: widget.controller,
      child: editor,
    );
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

  TextSelection _textSelection(SovereignSelection selection) {
    return TextSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }

  SovereignTransaction? _transactionForTextChange(
    String before,
    String after,
    TextSelection selection, {
    int? undoGroupId,
  }) {
    if (before == after) return null;

    var prefix = 0;
    final shortest =
        before.length < after.length ? before.length : after.length;
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
    return SovereignTransaction.single(
      SovereignSourceOperation.replace(
        replacedRange: SovereignSourceRange(prefix, beforeSuffix),
        replacementText: replacementText,
      ),
      selectionAfter: _selectionFromTextSelection(selection),
      metadata: SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.input,
        userEvent: 'input.editableText',
        undoGroupId: undoGroupId,
        parseInvalidationRange: SovereignSourceRange(prefix, beforeSuffix),
        projectionInvalidationRange: SovereignSourceRange(prefix, beforeSuffix),
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

  SovereignSelection? _selectionFromTextSelection(TextSelection selection) {
    return SovereignMarkdownInputPolicy.selectionFromTextSelection(selection);
  }

  SovereignMarkdownInputPolicy get _markdownInputPolicy {
    return SovereignMarkdownInputPolicy(
      controller: widget.controller,
      enterUserEvent: 'input.editableText.enter',
      backspaceUserEvent: 'input.editableText.backspace',
    );
  }
}

Color _selectionColorForCursor(Color cursorColor) {
  return cursorColor.withValues(alpha: 0.24);
}
