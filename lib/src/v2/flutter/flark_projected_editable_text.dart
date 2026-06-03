import 'dart:async';
import 'dart:ui' show BoxHeightStyle;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_policy.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../render_plan/render_plan.dart';
import 'flark_command_actions.dart';
import 'flark_code_syntax_highlighting.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_input_policy.dart';
import 'flark_markdown_interactions.dart';
import 'flark_text_selection_gestures.dart';

final class FlarkProjectedEditableText extends StatefulWidget {
  const FlarkProjectedEditableText({
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
  State<FlarkProjectedEditableText> createState() {
    return _FlarkProjectedEditableTextState();
  }
}

final class _FlarkProjectedEditableTextState
    extends State<FlarkProjectedEditableText> {
  @override
  Widget build(BuildContext context) {
    return _FlarkProjectedEditableHost(
      controller: widget.controller,
      focusNode: widget.focusNode,
      style: widget.style,
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      expands: widget.expands,
      autofocus: widget.autofocus,
      shortcuts: widget.shortcuts,
    );
  }
}

final class FlarkLiveRenderedEditableText extends StatefulWidget {
  const FlarkLiveRenderedEditableText({
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
  State<FlarkLiveRenderedEditableText> createState() {
    return _FlarkLiveRenderedEditableTextState();
  }
}

final class _FlarkLiveRenderedEditableTextState
    extends State<FlarkLiveRenderedEditableText> {
  FocusNode? _ownedFocusNode;

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
  }

  @override
  void didUpdateWidget(FlarkLiveRenderedEditableText oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.focusNode == null && widget.focusNode != null) {
      _ownedFocusNode?.dispose();
      _ownedFocusNode = null;
    } else if (oldWidget.focusNode != null && widget.focusNode == null) {
      _ownedFocusNode = FocusNode();
    }
  }

  @override
  void dispose() {
    _ownedFocusNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return _FlarkLiveRenderedBlockEditor(
      controller: widget.controller,
      focusNode: _focusNode,
      style: widget.style,
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      expands: widget.expands,
      autofocus: widget.autofocus,
      shortcuts: widget.shortcuts,
    );
  }
}

final class _FlarkProjectedEditableHost extends StatefulWidget {
  const _FlarkProjectedEditableHost({
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
    this.liveRendered = false,
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
  final bool liveRendered;

  @override
  State<_FlarkProjectedEditableHost> createState() {
    return _FlarkProjectedEditableHostState();
  }
}

final class _FlarkProjectedEditableHostState
    extends State<_FlarkProjectedEditableHost> {
  late final TextEditingController _textController;
  late final ScrollController _scrollController;
  final _editableStateKey = GlobalKey<EditableTextState>();
  FocusNode? _ownedFocusNode;
  bool _syncingFromRuntime = false;
  bool _keyboardSyncScheduled = false;
  final _compositionUndoGrouping = _FlarkCompositionUndoGrouping();
  String? _cachedLiveDisplayText;
  FlarkRenderPlan? _cachedLiveRenderPlan;
  bool? _cachedLiveHasRenderPlan;
  _FlarkLiveRenderedTextState? _cachedLiveRenderState;

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    _textController = widget.liveRendered
        ? _FlarkLiveRenderedTextController()
        : TextEditingController();
    _scrollController = ScrollController();
    _textController.addListener(_handleTextEditingValueChanged);
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
    widget.controller.addListener(_syncFromRuntime);
    _syncFromRuntime(rebuildLiveRender: false);
  }

  @override
  void didUpdateWidget(_FlarkProjectedEditableHost oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_syncFromRuntime);
      widget.controller.addListener(_syncFromRuntime);
      _clearLiveRenderStateCache();
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
    _scrollController.dispose();
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style.merge(widget.style);
    final displayText = _projectedText();
    final hasRenderPlan =
        widget.controller.hasAuthoritativeRenderPlan ||
        widget.controller.renderPlan.blocks.isNotEmpty;
    if (_textController is _FlarkLiveRenderedTextController) {
      _textController.renderState = _liveRenderState(
        displayText: displayText,
        renderPlan: widget.controller.renderPlan,
        hasRenderPlan: hasRenderPlan,
      );
    }
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
      scrollController: _scrollController,
      paintCursorAboveText: true,
      rendererIgnoresPointer: true,
    );
    editor = flarkEditableTextGestureDetector(
      editableTextKey: _editableStateKey,
      child: editor,
    );
    _scheduleKeyboardConnectionForInheritedFocus();
    if (widget.liveRendered) {
      editor = _FlarkLiveRenderedEditableChrome(
        textController: _textController,
        scrollController: _scrollController,
        renderPlan: widget.controller.renderPlan,
        displayText: displayText,
        hasRenderPlan: hasRenderPlan,
        style: style,
        child: editor,
      );
    }
    editor = _markdownInputPolicy.wrapKeyboardShortcuts(
      child: editor,
      currentSelection: () =>
          FlarkMarkdownInputPolicy.selectionFromTextSelection(
            _textController.selection,
          ),
      applySelection: _applyProjectedSelection,
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

  void _scheduleKeyboardConnectionForInheritedFocus() {
    if (!_focusNode.hasFocus || _keyboardSyncScheduled) return;
    _keyboardSyncScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _keyboardSyncScheduled = false;
      if (!mounted || !_focusNode.hasFocus) return;
      _editableStateKey.currentState?.requestKeyboard();
    });
  }

  _FlarkLiveRenderedTextState _liveRenderState({
    required String displayText,
    required FlarkRenderPlan renderPlan,
    required bool hasRenderPlan,
  }) {
    final cached = _cachedLiveRenderState;
    if (cached != null &&
        _cachedLiveDisplayText == displayText &&
        identical(_cachedLiveRenderPlan, renderPlan) &&
        _cachedLiveHasRenderPlan == hasRenderPlan) {
      return cached;
    }

    final next = _FlarkLiveRenderedTextState.fromRenderPlan(
      displayText: displayText,
      renderPlan: renderPlan,
      hasRenderPlan: hasRenderPlan,
    );
    _cachedLiveDisplayText = displayText;
    _cachedLiveRenderPlan = renderPlan;
    _cachedLiveHasRenderPlan = hasRenderPlan;
    _cachedLiveRenderState = next;
    return next;
  }

  void _clearLiveRenderStateCache() {
    _cachedLiveDisplayText = null;
    _cachedLiveRenderPlan = null;
    _cachedLiveHasRenderPlan = null;
    _cachedLiveRenderState = null;
  }

  void _handleTextEditingValueChanged() {
    if (_syncingFromRuntime) return;

    final oldDisplayText = _projectedText();
    final oldDisplaySelection = widget.controller.projection
        .sourceSelectionToDisplay(widget.controller.selection);
    final value = _textValueWithPureInsertionSelection(
      oldText: oldDisplayText,
      oldSelection: _textSelection(oldDisplaySelection),
      newValue: _textController.value,
    );
    if (_textController.value != value) {
      _syncingFromRuntime = true;
      _textController.value = value;
      _syncingFromRuntime = false;
    }
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    if (value.text != oldDisplayText) {
      final completedCodeFenceText = widget.liveRendered
          ? _displayTextAfterCompletingStandaloneCodeFenceOpener(
              oldDisplayText: oldDisplayText,
              newValue: value,
            )
          : null;
      final newDisplayText = completedCodeFenceText ?? value.text;
      if (_markdownInputPolicy.handlePlatformTextChange(
        oldText: oldDisplayText,
        newValue: value.copyWith(text: newDisplayText),
        oldTextSelection: oldDisplaySelection,
        applyOldTextSelection: _applyProjectedSelection,
      )) {
        _compositionUndoGrouping.clearIfCommitted(value);
        return;
      }
      final needsImmediateParse =
          widget.liveRendered &&
          (completedCodeFenceText != null ||
              _hasImmediatelyRenderableBlockLine(newDisplayText));
      final applied = widget.controller.applyProjectedTextEdit(
        oldDisplayText: oldDisplayText,
        newDisplayText: newDisplayText,
        undoGroupId: compositionUndoGroupId,
      );
      if (!applied) {
        _syncFromRuntime();
      } else if (needsImmediateParse) {
        _adoptImmediateMarkdownParse();
      }
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }

    final selection = _selectionFromTextSelection(value.selection);
    if (selection == null) {
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    widget.controller.applyProjectedSelection(selection);
    _compositionUndoGrouping.clearIfCommitted(value);
  }

  void _syncFromRuntime({bool rebuildLiveRender = true}) {
    final projectedText = _projectedText();
    final currentValue = _textController.value;
    final nextValue = TextEditingValue(
      text: projectedText,
      selection: _textSelection(
        widget.controller.projection.sourceSelectionToDisplay(
          widget.controller.selection,
        ),
      ),
      composing: currentValue.text == projectedText
          ? currentValue.composing
          : TextRange.empty,
    );
    if (_textController.value == nextValue) {
      if (rebuildLiveRender && widget.liveRendered && mounted) setState(() {});
      return;
    }
    _syncingFromRuntime = true;
    _textController.value = nextValue;
    _syncingFromRuntime = false;
    if (rebuildLiveRender && widget.liveRendered && mounted) setState(() {});
  }

  String _projectedText() {
    return widget.controller.projection.projectText(widget.controller.markdown);
  }

  void _adoptImmediateMarkdownParse() {
    _adoptImmediateMarkdownParseForController(widget.controller);
  }

  void _applyProjectedSelection(FlarkSelection displaySelection) {
    widget.controller.applyProjectedSelection(
      displaySelection,
      affinity: FlarkMapAffinity.downstream,
    );
  }

  FlarkMarkdownInputPolicy get _markdownInputPolicy {
    return FlarkMarkdownInputPolicy(
      controller: widget.controller,
      enterUserEvent: 'input.projected.enter',
      backspaceUserEvent: 'input.projected.backspace',
      onHandled: widget.liveRendered ? _adoptImmediateMarkdownParse : null,
    );
  }

  TextSelection _textSelection(FlarkSelection selection) {
    return TextSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }

  FlarkSelection? _selectionFromTextSelection(TextSelection selection) {
    return FlarkMarkdownInputPolicy.selectionFromTextSelection(selection);
  }
}

/// Requests an immediate authoritative parse of the controller's current
/// revision, bypassing the debounce window.
///
/// Live-rendered block editing needs an authoritative render plan as soon as a
/// structural edit lands. The controller owns the single parser, so this routes
/// through [FlarkFlutterController.parseNow] rather than spinning up a second
/// backend call. Parse errors are routed to the controller's configured
/// `onParseError`.
void _adoptImmediateMarkdownParseForController(
  FlarkFlutterController controller,
) {
  unawaited(controller.parseNow());
}

final class _FlarkLiveRenderedBlockEditor extends StatefulWidget {
  const _FlarkLiveRenderedBlockEditor({
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
  State<_FlarkLiveRenderedBlockEditor> createState() {
    return _FlarkLiveRenderedBlockEditorState();
  }
}

final class _FlarkLiveRenderedBlockEditorState
    extends State<_FlarkLiveRenderedBlockEditor> {
  final _focusCoordinator = _LiveRenderedBlockFocusCoordinator();
  final _contentBoundsKey = GlobalKey();
  int? _appendHostOffset;

  @override
  void dispose() {
    _focusCoordinator.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.controller,
      builder: (context, _) {
        final baseStyle = DefaultTextStyle.of(
          context,
        ).style.merge(widget.style);
        final displayText = _projectedText(widget.controller);
        final hasRenderPlan =
            widget.controller.hasAuthoritativeRenderPlan ||
            widget.controller.renderPlan.blocks.isNotEmpty;
        final blocks = hasRenderPlan
            ? _editableBlocks(
                    widget.controller.renderPlan.blocks,
                    controller: widget.controller,
                  )
                  .where((block) => _isVisibleEditableBlock(block, displayText))
                  .toList(growable: false)
            : const <FlarkRenderBlock>[];

        if (blocks.isEmpty || !_requiresBlockWidgetEditing(blocks)) {
          return _FlarkProjectedEditableHost(
            controller: widget.controller,
            focusNode: widget.focusNode,
            style: widget.style,
            cursorColor: widget.cursorColor,
            backgroundCursorColor: widget.backgroundCursorColor,
            minLines: widget.minLines,
            maxLines: widget.maxLines,
            expands: widget.expands,
            autofocus: widget.autofocus,
            shortcuts: widget.shortcuts,
            liveRendered: true,
          );
        }

        final appendHostOffset = _activeAppendHostOffset();
        final editableBlocks = _blocksWithSourceGapHosts(
          blocks: blocks,
          controller: widget.controller,
          appendHostOffset: appendHostOffset,
        );
        final blockEntries = [
          for (final block in editableBlocks)
            _LiveRenderedBlockEntry(
              id: _liveRenderedBlockId(block),
              block: block,
            ),
        ];
        _focusCoordinator.reconcile(blockEntries);
        _focusCoordinator.scheduleSelectionSync(
          entries: blockEntries,
          controller: widget.controller,
          autofocus: widget.autofocus || appendHostOffset != null,
          restoreSelectionFocus: _selectionTargetsTerminalAppendHost(
            editableBlocks,
            widget.controller,
          ),
          externalFirstFocusNode: widget.focusNode,
          isMounted: () => mounted,
        );

        final content = KeyedSubtree(
          key: _contentBoundsKey,
          child: Column(
            key: const Key('FlarkLiveBlockEditor'),
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              // Each block is a RepaintBoundary so one block's repaint (cursor,
              // selection, text edit) does not repaint its siblings. The
              // per-block ValueKey lives here to preserve each block's editable
              // state across rebuilds and reorders.
              for (var index = 0; index < blockEntries.length; index++)
                RepaintBoundary(
                  key: ValueKey(blockEntries[index].id),
                  child: _FlarkLiveRenderedBlock(
                    controller: widget.controller,
                    block: blockEntries[index].block,
                    displayText: displayText,
                    style: baseStyle,
                    cursorColor: widget.cursorColor,
                    backgroundCursorColor: widget.backgroundCursorColor,
                    autofocus: widget.autofocus && index == 0,
                    focusNode: _focusCoordinator.focusNodeForBlock(
                      entry: blockEntries[index],
                      index: index,
                      externalFirstFocusNode: widget.focusNode,
                    ),
                    onMoveToPreviousBlock: index == 0
                        ? null
                        : () => _moveSelectionToBlockBoundary(
                            blockEntries[index - 1].block,
                            after: true,
                          ),
                    onMoveToNextBlock: index + 1 >= blockEntries.length
                        ? blockEntries[index].block.codeBlock == null
                              ? null
                              : () => _moveSelectionToDocumentBoundary(
                                  blockEntries[index].block,
                                  after: true,
                                )
                        : () => _moveSelectionToBlockBoundary(
                            blockEntries[index + 1].block,
                            after: false,
                          ),
                  ),
                ),
            ],
          ),
        );
        Widget editor = LayoutBuilder(
          builder: (context, constraints) {
            final scrollable = SingleChildScrollView(child: content);
            if (widget.expands) {
              return SizedBox.expand(child: scrollable);
            }
            if (constraints.hasBoundedHeight) {
              return scrollable;
            }
            return content;
          },
        );
        editor = GestureDetector(
          behavior: HitTestBehavior.translucent,
          onTapUp: _handleEditorTapUp,
          child: editor,
        );
        editor = FlarkCommandActions(
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
      },
    );
  }

  static bool _requiresBlockWidgetEditing(Iterable<FlarkRenderBlock> blocks) {
    return blocks.any((block) {
      return block.table != null ||
          block.codeBlock != null ||
          block.listItem != null ||
          block.taskListItem != null ||
          block.kind == FlarkMarkdownBlockKind.blockquote;
    });
  }

  static String _projectedText(FlarkFlutterController controller) {
    try {
      return controller.projection.projectText(controller.markdown);
    } on ArgumentError {
      return controller.markdown;
    }
  }

  void _moveSelectionToBlockBoundary(
    FlarkRenderBlock block, {
    required bool after,
  }) {
    widget.controller.applySelection(
      FlarkSelection.collapsed(
        _sourceNavigationBoundaryOffset(
          markdown: widget.controller.markdown,
          block: block,
          after: after,
        ),
      ),
      userEvent: after
          ? 'selection.liveBlock.verticalBoundary.previous'
          : 'selection.liveBlock.verticalBoundary.next',
    );
  }

  void _moveSelectionToDocumentBoundary(
    FlarkRenderBlock block, {
    required bool after,
  }) {
    final offset = (after ? block.sourceRange.end : block.sourceRange.start)
        .clamp(0, widget.controller.markdown.length);
    _appendHostOffset = after ? offset : null;
    widget.controller.applySelection(
      FlarkSelection.collapsed(offset),
      userEvent: after
          ? 'selection.liveBlock.verticalBoundary.documentEnd'
          : 'selection.liveBlock.verticalBoundary.documentStart',
    );
  }

  int? _activeAppendHostOffset() {
    final offset = _appendHostOffset;
    if (offset == null) return null;
    final selection = widget.controller.selection;
    final extentOffset = selection.extentOffset;
    if (!selection.isCollapsed ||
        extentOffset < offset ||
        extentOffset > widget.controller.markdown.length ||
        offset < 0 ||
        offset > widget.controller.markdown.length ||
        !_isAppendHostContinuation(
          widget.controller.markdown.substring(offset, extentOffset),
        )) {
      _appendHostOffset = null;
      return null;
    }
    return offset;
  }

  bool _isAppendHostContinuation(String text) {
    if (text.isEmpty) return true;
    if (!_isLineBreakCodeUnit(text.codeUnitAt(0))) {
      return !text.contains('\n');
    }
    final contentStart = _leadingLineBreakCount(text);
    if (contentStart > 2) return false;
    return !text.substring(contentStart).contains('\n');
  }

  void _handleEditorTapUp(TapUpDetails details) {
    if (!_tapIsBelowRenderedContent(details.globalPosition)) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final offset = widget.controller.markdown.length;
      _appendHostOffset = offset;
      widget.controller.applySelection(
        FlarkSelection.collapsed(offset),
        userEvent: 'selection.liveBlock.appendHost',
      );
      if (mounted) setState(() {});
    });
  }

  bool _tapIsBelowRenderedContent(Offset globalPosition) {
    final renderObject = _contentBoundsKey.currentContext?.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.attached) return false;
    final localPosition = renderObject.globalToLocal(globalPosition);
    final lastChildBottom = _lastChildBottomIn(renderObject);
    if (lastChildBottom != null && localPosition.dy > lastChildBottom) {
      return true;
    }
    return localPosition.dy > renderObject.size.height;
  }

