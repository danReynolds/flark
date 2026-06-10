import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import 'flark_command_actions.dart';
import 'flark_editor_read_only_scope.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_input_policy.dart';
import 'flark_text_selection_gestures.dart';

const _virtualizedSourceDocumentLengthThreshold = 250000;

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
    if (_usesVirtualizedSource) {
      return _buildVirtualizedSourceEditor(style);
    }
    return _buildFullSourceEditor(style);
  }

  Widget _buildFullSourceEditor(TextStyle style) {
    Widget editor = EditableText(
      key: _editableStateKey,
      readOnly: FlarkEditorReadOnlyScope.of(context),
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

  Widget _buildVirtualizedSourceEditor(TextStyle style) {
    Widget editor = _FlarkVirtualizedSourceEditor(
      controller: widget.controller,
      inheritedFocusNode: widget.focusNode,
      style: style,
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      autofocus: widget.autofocus,
    );
    editor = _markdownInputPolicy.wrapKeyboardShortcuts(
      child: editor,
      currentSelection: () => widget.controller.selection,
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
    if (_usesVirtualizedSource) {
      if (mounted) setState(() {});
      return;
    }
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

  bool get _usesVirtualizedSource {
    if (widget.minLines != null || widget.maxLines != null || widget.expands) {
      return false;
    }
    return widget.controller.markdown.length >=
        _virtualizedSourceDocumentLengthThreshold;
  }
}

Color _selectionColorForCursor(Color cursorColor) {
  return cursorColor.withValues(alpha: 0.24);
}

final class _FlarkVirtualizedSourceEditor extends StatelessWidget {
  const _FlarkVirtualizedSourceEditor({
    required this.controller,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.inheritedFocusNode,
    this.autofocus = false,
  });

  final FlarkFlutterController controller;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? inheritedFocusNode;
  final bool autofocus;

  @override
  Widget build(BuildContext context) {
    final buffer = controller.state.document.buffer;
    return LayoutBuilder(
      builder: (context, constraints) {
        return ListView.builder(
          key: const Key('FlarkVirtualizedSourceEditor'),
          shrinkWrap: !constraints.hasBoundedHeight,
          itemCount: buffer.lineCount,
          itemBuilder: (context, index) {
            final lineStart = buffer.lineStart(index);
            final lineEnd = buffer.lineEnd(index);
            final lineText = controller.markdown.substring(lineStart, lineEnd);
            final selected = _lineContainsSelection(
              controller.selection,
              lineStart,
              lineEnd,
            );
            return _FlarkVirtualizedSourceLine(
              key: ValueKey('sourceLine:$index'),
              controller: controller,
              lineIndex: index,
              lineStart: lineStart,
              lineEnd: lineEnd,
              text: lineText,
              selected: selected,
              inheritedFocusNode: selected ? inheritedFocusNode : null,
              style: style,
              cursorColor: cursorColor,
              backgroundCursorColor: backgroundCursorColor,
              autofocus: autofocus && selected,
            );
          },
        );
      },
    );
  }

  static bool _lineContainsSelection(
    FlarkSelection selection,
    int lineStart,
    int lineEnd,
  ) {
    final selectionStart = selection.start;
    final selectionEnd = selection.end;
    return selectionEnd >= lineStart && selectionStart <= lineEnd;
  }
}

final class _FlarkVirtualizedSourceLine extends StatefulWidget {
  const _FlarkVirtualizedSourceLine({
    super.key,
    required this.controller,
    required this.lineIndex,
    required this.lineStart,
    required this.lineEnd,
    required this.text,
    required this.selected,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.inheritedFocusNode,
    this.autofocus = false,
  });

  final FlarkFlutterController controller;
  final int lineIndex;
  final int lineStart;
  final int lineEnd;
  final String text;
  final bool selected;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? inheritedFocusNode;
  final bool autofocus;

  @override
  State<_FlarkVirtualizedSourceLine> createState() {
    return _FlarkVirtualizedSourceLineState();
  }
}

final class _FlarkVirtualizedSourceLineState
    extends State<_FlarkVirtualizedSourceLine> {
  late final TextEditingController _textController;
  final _editableStateKey = GlobalKey<EditableTextState>();
  FocusNode? _ownedFocusNode;
  bool _syncing = false;

  FocusNode get _focusNode => widget.inheritedFocusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    _textController = TextEditingController();
    _textController.addListener(_handleTextEditingValueChanged);
    if (widget.inheritedFocusNode == null) _ownedFocusNode = FocusNode();
    _syncFromWidget();
  }

  @override
  void didUpdateWidget(_FlarkVirtualizedSourceLine oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.inheritedFocusNode == null &&
        widget.inheritedFocusNode != null) {
      _ownedFocusNode?.dispose();
      _ownedFocusNode = null;
    } else if (oldWidget.inheritedFocusNode != null &&
        widget.inheritedFocusNode == null) {
      _ownedFocusNode = FocusNode();
    }
    _syncFromWidget();
  }

  @override
  void dispose() {
    _textController.removeListener(_handleTextEditingValueChanged);
    _ownedFocusNode?.dispose();
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    Widget editor = EditableText(
      key: _editableStateKey,
      readOnly: FlarkEditorReadOnlyScope.of(context),
      controller: _textController,
      focusNode: _focusNode,
      style: widget.style,
      cursorColor: widget.cursorColor,
      selectionColor: _selectionColorForCursor(widget.cursorColor),
      selectionControls: flarkTextSelectionControlsForPlatform(context),
      backgroundCursorColor: widget.backgroundCursorColor,
      maxLines: null,
      autofocus: widget.autofocus,
      keyboardType: TextInputType.multiline,
      textInputAction: TextInputAction.newline,
      paintCursorAboveText: true,
    );
    editor = flarkEditableTextGestureDetector(
      editableTextKey: _editableStateKey,
      child: editor,
    );
    return editor;
  }

  void _syncFromWidget() {
    final next = TextEditingValue(
      text: widget.text,
      selection: _lineTextSelection(),
      composing: _textController.value.text == widget.text
          ? _textController.value.composing
          : TextRange.empty,
    );
    if (_textController.value == next) return;
    _syncing = true;
    _textController.value = next;
    _syncing = false;
  }

  TextSelection _lineTextSelection() {
    final current = _textController.selection;
    if (!widget.selected) {
      if (current.isValid &&
          current.baseOffset <= widget.text.length &&
          current.extentOffset <= widget.text.length) {
        return current;
      }
      return TextSelection.collapsed(offset: widget.text.length);
    }
    final selection = widget.controller.selection;
    return TextSelection(
      baseOffset: (selection.baseOffset - widget.lineStart).clamp(
        0,
        widget.text.length,
      ),
      extentOffset: (selection.extentOffset - widget.lineStart).clamp(
        0,
        widget.text.length,
      ),
    );
  }

  void _handleTextEditingValueChanged() {
    if (_syncing) return;
    final value = _textController.value;
    if (value.text != widget.text) {
      if (_markdownInputPolicy.handlePlatformTextChange(
        oldText: widget.text,
        newValue: value,
        oldTextSelection: _localFlarkSelection(),
        applyOldTextSelection: _applyLocalSelection,
      )) {
        return;
      }
      final textLengthAfter =
          widget.controller.markdown.length -
          (widget.lineEnd - widget.lineStart) +
          value.text.length;
      final selection = _sourceSelection(
        value.selection,
        textLength: textLengthAfter,
      );
      widget.controller.applyTransaction(
        FlarkTransaction.single(
          FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(widget.lineStart, widget.lineEnd),
            replacementText: value.text,
          ),
          selectionAfter: selection,
          metadata: FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.input,
            userEvent: 'input.virtualizedSourceLine',
            parseInvalidationRange: FlarkSourceRange(
              widget.lineStart,
              widget.lineEnd,
            ),
            projectionInvalidationRange: FlarkSourceRange(
              widget.lineStart,
              widget.lineEnd,
            ),
          ),
        ),
      );
      return;
    }

    final selection = _sourceSelection(
      value.selection,
      textLength: widget.controller.markdown.length,
    );
    if (selection == null || selection == widget.controller.selection) return;
    widget.controller.applySelection(
      selection,
      userEvent: 'selection.virtualizedSourceLine',
    );
  }

  FlarkSelection? _localFlarkSelection() {
    if (!widget.selected) return null;
    final selection = widget.controller.selection;
    return FlarkSelection(
      baseOffset: (selection.baseOffset - widget.lineStart).clamp(
        0,
        widget.text.length,
      ),
      extentOffset: (selection.extentOffset - widget.lineStart).clamp(
        0,
        widget.text.length,
      ),
    );
  }

  void _applyLocalSelection(FlarkSelection localSelection) {
    widget.controller.applySelection(
      FlarkSelection(
        baseOffset: widget.lineStart + localSelection.baseOffset,
        extentOffset: widget.lineStart + localSelection.extentOffset,
      ).validate(widget.controller.markdown.length),
      userEvent: 'selection.virtualizedSourceLine.markdownPolicy',
    );
  }

  FlarkSelection? _sourceSelection(
    TextSelection selection, {
    required int textLength,
  }) {
    final local = FlarkMarkdownInputPolicy.selectionFromTextSelection(
      selection,
    );
    if (local == null) return null;
    return FlarkSelection(
      baseOffset: widget.lineStart + local.baseOffset,
      extentOffset: widget.lineStart + local.extentOffset,
    ).validate(textLength);
  }

  FlarkMarkdownInputPolicy get _markdownInputPolicy {
    return FlarkMarkdownInputPolicy(
      controller: widget.controller,
      enterUserEvent: 'input.virtualizedSourceLine.enter',
      backspaceUserEvent: 'input.virtualizedSourceLine.backspace',
    );
  }
}
