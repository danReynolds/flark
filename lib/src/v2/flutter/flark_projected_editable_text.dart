import 'dart:async';
import 'dart:ui' show BoxHeightStyle;

import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart' show RenderEditable;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_policy.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../render_plan/render_plan.dart';
import 'flark_command_actions.dart';
import 'flark_code_syntax_highlighting.dart';
import 'flark_editor_read_only_scope.dart';
import 'flark_markdown_theme.dart';
import 'flark_flutter_controller.dart';
import 'flark_link_popover.dart';
import 'flark_live_block_source_edit.dart';
import 'flark_live_edit_classifier.dart';
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
    this.shortcuts = const <ShortcutActivator, Intent>{},
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
  final Map<ShortcutActivator, Intent> shortcuts;

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
    this.shortcuts = const <ShortcutActivator, Intent>{},
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
  final Map<ShortcutActivator, Intent> shortcuts;

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

/// Shared inline-link popover behavior for the two editable surfaces (the
/// whole-document projected host and the per-block text). While the collapsed
/// caret rests inside a link, a [FlarkLinkPopover] floats just beneath the
/// surface through an [OverlayPortal] anchored with a [LayerLink]. Pure
/// rebuild-driven: it re-evaluates on every build and the portal follows the
/// caret in/out of links.
mixin _InlineLinkPopoverHost<T extends StatefulWidget> on State<T> {
  final LayerLink _linkPopoverLink = LayerLink();
  final OverlayPortalController _linkOverlayController =
      OverlayPortalController();
  FlarkLinkActionContext? _linkAtCaret;

  /// The document controller for this surface.
  FlarkFlutterController get linkPopoverController;

  /// The source range this surface edits, scoping detection to its own block;
  /// null for the whole-document host.
  FlarkSourceRange? get linkPopoverBlockRange;

  /// This surface's render editable, used to anchor the popover to the link's
  /// on-screen position. Null falls back to anchoring beneath the surface.
  RenderEditable? get linkPopoverRenderEditable;

  Widget wrapWithLinkPopover(BuildContext context, Widget child) {
    final interactions = FlarkMarkdownInteractions.maybeOf(context);
    _linkAtCaret = _resolveLinkAtCaret(context, interactions);
    _syncLinkPopoverVisibility(_linkAtCaret != null);
    if (interactions == null) return child;
    return CompositedTransformTarget(
      link: _linkPopoverLink,
      child: OverlayPortal(
        controller: _linkOverlayController,
        overlayChildBuilder: _buildLinkPopoverOverlay,
        child: child,
      ),
    );
  }

  FlarkLinkActionContext? _resolveLinkAtCaret(
    BuildContext context,
    FlarkMarkdownInteractions? interactions,
  ) {
    if (interactions == null || !interactions.config.enableLinkMenus) {
      return null;
    }
    final controller = linkPopoverController;
    final selection = controller.selection;
    if (!selection.isCollapsed) return null;
    final caret = selection.extentOffset;
    final blockRange = linkPopoverBlockRange;
    if (blockRange != null &&
        (caret < blockRange.start || caret > blockRange.end)) {
      return null;
    }
    final link = FlarkMarkdownLinkCommands.resolveLinkEditContext(
      controller.state,
    );
    if (!link.isExisting || link.url.isEmpty) return null;
    if (blockRange != null &&
        (link.replaceRange.start < blockRange.start ||
            link.replaceRange.end > blockRange.end)) {
      return null;
    }
    return FlarkLinkActionContext(
      context: context,
      url: link.url,
      label: link.label,
      range: link.replaceRange,
      controller: controller,
      interactions: interactions,
      dismiss: _hideLinkPopover,
    );
  }

  void _syncLinkPopoverVisibility(bool shouldShow) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final showing = _linkOverlayController.isShowing;
      if (shouldShow && !showing) {
        _linkOverlayController.show();
      } else if (!shouldShow && showing) {
        _linkOverlayController.hide();
      }
    });
  }

  void _hideLinkPopover() {
    if (_linkOverlayController.isShowing) _linkOverlayController.hide();
  }

  Widget _buildLinkPopoverOverlay(BuildContext overlayContext) {
    final link = _linkAtCaret;
    if (link == null) return const SizedBox.shrink();
    // Read theme/text style from this surface's context — the Overlay sits
    // above the editor's theme scope and would otherwise lose the theme.
    final theme = FlarkMarkdownTheme.of(context);
    final textStyle = DefaultTextStyle.of(context).style;
    final actions =
        link.interactions.config.linkActions ?? FlarkLinkAction.defaults;
    // Anchor just beneath the link's rendered position when we can measure it;
    // otherwise fall back to beneath the surface.
    final linkOffset = _linkAnchorOffset(link);
    return CompositedTransformFollower(
      link: _linkPopoverLink,
      showWhenUnlinked: false,
      targetAnchor: Alignment.topLeft,
      followerAnchor: Alignment.topLeft,
      offset:
          linkOffset == null
              ? const Offset(0, 6)
              : linkOffset + const Offset(0, 4),
      child: UnconstrainedBox(
        alignment: Alignment.topLeft,
        child: FlarkMarkdownTheme(
          data: theme,
          child: DefaultTextStyle(
            style: textStyle,
            child: FlarkLinkPopover(link: link, actions: actions),
          ),
        ),
      ),
    );
  }

  /// The link's bottom-left, in this surface's local coordinates (the popover
  /// anchor is the surface's top-left). Null when the surface can't be measured
  /// yet, in which case the caller falls back to a surface-relative anchor.
  Offset? _linkAnchorOffset(FlarkLinkActionContext link) {
    final renderEditable = linkPopoverRenderEditable;
    if (renderEditable == null || !renderEditable.attached) return null;
    final surfaceBox = context.findRenderObject();
    if (surfaceBox is! RenderBox || !surfaceBox.attached) return null;
    final projection = link.controller.projection;
    final start = link.range.start;
    if (start < 0 || start > projection.textLength) return null;
    final display = projection
        .sourceToDisplayOffset(start)
        .clamp(0, projection.displayLength);
    final caretRect = renderEditable.getLocalRectForCaret(
      TextPosition(offset: display),
    );
    final global = renderEditable.localToGlobal(caretRect.bottomLeft);
    return surfaceBox.globalToLocal(global);
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
    this.shortcuts = const <ShortcutActivator, Intent>{},
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
  final Map<ShortcutActivator, Intent> shortcuts;
  final bool liveRendered;

  @override
  State<_FlarkProjectedEditableHost> createState() {
    return _FlarkProjectedEditableHostState();
  }
}

