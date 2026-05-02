import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/painters/tier1_painter.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/read_only_markdown_interaction.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/read_only_task_checkbox_overlay.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/read_only_task_checkbox_visual_layer.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/sovereign_inline_actions_overlay.dart';

import '../controllers/sovereign_controller.dart';
import '../engine/syntax_engine.dart';
import '../models/decoration_model.dart';
import '../theme/sovereign_editor_theme.dart';

/// Read-only markdown surface backed by Sovereign parse/render contracts.
///
/// The view uses the same syntax and rendering pipeline as `SovereignEditor`
/// without exposing an editable text field.
class SovereignMarkdownView extends StatefulWidget {
  /// Creates a read-only markdown view for [markdown].
  const SovereignMarkdownView({
    super.key,
    required this.markdown,
    this.profile = MarkdownSyntaxProfile.commonMarkCore,
    this.theme,
    this.selectable = true,
    this.showLinkActionsOverlay = false,
    this.freezeVisualOverlays = false,
    this.onOpenLink,
    this.onEditInlineTarget,
    this.syntaxEngine,
  });

  /// Markdown source text to render.
  final String markdown;

  /// Syntax profile requested from the parser.
  final MarkdownSyntaxProfile profile;

  /// Theme overrides for markdown rendering and link actions.
  final SovereignEditorThemeData? theme;

  /// Whether rendered text should be selectable.
  final bool selectable;

  /// Whether inline link/image actions are shown after taps.
  final bool showLinkActionsOverlay;

  /// Freezes visual overlays for deterministic tests and snapshots.
  final bool freezeVisualOverlays;

  /// Callback used when a link or image target should be opened.
  final Future<void> Function(String url)? onOpenLink;

  /// Callback used to edit a read-only inline link or image target.
  final Future<void> Function(
    BuildContext context,
    String label,
    String url,
    bool isImage,
  )? onEditInlineTarget;

  /// Optional syntax engine override for custom parsing or tests.
  final SyntaxEngine? syntaxEngine;

  @override
  State<SovereignMarkdownView> createState() => _SovereignMarkdownViewState();
}

class _SovereignMarkdownViewState extends State<SovereignMarkdownView> {
  static const double _kFallbackFontSize = 15.0;
  static const double _kFallbackLineHeight = 1.6;

  final GlobalKey _textKey = GlobalKey(debugLabel: 'SovereignMarkdownViewText');
  late SovereignController _controller;
  OverlayEntry? _inlineActionsOverlayEntry;
  bool _taskCheckboxVisualRefreshScheduled = false;
  List<SovereignReadOnlyTaskCheckboxVisualData> _taskCheckboxVisuals =
      const <SovereignReadOnlyTaskCheckboxVisualData>[];
  int _taskCheckboxVisualSignature = 0;
  Size? _lastTaskCheckboxParagraphSize;
  String _lastTaskCheckboxTextSnapshot = '';
  final SovereignReadOnlyTapTracker _tapTracker = SovereignReadOnlyTapTracker();

  @override
  void initState() {
    super.initState();
    _controller = _buildController();
    _lastTaskCheckboxTextSnapshot = _controller.text;
  }

  @override
  void didUpdateWidget(covariant SovereignMarkdownView oldWidget) {
    super.didUpdateWidget(oldWidget);
    final actionConfigChanged =
        oldWidget.showLinkActionsOverlay != widget.showLinkActionsOverlay ||
            oldWidget.onOpenLink != widget.onOpenLink ||
            oldWidget.onEditInlineTarget != widget.onEditInlineTarget;
    if (actionConfigChanged) {
      _removeInlineActionsOverlay();
    }
    if (oldWidget.markdown == widget.markdown &&
        oldWidget.profile == widget.profile &&
        oldWidget.syntaxEngine == widget.syntaxEngine) {
      return;
    }

    final previous = _controller;
    _controller = _buildController();
    previous.dispose();
    _removeInlineActionsOverlay();
    _taskCheckboxVisuals = const <SovereignReadOnlyTaskCheckboxVisualData>[];
    _taskCheckboxVisualSignature = 0;
    _lastTaskCheckboxParagraphSize = null;
    _lastTaskCheckboxTextSnapshot = _controller.text;
  }

  SovereignController _buildController() {
    return SovereignController.readOnly(
      text: widget.markdown,
      syntaxEngine: widget.syntaxEngine,
      markdownProfile: widget.profile,
    );
  }