  double? _lastChildBottomIn(RenderBox renderObject) {
    RenderBox? lastChild;
    renderObject.visitChildren((child) {
      if (child is RenderBox && child.attached) lastChild = child;
    });
    final child = lastChild;
    if (child == null) return null;
    final childBottomGlobal = child.localToGlobal(Offset(0, child.size.height));
    return renderObject.globalToLocal(childBottomGlobal).dy;
  }

  static List<FlarkRenderBlock> _editableBlocks(
    Iterable<FlarkRenderBlock> blocks, {
    required FlarkFlutterController controller,
  }) {
    return [
      for (final block in blocks)
        for (final editable in _editableBlockAndDescendants(block))
          _normalizePredictedEditableBlock(editable, controller),
    ];
  }

  static bool _selectionTargetsTerminalAppendHost(
    Iterable<FlarkRenderBlock> blocks,
    FlarkFlutterController controller,
  ) {
    final selection = controller.selection;
    if (!selection.isCollapsed) return false;
    return blocks.any((block) {
      final stableId = block.attributes['stableId'];
      return stableId is String &&
          stableId.startsWith('terminalAppendHost:') &&
          _blockOwnsSourceSelection(
            markdown: controller.markdown,
            block: block,
            selection: selection,
          );
    });
  }

  static FlarkRenderBlock _normalizePredictedEditableBlock(
    FlarkRenderBlock block,
    FlarkFlutterController controller,
  ) {
    if (block.codeBlock == null) return block;
    final markdown = controller.markdown;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null || !context.isClosed) return block;
    final closedEnd = context.closingLineEndWithBreak ?? context.closingLineEnd;
    if (closedEnd == null || closedEnd >= block.sourceRange.end) return block;
    final sourceRange = FlarkSourceRange(
      block.sourceRange.start,
      closedEnd,
    ).validate(markdown.length);
    return FlarkRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: sourceRange,
      displayRange: FlarkSourceRange(
        controller.projection.sourceToDisplayOffset(sourceRange.start),
        controller.projection.sourceToDisplayOffset(sourceRange.end),
      ),
      styleToken: block.styleToken,
      inlineRuns: block.inlineRuns,
      children: block.children,
      table: block.table,
      listItem: block.listItem,
      taskListItem: block.taskListItem,
      codeBlock: block.codeBlock,
      attributes: block.attributes,
    );
  }

  static Iterable<FlarkRenderBlock> _editableBlockAndDescendants(
    FlarkRenderBlock block,
  ) sync* {
    if (block.kind == FlarkMarkdownBlockKind.document ||
        block.kind == FlarkMarkdownBlockKind.list) {
      for (final child in block.children) {
        yield* _editableBlockAndDescendants(child);
      }
      return;
    }
    yield block;
  }

  static List<FlarkRenderBlock> _blocksWithSourceGapHosts({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) {
    final next = [
      ...blocks,
      ..._parserOmittedLineHosts(
        blocks: blocks,
        controller: controller,
        appendHostOffset: appendHostOffset,
      ),
    ];
    final appendHost = _terminalAppendHost(
      blocks: blocks,
      controller: controller,
      appendHostOffset: appendHostOffset,
    );
    if (appendHost != null) {
      next.add(appendHost);
      return _sortedBlocks(next);
    }
    final selection = controller.selection;
    if (!selection.isCollapsed) return _sortedBlocks(next);
    final useTerminalCodeFenceHost = _selectionIsAfterTerminalCodeFence(
      markdown: controller.markdown,
      blocks: blocks,
      selection: selection,
    );
    if (!useTerminalCodeFenceHost &&
        next.any(
          (block) => _blockOwnsSourceSelection(
            markdown: controller.markdown,
            block: block,
            selection: selection,
          ),
        )) {
      return _sortedBlocks(next);
    }

    final displaySelection = controller.projection.sourceSelectionToDisplay(
      selection,
    );
    final sourceOffset = selection.extentOffset.clamp(
      0,
      controller.markdown.length,
    );
    final displayOffset = displaySelection.extentOffset.clamp(
      0,
      controller.projection.textLength,
    );
    final selectionHost = FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSelectionHost',
      sourceRange: FlarkSourceRange(sourceOffset, sourceOffset),
      displayRange: FlarkSourceRange(displayOffset, displayOffset),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: const {'synthetic': true, 'reason': 'selectionHost'},
    );
    next.add(selectionHost);
    return _sortedBlocks(next);
  }

  static FlarkRenderBlock? _terminalAppendHost({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) {
    if (appendHostOffset == null) return null;
    final markdown = controller.markdown;
    if (appendHostOffset != markdown.length) return null;
    final terminal = _terminalEditableBlock(blocks, appendHostOffset);
    if (terminal == null) return null;
    final displayOffset = controller.projection.sourceToDisplayOffset(
      appendHostOffset,
    );
    return FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSelectionHost',
      sourceRange: FlarkSourceRange(appendHostOffset, appendHostOffset),
      displayRange: FlarkSourceRange(displayOffset, displayOffset),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: {
        'synthetic': true,
        'reason': 'terminalAppendHost',
        'stableId': 'terminalAppendHost:$appendHostOffset',
        'sourcePrefix': _terminalAppendPrefix(markdown, terminal),
      },
    );
  }

  static FlarkRenderBlock? _terminalEditableBlock(
    List<FlarkRenderBlock> blocks,
    int sourceOffset,
  ) {
    FlarkRenderBlock? best;
    for (final block in blocks) {
      if (block.sourceRange.end != sourceOffset) continue;
      if (best == null || block.sourceRange.start > best.sourceRange.start) {
        best = block;
      }
    }
    return best;
  }

  static String _terminalAppendPrefix(String markdown, FlarkRenderBlock block) {
    final requiredBreaks =
        block.listItem != null ||
            block.taskListItem != null ||
            block.kind == FlarkMarkdownBlockKind.blockquote
        ? 2
        : 1;
    final missingBreaks = requiredBreaks - _trailingLineBreakCount(markdown);
    if (missingBreaks <= 0) return '';
    return '\n' * missingBreaks;
  }

  static int _trailingLineBreakCount(String markdown) {
    var count = 0;
    for (var index = markdown.length - 1; index >= 0; index--) {
      if (!_isLineBreakCodeUnit(markdown.codeUnitAt(index))) break;
      count++;
    }
    return count;
  }

  static int _leadingLineBreakCount(String markdown) {
    var count = 0;
    for (var index = 0; index < markdown.length; index++) {
      if (!_isLineBreakCodeUnit(markdown.codeUnitAt(index))) break;
      count++;
    }
    return count;
  }

  static bool _selectionIsAfterTerminalCodeFence({
    required String markdown,
    required List<FlarkRenderBlock> blocks,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) return false;
    final offset = selection.extentOffset;
    if (offset < 0 || offset > markdown.length) return false;
    if (!FlarkMarkdownFencedCodeScanner.isWhitespace(
      markdown.substring(offset),
    )) {
      return false;
    }

    for (final block in blocks) {
      if (block.codeBlock == null || block.sourceRange.end != offset) {
        continue;
      }
      final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
        markdown,
        block.sourceRange.start,
      );
      if (context?.isClosed ?? false) return true;
    }
    return false;
  }

  static Iterable<FlarkRenderBlock> _parserOmittedLineHosts({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) sync* {
    final markdown = controller.markdown;
    if (markdown.isEmpty) return;
    final selection = controller.selection;
    var lineStart = 0;
    while (lineStart <= markdown.length) {
      final newline = markdown.indexOf('\n', lineStart);
      final lineEnd = newline < 0 ? markdown.length : newline;
      final lineText = markdown.substring(lineStart, lineEnd);
      final lineRange = FlarkSourceRange(lineStart, lineEnd);
      final selectedLine =
          selection.isCollapsed &&
          selection.extentOffset >= lineRange.start &&
          selection.extentOffset <= lineRange.end;
      final shouldHost = lineText.trim().isEmpty || selectedLine;
      if (shouldHost &&
          !_sourceRangeCoveredByBlocks(lineRange, blocks) &&
          !_sourceOffsetHasCollapsedBlock(lineStart, blocks)) {
        yield _syntheticSourceLineHost(
          controller: controller,
          sourceRange: lineRange,
          stableId: _appendContinuationStableId(
            appendHostOffset: appendHostOffset,
            blocks: blocks,
            markdown: markdown,
            lineRange: lineRange,
            selection: selection,
          ),
        );
      }
      if (newline < 0) break;
      lineStart = newline + 1;
    }
  }

  static bool _sourceRangeCoveredByBlocks(
    FlarkSourceRange range,
    Iterable<FlarkRenderBlock> blocks,
  ) {
    for (final block in blocks) {
      final blockRange = block.sourceRange;
      if (range.isCollapsed) {
        if (blockRange.start <= range.start && range.start < blockRange.end) {
          return true;
        }
        if (block.codeBlock != null && blockRange.end == range.start) {
          return true;
        }
      } else if (blockRange.start < range.end && range.start < blockRange.end) {
        return true;
      }
    }
    return false;
  }

  static bool _sourceOffsetHasCollapsedBlock(
    int sourceOffset,
    Iterable<FlarkRenderBlock> blocks,
  ) {
    return blocks.any(
      (block) =>
          block.sourceRange.isCollapsed &&
          block.sourceRange.start == sourceOffset,
    );
  }

  static FlarkRenderBlock _syntheticSourceLineHost({
    required FlarkFlutterController controller,
    required FlarkSourceRange sourceRange,
    String? stableId,
  }) {
    final displayStart = controller.projection.sourceToDisplayOffset(
      sourceRange.start,
    );
    final displayEnd = controller.projection.sourceToDisplayOffset(
      sourceRange.end,
    );
    return FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSourceLineHost',
      sourceRange: sourceRange,
      displayRange: FlarkSourceRange(displayStart, displayEnd),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: stableId == null
          ? const {'synthetic': true, 'reason': 'parserOmittedLine'}
          : {
              'synthetic': true,
              'reason': 'parserOmittedLine',
              'stableId': stableId,
            },
    );
  }

  static String? _appendContinuationStableId({
    required int? appendHostOffset,
    required List<FlarkRenderBlock> blocks,
    required String markdown,
    required FlarkSourceRange lineRange,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) return null;
    final stableOffset =
        appendHostOffset ??
        _inferredAppendContinuationOffset(
          blocks: blocks,
          markdown: markdown,
          lineRange: lineRange,
        );
    if (stableOffset == null) return null;
    if (lineRange.start <= stableOffset) return null;
    if (selection.extentOffset < lineRange.start ||
        selection.extentOffset > lineRange.end) {
      return null;
    }
    return 'terminalAppendHost:$stableOffset';
  }

  static int? _inferredAppendContinuationOffset({
    required List<FlarkRenderBlock> blocks,
    required String markdown,
    required FlarkSourceRange lineRange,
  }) {
    if (lineRange.start <= 0 || lineRange.start > markdown.length) {
      return null;
    }
    int? best;
    for (final block in blocks) {
      if (!_supportsTerminalAppendContinuation(block)) continue;
      final appendOffset = _appendContinuationBaseOffset(
        markdown: markdown,
        block: block,
        lineStart: lineRange.start,
      );
      if (appendOffset == null) continue;
      if (!FlarkMarkdownFencedCodeScanner.isWhitespace(
        markdown.substring(appendOffset, lineRange.start),
      )) {
        continue;
      }
      if (best == null || appendOffset > best) best = appendOffset;
    }
    return best;
  }

  static int? _appendContinuationBaseOffset({
    required String markdown,
    required FlarkRenderBlock block,
    required int lineStart,
  }) {
    final end = block.sourceRange.end;
    if (end < 0 || end > lineStart || end > markdown.length) return null;
    if (end > 0 && _isLineBreakCodeUnit(markdown.codeUnitAt(end - 1))) {
      return end - 1;
    }
    if (end == lineStart) return null;
    return end;
  }

  static bool _supportsTerminalAppendContinuation(FlarkRenderBlock block) {
    return block.codeBlock != null ||
        block.listItem != null ||
        block.taskListItem != null ||
        block.kind == FlarkMarkdownBlockKind.blockquote;
  }

  static List<FlarkRenderBlock> _sortedBlocks(List<FlarkRenderBlock> blocks) {
    blocks.sort((a, b) {
      final bySource = a.sourceRange.start.compareTo(b.sourceRange.start);
      if (bySource != 0) return bySource;
      final byLength = b.sourceRange.length.compareTo(a.sourceRange.length);
      if (byLength != 0) return byLength;
      return a.type.compareTo(b.type);
    });
    return blocks;
  }
}

