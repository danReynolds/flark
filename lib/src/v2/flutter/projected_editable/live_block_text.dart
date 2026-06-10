// The per-block editable text surface: the projected block text field,
// its platform-echo handling pipeline, composition undo grouping, and
// the keyboard-synced EditableText wrapper.

part of '../flark_projected_editable_text.dart';

final class _EditableProjectedBlockText extends StatefulWidget {
  const _EditableProjectedBlockText({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.blockHandle,
    this.focusNode,
    this.autofocus = false,
    this.editableKey,
    this.markdownInputPolicy = false,
    this.sourceRangeForEdits,
    this.sourceEditForReplacement,
    this.codeSyntaxLanguage,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final Key? editableKey;
  final bool markdownInputPolicy;
  final FlarkSourceRange? Function(String markdown, FlarkRenderBlock block)?
  sourceRangeForEdits;
  final FlarkLiveBlockSourceEdit Function({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange range,
    required TextEditingValue value,
  })?
  sourceEditForReplacement;
  final String? codeSyntaxLanguage;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;
  String get currentDisplayText => blockHandle?.displayText ?? displayText;

  @override
  State<_EditableProjectedBlockText> createState() {
    return _EditableProjectedBlockTextState();
  }
}

final class _EditableProjectedBlockTextState
    extends State<_EditableProjectedBlockText> {
  late final _FlarkBlockTextController _textController;
  FocusNode? _ownedFocusNode;
  bool _syncing = false;
  final _compositionUndoGrouping = _FlarkCompositionUndoGrouping();
  TextEditingValue? _localValueSnapshot;
  FlarkSourceRange? _directSourceRangeSnapshot;
  String? _pendingCodeBodyPlatformEchoText;

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    _textController = _FlarkBlockTextController();
    _textController.addListener(_handleTextChanged);
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
    _syncFromController();
  }

  @override
  void didUpdateWidget(_EditableProjectedBlockText oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.focusNode == null && widget.focusNode != null) {
      _ownedFocusNode?.dispose();
      _ownedFocusNode = null;
    } else if (oldWidget.focusNode != null && widget.focusNode == null) {
      _ownedFocusNode = FocusNode();
    }
    _syncFromController();
  }

  @override
  void dispose() {
    _textController.removeListener(_handleTextChanged);
    _ownedFocusNode?.dispose();
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final block = widget.currentBlock;
    final displayText = widget.currentDisplayText;
    _textController.block = block;
    _textController.displayText = displayText;
    _textController.codeSyntaxLanguage = widget.codeSyntaxLanguage;
    Widget editor = _KeyboardSyncedEditableText(
      key: widget.editableKey,
      controller: _textController,
      focusNode: _focusNode,
      style: _blockTextStyle(widget.style, block),
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: _minimumEditableLineCount(_textController.text),
      maxLines: null,
      autofocus: widget.autofocus,
      keyboardType: TextInputType.multiline,
      textInputAction: TextInputAction.newline,
      onKeyEvent: _handleVerticalBoundaryKeyEvent,
    );
    editor = _LiveRenderedDocumentIntentActions(
      onSelectAll: _selectAllDocument,
      onDeleteSelection: _deleteControllerSelection,
      onMoveVerticallyToAdjacentLine: _moveOutOfBlockOnVerticalBoundary,
      child: editor,
    );
    final markdownInputPolicy =
        widget.markdownInputPolicy && _markdownInputPolicy.isEnabled;
    if (!markdownInputPolicy) return editor;

    return _markdownInputPolicy.wrapKeyboardShortcuts(
      child: editor,
      currentSelection: () =>
          FlarkMarkdownInputPolicy.selectionFromTextSelection(
            _textController.selection,
          ),
      applySelection: _applyLocalDisplaySelectionToController,
    );
  }