final class _FlarkProjectedEditableHostState
    extends State<_FlarkProjectedEditableHost>
    with _InlineLinkPopoverHost<_FlarkProjectedEditableHost> {
  late final TextEditingController _textController;
  late final ScrollController _scrollController;
  final _editableStateKey = GlobalKey<EditableTextState>();
  FocusNode? _ownedFocusNode;

  @override
  FlarkFlutterController get linkPopoverController => widget.controller;

  // The host edits the whole document, so detection is not block-scoped.
  @override
  FlarkSourceRange? get linkPopoverBlockRange => null;

  @override
  RenderEditable? get linkPopoverRenderEditable =>
      _editableStateKey.currentState?.renderEditable;
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
      readOnly: FlarkEditorReadOnlyScope.of(context),
      style: style,
      cursorColor: widget.cursorColor,
      selectionColor:
          FlarkMarkdownTheme.of(context).selectionColor ??
          _selectionColorForCursor(widget.cursorColor),
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
    editor = Focus(
      canRequestFocus: false,
      skipTraversal: true,
      onKeyEvent: _handleInlineRunBoundaryKeyEvent,
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
    editor = wrapWithLinkPopover(context, editor);
    if (widget.shortcuts.isNotEmpty) {
      editor = Shortcuts(
        shortcuts: <ShortcutActivator, Intent>{...widget.shortcuts},
        child: editor,
      );
    }
    return editor;
  }

  /// Steps the caret between the two source positions that share a styled
  /// run's trailing display edge (inside the run vs after its closing
  /// marker). The first arrow press at the edge changes which side typing
  /// lands on without moving the caret visually; the next press moves on.
  KeyEventResult _handleInlineRunBoundaryKeyEvent(
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
    final bool forward;
    if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
      forward = true;
    } else if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
      forward = false;
    } else {
      return KeyEventResult.ignored;
    }
    return flarkStepInlineRunBoundary(widget.controller, forward: forward)
        ? KeyEventResult.handled
        : KeyEventResult.ignored;
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

    final classification = classifyFlarkHostEdit(
      FlarkHostEditContext(
        markdown: widget.controller.markdown,
        oldDisplayText: _projectedText(),
        oldDisplaySelection: widget.controller.projection
            .sourceSelectionToDisplay(widget.controller.selection),
        newValue: _textController.value,
        liveRendered: widget.liveRendered,
      ),
    );
    final value = classification.normalizedValue;
    if (_textController.value != value) {
      _syncingFromRuntime = true;
      _textController.value = value;
      _syncingFromRuntime = false;
    }
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    _executeHostIntent(classification.intent, compositionUndoGroupId);
    _compositionUndoGrouping.clearIfCommitted(value);
  }

  void _executeHostIntent(
    FlarkHostEditIntent intent,
    int? compositionUndoGroupId,
  ) {
    switch (intent) {
      case FlarkHostWholeDocumentReplaceIntent(:final replacementMarkdown):
        _replaceSourceRange(
          controller: widget.controller,
          range: FlarkSourceRange(0, widget.controller.markdown.length),
          replacementText: replacementMarkdown,
          selectionAfter: FlarkSelection.collapsed(replacementMarkdown.length),
          userEvent: 'input.liveRendered.codeFenceAutoCloseEcho',
          undoGroupId: compositionUndoGroupId,
        );
        _adoptImmediateMarkdownParse();
        _syncFromRuntime();
      case FlarkHostPlatformTextChangeIntent():
        if (_markdownInputPolicy.handlePlatformTextChange(
          oldText: intent.oldText,
          newValue: intent.policyValue,
          oldTextSelection: intent.oldTextSelection,
          applyOldTextSelection: _applyProjectedSelection,
        )) {
          return;
        }
        _executeHostIntent(intent.fallback, compositionUndoGroupId);
      case FlarkHostProjectedEditIntent():
        final applied = widget.controller.applyProjectedTextEdit(
          oldDisplayText: intent.oldDisplayText,
          newDisplayText: intent.newDisplayText,
          undoGroupId: compositionUndoGroupId,
        );
        if (!applied) {
          _syncFromRuntime();
        } else if (intent.immediateParseAfterApply ||
            widget.controller.lastEditRequestsImmediateParse) {
          _adoptImmediateMarkdownParse();
        }
      case FlarkHostProjectedSelectionIntent(:final selection):
        widget.controller.applyProjectedSelection(selection);
      case FlarkHostIgnoreIntent():
        break;
    }
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
    // This applier anchors edits and command dispatches, so when the
    // controller's source selection already renders at the requested
    // display position, keep it: the source caret distinguishes inside
    // vs outside a styled run's hidden closing marker, and a display
    // round trip would erase that.
    final controller = widget.controller;
    if (controller.projection.sourceSelectionToDisplay(controller.selection) ==
        displaySelection) {
      return;
    }
    // No explicit affinity: collapsed carets use the controller's
    // caret-placement mapping, which keeps a caret at the trailing edge of
    // an inline styled run inside the run.
    controller.applyProjectedSelection(displaySelection);
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
}

/// Applies the inline-run boundary caret step for a plain horizontal arrow
/// key, if the collapsed caret sits on one of the two source positions at a
/// styled run's trailing display edge. Returns whether the key was handled.
bool flarkStepInlineRunBoundary(
  FlarkFlutterController controller, {
  required bool forward,
}) {
  final selection = controller.selection;
  if (!selection.isCollapsed) return false;
  final projection = controller.projection;
  final offset = selection.extentOffset;
  if (offset < 0 || offset > projection.textLength) return false;
  final stepped = projection.inlineRunBoundaryStep(offset, forward: forward);
  if (stepped == null) return false;
  controller.applySelection(
    FlarkSelection.collapsed(stepped),
    userEvent: 'selection.inlineRunBoundaryStep',
  );
  return true;
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