final class _LiveRenderedBlockEntry {
  const _LiveRenderedBlockEntry({required this.id, required this.block});

  final String id;
  final FlarkRenderBlock block;
}

final class _LiveRenderedBlockFocusCoordinator {
  final Map<String, FocusNode> _ownedBlockFocusNodes = {};
  int _focusSyncGeneration = 0;
  bool _hasObservedEditorFocus = false;
  bool _needsFocusRestoreAfterReconcile = false;

  void dispose() {
    for (final node in _ownedBlockFocusNodes.values) {
      node.dispose();
    }
    _ownedBlockFocusNodes.clear();
  }

  void reconcile(List<_LiveRenderedBlockEntry> entries) {
    final currentIds = entries.map((entry) => entry.id).toSet();
    final staleIds = [
      for (final id in _ownedBlockFocusNodes.keys)
        if (!currentIds.contains(id)) id,
    ];
    for (final id in staleIds) {
      final node = _ownedBlockFocusNodes.remove(id);
      if (node == null) continue;
      if (node.hasFocus) _needsFocusRestoreAfterReconcile = true;
      node.dispose();
    }
  }

  FocusNode focusNodeForBlock({
    required _LiveRenderedBlockEntry entry,
    required int index,
    required FocusNode? externalFirstFocusNode,
  }) {
    if (index == 0 && externalFirstFocusNode != null) {
      return externalFirstFocusNode;
    }
    return _ownedBlockFocusNodes.putIfAbsent(
      entry.id,
      () => FocusNode(debugLabel: entry.id),
    );
  }

  void scheduleSelectionSync({
    required List<_LiveRenderedBlockEntry> entries,
    required FlarkFlutterController controller,
    required bool autofocus,
    required bool restoreSelectionFocus,
    required FocusNode? externalFirstFocusNode,
    required bool Function() isMounted,
  }) {
    final hasFocus = _editorHasFocus(externalFirstFocusNode);
    if (hasFocus) _hasObservedEditorFocus = true;
    final canRestoreFocus =
        autofocus ||
        _needsFocusRestoreAfterReconcile ||
        (restoreSelectionFocus &&
            (_hasObservedEditorFocus || controller.state.revision > 0));
    if (!hasFocus && !canRestoreFocus) return;
    final target = _blockEntryForSelection(entries, controller);
    if (target == null) {
      _needsFocusRestoreAfterReconcile = false;
      return;
    }

    final generation = ++_focusSyncGeneration;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (generation != _focusSyncGeneration) return;
      if (!isMounted()) return;
      final currentHasFocus = _editorHasFocus(externalFirstFocusNode);
      if (currentHasFocus) _hasObservedEditorFocus = true;
      final canStillRestoreFocus =
          autofocus ||
          _needsFocusRestoreAfterReconcile ||
          (restoreSelectionFocus &&
              (_hasObservedEditorFocus || controller.state.revision > 0));
      if (!currentHasFocus && !canStillRestoreFocus) return;
      final currentTarget = _blockEntryForSelection(entries, controller);
      if (currentTarget == null) {
        _needsFocusRestoreAfterReconcile = false;
        return;
      }
      final targetIndex = entries.indexOf(currentTarget);
      final node = focusNodeForBlock(
        entry: currentTarget,
        index: targetIndex,
        externalFirstFocusNode: externalFirstFocusNode,
      );
      if (!node.hasFocus) node.requestFocus();
      _hasObservedEditorFocus = true;
      _needsFocusRestoreAfterReconcile = false;
    });
  }

  bool _editorHasFocus(FocusNode? externalFirstFocusNode) {
    if (externalFirstFocusNode?.hasFocus ?? false) return true;
    return _ownedBlockFocusNodes.values.any((node) => node.hasFocus);
  }

  _LiveRenderedBlockEntry? _blockEntryForSelection(
    List<_LiveRenderedBlockEntry> entries,
    FlarkFlutterController controller,
  ) {
    _LiveRenderedBlockEntry? best;
    for (final entry in entries) {
      if (!_blockOwnsSelection(entry.block, controller)) continue;
      if (best == null ||
          entry.block.sourceRange.start > best.block.sourceRange.start) {
        best = entry;
      }
    }
    return best;
  }

  bool _blockOwnsSelection(
    FlarkRenderBlock block,
    FlarkFlutterController controller,
  ) {
    return _blockOwnsSourceSelection(
      markdown: controller.markdown,
      block: block,
      selection: controller.selection,
    );
  }
}

bool _blockOwnsSourceSelection({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSelection selection,
}) {
  final focusRange = _sourceFocusRangeForBlock(markdown, block);
  return selection.start >= focusRange.start && selection.end <= focusRange.end;
}

FlarkSourceRange _sourceFocusRangeForBlock(
  String markdown,
  FlarkRenderBlock block,
) {
  final start = block.sourceRange.start.clamp(0, markdown.length);
  var end = block.sourceRange.end.clamp(start, markdown.length);
  final lineEnd = markdown.indexOf('\n', start);
  if (lineEnd < 0) {
    end = markdown.length;
  } else if (lineEnd >= start && lineEnd > end) {
    end = lineEnd;
  }
  return FlarkSourceRange(start, end);
}

int _sourceNavigationBoundaryOffset({
  required String markdown,
  required FlarkRenderBlock block,
  required bool after,
}) {
  if (block.codeBlock != null) {
    final bodyRange = _codeBodyRange(markdown, block);
    if (bodyRange != null) return after ? bodyRange.end : bodyRange.start;
  }
  final focusRange = _sourceFocusRangeForBlock(markdown, block);
  return after
      ? _sourceRangeEditableEnd(markdown, focusRange)
      : focusRange.start;
}

int _sourceRangeEditableEnd(String markdown, FlarkSourceRange range) {
  var end = range.end.clamp(range.start, markdown.length);
  while (end > range.start &&
      _isLineBreakCodeUnit(markdown.codeUnitAt(end - 1))) {
    end--;
  }
  return end;
}

bool _isLineBreakCodeUnit(int codeUnit) {
  return codeUnit == 0x0A || codeUnit == 0x0D;
}

Color _selectionColorForCursor(Color cursorColor) {
  return cursorColor.withValues(alpha: 0.24);
}

String _liveRenderedBlockId(FlarkRenderBlock block) {
  final stableId = block.attributes['stableId'];
  if (stableId is String) return 'live-block:$stableId';
  return 'live-block:${block.type}:${block.sourceRange.start}';
}

bool _isVisibleEditableBlock(FlarkRenderBlock block, String displayText) {
  if (_rangeOverlapsText(block.displayRange, displayText)) return true;
  return (block.kind == FlarkMarkdownBlockKind.blockquote ||
          block.kind == FlarkMarkdownBlockKind.listItem ||
          block.kind == FlarkMarkdownBlockKind.codeBlock) &&
      block.displayRange.isCollapsed;
}

bool _hasImmediatelyRenderableBlockLine(String text) {
  var lineStart = 0;
  while (lineStart <= text.length) {
    final lineEndWithBreak = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
      text,
      lineStart,
    );
    final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      text,
      lineStart,
    );
    final line = text.substring(lineStart, lineEnd);
    if (_isImmediatelyRenderableQuoteLine(line) ||
        _isImmediatelyRenderableListLine(line) ||
        _isImmediatelyRenderableCodeFenceLine(
          line,
          hasLineBreak: lineEndWithBreak > lineEnd,
        )) {
      return true;
    }
    if (lineEndWithBreak <= lineStart || lineEndWithBreak >= text.length) {
      break;
    }
    lineStart = lineEndWithBreak;
  }
  return false;
}

bool _isImmediatelyRenderableQuoteLine(String line) {
  var index = _skipHorizontalWhitespace(line, 0);
  if (index >= line.length || line.codeUnitAt(index) != 0x3E) return false;
  index++;
  return index < line.length && _isHorizontalWhitespace(line.codeUnitAt(index));
}

bool _isImmediatelyRenderableListLine(String line) {
  final index = _skipHorizontalWhitespace(line, 0);
  if (index >= line.length) return false;

  final marker = line.codeUnitAt(index);
  if (marker == 0x2D || marker == 0x2A || marker == 0x2B) {
    final afterMarker = index + 1;
    return afterMarker < line.length &&
        _isHorizontalWhitespace(line.codeUnitAt(afterMarker));
  }

  return _orderedListMarkerLabel(line, requireFollowingWhitespace: true) !=
      null;
}

bool _isImmediatelyRenderableCodeFenceLine(
  String line, {
  required bool hasLineBreak,
}) {
  if (!hasLineBreak) return false;
  return FlarkMarkdownFencedCodeScanner.fenceLine(line) != null;
}

String? _displayTextAfterCompletingStandaloneCodeFenceOpener({
  required String oldDisplayText,
  required TextEditingValue newValue,
}) {
  final selection = newValue.selection;
  if (!selection.isValid || !selection.isCollapsed) return null;
  final text = newValue.text;
  final caret = selection.extentOffset;
  if (caret < 0 || caret > text.length) return null;

  final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    text,
    caret,
  );
  final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    text,
    lineStart,
  );
  if (caret != lineEnd) return null;
  if (lineEnd < text.length && text.codeUnitAt(lineEnd) == 0x0A) return null;

  final line = text.substring(lineStart, lineEnd);
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
  if (fence == null || fence.infoString != null) return null;
  if (_lineClosesExistingCodeFence(
    text: text,
    lineStart: lineStart,
    closingFence: fence,
  )) {
    return null;
  }

  final oldLineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    oldDisplayText,
    caret.clamp(0, oldDisplayText.length),
  );
  final oldLineEnd = oldLineStart <= oldDisplayText.length
      ? FlarkMarkdownFencedCodeScanner.lineContentEnd(
          oldDisplayText,
          oldLineStart,
        )
      : oldDisplayText.length;
  if (oldLineStart <= oldLineEnd && oldLineEnd <= oldDisplayText.length) {
    final oldLine = oldDisplayText.substring(oldLineStart, oldLineEnd);
    if (_justCompletedFenceMarkerBySingleCharacter(
      oldLine: oldLine,
      line: line,
      fence: fence,
    )) {
      return null;
    }
    final oldFence = FlarkMarkdownFencedCodeScanner.fenceLine(oldLine);
    if (oldFence != null && oldFence.infoString == null) return null;
  }

  return text.replaceRange(lineEnd, lineEnd, '\n');
}

bool _justCompletedFenceMarkerBySingleCharacter({
  required String oldLine,
  required String line,
  required FlarkMarkdownFenceLine fence,
}) {
  if (fence.infoString != null) return false;
  if (line.length != oldLine.length + 1) return false;
  if (!line.startsWith(oldLine)) return false;
  final markerText =
      fence.indent + List.filled(fence.markerLength, fence.marker).join();
  return line == markerText &&
      oldLine == markerText.substring(0, line.length - 1);
}