  void _handleTextChanged() {
    if (_syncing) return;

    final externalFocusNodeToRestore = widget.focusNode;
    final shouldRestoreExternalFocus = externalFocusNodeToRestore != null;
    final directSourceRange = _sourceEditRange();
    final snapshot = _localEditSnapshot(directSourceRange);
    final classification = classifyFlarkLiveBlockEdit(
      FlarkLiveBlockEditContext(
        markdown: widget.controller.markdown,
        block: widget.currentBlock,
        displayText: widget.currentDisplayText,
        sourceRange: snapshot.directSourceRange ?? directSourceRange,
        oldValue: snapshot.value,
        newValue: _textController.value,
        markdownInputPolicyEnabled: widget.markdownInputPolicy,
        pendingCodeBodyEchoText: _pendingCodeBodyPlatformEchoText,
      ),
    );
    _pendingCodeBodyPlatformEchoText =
        classification.nextPendingCodeBodyEchoText;
    final value = classification.normalizedValue;
    _adoptNormalizedTextControllerValue(value);
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    _executeBlockIntent(
      classification.intent,
      value: value,
      compositionUndoGroupId: compositionUndoGroupId,
      externalFocusNodeToRestore: externalFocusNodeToRestore,
      shouldRestoreExternalFocus: shouldRestoreExternalFocus,
    );
    _compositionUndoGrouping.clearIfCommitted(value);
  }

  void _executeBlockIntent(
    FlarkLiveBlockEditIntent intent, {
    required TextEditingValue value,
    required int? compositionUndoGroupId,
    required FocusNode? externalFocusNodeToRestore,
    required bool shouldRestoreExternalFocus,
  }) {
    switch (intent) {
      case FlarkLiveBlockResyncIntent():
        _syncFromController();
      case FlarkLiveBlockCaretMoveIntent(:final selection, :final userEvent):
        widget.controller.applySelection(selection, userEvent: userEvent);
        _syncFromController();
        _restoreExternalFocusAfterFrame(
          externalFocusNodeToRestore,
          shouldRestoreExternalFocus,
        );
      case FlarkLiveBlockEnterDispatchIntent(:final currentSelection):
        _markdownInputPolicy.dispatchEnter(
          currentSelection: () => currentSelection,
          applySelection: _applyLocalDisplaySelectionToController,
        );
        _restoreExternalFocusAfterFrame(
          externalFocusNodeToRestore,
          shouldRestoreExternalFocus,
        );
      case FlarkLiveBlockLanguageShortcutIntent(:final edit):
        _replaceSourceRange(
          controller: widget.controller,
          range: edit.range,
          replacementText: edit.replacementText,
          selectionAfter: edit.selectionAfter,
          userEvent: 'input.liveBlock.codeFenceLanguageShortcut',
          undoGroupId: compositionUndoGroupId,
        );
        _rememberLocalEditSnapshot(
          TextEditingValue(
            text: '',
            selection: const TextSelection.collapsed(offset: 0),
          ),
          directSourceRange: edit.editableRangeAfter,
        );
        _restoreExternalFocusAfterFrame(
          externalFocusNodeToRestore,
          shouldRestoreExternalFocus,
        );
      case FlarkLiveBlockPlatformTextChangeIntent():
        if (_markdownInputPolicy.handlePlatformTextChange(
          oldText: intent.oldText,
          newValue: intent.policyValue,
          oldTextSelection: intent.oldTextSelection,
          applyOldTextSelection: _applyLocalDisplaySelectionToController,
        )) {
          if (intent.resyncWhenHandled) _syncFromController();
          _restoreExternalFocusAfterFrame(
            externalFocusNodeToRestore,
            shouldRestoreExternalFocus,
          );
          return;
        }
        _executeBlockIntent(
          intent.fallback,
          value: value,
          compositionUndoGroupId: compositionUndoGroupId,
          externalFocusNodeToRestore: externalFocusNodeToRestore,
          shouldRestoreExternalFocus: shouldRestoreExternalFocus,
        );
      case FlarkLiveBlockDirectReplacementIntent(:final sourceRange):
        final sourceEdit =
            widget.sourceEditForReplacement?.call(
              markdown: widget.controller.markdown,
              block: widget.currentBlock,
              range: sourceRange,
              value: value,
            ) ??
            FlarkLiveBlockSourceEdit(
              range: sourceRange,
              replacementText: value.text,
              editableRangeAfter: FlarkSourceRange(
                sourceRange.start,
                sourceRange.start + value.text.length,
              ),
              selectionAfter: _sourceSelectionAfterReplacement(
                range: sourceRange,
                localSelection: value.selection,
                replacementTextLength: value.text.length,
              ),
            );
        _replaceSourceRange(
          controller: widget.controller,
          range: sourceEdit.range,
          replacementText: sourceEdit.replacementText,
          selectionAfter: sourceEdit.selectionAfter,
          userEvent: 'input.liveBlock.text',
          undoGroupId: compositionUndoGroupId,
        );
        _rememberLocalEditSnapshot(
          value,
          directSourceRange: sourceEdit.editableRangeAfter,
        );
        _restoreExternalFocusAfterFrame(
          externalFocusNodeToRestore,
          shouldRestoreExternalFocus,
        );
      case FlarkLiveBlockProjectedEditIntent():
        if (intent.adoptBlockValue) {
          _adoptNormalizedTextControllerValue(intent.blockValue);
        }
        final applied = widget.controller.applyProjectedTextEdit(
          oldDisplayText: intent.oldDisplayText,
          newDisplayText: intent.newDisplayText,
          undoGroupId: compositionUndoGroupId,
        );
        if (!applied) {
          _syncFromController();
        } else if (intent.immediateParseAfterApply) {
          _adoptImmediateMarkdownParseForController(widget.controller);
        }
        _restoreExternalFocusAfterFrame(
          externalFocusNodeToRestore,
          shouldRestoreExternalFocus,
        );
        _rememberLocalEditSnapshot(intent.blockValue, directSourceRange: null);
      case FlarkLiveBlockSourceSelectionIntent(
        :final selection,
        :final snapshotRange,
      ):
        widget.controller.applySelection(
          selection,
          userEvent: 'selection.liveBlock',
        );
        _rememberLocalEditSnapshot(value, directSourceRange: snapshotRange);
      case FlarkLiveBlockProjectedSelectionIntent(:final selection):
        widget.controller.applyProjectedSelection(selection);
        _rememberLocalEditSnapshot(value, directSourceRange: null);
      case FlarkLiveBlockIgnoreIntent():
        break;
    }
  }

