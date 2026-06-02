import 'package:flutter/widgets.dart';

import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'sovereign_flutter_controller.dart';
import 'sovereign_markdown_interactions.dart';
import 'sovereign_parse_scheduler.dart';
import 'sovereign_read_only_preview.dart';
import 'sovereign_render_plan_overlay_controls.dart';

final class Markdown extends StatefulWidget {
  const Markdown({
    super.key,
    this.markdown,
    this.controller,
    this.parseBackend,
    this.onParseError,
    this.profile = FlarkMarkdownProfile.commonMarkGfm,
    this.parseDebounce = const Duration(milliseconds: 80),
    this.textStyle,
    this.blockBuilder,
    this.showOverlayControls = false,
    this.overlayControlBuilder,
    this.onOverlayTargetPressed,
    this.interactionConfig = const FlarkMarkdownInteractionConfig(),
  }) : assert(
         (markdown == null) != (controller == null),
         'Provide exactly one of markdown or controller.',
       ),
       assert(
         controller == null || parseBackend == null,
         'parseBackend is only valid for standalone markdown previews. When '
         'using a controller, the controller owner is responsible for parser '
         'scheduling.',
       );

  /// Markdown source for a standalone preview.
  ///
  /// Provide either this or [controller], not both.
  final String? markdown;

  /// Shared controller for previewing the same render plan as an editor.
  ///
  /// Provide either this or [markdown], not both. When a controller is
  /// provided, this widget only consumes the controller's current render plan.
  final FlarkFlutterController? controller;

  /// Parser backend used by standalone [markdown] previews.
  ///
  /// Shared-controller previews do not schedule parsing. In split editor and
  /// preview layouts, the editor or controller owner drives parsing once.
  /// When this is null for a standalone preview, the widget requires the
  /// packaged Comrak backend. Backend load failures are surfaced directly
  /// instead of falling back to a second markdown implementation.
  final FlarkMarkdownParseBackend? parseBackend;

  /// Called when a scheduled background parse fails.
  final void Function(Object error, StackTrace stackTrace)? onParseError;

  final FlarkMarkdownProfile profile;
  final Duration parseDebounce;
  final TextStyle? textStyle;
  final FlarkPreviewBlockWidgetBuilder? blockBuilder;
  final bool showOverlayControls;
  final FlarkOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<FlarkRenderOverlayTarget>? onOverlayTargetPressed;
  final FlarkMarkdownInteractionConfig interactionConfig;

  @override
  State<Markdown> createState() {
    return _MarkdownState();
  }
}

final class _MarkdownState extends State<Markdown> {
  FlarkFlutterController? _ownedController;
  FlarkParseScheduler? _parseScheduler;

  FlarkFlutterController get _controller {
    return widget.controller ?? _ownedController!;
  }

  @override
  void initState() {
    super.initState();
    _ensureOwnedController();
    _configureParseSchedulerIfNeeded();
  }

  @override
  void didUpdateWidget(Markdown oldWidget) {
    super.didUpdateWidget(oldWidget);
    final sourceChanged =
        oldWidget.controller != widget.controller ||
        oldWidget.markdown != widget.markdown;
    if (sourceChanged) {
      _parseScheduler?.dispose();
      if (widget.controller == null) {
        _ownedController?.dispose();
        _ownedController = FlarkFlutterController.fromMarkdown(
          widget.markdown!,
        );
      } else {
        _ownedController?.dispose();
        _ownedController = null;
      }
      _configureParseSchedulerIfNeeded();
      return;
    }

    if (oldWidget.parseBackend != widget.parseBackend ||
        oldWidget.onParseError != widget.onParseError ||
        oldWidget.profile != widget.profile ||
        oldWidget.parseDebounce != widget.parseDebounce) {
      _parseScheduler?.dispose();
      _configureParseSchedulerIfNeeded();
    }
  }

  @override
  void dispose() {
    _parseScheduler?.dispose();
    _ownedController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final preview = _previewWithOptionalOverlay();
    if (FlarkMarkdownInteractions.maybeOf(context) != null) {
      return preview;
    }

    if (!widget.showOverlayControls) {
      return FlarkMarkdownInteractions(
        controller: _controller,
        config: widget.interactionConfig,
        editable: false,
        child: preview,
      );
    }

    return FlarkMarkdownInteractions(
      controller: _controller,
      config: widget.interactionConfig,
      editable: false,
      child: preview,
    );
  }

  Widget _previewWithOptionalOverlay() {
    final preview = FlarkReadOnlyPreview(
      controller: _controller,
      textStyle: widget.textStyle,
      blockBuilder: widget.blockBuilder,
    );
    if (!widget.showOverlayControls) return preview;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        FlarkRenderPlanOverlayControls(
          controller: _controller,
          builder: widget.overlayControlBuilder,
          onPressed: widget.onOverlayTargetPressed,
        ),
        preview,
      ],
    );
  }

  void _ensureOwnedController() {
    if (widget.controller != null) return;
    _ownedController = FlarkFlutterController.fromMarkdown(widget.markdown!);
  }

  void _configureParseSchedulerIfNeeded() {
    if (widget.controller != null) {
      _parseScheduler = null;
      return;
    }
    final backend = widget.parseBackend ?? _resolveDefaultParseBackend();
    _parseScheduler = FlarkParseScheduler(
      controller: _controller,
      backend: backend,
      profile: widget.profile,
      debounce: widget.parseDebounce,
      onError: widget.onParseError,
    )..start();
  }

  FlarkMarkdownParseBackend _resolveDefaultParseBackend() {
    return FlarkNativeComrakParseBackend.requiredDefault();
  }
}