bool _lineClosesExistingCodeFence({
  required String text,
  required int lineStart,
  required FlarkMarkdownFenceLine closingFence,
}) {
  FlarkMarkdownFenceLine? openFence;
  var scanLineStart = 0;
  while (scanLineStart < lineStart && scanLineStart < text.length) {
    final scanLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      text,
      scanLineStart,
    );
    final line = text.substring(scanLineStart, scanLineEnd);
    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
    if (fence != null) {
      if (openFence == null) {
        openFence = fence;
      } else if (_fenceLineCloses(openFence, fence)) {
        openFence = null;
      }
    }

    final next = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
      text,
      scanLineStart,
    );
    if (next <= scanLineStart) break;
    scanLineStart = next;
  }

  return openFence != null && _fenceLineCloses(openFence, closingFence);
}

bool _fenceLineCloses(
  FlarkMarkdownFenceLine openFence,
  FlarkMarkdownFenceLine candidate,
) {
  return candidate.canClose &&
      candidate.marker == openFence.marker &&
      candidate.markerLength >= openFence.markerLength;
}

bool _isSyntheticSourceHost(FlarkRenderBlock block) {
  return block.attributes['synthetic'] == true &&
      (block.type == 'syntheticSourceLineHost' ||
          block.type == 'syntheticSelectionHost');
}

FlarkSourceRange? _syntheticSourceHostRange(
  String markdown,
  FlarkRenderBlock block,
) {
  if (!_isSyntheticSourceHost(block)) return null;
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start > block.sourceRange.end ||
      block.sourceRange.end > markdown.length) {
    return null;
  }
  return block.sourceRange;
}

_SourceEdit _syntheticSourceHostEdit({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSourceRange range,
  required TextEditingValue value,
}) {
  final sourcePrefix = block.attributes['sourcePrefix'];
  final explicitPrefix = sourcePrefix is String ? sourcePrefix : null;
  final needsLeadingLineBreak =
      range.isCollapsed &&
      explicitPrefix == null &&
      value.text.isNotEmpty &&
      range.start > 0 &&
      range.start <= markdown.length &&
      !_isLineBreakCodeUnit(markdown.codeUnitAt(range.start - 1));
  final prefix = explicitPrefix ?? (needsLeadingLineBreak ? '\n' : '');
  final prefixLength = prefix.length;
  return _SourceEdit(
    range: range,
    replacementText: '$prefix${value.text}',
    editableRangeAfter: FlarkSourceRange(
      range.start + prefixLength,
      range.start + prefixLength + value.text.length,
    ),
    selectionAfter: FlarkSelection(
      baseOffset: range.start + prefixLength + value.selection.baseOffset,
      extentOffset: range.start + prefixLength + value.selection.extentOffset,
    ),
  );
}

/// Test-only counter of live-rendered block widget builds, used to verify
/// per-block rebuild isolation. Reset to zero and read it across an edit.
@visibleForTesting
int flarkDebugLiveBlockBuildCount = 0;

final class _FlarkLiveRenderedBlock extends StatelessWidget {
  const _FlarkLiveRenderedBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  @override
  Widget build(BuildContext context) {
    assert(() {
      flarkDebugLiveBlockBuildCount += 1;
      return true;
    }());
    if (block.table != null) {
      return _EditableTableBlock(
        controller: controller,
        block: block,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
      );
    }
    if (block.codeBlock != null) {
      return _EditableCodeBlock(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.taskListItem != null) {
      return _EditableTaskListItemBlock(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.listItem != null) {
      return _EditableListItemBlock(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.kind == FlarkMarkdownBlockKind.blockquote) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: DecoratedBox(
          key: const Key('FlarkLiveBlockBlockquote'),
          decoration: const BoxDecoration(
            color: Color(0xFFF8FAFC),
            border: Border(
              left: BorderSide(color: Color(0xFF7A8CA3), width: 3),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(10, 6, 8, 6),
            child: _EditableProjectedBlockText(
              controller: controller,
              block: block,
              displayText: displayText,
              style: style.copyWith(color: const Color(0xFF42526E)),
              cursorColor: cursorColor,
              backgroundCursorColor: backgroundCursorColor,
              focusNode: focusNode,
              autofocus: autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: onMoveToPreviousBlock,
              onMoveToNextBlock: onMoveToNextBlock,
            ),
          ),
        ),
      );
    }
    final syntheticSourceHost = _isSyntheticSourceHost(block);
    return Padding(
      padding: _plainBlockPadding(block),
      child: _EditableProjectedBlockText(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        markdownInputPolicy: true,
        sourceRangeForEdits: syntheticSourceHost
            ? _syntheticSourceHostRange
            : null,
        sourceEditForReplacement: syntheticSourceHost
            ? _syntheticSourceHostEdit
            : null,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      ),
    );
  }

  EdgeInsets _plainBlockPadding(FlarkRenderBlock block) {
    if (block.kind == FlarkMarkdownBlockKind.heading) {
      return const EdgeInsets.only(bottom: 6);
    }
    return const EdgeInsets.symmetric(vertical: 2);
  }
}

final class _EditableProjectedBlockText extends StatefulWidget {
  const _EditableProjectedBlockText({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
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
  final _SourceEdit Function({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange range,
    required TextEditingValue value,
  })?
  sourceEditForReplacement;
  final String? codeSyntaxLanguage;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

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
    _textController.block = widget.block;
    _textController.displayText = widget.displayText;
    _textController.codeSyntaxLanguage = widget.codeSyntaxLanguage;
    Widget editor = _KeyboardSyncedEditableText(
      key: widget.editableKey,
      controller: _textController,
      focusNode: _focusNode,
      style: _blockTextStyle(widget.style, widget.block),
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

    final directSourceRange = _sourceEditRange();
    final snapshot = _localEditSnapshot(directSourceRange);
    final oldLocalText = snapshot.value.text;
    final oldLocalSelection = snapshot.value.selection;
    final value = _textValueWithPureInsertionSelection(
      oldText: oldLocalText,
      oldSelection: oldLocalSelection,
      newValue: _textController.value,
    );
    _adoptNormalizedTextControllerValue(value);
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    if (value.text != oldLocalText) {
      if (widget.markdownInputPolicy) {
        if (_markdownInputPolicy.handlePlatformTextChange(
          oldText: oldLocalText,
          newValue: value,
          oldTextSelection: FlarkMarkdownInputPolicy.selectionFromTextSelection(
            oldLocalSelection,
          ),
          applyOldTextSelection: _applyLocalDisplaySelectionToController,
        )) {
          _compositionUndoGrouping.clearIfCommitted(value);
          return;
        }
      }
      final sourceRange = snapshot.directSourceRange ?? directSourceRange;
      if (sourceRange != null) {
        final sourceEdit =
            widget.sourceEditForReplacement?.call(
              markdown: widget.controller.markdown,
              block: widget.block,
              range: sourceRange,
              value: value,
            ) ??
            _SourceEdit(
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
        _compositionUndoGrouping.clearIfCommitted(value);
        return;
      }
      final range = _clampedDisplayRange(widget.block, widget.displayText);
      final newDisplayText = widget.displayText.replaceRange(
        range.start,
        range.end,
        value.text,
      );
      final applied = widget.controller.applyProjectedTextEdit(
        oldDisplayText: widget.displayText,
        newDisplayText: newDisplayText,
        undoGroupId: compositionUndoGroupId,
      );
      if (!applied) _syncFromController();
      _rememberLocalEditSnapshot(value, directSourceRange: null);
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }

    final selection = value.selection;
    if (!selection.isValid) {
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    final sourceRange = snapshot.directSourceRange ?? directSourceRange;
    if (sourceRange != null) {
      widget.controller.applySelection(
        FlarkSelection(
          baseOffset: sourceRange.start + selection.baseOffset,
          extentOffset: sourceRange.start + selection.extentOffset,
        ),
        userEvent: 'selection.liveBlock',
      );
      _rememberLocalEditSnapshot(value, directSourceRange: sourceRange);
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    final range = _clampedDisplayRange(widget.block, widget.displayText);
    widget.controller.applyProjectedSelection(
      FlarkSelection(
        baseOffset: range.start + selection.baseOffset,
        extentOffset: range.start + selection.extentOffset,
      ),
    );
    _rememberLocalEditSnapshot(value, directSourceRange: null);
    _compositionUndoGrouping.clearIfCommitted(value);
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
    final range = _clampedDisplayRange(widget.block, widget.displayText);
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
    _textController.block = widget.block;
    _textController.displayText = widget.displayText;
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
        snapshotRange != null &&
        _sourceRangeStillMatches(snapshotRange, snapshotValue.text)) {
      return _LocalTextEditSnapshot(
        value: snapshotValue,
        directSourceRange: snapshotRange,
      );
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
    final range = _clampedDisplayRange(widget.block, widget.displayText);
    return widget.displayText.substring(range.start, range.end);
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
    final range = _clampedDisplayRange(widget.block, widget.displayText);
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

  bool _sourceSelectionCoversBlock(FlarkSelection selection) {
    return _sourceSelectionCoversRange(selection, widget.block.sourceRange);
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
    final range = resolver(widget.controller.markdown, widget.block);
    if (range == null) return null;
    if (range.start < 0 ||
        range.start > range.end ||
        range.end > widget.controller.markdown.length) {
      return null;
    }
    return range;
  }
}

TextEditingValue _textValueWithPureInsertionSelection({
  required String oldText,
  required TextSelection oldSelection,
  required TextEditingValue newValue,
}) {
  if (!oldSelection.isValid || !oldSelection.isCollapsed) return newValue;
  final newSelection = newValue.selection;
  if (!newSelection.isValid || !newSelection.isCollapsed) return newValue;
  final insertion = _pureTextInsertion(
    oldText: oldText,
    newText: newValue.text,
  );
  if (insertion == null) return newValue;
  if (oldSelection.extentOffset != insertion.offset ||
      newSelection.extentOffset != insertion.offset) {
    return newValue;
  }
  return newValue.copyWith(
    selection: TextSelection.collapsed(
      offset: insertion.offset + insertion.length,
      affinity: newSelection.affinity,
    ),
  );
}

_PureTextInsertion? _pureTextInsertion({
  required String oldText,
  required String newText,
}) {
  if (newText.length <= oldText.length) return null;
  var prefixLength = 0;
  while (prefixLength < oldText.length &&
      prefixLength < newText.length &&
      oldText.codeUnitAt(prefixLength) == newText.codeUnitAt(prefixLength)) {
    prefixLength++;
  }

  var oldSuffix = oldText.length;
  var newSuffix = newText.length;
  while (oldSuffix > prefixLength &&
      newSuffix > prefixLength &&
      oldText.codeUnitAt(oldSuffix - 1) == newText.codeUnitAt(newSuffix - 1)) {
    oldSuffix--;
    newSuffix--;
  }

  if (oldSuffix != prefixLength) return null;
  final insertedLength = newSuffix - prefixLength;
  if (insertedLength <= 0) return null;
  return _PureTextInsertion(prefixLength, insertedLength);
}

final class _PureTextInsertion {
  const _PureTextInsertion(this.offset, this.length);

  final int offset;
  final int length;
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

    final displayRange = _clampedDisplayRange(renderBlock, displayText);
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

final class _EditableListItemBlock extends StatelessWidget {
  const _EditableListItemBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  @override
  Widget build(BuildContext context) {
    final marker = _listMarkerInfo(controller.markdown, block);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _ListMarkerGlyph(marker: marker, style: style),
          const SizedBox(width: 8),
          Expanded(
            child: _EditableProjectedBlockText(
              controller: controller,
              block: block,
              displayText: displayText,
              style: style,
              cursorColor: cursorColor,
              backgroundCursorColor: backgroundCursorColor,
              focusNode: focusNode,
              autofocus: autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: onMoveToPreviousBlock,
              onMoveToNextBlock: onMoveToNextBlock,
            ),
          ),
        ],
      ),
    );
  }
}

final class _ListMarkerGlyph extends StatelessWidget {
  const _ListMarkerGlyph({required this.marker, required this.style});

  final _ListMarkerInfo marker;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final markerText = marker.orderedLabel;
    if (markerText != null) {
      return SizedBox(
        key: const Key('FlarkLiveBlockListMarker'),
        width: 24,
        child: Text(
          markerText,
          textAlign: TextAlign.right,
          style: style.copyWith(color: const Color(0xFF5B677A)),
        ),
      );
    }
    return SizedBox(
      key: const Key('FlarkLiveBlockListMarker'),
      width: 16,
      height: (style.fontSize ?? 14) * (style.height ?? 1.2),
      child: CustomPaint(painter: const _BulletMarkerPainter()),
    );
  }
}

final class _BulletMarkerPainter extends CustomPainter {
  const _BulletMarkerPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = const Color(0xFF42526E);
    canvas.drawCircle(Offset(size.width / 2, size.height * 0.52), 2.3, paint);
  }

  @override
  bool shouldRepaint(_BulletMarkerPainter oldDelegate) => false;
}

final class _EditableTaskListItemBlock extends StatefulWidget {
  const _EditableTaskListItemBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  @override
  State<_EditableTaskListItemBlock> createState() {
    return _EditableTaskListItemBlockState();
  }
}

final class _EditableTaskListItemBlockState
    extends State<_EditableTaskListItemBlock> {
  @override
  Widget build(BuildContext context) {
    final checked = widget.block.taskListItem?.checked ?? false;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: EdgeInsets.only(
              top: ((widget.style.fontSize ?? 14) * 0.18),
            ),
            child: Semantics(
              key: const Key('FlarkLiveBlockTaskCheckbox'),
              checked: checked,
              label: checked ? 'Task, completed' : 'Task, not completed',
              container: true,
              onTap: () => _toggle(context),
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                excludeFromSemantics: true,
                onTap: () => _toggle(context),
                child: _TaskCheckboxGlyph(checked: checked),
              ),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: _EditableProjectedBlockText(
              controller: widget.controller,
              block: widget.block,
              displayText: widget.displayText,
              style: widget.style,
              cursorColor: widget.cursorColor,
              backgroundCursorColor: widget.backgroundCursorColor,
              focusNode: widget.focusNode,
              autofocus: widget.autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: widget.onMoveToPreviousBlock,
              onMoveToNextBlock: widget.onMoveToNextBlock,
            ),
          ),
        ],
      ),
    );
  }