  void _restoreExternalFocusAfterFrame(
    FocusNode? focusNode,
    bool shouldRestore,
  ) {
    if (!shouldRestore || focusNode == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (focusNode.canRequestFocus && !focusNode.hasFocus) {
        focusNode.requestFocus();
      }
    });
  }

  void _applyLocalDisplaySelectionToController(FlarkSelection localSelection) {
    final directSourceRange = _sourceEditRange();
    if (directSourceRange != null) {
      widget.controller.applySelection(
        FlarkSelection(
          baseOffset: directSourceRange.start + localSelection.baseOffset,
          extentOffset: directSourceRange.start + localSelection.extentOffset,
        ),
        userEvent: 'selection.liveBlock',
      );
      return;
    }
    final range = flarkClampedDisplayRange(
      widget.currentBlock,
      widget.currentDisplayText,
    );
    widget.controller.applyProjectedSelection(
      FlarkSelection(
        baseOffset: range.start + localSelection.baseOffset,
        extentOffset: range.start + localSelection.extentOffset,
      ),
    );
  }

  FlarkMarkdownInputPolicy get _markdownInputPolicy {
    return FlarkMarkdownInputPolicy(
      controller: widget.controller,
      enterUserEvent: 'input.liveBlock.enter',
      backspaceUserEvent: 'input.liveBlock.backspace',
      onHandled: () {
        _adoptImmediateMarkdownParseForController(widget.controller);
        _rememberPendingCodeBodyPlatformEcho();
      },
    );
  }

  void _syncFromController() {
    final text = _localText();
    final selection = _localSelection(text.length);
    final current = _textController.value;
    final directSourceRange = _sourceEditRange();
    final next = TextEditingValue(
      text: text,
      selection: selection,
      composing: current.text == text ? current.composing : TextRange.empty,
    );
    _textController.block = widget.currentBlock;
    _textController.displayText = widget.currentDisplayText;
    _textController.codeSyntaxLanguage = widget.codeSyntaxLanguage;
    _rememberLocalEditSnapshot(next, directSourceRange: directSourceRange);
    if (current == next) return;
    _syncing = true;
    _textController.value = next;
    _syncing = false;
  }

  _LocalTextEditSnapshot _localEditSnapshot(
    FlarkSourceRange? directSourceRange,
  ) {
    final snapshotValue = _localValueSnapshot;
    final snapshotRange = _directSourceRangeSnapshot;
    if (directSourceRange != null &&
        snapshotValue != null &&
        snapshotRange != null) {
      if (_sourceRangeStillMatches(directSourceRange, snapshotValue.text)) {
        return _LocalTextEditSnapshot(
          value: snapshotValue,
          directSourceRange: directSourceRange,
        );
      }
      if (_sourceRangeStillMatches(snapshotRange, snapshotValue.text)) {
        return _LocalTextEditSnapshot(
          value: snapshotValue,
          directSourceRange: snapshotRange,
        );
      }
    }

    final text = _localText();
    return _LocalTextEditSnapshot(
      value: TextEditingValue(
        text: text,
        selection: _localSelection(text.length),
        composing: TextRange.empty,
      ),
      directSourceRange: directSourceRange,
    );
  }

  bool _sourceRangeStillMatches(FlarkSourceRange range, String text) {
    final markdown = widget.controller.markdown;
    if (range.start < 0 ||
        range.start > range.end ||
        range.end > markdown.length) {
      return false;
    }
    return markdown.substring(range.start, range.end) == text;
  }

  void _rememberPendingCodeBodyPlatformEcho() {
    _pendingCodeBodyPlatformEchoText =
        FlarkLiveCodeFenceInputPolicy.pendingEchoText(
          markdown: widget.controller.markdown,
          block: widget.currentBlock,
          range: _sourceEditRange(),
          text: _localText(),
        );
  }

  void _adoptNormalizedTextControllerValue(TextEditingValue value) {
    if (_textController.value == value) return;
    _syncing = true;
    _textController.value = value;
    _syncing = false;
  }

  void _rememberLocalEditSnapshot(
    TextEditingValue value, {
    required FlarkSourceRange? directSourceRange,
  }) {
    _localValueSnapshot = value;
    _directSourceRangeSnapshot = directSourceRange;
  }

  String _localText() {
    final directSourceRange = _sourceEditRange();
    if (directSourceRange != null) {
      return widget.controller.markdown.substring(
        directSourceRange.start,
        directSourceRange.end,
      );
    }
    final block = widget.currentBlock;
    final displayText = widget.currentDisplayText;
    final range = flarkClampedDisplayRange(block, displayText);
    return displayText.substring(range.start, range.end);
  }

  TextSelection _localSelection(int textLength) {
    final directSourceRange = _sourceEditRange();
    if (directSourceRange != null) {
      final sourceSelection = widget.controller.selection;
      if (_sourceSelectionCoversRange(sourceSelection, directSourceRange)) {
        return TextSelection(baseOffset: 0, extentOffset: textLength);
      }
      if (sourceSelection.start < directSourceRange.start ||
          sourceSelection.end > directSourceRange.end) {
        final current = _textController.selection;
        if (current.isValid &&
            current.baseOffset <= textLength &&
            current.extentOffset <= textLength) {
          return current;
        }
        return TextSelection.collapsed(offset: textLength);
      }
      return TextSelection(
        baseOffset: sourceSelection.baseOffset - directSourceRange.start,
        extentOffset: sourceSelection.extentOffset - directSourceRange.start,
      );
    }
    final sourceSelection = widget.controller.selection;
    if (_sourceSelectionCoversBlock(sourceSelection)) {
      return TextSelection(baseOffset: 0, extentOffset: textLength);
    }
    final displaySelection = widget.controller.projection
        .sourceSelectionToDisplay(widget.controller.selection);
    final range = flarkClampedDisplayRange(
      widget.currentBlock,
      widget.currentDisplayText,
    );
    if (displaySelection.start < range.start ||
        displaySelection.end > range.end) {
      final current = _textController.selection;
      if (current.isValid &&
          current.baseOffset <= textLength &&
          current.extentOffset <= textLength) {
        return current;
      }
      return TextSelection.collapsed(offset: textLength);
    }
    return TextSelection(
      baseOffset: displaySelection.baseOffset - range.start,
      extentOffset: displaySelection.extentOffset - range.start,
    );
  }

  void _selectAllDocument(SelectionChangedCause cause) {
    final localSelection = TextSelection(
      baseOffset: 0,
      extentOffset: _textController.text.length,
    );
    _syncing = true;
    _textController.selection = localSelection;
    _syncing = false;
    widget.controller.applySelection(
      FlarkSelection(
        baseOffset: 0,
        extentOffset: widget.controller.markdown.length,
      ),
      userEvent: 'selection.liveBlock.selectAll',
    );
  }

  bool _deleteControllerSelection() {
    if (widget.controller.selection.isCollapsed) return false;
    final result = widget.controller.dispatch(
      command: FlarkMarkdownInputCommands.handleBackspace,
      payload: const FlarkHandleBackspacePayload(
        userEvent: 'input.liveBlock.deleteSelection',
      ),
    );
    final handled =
        result.commandResult.isHandled &&
        result.commandResult.transaction != null;
    if (handled) {
      _adoptImmediateMarkdownParseForController(widget.controller);
    }
    return handled;
  }

  bool _moveOutOfBlockOnVerticalBoundary(
    ExtendSelectionVerticallyToAdjacentLineIntent intent,
  ) {
    if (!intent.collapseSelection) return false;
    final selection = _textController.selection;
    if (!selection.isValid || !selection.isCollapsed) return false;

    if (intent.forward) {
      final moveNext = widget.onMoveToNextBlock;
      if (moveNext == null ||
          !_isCaretOnLastTextLine(
            _textController.text,
            selection.extentOffset,
          )) {
        return false;
      }
      moveNext();
      return true;
    }

    final movePrevious = widget.onMoveToPreviousBlock;
    if (movePrevious == null ||
        !_isCaretOnFirstTextLine(
          _textController.text,
          selection.extentOffset,
        )) {
      return false;
    }
    movePrevious();
    return true;
  }

  KeyEventResult _handleVerticalBoundaryKeyEvent(
    FocusNode node,
    KeyEvent event,
  ) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    if (HardwareKeyboard.instance.isShiftPressed ||
        HardwareKeyboard.instance.isAltPressed ||
        HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isMetaPressed) {
      return KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.numpadEnter) {
      if (_ignoreEmptyOpeningCodeBodyKeyboardEnter()) {
        return KeyEventResult.handled;
      }
      if (_promoteCodeBodyLanguageShortcutFromKeyboardEnter()) {
        return KeyEventResult.handled;
      }
      return _handleCodeBodyKeyboardEnter()
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
      return _moveOutOfBlockOnVerticalBoundary(
            const ExtendSelectionVerticallyToAdjacentLineIntent(
              forward: false,
              collapseSelection: true,
            ),
          )
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
      return _moveOutOfBlockOnVerticalBoundary(
            const ExtendSelectionVerticallyToAdjacentLineIntent(
              forward: true,
              collapseSelection: true,
            ),
          )
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    return KeyEventResult.ignored;
  }

  bool _ignoreEmptyOpeningCodeBodyKeyboardEnter() {
    if (_textController.text.isNotEmpty) return false;
    if (!FlarkLiveCodeFenceInputPolicy.isCollapsedSelectionAt(
      _textController.selection,
      0,
    )) {
      return false;
    }
    final sourceRange = _sourceEditRange();
    if (sourceRange == null || !sourceRange.isCollapsed) return false;
    final block = widget.currentBlock;
    if (block.codeBlock == null) return false;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      widget.controller.markdown,
      block.sourceRange.start,
    );
    if (context == null) return false;
    final bodyRange = context.bodyContentRange(widget.controller.markdown);
    if (!bodyRange.isCollapsed || sourceRange.start != bodyRange.start) {
      return false;
    }
    if (context.closingLineStart == null) return true;
    final afterFence =
        context.closingLineEndWithBreak ?? context.closingLineEnd;
    if (afterFence == null || afterFence >= widget.controller.markdown.length) {
      return false;
    }
    return !FlarkMarkdownFencedCodeScanner.isWhitespace(
      widget.controller.markdown.substring(afterFence),
    );
  }

  bool _promoteCodeBodyLanguageShortcutFromKeyboardEnter() {
    final text = _textController.text;
    if (text.isEmpty || text.contains('\n')) return false;
    if (!FlarkLiveCodeFenceInputPolicy.isCollapsedSelectionAt(
      _textController.selection,
      text.length,
    )) {
      return false;
    }
    final sourceRange = _sourceEditRange();
    final sourceEdit = FlarkLiveCodeFenceInputPolicy.languageShortcutEdit(
      markdown: widget.controller.markdown,
      block: widget.currentBlock,
      range: sourceRange,
      oldText: text,
      value: TextEditingValue(
        text: '$text\n',
        selection: TextSelection.collapsed(offset: text.length + 1),
      ),
    );
    if (sourceEdit == null) return false;

    _replaceSourceRange(
      controller: widget.controller,
      range: sourceEdit.range,
      replacementText: sourceEdit.replacementText,
      selectionAfter: sourceEdit.selectionAfter,
      userEvent: 'input.liveBlock.codeFenceLanguageShortcut',
    );
    _rememberLocalEditSnapshot(
      TextEditingValue(
        text: '',
        selection: const TextSelection.collapsed(offset: 0),
      ),
      directSourceRange: sourceEdit.editableRangeAfter,
    );
    return true;
  }

  bool _handleCodeBodyKeyboardEnter() {
    if (widget.currentBlock.codeBlock == null) return false;
    if (!widget.markdownInputPolicy || !_markdownInputPolicy.isEnabled) {
      return false;
    }
    return _markdownInputPolicy.dispatchEnter(
      currentSelection: () =>
          FlarkMarkdownInputPolicy.selectionFromTextSelection(
            _textController.selection,
          ),
      applySelection: _applyLocalDisplaySelectionToController,
    );
  }

  bool _sourceSelectionCoversBlock(FlarkSelection selection) {
    return _sourceSelectionCoversRange(
      selection,
      widget.currentBlock.sourceRange,
    );
  }

  bool _sourceSelectionCoversRange(
    FlarkSelection selection,
    FlarkSourceRange range,
  ) {
    return !selection.isCollapsed &&
        selection.start <= range.start &&
        selection.end >= range.end;
  }

  FlarkSourceRange? _sourceEditRange() {
    final resolver = widget.sourceRangeForEdits;
    if (resolver == null) return null;
    final range = resolver(widget.controller.markdown, widget.currentBlock);
    if (range == null) return null;
    if (range.start < 0 ||
        range.start > range.end ||
        range.end > widget.controller.markdown.length) {
      return null;
    }
    return range;
  }
}

