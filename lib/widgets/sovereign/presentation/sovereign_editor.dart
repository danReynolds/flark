import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:sovereign_editor/theme/dune_markdown_theme.dart';

import '../controllers/sovereign_controller.dart';
import '../logic/fenced_code_scanner.dart';
import '../logic/sovereign_code_highlighter.dart';
import '../logic/sovereign_style_scanner.dart';
import '../models/decoration_model.dart';
import '../models/geometry_model.dart';
import '../models/block_node.dart';
import '../models/block_tree.dart';
import '../theme/sovereign_editor_theme.dart';
import '../core/rendering/editor_heading_style_policy.dart';
import 'sovereign_inline_actions_overlay.dart';
import 'painters/tier1_painter.dart';
part 'sovereign_editor_inline_actions_overlay.dart';
part 'sovereign_editor_inline_actions_overlay_entry.dart';
part 'sovereign_editor_task_checkbox_overlay.dart';

const Key _kCodeFenceLanguagePickerKey = Key(
  'SovereignCodeFenceLanguagePicker',
);
const Key _kTaskCheckboxTapTargetKey = Key('SovereignTaskCheckboxTapTarget');
const Key _kTaskCheckboxVisualKey = Key('SovereignTaskCheckboxVisual');

class SovereignEditor extends StatefulWidget {
  final SovereignController controller;
  final bool autofocus;
  final FocusNode? focusNode;
  final bool enableTestShortcuts;
  final bool wrapText;
  final bool showCursorForExpandedSelection;
  final TextStyle? textStyle;
  final Color? cursorColor;
  final ScrollController? scrollController;
  final SovereignEditorThemeData? theme;
  final Future<void> Function(String url)? onOpenLink;
  final Future<({String label, String url})?> Function(
    BuildContext context,
    String initialLabel,
    String initialUrl,
    bool isImage,
  )? onEditInlineTarget;
  final Widget Function(
    BuildContext context, {
    required String? url,
    required bool isImage,
    required VoidCallback? onOpen,
    required VoidCallback? onCopy,
    required VoidCallback onEdit,
  })? inlineToolbarBuilder;
  final bool showLinkActionsOverlay;

  const SovereignEditor({
    super.key,
    required this.controller,
    this.autofocus = false,
    this.focusNode,
    this.enableTestShortcuts = false,
    this.wrapText = false,
    this.showCursorForExpandedSelection = false,
    this.textStyle,
    this.cursorColor,
    this.scrollController,
    this.theme,
    this.onOpenLink,
    this.onEditInlineTarget,
    this.inlineToolbarBuilder,
    this.showLinkActionsOverlay = true,
  });

  @override
  State<SovereignEditor> createState() => _SovereignEditorState();
}

class _SovereignEditorState extends State<SovereignEditor> {
  // Tier 1 Constraints
  static const double kFontSize = 15.0;
  static const double kLineHeightMultiplier = 1.6;
  static const List<_FenceLanguageOption> _fenceLanguageOptions = [
    _FenceLanguageOption('Plain', 'plain'),
    _FenceLanguageOption('Dart', 'dart'),
    _FenceLanguageOption('JSON', 'json'),
    _FenceLanguageOption('YAML', 'yaml'),
    _FenceLanguageOption('Bash', 'bash'),
    _FenceLanguageOption('Python', 'python'),
    _FenceLanguageOption('JavaScript', 'javascript'),
    _FenceLanguageOption('TypeScript', 'typescript'),
    _FenceLanguageOption('HTML', 'html'),
    _FenceLanguageOption('XML', 'xml'),
    _FenceLanguageOption('CSS', 'css'),
    _FenceLanguageOption('SQL', 'sql'),
    _FenceLanguageOption('Markdown', 'markdown'),
  ];

  late double _lineHeightPixels;
  late double _charWidth;
  late TextStyle _style;
  late StrutStyle _strutStyle;