  void _toggle(BuildContext context) {
    final interactions = FlarkMarkdownInteractions.maybeOf(context);
    final checked = widget.block.taskListItem?.checked ?? false;
    if (interactions != null) {
      if (!interactions.config.enableTaskCheckboxToggles) return;
      interactions.setTaskListChecked(widget.block, !checked);
      _restoreFocusAfterToggle();
      return;
    }
    widget.controller.dispatch(
      command: FlarkMarkdownBlockCommands.setTaskListChecked,
      payload: FlarkSetTaskListCheckedPayload(
        taskItemRange: widget.block.sourceRange,
        checked: !checked,
        userEvent: 'input.liveBlock.taskToggle',
      ),
    );
    _restoreFocusAfterToggle();
  }

  void _restoreFocusAfterToggle() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.focusNode?.requestFocus();
    });
  }
}

final class _TaskCheckboxGlyph extends StatelessWidget {
  const _TaskCheckboxGlyph({required this.checked});

  final bool checked;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 15,
      height: 15,
      child: CustomPaint(painter: _TaskCheckboxPainter(checked: checked)),
    );
  }
}

final class _TaskCheckboxPainter extends CustomPainter {
  const _TaskCheckboxPainter({required this.checked});

  final bool checked;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final borderColor = checked
        ? const Color(0xFF2E7D32)
        : const Color(0xFF7A8CA3);
    final fill = Paint()
      ..color = checked ? const Color(0xFF2E7D32) : const Color(0xFFFFFFFF);
    final border = Paint()
      ..color = borderColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.4;
    final shape = RRect.fromRectAndRadius(rect, const Radius.circular(3));
    canvas.drawRRect(shape, fill);
    canvas.drawRRect(shape, border);
    if (!checked) return;

    final check = Paint()
      ..color = const Color(0xFFFFFFFF)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final path = Path()
      ..moveTo(size.width * 0.25, size.height * 0.52)
      ..lineTo(size.width * 0.43, size.height * 0.70)
      ..lineTo(size.width * 0.76, size.height * 0.30);
    canvas.drawPath(path, check);
  }

  @override
  bool shouldRepaint(_TaskCheckboxPainter oldDelegate) {
    return oldDelegate.checked != checked;
  }
}

final class _EditableCodeBlock extends StatelessWidget {
  const _EditableCodeBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  static const double _copyChromeReserveWidth = 52;
  static const double _languageChromeReserveWidth = 104;

  @override
  Widget build(BuildContext context) {
    final language =
        _codeFenceLanguageFromSource(controller.markdown, block) ??
        block.codeBlock?.language;
    final editingOpeningLine = _selectionInCodeFenceOpeningLine(
      controller.markdown,
      block,
      controller.selection,
    );
    if (editingOpeningLine) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: _EditableProjectedBlockText(
          editableKey: const Key('FlarkLiveBlockCodeOpeningEditable'),
          controller: controller,
          block: block,
          displayText: displayText,
          style: style,
          cursorColor: cursorColor,
          backgroundCursorColor: backgroundCursorColor,
          focusNode: focusNode,
          autofocus: autofocus,
          markdownInputPolicy: true,
          sourceRangeForEdits: _codeFenceOpeningLineRange,
          onMoveToPreviousBlock: onMoveToPreviousBlock,
          onMoveToNextBlock: onMoveToNextBlock,
        ),
      );
    }
    final interactions = FlarkMarkdownInteractions.maybeOf(context);
    final showLanguageSelector =
        interactions != null &&
        interactions.editable &&
        interactions.config.enableCodeFenceLanguagePicker &&
        interactions.config.codeLanguages.isNotEmpty;
    final hasLanguageChrome =
        showLanguageSelector || (language != null && language.isNotEmpty);
    final chromeReserveWidth =
        _copyChromeReserveWidth +
        (hasLanguageChrome ? _languageChromeReserveWidth : 0);
    final editable = _EditableProjectedBlockText(
      editableKey: const Key('FlarkLiveBlockCodeEditable'),
      controller: controller,
      block: block,
      displayText: displayText,
      style: style.copyWith(
        color: const Color(0xFF17202A),
        fontFamily: 'monospace',
        height: 1.35,
      ),
      cursorColor: cursorColor,
      backgroundCursorColor: backgroundCursorColor,
      focusNode: focusNode,
      autofocus: autofocus,
      markdownInputPolicy: true,
      sourceRangeForEdits: _codeBodyRange,
      sourceEditForReplacement: _codeBodySourceEdit,
      codeSyntaxLanguage: language,
      onMoveToPreviousBlock: onMoveToPreviousBlock,
      onMoveToNextBlock: onMoveToNextBlock,
    );
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: DecoratedBox(
        key: const Key('FlarkLiveBlockCodeFence'),
        decoration: BoxDecoration(
          color: const Color(0xFFF1F4F8),
          border: Border.all(color: const Color(0xFFD7DEE8)),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 9),
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Actions(
                actions: {
                  _CodeIndentIntent: CallbackAction<_CodeIndentIntent>(
                    onInvoke: (intent) {
                      _applyCodeIndent(indent: intent.indent);
                      return null;
                    },
                  ),
                },
                child: Shortcuts(
                  shortcuts: const {
                    SingleActivator(LogicalKeyboardKey.tab): _CodeIndentIntent(
                      indent: true,
                    ),
                    SingleActivator(LogicalKeyboardKey.tab, shift: true):
                        _CodeIndentIntent(indent: false),
                  },
                  child: Padding(
                    padding: EdgeInsets.only(right: chromeReserveWidth),
                    child: editable,
                  ),
                ),
              ),
              Positioned(
                top: 0,
                right: 0,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _CodeCopyButton(
                      source: _codeCopyText(controller.markdown, block),
                      focusNode: focusNode,
                      style: style,
                    ),
                    if (showLanguageSelector || hasLanguageChrome)
                      const SizedBox(width: 6),
                    if (showLanguageSelector)
                      _CodeLanguageSelector(
                        interactions: interactions,
                        block: block,
                        language: language,
                        style: style,
                        focusNode: focusNode,
                      )
                    else if (language != null && language.isNotEmpty)
                      _CodeLanguageBadge(language: language, style: style),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  bool _applyCodeIndent({required bool indent}) {
    final bodyRange = _codeBodyRange(controller.markdown, block);
    if (bodyRange == null) return false;
    final selection = controller.selection;
    if (selection.start < bodyRange.start || selection.end > bodyRange.end) {
      return false;
    }

    final operations = indent
        ? FlarkMarkdownFencedCodePolicy.indentOperations(
            markdown: controller.markdown,
            bodyRange: bodyRange,
            selection: selection,
          )
        : FlarkMarkdownFencedCodePolicy.outdentOperations(
            markdown: controller.markdown,
            bodyRange: bodyRange,
            selection: selection,
          );
    if (operations.isEmpty) return false;

    controller.applyTransaction(
      FlarkTransaction(
        operations: operations,
        selectionBefore: selection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: indent
              ? 'input.liveBlock.codeIndent'
              : 'input.liveBlock.codeOutdent',
          parseInvalidationRange: bodyRange,
          projectionInvalidationRange: bodyRange,
        ),
      ),
    );
    return true;
  }
}

final class _CodeCopyButton extends StatelessWidget {
  const _CodeCopyButton({
    required this.source,
    required this.focusNode,
    required this.style,
  });

  final String source;
  final FocusNode? focusNode;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final labelStyle = style.copyWith(
      color: const Color(0xFF42526E),
      fontFamily: 'monospace',
      fontSize: (style.fontSize ?? 14) - 1,
      fontWeight: FontWeight.w700,
    );
    void copy() {
      Clipboard.setData(ClipboardData(text: source));
      final focusNode = this.focusNode;
      if (focusNode == null) return;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (focusNode.canRequestFocus) focusNode.requestFocus();
      });
    }

    return Semantics(
      key: const Key('FlarkLiveBlockCodeCopyButton'),
      button: true,
      label: 'Copy code',
      onTap: copy,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        excludeFromSemantics: true,
        onTap: copy,
        child: DecoratedBox(
          decoration: const BoxDecoration(
            color: Color(0xFFE2E8F0),
            borderRadius: BorderRadius.all(Radius.circular(4)),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            child: Text('Copy', style: labelStyle),
          ),
        ),
      ),
    );
  }
}

final class _CodeLanguageBadge extends StatelessWidget {
  const _CodeLanguageBadge({required this.language, required this.style});

  final String language;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerRight,
      child: DecoratedBox(
        decoration: const BoxDecoration(
          color: Color(0xFFE2E8F0),
          borderRadius: BorderRadius.all(Radius.circular(4)),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
          child: Text(
            language,
            style: style.copyWith(
              color: const Color(0xFF42526E),
              fontFamily: 'monospace',
              fontSize: (style.fontSize ?? 14) - 1,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
      ),
    );
  }
}

final class _CodeLanguageSelector extends StatefulWidget {
  const _CodeLanguageSelector({
    required this.interactions,
    required this.block,
    required this.language,
    required this.style,
    required this.focusNode,
  });

  final FlarkMarkdownInteractions interactions;
  final FlarkRenderBlock block;
  final String? language;
  final TextStyle style;
  final FocusNode? focusNode;

  @override
  State<_CodeLanguageSelector> createState() => _CodeLanguageSelectorState();
}

final class _CodeLanguageSelectorState extends State<_CodeLanguageSelector> {
  static const double _menuWidth = 152;

  final LayerLink _menuAnchor = LayerLink();
  final GlobalKey _buttonBoundsKey = GlobalKey();
  final GlobalKey _menuBoundsKey = GlobalKey();
  OverlayEntry? _menuEntry;
  bool _globalPointerRouteAttached = false;

  bool get _open => _menuEntry != null;

  @override
  void dispose() {
    _closeMenu(notify: false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final currentValue = widget.language ?? '';
    final currentLabel = _languageLabel(currentValue);
    final labelStyle = widget.style.copyWith(
      color: const Color(0xFF42526E),
      fontFamily: 'monospace',
      fontSize: (widget.style.fontSize ?? 14) - 1,
      fontWeight: FontWeight.w700,
    );

    return CompositedTransformTarget(
      link: _menuAnchor,
      child: KeyedSubtree(
        key: const Key('FlarkLiveBlockCodeLanguageButton'),
        child: GestureDetector(
          key: _buttonBoundsKey,
          behavior: HitTestBehavior.opaque,
          onTap: _toggleMenu,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: _open ? const Color(0xFFD7DEE8) : const Color(0xFFE2E8F0),
              borderRadius: const BorderRadius.all(Radius.circular(4)),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              child: Text(currentLabel, style: labelStyle),
            ),
          ),
        ),
      ),
    );
  }

  void _toggleMenu() {
    if (_open) {
      _closeMenu();
    } else {
      _openMenu();
    }
  }

  void _openMenu() {
    final overlay = Overlay.maybeOf(context);
    if (overlay == null) return;
    _menuEntry = OverlayEntry(builder: _buildMenuOverlay);
    overlay.insert(_menuEntry!);
    _attachGlobalPointerRoute();
    if (mounted) setState(() {});
  }