final class _LocalTextEditSnapshot {
  const _LocalTextEditSnapshot({
    required this.value,
    required this.directSourceRange,
  });

  final TextEditingValue value;
  final FlarkSourceRange? directSourceRange;
}

final class _FlarkCompositionUndoGrouping {
  static int _nextUndoGroupId = 1;

  int? _undoGroupId;

  int? groupIdFor(TextEditingValue value) {
    if (!value.composing.isValid && _undoGroupId == null) return null;
    _undoGroupId ??= 0xC1000000 + _nextUndoGroupId++;
    return _undoGroupId;
  }

  void clearIfCommitted(TextEditingValue value) {
    if (!value.composing.isValid) _undoGroupId = null;
  }
}

int _minimumEditableLineCount(String text) {
  var lines = 1;
  for (final codeUnit in text.codeUnits) {
    if (codeUnit == 0x0A) lines++;
  }
  return lines;
}

bool _isCaretOnFirstTextLine(String text, int offset) {
  final clampedOffset = offset.clamp(0, text.length);
  final firstLineBreak = text.indexOf('\n');
  return firstLineBreak < 0 || clampedOffset <= firstLineBreak;
}

bool _isCaretOnLastTextLine(String text, int offset) {
  final clampedOffset = offset.clamp(0, text.length);
  final lastLineBreak = text.lastIndexOf('\n');
  return lastLineBreak < 0 || clampedOffset > lastLineBreak;
}