  final FocusNode _fallbackFocusNode = FocusNode();
  final GlobalKey _textFieldKey = GlobalKey(debugLabel: 'SovereignTextField');
  final GlobalKey _overlayStackKey = GlobalKey(
    debugLabel: 'SovereignOverlayStack',
  );
  final GlobalKey _editorLayersKey = GlobalKey(
    debugLabel: 'SovereignEditorLayers',
  );
  late final ScrollController _scrollController =
      widget.scrollController ?? ScrollController();
  final ValueNotifier<Offset> _viewportNotifier = ValueNotifier(Offset.zero);
  OverlayEntry? _linkActionsOverlayEntry;
  _LinkActionsOverlayData? _linkActionsOverlayData;
  bool _linkActionsOverlaySyncScheduled = false;
  bool _taskCheckboxTargetsRefreshScheduled = false;
  bool _taskCheckboxTargetsTemporarilyHidden = false;
  String _lastTaskCheckboxTargetTextSnapshot = '';
  String _lastTaskCheckboxTargetSignature = '';
  List<_TaskCheckboxTapTargetData> _cachedTaskCheckboxTargets =
      const <_TaskCheckboxTapTargetData>[];

  FocusNode get _effectiveFocusNode => widget.focusNode ?? _fallbackFocusNode;
  SovereignEditorThemeData get _editorThemeData =>
      widget.theme ?? const SovereignEditorThemeData();

  @override
  void initState() {
    super.initState();
    _configureTypography();
    _lastTaskCheckboxTargetTextSnapshot = widget.controller.text;
    _lastTaskCheckboxTargetSignature = _taskCheckboxTargetSignature();

    _scrollController.addListener(_updateViewport);
    _attachLinkOverlayListeners();
  }