  Widget _buildMenuOverlay(BuildContext context) {
    final currentValue = widget.language ?? '';
    return CompositedTransformFollower(
      link: _menuAnchor,
      showWhenUnlinked: false,
      targetAnchor: Alignment.bottomRight,
      followerAnchor: Alignment.topRight,
      offset: const Offset(0, 4),
      child: UnconstrainedBox(
        alignment: Alignment.topRight,
        child: SizedBox(
          width: _menuWidth,
          child: KeyedSubtree(
            key: const Key('FlarkLiveBlockCodeLanguageMenu'),
            child: DecoratedBox(
              key: _menuBoundsKey,
              decoration: BoxDecoration(
                color: const Color(0xFFFFFFFF),
                border: Border.all(color: const Color(0xFFD7DEE8)),
                borderRadius: const BorderRadius.all(Radius.circular(6)),
                boxShadow: const [
                  BoxShadow(
                    color: Color(0x1A000000),
                    blurRadius: 10,
                    offset: Offset(0, 4),
                  ),
                ],
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (final option
                        in widget.interactions.config.codeLanguages)
                      _CodeLanguageOptionButton(
                        option: option,
                        selected: option.value == currentValue,
                        style: widget.style,
                        onTap: () => _selectLanguage(option.value),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _attachGlobalPointerRoute() {
    if (_globalPointerRouteAttached) return;
    GestureBinding.instance.pointerRouter.addGlobalRoute(
      _handleGlobalPointerEvent,
    );
    _globalPointerRouteAttached = true;
  }

  void _detachGlobalPointerRoute() {
    if (!_globalPointerRouteAttached) return;
    GestureBinding.instance.pointerRouter.removeGlobalRoute(
      _handleGlobalPointerEvent,
    );
    _globalPointerRouteAttached = false;
  }

  void _handleGlobalPointerEvent(PointerEvent event) {
    if (event is! PointerDownEvent || !_open) return;
    if (_containsGlobalPoint(_buttonBoundsKey.currentContext, event.position)) {
      return;
    }
    if (_containsGlobalPoint(_menuBoundsKey.currentContext, event.position)) {
      return;
    }
    _closeMenu();
  }

  bool _containsGlobalPoint(BuildContext? context, Offset globalPosition) {
    final renderObject = context?.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.attached) return false;
    final localPosition = renderObject.globalToLocal(globalPosition);
    return renderObject.size.contains(localPosition);
  }

  void _closeMenu({bool notify = true}) {
    final entry = _menuEntry;
    if (entry == null) return;
    _menuEntry = null;
    _detachGlobalPointerRoute();
    entry.remove();
    if (notify && mounted) setState(() {});
  }

  void _selectLanguage(String language) {
    _closeMenu();
    final handled = widget.interactions.setCodeFenceLanguage(
      widget.block,
      language,
    );
    if (!handled) return;
    _adoptImmediateMarkdownParseForController(widget.interactions.controller);

    final focusNode = widget.focusNode;
    if (focusNode == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !focusNode.canRequestFocus) return;
      focusNode.requestFocus();
    });
  }

  String _languageLabel(String value) {
    for (final option in widget.interactions.config.codeLanguages) {
      if (option.value == value) return option.label;
    }
    return value.isEmpty ? 'Auto' : value;
  }
}

final class _CodeLanguageOptionButton extends StatelessWidget {
  const _CodeLanguageOptionButton({
    required this.option,
    required this.selected,
    required this.style,
    required this.onTap,
  });

  final FlarkCodeLanguageOption option;
  final bool selected;
  final TextStyle style;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      key: ValueKey('FlarkLiveBlockCodeLanguageOption:${option.value}'),
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Text(
          option.label,
          style: style.copyWith(
            color: selected ? const Color(0xFF17202A) : const Color(0xFF42526E),
            fontSize: (style.fontSize ?? 14) - 1,
            fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
          ),
        ),
      ),
    );
  }
}

final class _CodeIndentIntent extends Intent {
  const _CodeIndentIntent({required this.indent});

  final bool indent;
}

final class _EditableTableBlock extends StatelessWidget {
  const _EditableTableBlock({
    required this.controller,
    required this.block,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;

  @override
  Widget build(BuildContext context) {
    final table = _ParsedEditableTable.fromRenderBlock(
      controller.markdown,
      block,
    );
    if (table == null || table.rows.isEmpty) {
      final displayText = _FlarkLiveRenderedBlockEditorState._projectedText(
        controller,
      );
      return _EditableProjectedBlockText(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
      );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: DecoratedBox(
        key: const Key('FlarkLiveBlockTable'),
        decoration: BoxDecoration(
          border: Border.all(color: const Color(0xFFD7DEE8)),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: ClipRRect(
          borderRadius: const BorderRadius.all(Radius.circular(6)),
          child: Table(
            defaultVerticalAlignment: TableCellVerticalAlignment.middle,
            border: TableBorder.symmetric(
              inside: const BorderSide(color: Color(0xFFE2E8F0)),
            ),
            children: [
              for (var rowIndex = 0; rowIndex < table.rows.length; rowIndex++)
                TableRow(
                  decoration: BoxDecoration(
                    color: rowIndex == 0
                        ? const Color(0xFFF1F4F8)
                        : const Color(0xFFFFFFFF),
                  ),
                  children: [
                    for (
                      var columnIndex = 0;
                      columnIndex < table.rows[rowIndex].cells.length;
                      columnIndex++
                    )
                      Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 6,
                        ),
                        child: _EditableTableCell(
                          key: ValueKey(
                            'table-cell:$rowIndex:$columnIndex:'
                            '${table.rows[rowIndex].cells[columnIndex].range.start}',
                          ),
                          controller: controller,
                          cell: table.rows[rowIndex].cells[columnIndex],
                          style: _tableCellStyle(style, rowIndex),
                          textAlign: _tableTextAlign(block, columnIndex),
                          cursorColor: cursorColor,
                          backgroundCursorColor: backgroundCursorColor,
                          editableKey: Key(
                            'FlarkLiveBlockTableCell-$rowIndex-'
                            '$columnIndex',
                          ),
                        ),
                      ),
                  ],
                ),
            ],
          ),
        ),
      ),
    );
  }

  TextStyle _tableCellStyle(TextStyle base, int rowIndex) {
    if (rowIndex != 0) return base;
    return base.copyWith(fontWeight: FontWeight.w700);
  }

  TextAlign _tableTextAlign(FlarkRenderBlock block, int columnIndex) {
    final alignments = block.table?.columnAlignments ?? const [];
    if (columnIndex >= alignments.length) return TextAlign.left;
    return switch (alignments[columnIndex]) {
      FlarkRenderTableColumnAlignment.center => TextAlign.center,
      FlarkRenderTableColumnAlignment.right => TextAlign.right,
      FlarkRenderTableColumnAlignment.left ||
      FlarkRenderTableColumnAlignment.none ||
      FlarkRenderTableColumnAlignment.unknown => TextAlign.left,
    };
  }
}

final class _EditableTableCell extends StatefulWidget {
  const _EditableTableCell({
    super.key,
    required this.controller,
    required this.cell,
    required this.style,
    required this.textAlign,
    required this.cursorColor,
    required this.backgroundCursorColor,
    required this.editableKey,
  });

  final FlarkFlutterController controller;
  final _ParsedEditableTableCell cell;
  final TextStyle style;
  final TextAlign textAlign;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final Key editableKey;

  @override
  State<_EditableTableCell> createState() => _EditableTableCellState();
}

final class _EditableTableCellState extends State<_EditableTableCell> {
  final _editableStateKey = GlobalKey<EditableTextState>();
  late final TextEditingController _textController;
  late final FocusNode _focusNode;
  final _compositionUndoGrouping = _FlarkCompositionUndoGrouping();
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    _textController = TextEditingController();
    _focusNode = FocusNode();
    _textController.addListener(_handleTextChanged);
    _syncFromController();
  }

  @override
  void didUpdateWidget(_EditableTableCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncFromController();
  }

  @override
  void dispose() {
    _textController.removeListener(_handleTextChanged);
    _textController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final editor = EditableText(
      key: _editableStateKey,
      controller: _textController,
      focusNode: _focusNode,
      style: widget.style,
      textAlign: widget.textAlign,
      cursorColor: widget.cursorColor,
      selectionColor: _selectionColorForCursor(widget.cursorColor),
      selectionControls: flarkTextSelectionControlsForPlatform(context),
      backgroundCursorColor: widget.backgroundCursorColor,
      keyboardType: TextInputType.text,
      inputFormatters: const [_TableCellInputFormatter()],
      minLines: 1,
      maxLines: null,
      paintCursorAboveText: true,
      rendererIgnoresPointer: true,
    );
    return flarkEditableTextGestureDetector(
      key: widget.editableKey,
      editableTextKey: _editableStateKey,
      child: editor,
    );
  }

  void _handleTextChanged() {
    if (_syncing) return;
    final oldLocalSelection = _localSelection(widget.cell.text.length);
    final value = _textValueWithPureInsertionSelection(
      oldText: widget.cell.text,
      oldSelection: oldLocalSelection,
      newValue: _textController.value,
    );
    _adoptNormalizedTextControllerValue(value);
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    if (value.text != widget.cell.text) {
      final replacement = widget.cell.replacementText(value.text);
      _replaceSourceRange(
        controller: widget.controller,
        range: widget.cell.range,
        replacementText: replacement,
        userEvent: 'input.liveBlock.tableCell',
        undoGroupId: compositionUndoGroupId,
        selectionAfter: _tableCellSelectionAfterReplacement(
          cell: widget.cell,
          value: value,
        ),
      );
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }

    final selection = value.selection;
    if (!selection.isValid) {
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    widget.controller.applySelection(
      FlarkSelection(
        baseOffset: widget.cell.range.start + selection.baseOffset,
        extentOffset: widget.cell.range.start + selection.extentOffset,
      ),
      userEvent: 'selection.liveBlock.tableCell',
    );
    _compositionUndoGrouping.clearIfCommitted(value);
  }

  void _syncFromController() {
    final current = _textController.value;
    final selection = _localSelection(widget.cell.text.length);
    final next = TextEditingValue(
      text: widget.cell.text,
      selection: selection,
      composing: current.text == widget.cell.text
          ? current.composing
          : TextRange.empty,
    );
    if (current == next) return;
    _syncing = true;
    _textController.value = next;
    _syncing = false;
  }

  void _adoptNormalizedTextControllerValue(TextEditingValue value) {
    if (_textController.value == value) return;
    _syncing = true;
    _textController.value = value;
    _syncing = false;
  }

  TextSelection _localSelection(int textLength) {
    final selection = widget.controller.selection;
    if (selection.start < widget.cell.range.start ||
        selection.end > widget.cell.range.end) {
      final current = _textController.selection;
      if (current.isValid &&
          current.baseOffset <= textLength &&
          current.extentOffset <= textLength) {
        return current;
      }
      return TextSelection.collapsed(offset: textLength);
    }
    return TextSelection(
      baseOffset: _localOffsetInsideSanitizedTableCell(
        widget.cell.text,
        selection.baseOffset - widget.cell.range.start,
      ),
      extentOffset: _localOffsetInsideSanitizedTableCell(
        widget.cell.text,
        selection.extentOffset - widget.cell.range.start,
      ),
    );
  }
}

final class _ParsedEditableTable {
  const _ParsedEditableTable({required this.rows});

  final List<_ParsedEditableTableRow> rows;

  static _ParsedEditableTable? fromRenderBlock(
    String markdown,
    FlarkRenderBlock block,
  ) {
    final table = block.table;
    if (table == null || table.rows.isEmpty) return null;

    final columnCount = _resolvedRenderTableColumnCount(table);
    if (columnCount <= 0) return const _ParsedEditableTable(rows: []);

    return _ParsedEditableTable(
      rows: [
        for (final row in table.rows)
          _ParsedEditableTableRow(
            insertionOffset: _rowInsertionOffset(markdown, row),
            cells: [
              for (var index = 0; index < columnCount; index++)
                if (index < row.cells.length)
                  _cellFromDescriptor(markdown, row.cells[index])
                else
                  _emptyCellAfterDescriptor(markdown, row),
            ],
          ),
      ],
    );
  }

  static int _resolvedRenderTableColumnCount(FlarkRenderTableDescriptor table) {
    if (table.columnAlignments.isNotEmpty) return table.columnAlignments.length;
    var columnCount = 0;
    for (final row in table.rows) {
      if (row.cells.length > columnCount) columnCount = row.cells.length;
    }
    return columnCount;
  }

  static int _rowInsertionOffset(
    String markdown,
    FlarkRenderTableRowDescriptor row,
  ) {
    if (row.cells.isNotEmpty) {
      return _trimmedCellRange(markdown, row.cells.last.sourceRange).end;
    }
    return row.sourceRange.end;
  }

  static _ParsedEditableTableCell _cellFromDescriptor(
    String markdown,
    FlarkRenderTableCellDescriptor cell,
  ) {
    final contentRange = _trimmedCellRange(markdown, cell.sourceRange);
    return _ParsedEditableTableCell(
      text: _unescapeCellText(
        markdown.substring(contentRange.start, contentRange.end),
      ),
      range: contentRange,
    );
  }

  static FlarkSourceRange _trimmedCellRange(
    String markdown,
    FlarkSourceRange range,
  ) {
    var start = range.start.clamp(0, markdown.length);
    var end = range.end.clamp(start, markdown.length);
    while (start < end && _isWhitespace(markdown.codeUnitAt(start))) {
      start++;
    }
    while (end > start && _isWhitespace(markdown.codeUnitAt(end - 1))) {
      end--;
    }
    return FlarkSourceRange(start, end);
  }

  static _ParsedEditableTableCell _emptyCellAfterDescriptor(
    String markdown,
    FlarkRenderTableRowDescriptor row,
  ) {
    final insertionOffset = _rowInsertionOffset(markdown, row);
    return _ParsedEditableTableCell(
      text: '',
      range: FlarkSourceRange(insertionOffset, insertionOffset),
      replacementPrefix: ' | ',
    );
  }

  static bool _isWhitespace(int codeUnit) {
    return codeUnit == 32 || codeUnit == 9;
  }

  static String _unescapeCellText(String text) {
    return text.replaceAll(r'\|', '|');
  }
}

final class _ParsedEditableTableRow {
  const _ParsedEditableTableRow({
    required this.cells,
    required this.insertionOffset,
  });

  final List<_ParsedEditableTableCell> cells;
  final int insertionOffset;
}

final class _ParsedEditableTableCell {
  const _ParsedEditableTableCell({
    required this.text,
    required this.range,
    this.replacementPrefix = '',
  });

  final String text;
  final FlarkSourceRange range;
  final String replacementPrefix;

  String replacementText(String value) {
    return '$replacementPrefix${_sanitizeTableCell(value)}';
  }
}

List<_LiveRenderedTextSegment> _blockTextSegments({
  required int textLength,
  required int globalDisplayStart,
  required FlarkRenderBlock block,
}) {
  if (textLength <= 0) return const [];

  final boundaries = <int>{0, textLength};
  for (final run in block.inlineRuns) {
    final start = (run.displayRange.start - globalDisplayStart).clamp(
      0,
      textLength,
    );
    final end = (run.displayRange.end - globalDisplayStart).clamp(
      0,
      textLength,
    );
    if (start >= end) continue;
    boundaries
      ..add(start)
      ..add(end);
  }

  final sorted = boundaries.toList()..sort();
  final segments = <_LiveRenderedTextSegment>[];
  for (var index = 0; index < sorted.length - 1; index++) {
    final start = sorted[index];
    final end = sorted[index + 1];
    if (start >= end) continue;
    final signature = _LiveRenderedTextStyleSignature.forRange(
      globalDisplayStart + start,
      globalDisplayStart + end,
      blocks: [block],
      runs: block.inlineRuns,
    );
    if (segments.isNotEmpty && segments.last.signature == signature) {
      final previous = segments.removeLast();
      segments.add(
        _LiveRenderedTextSegment(
          start: previous.start,
          end: end,
          signature: signature,
        ),
      );
      continue;
    }
    segments.add(
      _LiveRenderedTextSegment(start: start, end: end, signature: signature),
    );
  }
  return List.unmodifiable(segments);
}

TextStyle _blockTextStyle(TextStyle baseStyle, FlarkRenderBlock block) {
  final signature = _LiveRenderedTextStyleSignature.forRange(
    block.displayRange.start,
    block.displayRange.end,
    blocks: [block],
    runs: const [],
  );
  return signature.resolve(baseStyle);
}

bool _rangeOverlapsText(FlarkSourceRange range, String text) {
  return range.end > 0 && range.start < text.length && range.start < range.end;
}

FlarkSourceRange _clampedDisplayRange(
  FlarkRenderBlock block,
  String displayText,
) {
  final start = block.displayRange.start.clamp(0, displayText.length);
  var end = block.displayRange.end.clamp(start, displayText.length);
  if (block.kind == FlarkMarkdownBlockKind.blockquote) {
    return FlarkSourceRange(start, end);
  }
  while (end > start) {
    final unit = displayText.codeUnitAt(end - 1);
    if (unit != 0x0A && unit != 0x0D) break;
    end--;
  }
  return FlarkSourceRange(start, end);
}

final class _ListMarkerInfo {
  const _ListMarkerInfo.unordered() : orderedLabel = null;
  const _ListMarkerInfo.ordered(this.orderedLabel);

  final String? orderedLabel;
}

_ListMarkerInfo _listMarkerInfo(String markdown, FlarkRenderBlock block) {
  final line = _sourceLineForBlock(markdown, block);
  final ordered = _orderedListMarkerLabel(line);
  if (block.listItem?.kind == FlarkRenderListKind.ordered || ordered != null) {
    return _ListMarkerInfo.ordered(ordered ?? '1.');
  }
  return const _ListMarkerInfo.unordered();
}

String? _orderedListMarkerLabel(
  String line, {
  bool requireFollowingWhitespace = false,
}) {
  var index = _skipHorizontalWhitespace(line, 0);
  final digitStart = index;
  while (index < line.length &&
      index - digitStart < 9 &&
      _isAsciiDigit(line.codeUnitAt(index))) {
    index++;
  }
  if (index == digitStart) return null;
  if (index < line.length && _isAsciiDigit(line.codeUnitAt(index))) {
    return null;
  }
  if (index >= line.length) return null;

  final delimiter = line.codeUnitAt(index);
  if (delimiter != 0x2E && delimiter != 0x29) return null;
  index++;
  if (requireFollowingWhitespace &&
      (index >= line.length ||
          !_isHorizontalWhitespace(line.codeUnitAt(index)))) {
    return null;
  }
  return line.substring(digitStart, index);
}

int _skipHorizontalWhitespace(String text, int start) {
  var index = start;
  while (index < text.length &&
      _isHorizontalWhitespace(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

bool _isHorizontalWhitespace(int codeUnit) {
  return codeUnit == 0x20 || codeUnit == 0x09;
}

bool _isAsciiDigit(int codeUnit) {
  return codeUnit >= 0x30 && codeUnit <= 0x39;
}

String _sourceLineForBlock(String markdown, FlarkRenderBlock block) {
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start >= markdown.length ||
      block.sourceRange.start >= block.sourceRange.end) {
    return '';
  }
  final lineEnd = markdown.indexOf('\n', block.sourceRange.start);
  final effectiveLineEnd = lineEnd < 0 || lineEnd > block.sourceRange.end
      ? block.sourceRange.end
      : lineEnd;
  return markdown.substring(block.sourceRange.start, effectiveLineEnd);
}

void _replaceSourceRange({
  required FlarkFlutterController controller,
  required FlarkSourceRange range,
  required String replacementText,
  required String userEvent,
  int? undoGroupId,
  FlarkSelection? selectionAfter,
}) {
  range.validate(controller.markdown.length);
  controller.applyTransaction(
    FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: range,
        replacementText: replacementText,
      ),
      selectionBefore: controller.selection,
      selectionAfter:
          selectionAfter ??
          FlarkSelection.collapsed(range.start + replacementText.length),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: userEvent,
        undoGroupId: undoGroupId,
        parseInvalidationRange: range,
        projectionInvalidationRange: range,
      ),
    ),
  );
}

final class _SourceEdit {
  const _SourceEdit({
    required this.range,
    required this.replacementText,
    required this.editableRangeAfter,
    required this.selectionAfter,
  });

  final FlarkSourceRange range;
  final String replacementText;
  final FlarkSourceRange editableRangeAfter;
  final FlarkSelection selectionAfter;
}

FlarkSelection _sourceSelectionAfterReplacement({
  required FlarkSourceRange range,
  required TextSelection localSelection,
  required int replacementTextLength,
}) {
  if (!localSelection.isValid) {
    return FlarkSelection.collapsed(range.start + replacementTextLength);
  }
  return FlarkSelection(
    baseOffset:
        range.start + localSelection.baseOffset.clamp(0, replacementTextLength),
    extentOffset:
        range.start +
        localSelection.extentOffset.clamp(0, replacementTextLength),
  );
}

_SourceEdit _codeBodySourceEdit({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSourceRange range,
  required TextEditingValue value,
}) {
  var replacementText = value.text;
  final replacementTextLengthForSelection = replacementText.length;

  final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
    markdown,
    block.sourceRange.start,
  );
  final closingLineStart = context?.closingLineStart;
  final typedClosingLineStart = context == null
      ? null
      : _typedCodeBodyClosingLineStart(value: value, context: context);
  if (typedClosingLineStart != null && context != null) {
    final bodyText = _bodyTextBeforeTypedClosingLine(
      value.text,
      typedClosingLineStart,
    );
    final typedClosingLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      value.text,
      typedClosingLineStart,
    );
    final typedClosingLine = value.text.substring(
      typedClosingLineStart,
      typedClosingLineEnd,
    );
    final replacingExistingClose = context.closingLineEnd != null;
    final closeReplacementText = replacingExistingClose
        ? bodyText
        : _bodyTextWithTypedClosingLine(bodyText, typedClosingLine);
    final selectionAfter = replacingExistingClose
        ? context.closingLineEnd!
        : range.start + closeReplacementText.length;
    return _SourceEdit(
      range: range,
      replacementText: closeReplacementText,
      editableRangeAfter: FlarkSourceRange(
        range.start,
        range.start + bodyText.length,
      ),
      selectionAfter: FlarkSelection.collapsed(selectionAfter),
    );
  }
  if (closingLineStart != null &&
      range.end == closingLineStart &&
      replacementText.isNotEmpty &&
      !_endsWithLineBreak(replacementText)) {
    replacementText = '$replacementText\n';
  }