final class _LiveRenderedDocumentIntentActions extends StatelessWidget {
  const _LiveRenderedDocumentIntentActions({
    required this.child,
    required this.onSelectAll,
    required this.onDeleteSelection,
    required this.onMoveVerticallyToAdjacentLine,
  });

  final Widget child;
  final void Function(SelectionChangedCause cause) onSelectAll;
  final bool Function() onDeleteSelection;
  final bool Function(ExtendSelectionVerticallyToAdjacentLineIntent intent)
  onMoveVerticallyToAdjacentLine;

  @override
  Widget build(BuildContext context) {
    return Actions(
      actions: {
        SelectAllTextIntent: CallbackAction<SelectAllTextIntent>(
          onInvoke: (intent) {
            onSelectAll(intent.cause);
            return null;
          },
        ),
        DeleteCharacterIntent: _LiveRenderedDeleteCharacterAction(
          onDeleteSelection: onDeleteSelection,
        ),
        ExtendSelectionVerticallyToAdjacentLineIntent:
            _LiveRenderedVerticalNavigationAction(
              onMoveOutOfBlock: onMoveVerticallyToAdjacentLine,
            ),
      },
      child: child,
    );
  }
}

final class _LiveRenderedDeleteCharacterAction
    extends Action<DeleteCharacterIntent> {
  _LiveRenderedDeleteCharacterAction({required this.onDeleteSelection});

  final bool Function() onDeleteSelection;

  @override
  Object? invoke(DeleteCharacterIntent intent) {
    if (onDeleteSelection()) return null;
    return callingAction?.invoke(intent);
  }
}

