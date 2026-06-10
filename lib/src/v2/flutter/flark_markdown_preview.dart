import 'package:flutter/widgets.dart';

import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_interactions.dart';
import 'flark_markdown_theme.dart';
import 'flark_read_only_preview.dart';
import 'flark_text_selection_gestures.dart';
import 'flark_render_plan_overlay_controls.dart';

/// A read-only rendered Markdown view (the non-scrolling body form; place it
/// inside your own scroll view, like `flutter_markdown`'s `MarkdownBody`).
final class FlarkMarkdown extends StatefulWidget {
  const FlarkMarkdown({
    super.key,
    this.markdown,
    this.controller,
    this.parseBackend,
    this.onParseError,
    this.parseProfile,
    @Deprecated('Renamed to parseProfile; will be removed before 1.0.')
    this.profile,
    this.parseDebounce,
    this.textStyle,
    this.theme,
    this.blockBuilder,
    this.selectable = false,
    this.showOverlayControls = false,
    this.overlayControlBuilder,
    this.onOverlayTargetPressed,
    this.interactionConfig = const FlarkMarkdownInteractionConfig(),
  }) : assert(
         (markdown == null) != (controller == null),
         'Provide exactly one of markdown or controller.',
       ),
       assert(
         parseProfile == null || profile == null,
         'Provide parseProfile only; profile is its deprecated alias.',
       ),
       assert(
         controller == null ||
             (parseBackend == null &&
                 onParseError == null &&
                 parseProfile == null &&
                 profile == null &&
                 parseDebounce == null),
         'parseBackend, onParseError, parseProfile, and parseDebounce are only '
         'valid for standalone markdown previews. When using a controller, '
         'configure parsing on the FlarkFlutterController instead.',
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
  final FlarkMarkdownProfile? parseProfile;

  /// Deprecated alias of [parseProfile].
  @Deprecated('Renamed to parseProfile; will be removed before 1.0.')
  final FlarkMarkdownProfile? profile;

  // ignore: deprecated_member_use_from_same_package
  FlarkMarkdownProfile? get _effectiveParseProfile => parseProfile ?? profile;

  /// Parse debounce for standalone [markdown] previews. Defaults to 80ms.
  final Duration? parseDebounce;
  final TextStyle? textStyle;

  /// Colors for markdown chrome (code fences, quotes, links, tables…).
  ///
  /// When null, the ambient [FlarkMarkdownTheme] applies, falling back to a
  /// platform-brightness default.
  final FlarkMarkdownThemeData? theme;
  final FlarkPreviewBlockWidgetBuilder? blockBuilder;

  /// Whether rendered text can be selected and copied.
  ///
  /// Selection is provided by a [SelectableRegion], which requires an
  /// [Overlay] ancestor — any [MaterialApp], [CupertinoApp], or [WidgetsApp]
  /// provides one.
  final bool selectable;
  final bool showOverlayControls;
  final FlarkOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<FlarkRenderOverlayTarget>? onOverlayTargetPressed;
  final FlarkMarkdownInteractionConfig interactionConfig;

  @override
  State<FlarkMarkdown> createState() {
    return _FlarkMarkdownState();
  }
}

/// Deprecated name of [FlarkMarkdown].
///
/// Renamed both for package-wide naming consistency and because the bare name
/// collides with `flutter_markdown`'s scrolling `Markdown` widget while this
/// widget is its non-scrolling `MarkdownBody` equivalent.
@Deprecated('Renamed to FlarkMarkdown; will be removed before 1.0.')
typedef Markdown = FlarkMarkdown;

final class _FlarkMarkdownState extends State<FlarkMarkdown> {
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
  void didUpdateWidget(FlarkMarkdown oldWidget) {
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

    final parseBackendChanged = oldWidget.parseBackend != widget.parseBackend;
    final parseProfileChanged =
        oldWidget._effectiveParseProfile != widget._effectiveParseProfile;
    final parseDebounceChanged =
        oldWidget.parseDebounce != widget.parseDebounce;
    final onParseErrorChanged = oldWidget.onParseError != widget.onParseError;
    if (widget.controller == null &&
        (parseBackendChanged ||
            parseProfileChanged ||
            parseDebounceChanged ||
            onParseErrorChanged)) {
      // Forward only what changed: any non-null backend/profile/debounce
      // makes configureParsing restart the parse scheduler, which a
      // callback-only swap (e.g. an inline onParseError closure recreated
      // every build) must not trigger.
      _controller.configureParsing(
        parseBackend: parseBackendChanged ? widget.parseBackend : null,
        parseProfile: parseProfileChanged
            ? widget._effectiveParseProfile
            : null,
        parseDebounce: parseDebounceChanged ? widget.parseDebounce : null,
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
    Widget preview = _previewWithOptionalOverlay();
    if (FlarkMarkdownInteractions.maybeOf(context) == null) {
      preview = FlarkMarkdownInteractions(
        controller: _controller,
        config: widget.interactionConfig,
        editable: false,
        child: preview,
      );
    }
    final theme = widget.theme;
    if (theme != null) {
      preview = FlarkMarkdownTheme(data: theme, child: preview);
    }
    return preview;
  }

  Widget _previewWithOptionalOverlay() {
    Widget preview = FlarkReadOnlyPreview(
      controller: _controller,
      textStyle: widget.textStyle,
      blockBuilder: widget.blockBuilder,
    );
    if (widget.selectable) {
      preview = SelectableRegion(
        selectionControls:
            flarkTextSelectionControlsForPlatform(context) ??
            emptyTextSelectionControls,
        child: preview,
      );
    }
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
      parseProfile:
          widget._effectiveParseProfile ?? FlarkMarkdownProfile.commonMarkGfm,
      parseDebounce: widget.parseDebounce ?? const Duration(milliseconds: 80),
      onParseError: widget.onParseError,
    );
  }
}