  return _SourceEdit(
    range: range,
    replacementText: replacementText,
    editableRangeAfter: FlarkSourceRange(
      range.start,
      range.start + replacementTextLengthForSelection,
    ),
    selectionAfter: _sourceSelectionAfterReplacement(
      range: range,
      localSelection: value.selection,
      replacementTextLength: replacementTextLengthForSelection,
    ),
  );
}

int? _typedCodeBodyClosingLineStart({
  required TextEditingValue value,
  required FlarkMarkdownFencedCodeContext context,
}) {
  final selection = value.selection;
  if (!selection.isValid || !selection.isCollapsed) return null;
  final caret = selection.extentOffset;
  if (caret < 0 || caret > value.text.length) return null;
  final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    value.text,
    caret,
  );
  final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    value.text,
    lineStart,
  );
  if (caret != lineEnd) return null;
  final line = value.text.substring(lineStart, lineEnd);
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
  if (fence == null) return null;
  if (fence.closes(context)) return lineStart;
  return null;
}

String _bodyTextBeforeTypedClosingLine(String text, int closingLineStart) {
  var end = closingLineStart.clamp(0, text.length);
  if (end > 0 && text.codeUnitAt(end - 1) == 0x0A) end--;
  if (end > 0 && text.codeUnitAt(end - 1) == 0x0D) end--;
  return text.substring(0, end);
}

String _bodyTextWithTypedClosingLine(String bodyText, String closingLine) {
  if (bodyText.isEmpty) return closingLine;
  return '$bodyText\n$closingLine';
}

bool _endsWithLineBreak(String text) {
  if (text.isEmpty) return false;
  final codeUnit = text.codeUnitAt(text.length - 1);
  return codeUnit == 0x0A || codeUnit == 0x0D;
}

FlarkSourceRange? _codeBodyRange(String markdown, FlarkRenderBlock block) {
  if (block.sourceRange.start < 0 || block.sourceRange.end > markdown.length) {
    return null;
  }
  final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
    markdown,
    block.sourceRange.start,
  );
  if (context != null) {
    return context.bodyContentRange(markdown);
  }

  final openerEnd = markdown.indexOf('\n', block.sourceRange.start);
  if (openerEnd < 0 || openerEnd >= block.sourceRange.end) return null;
  final bodyStart = openerEnd + 1;
  final closerStart = markdown.lastIndexOf('\n', block.sourceRange.end - 1);
  final bodyEnd = closerStart > bodyStart ? closerStart : block.sourceRange.end;
  return FlarkSourceRange(bodyStart, bodyEnd).validate(markdown.length);
}

String _codeCopyText(String markdown, FlarkRenderBlock block) {
  final range = _codeBodyRange(markdown, block);
  if (range == null) return '';
  return markdown.substring(range.start, range.end);
}

FlarkSourceRange? _codeFenceOpeningLineRange(
  String markdown,
  FlarkRenderBlock block,
) {
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start > markdown.length) {
    return null;
  }
  final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    markdown,
    block.sourceRange.start,
  );
  if (lineStart != block.sourceRange.start) return null;
  final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    markdown,
    lineStart,
  );
  return FlarkSourceRange(lineStart, lineEnd).validate(markdown.length);
}

bool _selectionInCodeFenceOpeningLine(
  String markdown,
  FlarkRenderBlock block,
  FlarkSelection selection,
) {
  if (block.codeBlock == null) return false;
  final openingLineRange = _codeFenceOpeningLineRange(markdown, block);
  if (openingLineRange == null) return false;
  return selection.start >= openingLineRange.start &&
      selection.end <= openingLineRange.end;
}

String? _codeFenceLanguageFromSource(String markdown, FlarkRenderBlock block) {
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start >= markdown.length ||
      block.sourceRange.end > markdown.length) {
    return null;
  }
  return FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
    markdown,
    block.sourceRange.start,
  )?.language;
}

String _sanitizeTableCell(String value) {
  return value.replaceAll('\n', ' ').replaceAll('|', r'\|');
}

FlarkSelection _tableCellSelectionAfterReplacement({
  required _ParsedEditableTableCell cell,
  required TextEditingValue value,
}) {
  final selection = value.selection;
  final replacementText = cell.replacementText(value.text);
  if (!selection.isValid) {
    return FlarkSelection.collapsed(cell.range.start + replacementText.length);
  }
  return FlarkSelection(
    baseOffset:
        cell.range.start +
        cell.replacementPrefix.length +
        _sourceOffsetInsideSanitizedTableCell(value.text, selection.baseOffset),
    extentOffset:
        cell.range.start +
        cell.replacementPrefix.length +
        _sourceOffsetInsideSanitizedTableCell(
          value.text,
          selection.extentOffset,
        ),
  );
}

int _sourceOffsetInsideSanitizedTableCell(String value, int localOffset) {
  final limit = localOffset.clamp(0, value.length);
  var sourceOffset = 0;
  for (var index = 0; index < limit; index++) {
    final codeUnit = value.codeUnitAt(index);
    sourceOffset += codeUnit == 124 ? 2 : 1;
  }
  return sourceOffset;
}

int _localOffsetInsideSanitizedTableCell(String value, int sourceOffset) {
  final target = sourceOffset.clamp(0, _sanitizeTableCell(value).length);
  if (target == 0) return 0;
  var consumedSource = 0;
  for (var index = 0; index < value.length; index++) {
    final codeUnit = value.codeUnitAt(index);
    consumedSource += codeUnit == 124 ? 2 : 1;
    if (target <= consumedSource) return index + 1;
  }
  return value.length;
}

final class _TableCellInputFormatter extends TextInputFormatter {
  const _TableCellInputFormatter();

  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    final text = newValue.text;
    if (!text.contains('\n') && !text.contains('\r')) return newValue;

    final normalized = StringBuffer();
    var removedCodeUnitsBeforeBase = 0;
    var removedCodeUnitsBeforeExtent = 0;
    var index = 0;
    while (index < text.length) {
      final codeUnit = text.codeUnitAt(index);
      if (codeUnit == 0x0D) {
        normalized.write(' ');
        if (index + 1 < text.length && text.codeUnitAt(index + 1) == 0x0A) {
          final lineFeedIndex = index + 1;
          if (lineFeedIndex < newValue.selection.baseOffset) {
            removedCodeUnitsBeforeBase++;
          }
          if (lineFeedIndex < newValue.selection.extentOffset) {
            removedCodeUnitsBeforeExtent++;
          }
          index += 2;
          continue;
        }
        index++;
        continue;
      }
      if (codeUnit == 0x0A) {
        normalized.write(' ');
        index++;
        continue;
      }
      normalized.writeCharCode(codeUnit);
      index++;
    }

    final selection = newValue.selection;
    final normalizedText = normalized.toString();
    return newValue.copyWith(
      text: normalizedText,
      selection: selection.isValid
          ? TextSelection(
              baseOffset: (selection.baseOffset - removedCodeUnitsBeforeBase)
                  .clamp(0, normalizedText.length),
              extentOffset:
                  (selection.extentOffset - removedCodeUnitsBeforeExtent).clamp(
                    0,
                    normalizedText.length,
                  ),
              affinity: selection.affinity,
              isDirectional: selection.isDirectional,
            )
          : selection,
      composing: TextRange.empty,
    );
  }
}

final class _FlarkLiveRenderedTextController extends TextEditingController {
  _FlarkLiveRenderedTextState renderState = _FlarkLiveRenderedTextState.empty;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final baseStyle = style ?? DefaultTextStyle.of(context).style;
    final composingRange = withComposing && value.isComposingRangeValid
        ? value.composing
        : null;
    if (!renderState.hasRenderPlan ||
        renderState.segments.isEmpty ||
        text.isEmpty) {
      return _plainTextSpan(
        baseStyle: baseStyle,
        composingRange: composingRange,
      );
    }

    final children = <TextSpan>[];
    var cursor = 0;
    for (final segment in renderState.segments) {
      if (segment.start > cursor) {
        _appendStyledText(
          children,
          start: cursor,
          end: segment.start,
          style: baseStyle,
          composingRange: composingRange,
        );
      }
      _appendStyledText(
        children,
        start: segment.start,
        end: segment.end,
        style: segment.signature.resolve(baseStyle),
        composingRange: composingRange,
      );
      cursor = segment.end;
    }
    if (cursor < text.length) {
      _appendStyledText(
        children,
        start: cursor,
        end: text.length,
        style: baseStyle,
        composingRange: composingRange,
      );
    }