final class _LiveRenderedVerticalNavigationAction
    extends Action<ExtendSelectionVerticallyToAdjacentLineIntent> {
  _LiveRenderedVerticalNavigationAction({required this.onMoveOutOfBlock});

  final bool Function(ExtendSelectionVerticallyToAdjacentLineIntent intent)
  onMoveOutOfBlock;

  @override
  Object? invoke(ExtendSelectionVerticallyToAdjacentLineIntent intent) {
    if (onMoveOutOfBlock(intent)) return null;
    return callingAction?.invoke(intent);
  }
}

final class _KeyboardSyncedEditableText extends StatefulWidget {
  const _KeyboardSyncedEditableText({
    super.key,
    required this.controller,
    required this.focusNode,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    required this.keyboardType,
    required this.textInputAction,
    this.minLines,
    this.maxLines,
    this.autofocus = false,
    this.onKeyEvent,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final TextInputType keyboardType;
  final TextInputAction textInputAction;
  final int? minLines;
  final int? maxLines;
  final bool autofocus;
  final FocusOnKeyEventCallback? onKeyEvent;

  @override
  State<_KeyboardSyncedEditableText> createState() {
    return _KeyboardSyncedEditableTextState();
  }
}

final class _KeyboardSyncedEditableTextState
    extends State<_KeyboardSyncedEditableText> {
  final _editableStateKey = GlobalKey<EditableTextState>();
  bool _keyboardSyncScheduled = false;
  FocusNode? _keyHandlerFocusNode;
  FocusOnKeyEventCallback? _previousOnKeyEvent;
  FocusOnKeyEventCallback? _installedOnKeyEvent;

  @override
  void initState() {
    super.initState();
    _syncFocusNodeKeyHandler();
  }

  @override
  void didUpdateWidget(_KeyboardSyncedEditableText oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncFocusNodeKeyHandler();
  }

  @override
  void dispose() {
    _detachFocusNodeKeyHandler();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final editor = EditableText(
      key: _editableStateKey,
      controller: widget.controller,
      focusNode: widget.focusNode,
      style: widget.style,
      cursorColor: widget.cursorColor,
      selectionColor: _selectionColorForCursor(widget.cursorColor),
      selectionControls: flarkTextSelectionControlsForPlatform(context),
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      autofocus: widget.autofocus,
      keyboardType: widget.keyboardType,
      textInputAction: widget.textInputAction,
      paintCursorAboveText: true,
      rendererIgnoresPointer: true,
    );
    _scheduleKeyboardConnectionForInheritedFocus();
    return flarkEditableTextGestureDetector(
      editableTextKey: _editableStateKey,
      child: editor,
    );
  }

  void _scheduleKeyboardConnectionForInheritedFocus() {
    if (!widget.focusNode.hasFocus || _keyboardSyncScheduled) return;
    _keyboardSyncScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _keyboardSyncScheduled = false;
      if (!mounted || !widget.focusNode.hasFocus) return;
      _editableStateKey.currentState?.requestKeyboard();
    });
  }