  @override
  void dispose() {
    _removeInlineActionsOverlay();
    _controller.dispose();
    super.dispose();
  }

  int? _offsetForGlobalPosition(Offset globalPosition) {
    final renderObject = _textKey.currentContext?.findRenderObject();
    if (renderObject is! RenderParagraph) return null;
    final localOffset = renderObject.globalToLocal(globalPosition);
    final rawOffset = renderObject.getPositionForOffset(localOffset).offset;
    return rawOffset.clamp(0, _controller.text.length).toInt();
  }

  void _removeInlineActionsOverlay() {
    _inlineActionsOverlayEntry?.remove();
    _inlineActionsOverlayEntry = null;
  }

  void _scheduleTaskCheckboxVisualRefresh() {
    if (widget.freezeVisualOverlays) return;
    if (_taskCheckboxVisualRefreshScheduled || !mounted) return;
    _taskCheckboxVisualRefreshScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _taskCheckboxVisualRefreshScheduled = false;
      if (!mounted) return;
      final snapshot = _computeReadOnlyTaskCheckboxSnapshot();
      // If marker topology says checkboxes exist but only a partial visual set
      // was measurable this frame, keep prior visuals to avoid drag-time flash.
      if (snapshot.markerCount > 0 &&
          snapshot.visuals.length < snapshot.markerCount &&
          _taskCheckboxVisuals.isNotEmpty) {
        return;
      }
      final signature = snapshot.signature;
      if (signature == _taskCheckboxVisualSignature) return;
      final paragraphSize = (() {
        final renderObject = _textKey.currentContext?.findRenderObject();
        return renderObject is RenderParagraph ? renderObject.size : null;
      })();
      setState(() {
        _taskCheckboxVisuals = snapshot.visuals;
        _taskCheckboxVisualSignature = signature;
        _lastTaskCheckboxParagraphSize = paragraphSize;
        _lastTaskCheckboxTextSnapshot = _controller.text;
      });
    });
  }

  void _maybeRefreshTaskCheckboxVisuals() {
    if (widget.freezeVisualOverlays) return;
    if (_taskCheckboxVisuals.isEmpty) {
      _scheduleTaskCheckboxVisualRefresh();
      return;
    }

    if (_lastTaskCheckboxTextSnapshot != _controller.text) {
      _scheduleTaskCheckboxVisualRefresh();
      return;
    }

    final renderObject = _textKey.currentContext?.findRenderObject();
    if (renderObject is! RenderParagraph) return;
    if (_lastTaskCheckboxParagraphSize != renderObject.size) {
      _scheduleTaskCheckboxVisualRefresh();
    }
  }

  SovereignReadOnlyTaskCheckboxVisualSnapshot
      _computeReadOnlyTaskCheckboxSnapshot() {
    final text = _controller.text;
    if (text.isEmpty) return SovereignReadOnlyTaskCheckboxVisualSnapshot.empty;

    final renderObject = _textKey.currentContext?.findRenderObject();
    if (renderObject is! RenderParagraph) {
      return SovereignReadOnlyTaskCheckboxVisualSnapshot.empty;
    }

    final lineIndex = _controller.decoration.lineIndex;
    return SovereignReadOnlyTaskCheckboxOverlay.computeSnapshot(
      text: text,
      lineCount: lineIndex.lineCount,
      renderObject: renderObject,
      markerRangeForLine: _controller.taskCheckboxMarkerRangeForLine,
      visualRangeForLine: _controller.taskCheckboxVisualRangeForLine,
    );
  }

  SovereignResolvedInlineActionsTarget? _targetAtOffset(int offset) {
    return resolveSovereignInlineActionsTargetAtCaret(_controller.text, offset);
  }

  Future<void> _openInlineTarget(
    SovereignResolvedInlineActionsTarget target,
  ) async {
    if (widget.onOpenLink == null || target.resolvedUrl.isEmpty) return;
    await widget.onOpenLink!.call(target.resolvedUrl);
  }

  Future<void> _editInlineTarget(
    SovereignResolvedInlineActionsTarget target,
  ) async {
    if (widget.onEditInlineTarget == null || target.resolvedUrl.isEmpty) return;
    await widget.onEditInlineTarget!(
      context,
      target.target.labelText(target.textSnapshot),
      target.resolvedUrl,
      target.target.isImage,
    );
  }

  Future<void> _showInlineActionsOverlay({
    required SovereignResolvedInlineActionsTarget target,
    required Offset anchorGlobal,
  }) async {
    final overlay = Overlay.of(context, rootOverlay: true);
    final overlayRender = overlay.context.findRenderObject();
    if (overlayRender is! RenderBox) return;

    final localAnchor = overlayRender.globalToLocal(anchorGlobal);
    final theme =
        (widget.theme ?? const SovereignEditorThemeData()).linkActions;
    final hasEdit = widget.onEditInlineTarget != null;
    final hasOpen = widget.onOpenLink != null && target.resolvedUrl.isNotEmpty;
    final hasCopy = target.resolvedUrl.isNotEmpty;
    final actionCount =
        (hasOpen ? 1 : 0) + (hasCopy ? 1 : 0) + (hasEdit ? 1 : 0);
    if (actionCount == 0) return;

    final imagePreviewWidthCap = target.target.isImage
        ? theme.overlayMaxInlineImageWidth
        : theme.overlayMaxLinkWidth;
    final estimatedWidth = target.target.isImage
        ? theme.estimatedInlineImageWidth
        : theme.estimatedLinkWidth;
    final estimatedHeight = target.target.isImage
        ? theme.estimatedInlineImageHeight
        : theme.estimatedLinkHeight;
    final placement = computeSovereignInlineActionsPlacement(
      theme: theme,
      hostRect: Offset.zero & overlayRender.size,
      anchorX: localAnchor.dx,
      anchorTop: localAnchor.dy,
      anchorBottom: localAnchor.dy,
      estimatedWidth: estimatedWidth,
      estimatedHeight: estimatedHeight,
      maxWidthCap: imagePreviewWidthCap,
      preferAbove: false,
    );

    _removeInlineActionsOverlay();

    final overlayEntry = OverlayEntry(
      builder: (overlayContext) {
        return Stack(
          children: [
            Positioned.fill(
              child: GestureDetector(
                behavior: HitTestBehavior.translucent,
                onTap: _removeInlineActionsOverlay,
              ),
            ),
            Positioned(
              left: placement.left,
              top: placement.top,
              child: Material(
                color: Colors.transparent,
                child: SovereignInlineActionsOverlayCard(
                  theme: theme,
                  maxWidth: placement.maxWidth,
                  target: target.target,
                  textSnapshot: target.textSnapshot,
                  resolvedUrl: target.resolvedUrl,
                  openEnabled: hasOpen,
                  standaloneImagePreview: false,
                  onOpen: hasOpen
                      ? () async {
                          _removeInlineActionsOverlay();
                          await _openInlineTarget(target);
                        }
                      : null,
                  onCopy: hasCopy
                      ? () async {
                          await copyInlineTargetUrlToClipboard(
                            target.resolvedUrl,
                          );
                          _removeInlineActionsOverlay();
                        }
                      : null,
                  onEdit: hasEdit
                      ? () async {
                          _removeInlineActionsOverlay();
                          await _editInlineTarget(target);
                        }
                      : null,
                ),
              ),
            ),
          ],
        );
      },
    );
    _inlineActionsOverlayEntry = overlayEntry;
    overlay.insert(overlayEntry);
  }

  void _handlePointerDown(PointerDownEvent event) =>
      _tapTracker.handlePointerDown(event);
  void _handlePointerMove(PointerMoveEvent event) =>
      _tapTracker.handlePointerMove(event);
  void _handlePointerCancel(PointerCancelEvent event) =>
      _tapTracker.handlePointerCancel(event);

  Future<void> _handlePointerUp(PointerUpEvent event) async {
    if (!_tapTracker.consumeIsTap(event)) return;
    final offset = _offsetForGlobalPosition(event.position);
    if (offset == null) return;
    final target = _targetAtOffset(offset);
    if (target == null) {
      _removeInlineActionsOverlay();
      return;
    }
    final renderObject = _textKey.currentContext?.findRenderObject();
    if (renderObject is! RenderParagraph ||
        !SovereignReadOnlyTapTracker.tapInsideInlineTarget(
          renderObject: renderObject,
          target: target,
          globalPosition: event.position,
        )) {
      _removeInlineActionsOverlay();
      return;
    }

    if (widget.showLinkActionsOverlay) {
      await _showInlineActionsOverlay(
        target: target,
        anchorGlobal: event.position,
      );
      return;
    }

    await _openInlineTarget(target);
  }

  @override
  Widget build(BuildContext context) {
    if (widget.markdown.isEmpty) {
      return const SizedBox.shrink();
    }

    final inheritedTheme = SovereignEditorThemeScope.maybeOf(context);
    final baseTheme =
        widget.theme ?? inheritedTheme ?? const SovereignEditorThemeData();
    final scopedTheme = baseTheme;
    final markdownTheme = scopedTheme.resolveMarkdownTheme(context);
    final resolvedTextStyle =
        scopedTheme.textStyle ?? DefaultTextStyle.of(context).style;
    final textScaler = MediaQuery.textScalerOf(context);
    final resolvedFontSize = resolvedTextStyle.fontSize ?? _kFallbackFontSize;
    final resolvedLineHeight = resolvedTextStyle.height ?? _kFallbackLineHeight;
    final lineHeightTextPainter = TextPainter(
      text: TextSpan(text: 'M', style: resolvedTextStyle),
      textDirection: TextDirection.ltr,
      textScaler: textScaler,
    )..layout();
    final lineHeight = lineHeightTextPainter.height;
    final strutStyle = StrutStyle(
      fontSize: resolvedFontSize,
      height: resolvedLineHeight,
      forceStrutHeight: true,
      fontFamily: resolvedTextStyle.fontFamily ??
          GoogleFonts.sourceCodePro().fontFamily,
    );

    Widget content = StreamBuilder<DecorationModel>(
      stream: _controller.decorationStream,
      initialData: _controller.decoration,
      builder: (context, _) {
        final span = _controller.buildTextSpan(
          context: context,
          style: resolvedTextStyle,
          withComposing: false,
        );

        Widget textChild = RichText(
          key: _textKey,
          text: span,
          strutStyle: strutStyle,
          textScaler: textScaler,
          selectionRegistrar:
              widget.selectable ? SelectionContainer.maybeOf(context) : null,
          selectionColor: widget.selectable
              ? (Theme.of(context).textSelectionTheme.selectionColor ??
                  Colors.blue.withValues(alpha: 0.24))
              : null,
        );

        if (widget.showLinkActionsOverlay || widget.onOpenLink != null) {
          textChild = Listener(
            behavior: HitTestBehavior.translucent,
            onPointerDown: _handlePointerDown,
            onPointerMove: _handlePointerMove,
            onPointerUp: _handlePointerUp,
            onPointerCancel: _handlePointerCancel,
            child: textChild,
          );
        }

        if (scopedTheme.taskCheckbox.useCustomOverlay) {
          _maybeRefreshTaskCheckboxVisuals();
        }

        final content = Padding(
          padding: scopedTheme.editorContentPadding,
          child: textChild,
        );

        return Stack(
          clipBehavior: Clip.none,
          children: [
            Positioned.fill(
              child: IgnorePointer(
                child: CustomPaint(
                  painter: Tier1Painter(
                    geometry: _controller.geometry,
                    lineHeight: lineHeight,
                    viewport: Offset.zero,
                    contentPadding: scopedTheme.editorContentPadding,
                    codeBlockBackgroundColor:
                        scopedTheme.codeBlock.backgroundColor ??
                            markdownTheme.codeBlockBackgroundColor,
                    codeBlockBorderRadius: markdownTheme.codeBlockBorderRadius,
                    codeBlockHorizontalInset:
                        scopedTheme.codeBlock.backgroundHorizontalInset,
                    codeBlockVerticalInset:
                        scopedTheme.codeBlock.backgroundVerticalInset,
                    quoteRailColor: scopedTheme.blockquote.railColor ??
                        markdownTheme.blockquoteBorderColor.withValues(
                          alpha: 0.92,
                        ),
                    quoteRailWidth: scopedTheme.blockquote.railWidth,
                    quoteRailInset: scopedTheme.blockquote.railInset,
                    quoteRailRadius: scopedTheme.blockquote.railRadius,
                  ),
                ),
              ),
            ),
            content,
            if (scopedTheme.taskCheckbox.useCustomOverlay &&
                _taskCheckboxVisuals.isNotEmpty)
              SovereignReadOnlyTaskCheckboxVisualLayer(
                visuals: _taskCheckboxVisuals,
                theme: scopedTheme.taskCheckbox,
                padding: scopedTheme.editorContentPadding,
              ),
          ],
        );
      },
    );
    if (widget.selectable) {
      content = SelectionArea(child: content);
    }

    return SovereignEditorThemeScope(data: scopedTheme, child: content);
  }
}