    return TextSpan(style: baseStyle, children: children);
  }

  TextSpan _plainTextSpan({
    required TextStyle baseStyle,
    required TextRange? composingRange,
  }) {
    if (composingRange == null) return TextSpan(style: baseStyle, text: text);
    return TextSpan(
      style: baseStyle,
      children: [
        TextSpan(text: composingRange.textBefore(text)),
        TextSpan(
          text: composingRange.textInside(text),
          style: baseStyle.merge(
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

final class _FlarkLiveRenderedTextState {
  const _FlarkLiveRenderedTextState({
    required this.displayText,
    required this.renderPlan,
    required this.hasRenderPlan,
    required this.segments,
  });

  factory _FlarkLiveRenderedTextState.fromRenderPlan({
    required String displayText,
    required FlarkRenderPlan renderPlan,
    required bool hasRenderPlan,
  }) {
    return _FlarkLiveRenderedTextState(
      displayText: displayText,
      renderPlan: renderPlan,
      hasRenderPlan: hasRenderPlan,
      segments: hasRenderPlan
          ? _LiveRenderedTextSegment.buildSegments(
              displayText: displayText,
              renderPlan: renderPlan,
            )
          : const [],
    );
  }

  static final empty = _FlarkLiveRenderedTextState(
    displayText: '',
    renderPlan: FlarkRenderPlan(blocks: const []),
    hasRenderPlan: false,
    segments: const [],
  );

  final String displayText;
  final FlarkRenderPlan renderPlan;
  final bool hasRenderPlan;
  final List<_LiveRenderedTextSegment> segments;
}

final class _LiveRenderedTextSegment {
  const _LiveRenderedTextSegment({
    required this.start,
    required this.end,
    required this.signature,
  });

  final int start;
  final int end;
  final _LiveRenderedTextStyleSignature signature;

  static List<_LiveRenderedTextSegment> buildSegments({
    required String displayText,
    required FlarkRenderPlan renderPlan,
  }) {
    if (displayText.isEmpty) return const [];

    final boundaries = <int>{0, displayText.length};
    final blocks = renderPlan.allBlocks
        .where(
          (block) => !_isCollapsedOrOutside(block.displayRange, displayText),
        )
        .toList();
    final runs = renderPlan.allInlineRuns
        .where((run) => !_isCollapsedOrOutside(run.displayRange, displayText))
        .toList();

    for (final block in blocks) {
      boundaries
        ..add(block.displayRange.start.clamp(0, displayText.length))
        ..add(block.displayRange.end.clamp(0, displayText.length));
    }
    for (final run in runs) {
      boundaries
        ..add(run.displayRange.start.clamp(0, displayText.length))
        ..add(run.displayRange.end.clamp(0, displayText.length));
    }

    final sortedBoundaries = boundaries.toList()..sort();
    final segments = <_LiveRenderedTextSegment>[];
    for (var index = 0; index < sortedBoundaries.length - 1; index++) {
      final start = sortedBoundaries[index];
      final end = sortedBoundaries[index + 1];
      if (start >= end) continue;
      final signature = _LiveRenderedTextStyleSignature.forRange(
        start,
        end,
        blocks: blocks,
        runs: runs,
      );
      if (segments.isNotEmpty && segments.last.signature == signature) {
        final previous = segments.removeLast();
        segments.add(
          _LiveRenderedTextSegment(
            start: previous.start,
            end: end,
            signature: signature,
          ),
        );
      } else {
        segments.add(
          _LiveRenderedTextSegment(
            start: start,
            end: end,
            signature: signature,
          ),
        );
      }
    }

    return List.unmodifiable(segments);
  }

  static bool _isCollapsedOrOutside(
    FlarkSourceRange range,
    String displayText,
  ) {
    return range.isCollapsed ||
        range.start >= displayText.length ||
        range.end <= 0;
  }
}

final class _LiveRenderedTextStyleSignature {
  const _LiveRenderedTextStyleSignature({
    this.headingLevel,
    this.codeBlock = false,
    this.blockquote = false,
    this.strong = false,
    this.emphasis = false,
    this.inlineCode = false,
    this.strikethrough = false,
    this.link = false,
  });

  final int? headingLevel;
  final bool codeBlock;
  final bool blockquote;
  final bool strong;
  final bool emphasis;
  final bool inlineCode;
  final bool strikethrough;
  final bool link;

  static _LiveRenderedTextStyleSignature forRange(
    int start,
    int end, {
    required List<FlarkRenderBlock> blocks,
    required List<FlarkRenderInlineRun> runs,
  }) {
    int? headingLevel;
    var codeBlock = false;
    var blockquote = false;
    for (final block in blocks) {
      if (!_covers(block.displayRange, start, end)) continue;
      if (block.codeBlock != null) codeBlock = true;
      if (block.kind == FlarkMarkdownBlockKind.blockquote) {
        blockquote = true;
      }
      headingLevel ??= _headingLevel(block.styleToken);
    }

    var strong = false;
    var emphasis = false;
    var inlineCode = false;
    var strikethrough = false;
    var link = false;
    for (final run in runs) {
      if (!_covers(run.displayRange, start, end)) continue;
      switch (run.styleToken) {
        case FlarkRenderTextStyleToken.strong:
          strong = true;
        case FlarkRenderTextStyleToken.emphasis:
          emphasis = true;
        case FlarkRenderTextStyleToken.inlineCode:
          inlineCode = true;
        case FlarkRenderTextStyleToken.strikethrough:
          strikethrough = true;
        case FlarkRenderTextStyleToken.link:
          link = true;
        case FlarkRenderTextStyleToken.body:
        case FlarkRenderTextStyleToken.heading1:
        case FlarkRenderTextStyleToken.heading2:
        case FlarkRenderTextStyleToken.heading3:
        case FlarkRenderTextStyleToken.heading4:
        case FlarkRenderTextStyleToken.heading5:
        case FlarkRenderTextStyleToken.heading6:
        case FlarkRenderTextStyleToken.image:
        case FlarkRenderTextStyleToken.rawHtml:
        case FlarkRenderTextStyleToken.unknown:
          break;
      }
    }

    return _LiveRenderedTextStyleSignature(
      headingLevel: headingLevel,
      codeBlock: codeBlock,
      blockquote: blockquote,
      strong: strong,
      emphasis: emphasis,
      inlineCode: inlineCode,
      strikethrough: strikethrough,
      link: link,
    );
  }

  TextStyle resolve(TextStyle baseStyle) {
    var style = baseStyle;
    if (codeBlock) {
      style = style.copyWith(
        color: const Color(0xFF17202A),
        fontFamily: 'monospace',
        height: 1.35,
      );
    } else if (blockquote) {
      style = style.copyWith(color: const Color(0xFF42526E));
    }

    if (headingLevel != null) {
      final baseSize = baseStyle.fontSize ?? 14;
      style = style.copyWith(
        fontSize: baseSize + (7 - headingLevel!) * 2,
        fontWeight: FontWeight.w700,
      );
    }
    if (strong) style = style.copyWith(fontWeight: FontWeight.w700);
    if (emphasis) style = style.copyWith(fontStyle: FontStyle.italic);
    if (inlineCode) {
      style = style.copyWith(
        fontFamily: 'monospace',
        backgroundColor: const Color(0xFFEFF3F7),
      );
    }
    if (strikethrough) {
      style = style.copyWith(decoration: TextDecoration.lineThrough);
    }
    if (link) {
      style = style.copyWith(
        color: const Color(0xFF0057B8),
        decoration: TextDecoration.underline,
      );
    }
    return style;
  }

  @override
  bool operator ==(Object other) {
    return other is _LiveRenderedTextStyleSignature &&
        other.headingLevel == headingLevel &&
        other.codeBlock == codeBlock &&
        other.blockquote == blockquote &&
        other.strong == strong &&
        other.emphasis == emphasis &&
        other.inlineCode == inlineCode &&
        other.strikethrough == strikethrough &&
        other.link == link;
  }

  @override
  int get hashCode {
    return Object.hash(
      headingLevel,
      codeBlock,
      blockquote,
      strong,
      emphasis,
      inlineCode,
      strikethrough,
      link,
    );
  }

  static bool _covers(FlarkSourceRange range, int start, int end) {
    return range.start <= start && range.end >= end;
  }

  static int? _headingLevel(FlarkRenderTextStyleToken token) {
    return switch (token) {
      FlarkRenderTextStyleToken.heading1 => 1,
      FlarkRenderTextStyleToken.heading2 => 2,
      FlarkRenderTextStyleToken.heading3 => 3,
      FlarkRenderTextStyleToken.heading4 => 4,
      FlarkRenderTextStyleToken.heading5 => 5,
      FlarkRenderTextStyleToken.heading6 => 6,
      _ => null,
    };
  }
}

final class _FlarkLiveRenderedEditableChrome extends StatelessWidget {
  const _FlarkLiveRenderedEditableChrome({
    required this.textController,
    required this.scrollController,
    required this.renderPlan,
    required this.displayText,
    required this.hasRenderPlan,
    required this.style,
    required this.child,
  });

  final TextEditingController textController;
  final ScrollController scrollController;
  final FlarkRenderPlan renderPlan;
  final String displayText;
  final bool hasRenderPlan;
  final TextStyle style;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final textSpan = textController.buildTextSpan(
      context: context,
      style: style,
      withComposing: false,
    );
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Positioned.fill(
          child: IgnorePointer(
            child: RepaintBoundary(
              child: CustomPaint(
                key: const Key('FlarkLiveRenderedEditableChrome'),
                painter: _FlarkLiveRenderedBlockPainter(
                  renderPlan: renderPlan,
                  displayText: displayText,
                  textSpan: textSpan,
                  textDirection: Directionality.of(context),
                  textScaler: MediaQuery.textScalerOf(context),
                  scrollController: scrollController,
                  hasRenderPlan: hasRenderPlan,
                ),
              ),
            ),
          ),
        ),
        child,
      ],
    );
  }
}

final class _FlarkLiveRenderedBlockPainter extends CustomPainter {
  _FlarkLiveRenderedBlockPainter({
    required this.renderPlan,
    required this.displayText,
    required this.textSpan,
    required this.textDirection,
    required this.textScaler,
    required this.scrollController,
    required this.hasRenderPlan,
  }) : super(repaint: scrollController);

  final FlarkRenderPlan renderPlan;
  final String displayText;
  final TextSpan textSpan;
  final TextDirection textDirection;
  final TextScaler textScaler;
  final ScrollController scrollController;
  final bool hasRenderPlan;

  @override
  void paint(Canvas canvas, Size size) {
    if (!hasRenderPlan || displayText.isEmpty || size.isEmpty) {
      return;
    }

    final textPainter = TextPainter(
      text: textSpan,
      textDirection: textDirection,
      textScaler: textScaler,
    )..layout(maxWidth: size.width);

    canvas.save();
    if (scrollController.hasClients) {
      canvas.translate(0, -scrollController.offset);
    }

    for (final block in renderPlan.allBlocks) {
      if (block.kind == FlarkMarkdownBlockKind.blockquote) {
        _paintBlockquote(canvas, size, textPainter, block);
      }
    }
    for (final block in renderPlan.codeBlocks) {
      _paintCodeBlock(canvas, size, textPainter, block);
    }

    canvas.restore();
  }

  void _paintBlockquote(
    Canvas canvas,
    Size size,
    TextPainter textPainter,
    FlarkRenderBlock block,
  ) {
    final rect = _rectForBlock(textPainter, block.displayRange, size.width);
    if (rect == null) return;
    final expanded = Rect.fromLTRB(
      0,
      rect.top - 5,
      size.width,
      rect.bottom + 5,
    );
    final background = Paint()..color = const Color(0xFFF8FAFC);
    canvas.drawRect(expanded, background);
    final rail = Paint()..color = const Color(0xFF7A8CA3);
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(0, expanded.top, 3, expanded.bottom),
        const Radius.circular(2),
      ),
      rail,
    );
  }

  void _paintCodeBlock(
    Canvas canvas,
    Size size,
    TextPainter textPainter,
    FlarkRenderBlock block,
  ) {
    final rect = _rectForBlock(textPainter, block.displayRange, size.width);
    if (rect == null) return;
    final expanded = Rect.fromLTRB(
      0,
      rect.top - 6,
      size.width,
      rect.bottom + 6,
    );
    final background = Paint()..color = const Color(0xFFF1F4F8);
    final border = Paint()
      ..color = const Color(0xFFD7DEE8)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    final shape = RRect.fromRectAndRadius(expanded, const Radius.circular(6));
    canvas.drawRRect(shape, background);
    canvas.drawRRect(shape, border);
  }

  Rect? _rectForBlock(
    TextPainter textPainter,
    FlarkSourceRange range,
    double width,
  ) {
    final start = range.start.clamp(0, displayText.length);
    final end = range.end.clamp(0, displayText.length);
    if (start >= end) return null;

    final boxes = textPainter.getBoxesForSelection(
      TextSelection(baseOffset: start, extentOffset: end),
      boxHeightStyle: BoxHeightStyle.max,
    );
    if (boxes.isEmpty) return null;

    var top = boxes.first.top;
    var bottom = boxes.first.bottom;
    for (final box in boxes.skip(1)) {
      if (box.top < top) top = box.top;
      if (box.bottom > bottom) bottom = box.bottom;
    }
    return Rect.fromLTRB(0, top, width, bottom);
  }

  @override
  bool shouldRepaint(_FlarkLiveRenderedBlockPainter oldDelegate) {
    return oldDelegate.renderPlan != renderPlan ||
        oldDelegate.displayText != displayText ||
        oldDelegate.textSpan != textSpan ||
        oldDelegate.textDirection != textDirection ||
        oldDelegate.textScaler != textScaler ||
        oldDelegate.hasRenderPlan != hasRenderPlan ||
        oldDelegate.scrollController != scrollController;
  }
}