  void _syncFocusNodeKeyHandler() {
    if (widget.onKeyEvent == null) {
      _detachFocusNodeKeyHandler();
      return;
    }
    if (identical(_keyHandlerFocusNode, widget.focusNode)) return;
    _detachFocusNodeKeyHandler();
    _keyHandlerFocusNode = widget.focusNode;
    _previousOnKeyEvent = widget.focusNode.onKeyEvent;
    _installedOnKeyEvent = _handleFocusKeyEvent;
    widget.focusNode.onKeyEvent = _installedOnKeyEvent;
  }

  void _detachFocusNodeKeyHandler() {
    final node = _keyHandlerFocusNode;
    final installed = _installedOnKeyEvent;
    if (node != null &&
        installed != null &&
        identical(node.onKeyEvent, installed)) {
      node.onKeyEvent = _previousOnKeyEvent;
    }
    _keyHandlerFocusNode = null;
    _previousOnKeyEvent = null;
    _installedOnKeyEvent = null;
  }

  KeyEventResult _handleFocusKeyEvent(FocusNode node, KeyEvent event) {
    final result =
        widget.onKeyEvent?.call(node, event) ?? KeyEventResult.ignored;
    if (result != KeyEventResult.ignored) return result;
    return _previousOnKeyEvent?.call(node, event) ?? KeyEventResult.ignored;
  }
}

