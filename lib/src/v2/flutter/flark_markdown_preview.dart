import 'package:flutter/widgets.dart';

import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_interactions.dart';
import 'flark_read_only_preview.dart';
import 'flark_render_plan_overlay_controls.dart';

final class Markdown extends StatefulWidget {
  const Markdown({
    super.key,
    this.markdown,
    this.controller,
    this.parseBackend,
    this.onParseError,
    this.profile,
    this.parseDebounce,
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
         controller == null ||
             (parseBackend == null &&
                 onParseError == null &&
                 profile == null &&
                 parseDebounce == null),
         'parseBackend, onParseError, profile, and parseDebounce are only valid '
         'for standalone markdown previews. When using a controller, configure '
         'parsing on the FlarkFlutterController instead.',
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
  ///
  /// Used only for standalone [markdown] previews. Shared-controller previews
  /// route parse errors through the controller's own configuration.
  final void Function(Object error, StackTrace stackTrace)? onParseError;

  /// Markdown profile for standalone [markdown] previews. Defaults to GFM.
  final FlarkMarkdownProfile? profile;

  /// Parse debounce for standalone [markdown] previews. Defaults to 80ms.
  final Duration? parseDebounce;
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

  FlarkFlutterController get _controller {
    return widget.controller ?? _ownedController!;
  }

  @override
  void initState() {
    super.initState();
    _ensureOwnedController();
    // A standalone preview owns parsing; a shared-controller preview is a
    // view only — the controller owner (e.g. an editor) drives parsing.
    if (widget.controller == null) _controller.attachParsingSurface();
  }

  @override
  void didUpdateWidget(Markdown oldWidget) {
    super.didUpdateWidget(oldWidget);
    final sourceChanged =
        oldWidget.controller != widget.controller ||
        oldWidget.markdown != widget.markdown;
    if (sourceChanged) {
      if (widget.controller == null) {
        _ownedController?.dispose();
        _ownedController = _createOwnedController();
        _controller.attachParsingSurface();
      } else {
        // Switching to a shared controller; the old owned controller (if any)
        // is disposed, which stops its parser.
        _ownedController?.dispose();
        _ownedController = null;
      }
      return;
    }

    if (widget.controller == null &&
        (oldWidget.parseBackend != widget.parseBackend ||
            oldWidget.onParseError != widget.onParseError ||
            oldWidget.profile != widget.profile ||
            oldWidget.parseDebounce != widget.parseDebounce)) {
      _controller.configureParsing(
        parseBackend: widget.parseBackend,
        parseProfile: widget.profile,
        parseDebounce: widget.parseDebounce,
        onParseError: widget.onParseError,
        clearOnParseError: widget.onParseError == null,
      );
    }
  }

  @override
  void dispose() {
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
    _ownedController = _createOwnedController();
  }

  FlarkFlutterController _createOwnedController() {
    return FlarkFlutterController.fromMarkdown(
      widget.markdown!,
      parseBackend: widget.parseBackend,
      parseProfile: widget.profile ?? FlarkMarkdownProfile.commonMarkGfm,
      parseDebounce: widget.parseDebounce ?? const Duration(milliseconds: 80),
      onParseError: widget.onParseError,
    );
  }
}
