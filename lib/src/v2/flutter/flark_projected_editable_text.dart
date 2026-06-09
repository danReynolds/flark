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
import 'flark_live_block_source_edit.dart';
import 'flark_live_block_reconciler.dart';
import 'flark_live_block_signature.dart';
import 'flark_live_code_fence_input_policy.dart';
import 'flark_markdown_input_policy.dart';
import 'flark_markdown_interactions.dart';
import 'flark_text_selection_gestures.dart';

part 'projected_editable/live_block_editor.dart';
part 'projected_editable/live_block_text.dart';
part 'projected_editable/live_block_widgets.dart';
part 'projected_editable/live_text_rendering.dart';

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
    final hasRenderPlan = widget.controller.hasUsableRenderPlan;
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

    final rawValue = _textController.value;
    final autoClosedWholeValueFenceText = widget.liveRendered
        ? FlarkLiveCodeFenceInputPolicy.displayTextAfterAutoClosedWholeValueEcho(
            rawValue,
          )
        : null;
    if (autoClosedWholeValueFenceText != null) {
      final normalizedValue = rawValue.copyWith(
        text: autoClosedWholeValueFenceText,
        selection: TextSelection.collapsed(
          offset: autoClosedWholeValueFenceText.length,
          affinity: rawValue.selection.affinity,
        ),
        composing: TextRange.empty,
      );
      if (_textController.value != normalizedValue) {
        _syncingFromRuntime = true;
        _textController.value = normalizedValue;
        _syncingFromRuntime = false;
      }
      final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(
        normalizedValue,
      );
      _replaceSourceRange(
        controller: widget.controller,
        range: FlarkSourceRange(0, widget.controller.markdown.length),
        replacementText: autoClosedWholeValueFenceText,
        selectionAfter: FlarkSelection.collapsed(
          autoClosedWholeValueFenceText.length,
        ),
        userEvent: 'input.liveRendered.codeFenceAutoCloseEcho',
        undoGroupId: compositionUndoGroupId,
      );
      _adoptImmediateMarkdownParse();
      _syncFromRuntime();
      _compositionUndoGrouping.clearIfCommitted(normalizedValue);
      return;
    }

    final oldDisplayText = _projectedText();
    final oldDisplaySelection = widget.controller.projection
        .sourceSelectionToDisplay(widget.controller.selection);
    final value = _textValueWithPureInsertionSelection(
      oldText: oldDisplayText,
      oldSelection: _textSelection(oldDisplaySelection),
      newValue: _textController.value,
      normalizeAutoClosedFenceEcho: widget.liveRendered,
    );
    if (_textController.value != value) {
      _syncingFromRuntime = true;
      _textController.value = value;
      _syncingFromRuntime = false;
    }
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    final autoClosedStandaloneFenceMarkdown = widget.liveRendered
        ? FlarkLiveCodeFenceInputPolicy.markdownAfterAutoClosedStandaloneEcho(
            oldMarkdown: widget.controller.markdown,
            newValue: value,
          )
        : null;
    if (autoClosedStandaloneFenceMarkdown != null) {
      _replaceSourceRange(
        controller: widget.controller,
        range: FlarkSourceRange(0, widget.controller.markdown.length),
        replacementText: autoClosedStandaloneFenceMarkdown,
        selectionAfter: FlarkSelection.collapsed(
          autoClosedStandaloneFenceMarkdown.length,
        ),
        userEvent: 'input.liveRendered.codeFenceAutoCloseEcho',
        undoGroupId: compositionUndoGroupId,
      );
      _adoptImmediateMarkdownParse();
      _syncFromRuntime();
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    if (value.text != oldDisplayText) {
      final completedCodeFenceText = widget.liveRendered
          ? FlarkLiveCodeFenceInputPolicy.displayTextAfterCompletingStandaloneOpener(
              oldDisplayText: oldDisplayText,
              oldSelection: _textSelection(oldDisplaySelection),
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