  @override
  void didUpdateWidget(covariant SovereignEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.textStyle != widget.textStyle ||
        oldWidget.theme != widget.theme) {
      _configureTypography();
    }
    if (oldWidget.controller != widget.controller ||
        oldWidget.focusNode != widget.focusNode) {
      _detachLinkOverlayListeners(
        controller: oldWidget.controller,
        focusNode: oldWidget.focusNode ?? _fallbackFocusNode,
      );
      _attachLinkOverlayListeners();
      _lastTaskCheckboxTargetTextSnapshot = widget.controller.text;
      _lastTaskCheckboxTargetSignature = _taskCheckboxTargetSignature();
    }
    _scheduleLinkActionsOverlaySync();
  }

  void _configureTypography() {
    final themeTextStyle = widget.theme?.textStyle;
    _style = widget.textStyle ??
        themeTextStyle ??
        GoogleFonts.sourceCodePro(
          fontSize: kFontSize,
          height: kLineHeightMultiplier,
          color: Colors.white,
        );

    final tp = TextPainter(
      text: TextSpan(text: 'M', style: _style),
      textDirection: TextDirection.ltr,
    )..layout();

    _lineHeightPixels = tp.height;
    _charWidth = tp.width;

    final resolvedFontSize = _style.fontSize ?? kFontSize;
    final resolvedHeight = _style.height ?? kLineHeightMultiplier;
    _strutStyle = StrutStyle(
      fontSize: resolvedFontSize,
      height: resolvedHeight,
      forceStrutHeight: true,
      fontFamily: _style.fontFamily ?? GoogleFonts.sourceCodePro().fontFamily,
    );
  }

  void _updateViewport() {
    // Only update if changed significant amount to avoid notification spam?
    // Actually, for smooth culling, we need it.
    if (_scrollController.hasClients) {
      _viewportNotifier.value = Offset(0, _scrollController.offset);
    }
  }

  @override
  void dispose() {
    _detachLinkOverlayListeners();
    _removeLinkActionsOverlayEntry();
    _scrollController.removeListener(_updateViewport);
    if (widget.scrollController == null) {
      _scrollController.dispose();
    }
    _viewportNotifier.dispose();
    _fallbackFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _scheduleLinkActionsOverlaySync();
    final scopedTheme = widget.theme ?? const SovereignEditorThemeData();
    return SovereignEditorThemeScope(
      data: scopedTheme,
      child: Builder(
        builder: (context) {
          final markdownTheme = SovereignEditorThemeScope.of(
            context,
          ).resolveMarkdownTheme(context);

          return StreamBuilder<DecorationModel>(
            stream: widget.controller.decorationStream,
            initialData: widget.controller.decoration,
            builder: (context, snapshot) {
              final decoration = snapshot.data ?? DecorationModel.empty();
              final maxCol = decoration.lineIndex.maxColumn;

              // Calculate required width to prevent wrapping
              final contentWidth = (maxCol * _charWidth) + 100.0; // Buffer

              return LayoutBuilder(
                builder: (context, constraints) {
                  final minWidth = constraints.maxWidth;
                  final shouldWrap = widget.wrapText;
                  // Ensure we match constraint if content is small, but expand if large.
                  final finalWidth = shouldWrap
                      ? minWidth
                      : (contentWidth > minWidth ? contentWidth : minWidth);

                  // Check if we need horizontal scroll behavior.
                  final needsHorizontalScroll =
                      !shouldWrap && contentWidth > minWidth;

                  final editorLayers = SizedBox(
                    width: finalWidth,
                    child: Stack(
                      key: _editorLayersKey,
                      children: [
                        // Layer 1: Painting (Behind)
                        Positioned.fill(
                          child: RepaintBoundary(
                            child: ListenableBuilder(
                              // Consolidate triggers: Viewport OR Controller (Sync text) OR Decoration (Async structure)
                              // We use Listenable.merge if possible, or nested builders.
                              // Since _viewportNotifier is distinct, we can nest or merge.
                              // Controller notifies on sync text changes (for shifting blocks).
                              listenable: Listenable.merge([
                                _viewportNotifier,
                                widget.controller,
                              ]),
                              builder: (context, _) {
                                final editorTheme =
                                    SovereignEditorThemeScope.of(context);
                                return CustomPaint(
                                  painter: Tier1Painter(
                                    geometry: widget.controller
                                        .geometry, // RFC 007: Sync Geometry
                                    lineHeight: (_style.height ??
                                            kLineHeightMultiplier) *
                                        (_style.fontSize ?? kFontSize),
                                    viewport: _viewportNotifier.value,
                                    contentPadding:
                                        _editorThemeData.editorContentPadding,
                                    codeBlockBackgroundColor: editorTheme
                                            .codeBlock.backgroundColor ??
                                        markdownTheme.codeBlockBackgroundColor,
                                    codeBlockBorderRadius:
                                        markdownTheme.codeBlockBorderRadius,
                                    codeBlockHorizontalInset: editorTheme
                                        .codeBlock.backgroundHorizontalInset,
                                    codeBlockVerticalInset: editorTheme
                                        .codeBlock.backgroundVerticalInset,
                                    quoteRailColor:
                                        editorTheme.blockquote.railColor ??
                                            markdownTheme.blockquoteBorderColor
                                                .withValues(alpha: 0.92),
                                    quoteRailWidth:
                                        editorTheme.blockquote.railWidth,
                                    quoteRailInset:
                                        editorTheme.blockquote.railInset,
                                    quoteRailRadius:
                                        editorTheme.blockquote.railRadius,
                                  ),
                                );
                              },
                            ),
                          ),
                        ),

                        // Layer 2: Input
                        _buildEditorField(
                          decoration: decoration,
                          markdownTheme: markdownTheme,
                        ),

                        // Layer 3: Task checkbox hit targets
                        _buildTaskCheckboxTapTargets(),
                      ],
                    ),
                  );

                  final scrollContent = SingleChildScrollView(
                    controller: _scrollController,
                    scrollDirection: Axis.vertical,
                    physics: const AlwaysScrollableScrollPhysics(),
                    child: shouldWrap
                        ? editorLayers
                        : SingleChildScrollView(
                            scrollDirection: Axis.horizontal,
                            physics: needsHorizontalScroll
                                ? const AlwaysScrollableScrollPhysics()
                                : const NeverScrollableScrollPhysics(),
                            child: editorLayers,
                          ),
                  );

                  return Listener(
                    behavior: HitTestBehavior.translucent,
                    onPointerDown: (_) {
                      // Allow tapping "empty" space in the editor to focus the field.
                      if (!_effectiveFocusNode.hasFocus) {
                        _effectiveFocusNode.requestFocus();
                      }
                    },
                    child: Stack(
                      key: _overlayStackKey,
                      fit: StackFit.loose,
                      children: [
                        scrollContent,
                        _buildFenceLanguageOverlay(constraints),
                      ],
                    ),
                  );
                },
              );
            },
          );
        },
      ),
    );
  }

  void _attachLinkOverlayListeners() {
    widget.controller.addListener(_handleControllerOverlayChange);
    _effectiveFocusNode.addListener(_handleFocusOrViewportOverlayChange);
    _viewportNotifier.addListener(_handleFocusOrViewportOverlayChange);
  }

  void _detachLinkOverlayListeners({
    SovereignController? controller,
    FocusNode? focusNode,
  }) {
    (controller ?? widget.controller).removeListener(
      _handleControllerOverlayChange,
    );
    (focusNode ?? _effectiveFocusNode).removeListener(
      _handleFocusOrViewportOverlayChange,
    );
    _viewportNotifier.removeListener(_handleFocusOrViewportOverlayChange);
  }

  void _handleControllerOverlayChange() {
    _scheduleLinkActionsOverlaySync();
    final currentText = widget.controller.text;
    final textChanged = currentText != _lastTaskCheckboxTargetTextSnapshot;
    _lastTaskCheckboxTargetTextSnapshot = currentText;
    if (textChanged) {
      final currentSignature = _taskCheckboxTargetSignature();
      final taskTargetTopologyChanged =
          currentSignature != _lastTaskCheckboxTargetSignature;
      _lastTaskCheckboxTargetSignature = currentSignature;
      if (taskTargetTopologyChanged) {
        _hideTaskCheckboxTargetsUntilNextLayout();
      }
    }
    _scheduleTaskCheckboxTargetsRefresh();
  }

  void _handleFocusOrViewportOverlayChange() {
    _scheduleLinkActionsOverlaySync();
    _scheduleTaskCheckboxTargetsRefresh();
  }

  void _scheduleTaskCheckboxTargetsRefresh() {
    if (_taskCheckboxTargetsRefreshScheduled || !mounted) return;
    _taskCheckboxTargetsRefreshScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _taskCheckboxTargetsRefreshScheduled = false;
      if (!mounted) return;
      _taskCheckboxTargetsTemporarilyHidden = false;
      setState(() {});
    });
  }

  void _hideTaskCheckboxTargetsUntilNextLayout() {
    if (_taskCheckboxTargetsTemporarilyHidden || !mounted) return;
    _taskCheckboxTargetsTemporarilyHidden = true;
    if (SchedulerBinding.instance.schedulerPhase ==
        SchedulerPhase.persistentCallbacks) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        setState(() {});
      });
      return;
    }
    setState(() {});
  }

  String _taskCheckboxTargetSignature() {
    final lineIndex = widget.controller.decoration.lineIndex;
    final out = StringBuffer()..write(lineIndex.lineCount);
    for (var line = 0; line < lineIndex.lineCount; line++) {
      final range = widget.controller.taskCheckboxMarkerRangeForLine(line);
      if (range == null) continue;
      out
        ..write('|')
        ..write(line)
        ..write(':')
        ..write(range.start)
        ..write('-')
        ..write(range.end);
    }
    return out.toString();
  }

  Widget _buildEditorField({
    required DecorationModel decoration,
    required DuneMarkdownTheme markdownTheme,
  }) {
    final cursorHeight = _cursorHeightForSelection(
      tree: decoration.tree,
      markdownTheme: markdownTheme,
    );

    Widget field = TextField(
      key: _textFieldKey,
      controller: widget.controller,
      focusNode: _effectiveFocusNode,
      autofocus: widget.autofocus,
      maxLines: null,
      style: _style,
      strutStyle: _strutStyle,
      decoration: InputDecoration(
        border: InputBorder.none,
        contentPadding: _editorThemeData.editorContentPadding,
        isDense: true,
        hoverColor: Colors.transparent,
      ),
      cursorColor:
          widget.cursorColor ?? widget.theme?.cursorColor ?? Colors.blue,
      showCursor: widget.showCursorForExpandedSelection ? true : null,
      cursorHeight: cursorHeight,
      cursorRadius: const Radius.circular(1.0),
      keyboardType: TextInputType.multiline,
      scrollPhysics: const NeverScrollableScrollPhysics(),
    );

    final shortcuts = <ShortcutActivator, Intent>{
      const SingleActivator(LogicalKeyboardKey.arrowDown):
          const _ArrowDownExitFenceIntent(),
      const SingleActivator(LogicalKeyboardKey.arrowUp):
          const _ArrowUpExitFenceIntent(),
      const SingleActivator(LogicalKeyboardKey.enter, shift: true):
          const _InsertLiteralNewlineIntent(),
      const SingleActivator(LogicalKeyboardKey.tab): const _IndentFenceIntent(),
      const SingleActivator(LogicalKeyboardKey.tab, shift: true):
          const _OutdentFenceIntent(),
    };
    final actions = <Type, Action<Intent>>{
      _ArrowDownExitFenceIntent: CallbackAction<_ArrowDownExitFenceIntent>(
        onInvoke: (_) {
          widget.controller.handleArrowDownKey();
          return null;
        },
      ),
      _ArrowUpExitFenceIntent: CallbackAction<_ArrowUpExitFenceIntent>(
        onInvoke: (_) {
          widget.controller.handleArrowUpKey();
          return null;
        },
      ),
      _InsertLiteralNewlineIntent: CallbackAction<_InsertLiteralNewlineIntent>(
        onInvoke: (_) {
          widget.controller.handleEnter(suppressFenceExit: true);
          return null;
        },
      ),
      _IndentFenceIntent: CallbackAction<_IndentFenceIntent>(
        onInvoke: (_) {
          if (widget.controller.value.composing.isValid) {
            return null;
          }
          final handled = widget.controller.handleTabKey(reverse: false);
          if (!handled) {
            FocusScope.of(context).nextFocus();
          }
          return null;
        },
      ),
      _OutdentFenceIntent: CallbackAction<_OutdentFenceIntent>(
        onInvoke: (_) {
          if (widget.controller.value.composing.isValid) {
            return null;
          }
          final handled = widget.controller.handleTabKey(reverse: true);
          if (!handled) {
            FocusScope.of(context).previousFocus();
          }
          return null;
        },
      ),
    };

    if (widget.enableTestShortcuts) {
      // Shortcut wiring for widget tests that use sendKeyEvent(enter).
      const enterIntent = _InsertNewlineIntent();
      shortcuts[const SingleActivator(LogicalKeyboardKey.enter)] = enterIntent;
      actions[_InsertNewlineIntent] = CallbackAction<_InsertNewlineIntent>(
        onInvoke: (_) {
          widget.controller.handleEnter();
          return null;
        },
      );
    }

    return Shortcuts(
      shortcuts: shortcuts,
      child: Actions(actions: actions, child: field),
    );
  }

  double _cursorHeightForSelection({
    required BlockTree tree,
    required DuneMarkdownTheme markdownTheme,
  }) {
    final text = widget.controller.text;
    final selection = widget.controller.selection;
    if (!selection.isValid || !selection.isCollapsed) return _lineHeightPixels;

    final caret = selection.baseOffset.clamp(0, text.length);
    final probeOffset = (caret == text.length && caret > 0) ? caret - 1 : caret;
    final block = tree.nodeAt(probeOffset);

    var style = _style;
    if (block.type == BlockType.header) {
      final rawLevel = block.payload?['level'];
      final level = rawLevel is int
          ? rawLevel
          : int.tryParse(rawLevel?.toString() ?? '') ?? 1;
      style = EditorHeadingStylePolicy.resolve(
        base: style,
        level: level,
        markdownTheme: markdownTheme,
        headingTheme: _editorThemeData.headings,
      );
    }

    final fontSize = style.fontSize ?? (_style.fontSize ?? kFontSize);
    final lineHeight = style.height ?? (_style.height ?? kLineHeightMultiplier);
    final computed = fontSize * lineHeight;
    return computed
        .clamp(_lineHeightPixels, _lineHeightPixels * 3.0)
        .toDouble();
  }

  RenderEditable? _findRenderEditable(RenderObject root) {
    RenderEditable? found;
    void visit(RenderObject node) {
      if (found != null) return;
      if (node is RenderEditable) {
        found = node;
        return;
      }
      node.visitChildren(visit);
    }

    visit(root);
    return found;
  }

  Rect? _caretRectInOverlaySpace(
    TextSelection selection, {
    required RenderBox targetRenderBox,
  }) {
    return _caretRectInTargetSpaceForOffset(
      selection.extentOffset,
      affinity: selection.affinity,
      targetRenderBox: targetRenderBox,
    );
  }

  Rect? _caretRectInTargetSpaceForOffset(
    int offset, {
    required RenderBox targetRenderBox,
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final textFieldContext = _textFieldKey.currentContext;
    if (textFieldContext == null) return null;
    final rootRender = textFieldContext.findRenderObject();
    if (rootRender is! RenderObject) return null;

    final editable = _findRenderEditable(rootRender);
    if (editable == null) return null;

    final text = widget.controller.text;
    final caretOffset = offset.clamp(0, text.length).toInt();
    final localRect = editable.getLocalRectForCaret(
      TextPosition(offset: caretOffset, affinity: affinity),
    );
    final globalTopLeft = editable.localToGlobal(localRect.topLeft);
    final globalBottomRight = editable.localToGlobal(localRect.bottomRight);
    final targetTopLeft = targetRenderBox.globalToLocal(globalTopLeft);
    final targetBottomRight = targetRenderBox.globalToLocal(globalBottomRight);
    return Rect.fromPoints(targetTopLeft, targetBottomRight);
  }

  bool _isOffsetInsideCodeBlock(int offset) {
    final text = widget.controller.text;
    for (final block in widget.controller.geometry.codeBlocks) {
      if (offset >= block.startOffset && offset < block.endOffset) return true;
      if (offset == text.length &&
          block.endOffset == text.length &&
          offset >= block.startOffset) {
        return true;
      }
    }
    return false;
  }

  Widget _buildFenceLanguageOverlay(BoxConstraints constraints) {
    return ListenableBuilder(
      listenable: Listenable.merge([_viewportNotifier, widget.controller]),
      builder: (context, _) {
        final selection = widget.controller.selection;
        if (!selection.isValid || !selection.isCollapsed) {
          return const SizedBox.shrink();
        }

        final caret = selection.baseOffset;
        final text = widget.controller.text;
        final blocks = widget.controller.geometry.codeBlocks;

        int lineStartForOffset(int offset) {
          if (text.isEmpty) return 0;
          int safe = offset.clamp(0, text.length - 1);
          // If the offset points at a newline, treat it as end-of-previous-line.
          if (text.codeUnitAt(safe) == 10 && safe > 0) {
            safe--;
          }
          final idx = text.lastIndexOf('\n', safe);
          return idx == -1 ? 0 : idx + 1;
        }

        bool isUnclosedFenceAtEof(MeasuredBlock b) {
          if (b.endOffset != text.length) return false;
          if (b.endOffset <= 0) return true;
          final closeLineStart = lineStartForOffset(b.endOffset - 1);
          final hasClosingFence = closeLineStart != b.startOffset &&
              closeLineStart + 3 <= text.length &&
              text.startsWith('```', closeLineStart);
          return !hasClosingFence;
        }

        MeasuredBlock? containing;
        for (final b in blocks) {
          final inside = caret >= b.startOffset && caret < b.endOffset;
          final atUnclosedEofEnd =
              caret == b.endOffset && isUnclosedFenceAtEof(b);
          if (inside || atUnclosedEofEnd) {
            containing = b;
            break;
          }
        }
        if (containing == null) return const SizedBox.shrink();

        final start = containing.startOffset;
        if (start < 0 ||
            start + 3 > text.length ||
            !text.startsWith('```', start)) {
          return const SizedBox.shrink();
        }

        final openLineEnd = FencedCodeScanner.endOfLine(text, start);
        final openLineContentEnd =
            (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
                ? openLineEnd - 1
                : openLineEnd;
        final infoStart = (start + 3).clamp(0, text.length);

        String? rawTag;
        if (infoStart < openLineContentEnd) {
          final info = text.substring(infoStart, openLineContentEnd).trim();
          if (info.isNotEmpty) {
            rawTag = info.split(RegExp(r'\s+')).first.trim().toLowerCase();
          }
        }

        final option = _optionForFenceTag(rawTag);
        // Treat unknown opener text as code content; only recognized tags are
        // presented as the fence language.
        final label = option?.label ?? 'Plain';
        final scopedTheme = SovereignEditorThemeScope.of(context);
        final codeBlockTheme = scopedTheme.codeBlock;
        final pickerStyle = codeBlockTheme.languagePicker;
        final contentPadding = _editorThemeData.editorContentPadding;

        // Position in viewport coordinates; keep it accessible even when the
        // document is wider than the viewport.
        final top = contentPadding.top +
            (containing.paintStartLine * _lineHeightPixels) -
            _viewportNotifier.value.dy -
            codeBlockTheme.backgroundVerticalInset;
        if (constraints.hasBoundedHeight) {
          if (top < -_lineHeightPixels || top > constraints.maxHeight) {
            return const SizedBox.shrink();
          }
        }

        final pickerRight = (contentPadding.right +
                codeBlockTheme.backgroundHorizontalInset +
                pickerStyle.margin.right)
            .clamp(0.0, double.infinity)
            .toDouble();
        final availableWidth = constraints.hasBoundedWidth
            ? (constraints.maxWidth - pickerRight - pickerStyle.margin.left)
            : pickerStyle.maxWidth;
        final pickerMaxWidth = availableWidth.isFinite
            ? availableWidth.clamp(56.0, pickerStyle.maxWidth).toDouble()
            : pickerStyle.maxWidth;
        final pickerHeight = pickerStyle.height.clamp(
          18.0,
          (_lineHeightPixels - 2).clamp(18.0, 28.0),
        );

        return Positioned(
          top: top + pickerStyle.verticalOffset,
          right: pickerRight,
          child: Material(
            color: Colors.transparent,
            child: PopupMenuButton<String>(
              key: _kCodeFenceLanguagePickerKey,
              tooltip: 'Code language',
              padding: EdgeInsets.zero,
              color: pickerStyle.menuBackgroundColor,
              elevation: pickerStyle.elevation,
              onSelected: (tag) {
                widget.controller.setFencedCodeLanguageForSelection(
                  tag == 'plain' ? null : tag,
                );
                _effectiveFocusNode.requestFocus();
              },
              itemBuilder: (context) {
                return _fenceLanguageOptions
                    .map(
                      (o) => PopupMenuItem<String>(
                        value: o.tag,
                        child: Text(o.label, style: pickerStyle.menuTextStyle),
                      ),
                    )
                    .toList();
              },
              child: Container(
                constraints: BoxConstraints(
                  maxWidth: pickerMaxWidth,
                  minHeight: pickerHeight,
                ),
                padding: pickerStyle.padding,
                decoration: BoxDecoration(
                  color: pickerStyle.backgroundColor,
                  borderRadius: BorderRadius.circular(pickerStyle.borderRadius),
                  border: Border.all(color: pickerStyle.borderColor),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Flexible(
                      child: Text(
                        label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        softWrap: false,
                        style: pickerStyle.textStyle,
                      ),
                    ),
                    SizedBox(width: pickerStyle.iconGap),
                    Icon(
                      Icons.arrow_drop_down,
                      color: pickerStyle.iconColor,
                      size: pickerStyle.iconSize,
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  _FenceLanguageOption? _optionForFenceTag(String? rawTag) {
    if (rawTag == null || rawTag.isEmpty) {
      return _fenceLanguageOptions.first;
    }

    // Prefer exact match.
    for (final o in _fenceLanguageOptions) {
      if (o.tag == rawTag) return o;
    }

    // Fallback: match by normalized highlight language (e.g. html -> xml).
    final normalized = SovereignCodeHighlighter.normalizeFenceTag(rawTag);
    if (normalized == null) return null;
    for (final o in _fenceLanguageOptions) {
      final oNorm = SovereignCodeHighlighter.normalizeFenceTag(o.tag);
      if (oNorm == normalized) return o;
    }

    return null;
  }
}

class _InsertNewlineIntent extends Intent {
  const _InsertNewlineIntent();
}

class _InsertLiteralNewlineIntent extends Intent {
  const _InsertLiteralNewlineIntent();
}

class _ArrowDownExitFenceIntent extends Intent {
  const _ArrowDownExitFenceIntent();
}

class _ArrowUpExitFenceIntent extends Intent {
  const _ArrowUpExitFenceIntent();
}

class _IndentFenceIntent extends Intent {
  const _IndentFenceIntent();
}

class _OutdentFenceIntent extends Intent {
  const _OutdentFenceIntent();
}

class _FenceLanguageOption {
  final String label;
  final String tag;

  const _FenceLanguageOption(this.label, this.tag);
}