final class _FlarkBlockTextController extends TextEditingController {
  FlarkRenderBlock? block;
  String displayText = '';
  String? codeSyntaxLanguage;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final effectiveStyle = style ?? DefaultTextStyle.of(context).style;
    final renderBlock = block;
    final composingRange = withComposing && value.isComposingRangeValid
        ? value.composing
        : null;
    if (renderBlock == null || text.isEmpty) {
      return _plainTextSpan(effectiveStyle, composingRange);
    }
    if (renderBlock.codeBlock != null) {
      final highlighted = buildFlarkHighlightedCodeSpan(
        source: text,
        language: codeSyntaxLanguage ?? renderBlock.codeBlock?.language,
        baseStyle: effectiveStyle,
        composingRange: composingRange,
      );
      if (highlighted != null) return highlighted;
    }
    if (renderBlock.inlineRuns.isEmpty) {
      return _plainTextSpan(effectiveStyle, composingRange);
    }

    final displayRange = flarkClampedDisplayRange(renderBlock, displayText);
    final segments = _blockTextSegments(
      textLength: text.length,
      globalDisplayStart: displayRange.start,
      block: renderBlock,
    );
    if (segments.isEmpty) return _plainTextSpan(effectiveStyle, composingRange);

    final children = <TextSpan>[];
    var cursor = 0;
    for (final segment in segments) {
      if (segment.start > cursor) {
        _appendStyledText(
          children,
          start: cursor,
          end: segment.start,
          style: effectiveStyle,
          composingRange: composingRange,
        );
      }
      _appendStyledText(
        children,
        start: segment.start,
        end: segment.end,
        style: segment.signature.resolve(effectiveStyle),
        composingRange: composingRange,
      );
      cursor = segment.end;
    }
    if (cursor < text.length) {
      _appendStyledText(
        children,
        start: cursor,
        end: text.length,
        style: effectiveStyle,
        composingRange: composingRange,
      );
    }
    return TextSpan(style: effectiveStyle, children: children);
  }

  TextSpan _plainTextSpan(TextStyle style, TextRange? composingRange) {
    if (composingRange == null) return TextSpan(style: style, text: text);
    return TextSpan(
      style: style,
      children: [
        TextSpan(text: composingRange.textBefore(text)),
        TextSpan(
          text: composingRange.textInside(text),
          style: style.merge(
            const TextStyle(decoration: TextDecoration.underline),
          ),
        ),
        TextSpan(text: composingRange.textAfter(text)),
      ],
    );
  }

  void _appendStyledText(
    List<TextSpan> spans, {
    required int start,
    required int end,
    required TextStyle style,
    required TextRange? composingRange,
  }) {
    if (start >= end) return;
    if (composingRange == null ||
        end <= composingRange.start ||
        start >= composingRange.end) {
      spans.add(TextSpan(text: text.substring(start, end), style: style));
      return;
    }

    final composingStart = composingRange.start.clamp(start, end);
    final composingEnd = composingRange.end.clamp(start, end);
    if (start < composingStart) {
      spans.add(
        TextSpan(text: text.substring(start, composingStart), style: style),
      );
    }
    spans.add(
      TextSpan(
        text: text.substring(composingStart, composingEnd),
        style: style.merge(
          const TextStyle(decoration: TextDecoration.underline),
        ),
      ),
    );
    if (composingEnd < end) {
      spans.add(
        TextSpan(text: text.substring(composingEnd, end), style: style),
      );
    }
  }
}
