import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core.dart' show SovereignExtensionSet;
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'sovereign_command_actions.dart';
import 'sovereign_editable_text.dart';
import 'sovereign_flutter_controller.dart';
import 'sovereign_markdown_interactions.dart';
import 'sovereign_parse_scheduler.dart';
import 'sovereign_projected_editable_text.dart';
import 'sovereign_render_plan_overlay_controls.dart';

enum SovereignMarkdownEditingMode { source, projected, liveRendered }

final class MarkdownEditor extends StatefulWidget {
  const MarkdownEditor({
    super.key,
    this.controller,
    this.initialMarkdown,
    this.extensions,
    this.onChanged,
    this.parseBackend,
    this.onParseError,
    this.profile = SovereignMarkdownProfile.commonMarkGfm,
    this.parseDebounce = const Duration(milliseconds: 80),
    this.editingMode = SovereignMarkdownEditingMode.projected,
    this.focusNode,
    this.style,
    this.cursorColor = const Color(0xFF006ADC),
    this.backgroundCursorColor = const Color(0x00000000),
    this.minLines,
    this.maxLines,
    this.expands = false,
    this.autofocus = false,
    this.shortcuts = const <ShortcutActivator, SovereignCommandIntent>{},
    this.showOverlayControls = false,
    this.overlayControlBuilder,
    this.onOverlayTargetPressed,
    this.interactionConfig = const SovereignMarkdownInteractionConfig(),
  }) : assert(
         controller == null || initialMarkdown == null,
         'Provide either controller or initialMarkdown, not both.',
       ),
       assert(
         controller == null || extensions == null,
         'Provide extensions through SovereignFlutterController when using a '
         'controller.',
       );

  /// Controller for shared editor state.
  ///
  /// When omitted, the editor creates and owns a controller initialized from
  /// [initialMarkdown].
  final SovereignFlutterController? controller;

  /// Markdown used when this widget creates its own controller.
  ///
  /// This is initial-only for the widget state. Use a new widget [Key] to switch
  /// documents, or provide [controller] when document identity is managed
  /// outside the widget.
  final String? initialMarkdown;

  /// Runtime extensions for the owned controller.
  ///
  /// When [controller] is provided, configure extensions on that controller.
  final SovereignExtensionSet? extensions;

  /// Called after document-changing controller events.
  final ValueChanged<String>? onChanged;

  /// Parser backend used to adopt authoritative markdown structure.
  ///
  /// When this is null, the widget requires the packaged Comrak backend.
  /// Backend load failures are surfaced directly instead of falling back to a
  /// second markdown implementation.
  final SovereignMarkdownParseBackend? parseBackend;

  /// Called when a scheduled background parse fails.
  final void Function(Object error, StackTrace stackTrace)? onParseError;

  final SovereignMarkdownProfile profile;
  final Duration parseDebounce;
  final SovereignMarkdownEditingMode editingMode;
  final FocusNode? focusNode;
  final TextStyle? style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, SovereignCommandIntent> shortcuts;
  final bool showOverlayControls;
  final SovereignOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<SovereignRenderOverlayTarget>? onOverlayTargetPressed;
  final SovereignMarkdownInteractionConfig interactionConfig;

  @override
  State<MarkdownEditor> createState() {
    return _MarkdownEditorState();
  }
}

final class _MarkdownEditorState extends State<MarkdownEditor> {
  SovereignFlutterController? _ownedController;
  SovereignParseScheduler? _parseScheduler;
  StreamSubscription<SovereignControllerEvent>? _eventSubscription;

  SovereignFlutterController get _controller {
    return widget.controller ?? _ownedController!;
  }

  @override
  void initState() {
    super.initState();
    _ensureOwnedController();
    _listenForDocumentChanges();
    _configureParseScheduler();
  }

  @override
  void didUpdateWidget(MarkdownEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    final controllerChanged = oldWidget.controller != widget.controller;
    final ownedExtensionsChanged =
        widget.controller == null && oldWidget.extensions != widget.extensions;

    if (controllerChanged || ownedExtensionsChanged) {
      _parseScheduler?.dispose();
      _eventSubscription?.cancel();
      if (widget.controller == null) {
        final markdown = oldWidget.controller?.markdown ?? _controller.markdown;
        _ownedController?.dispose();
        _ownedController = _createOwnedController(markdown);
      } else {
        _ownedController?.dispose();
        _ownedController = null;
      }
      _listenForDocumentChanges();
      _configureParseScheduler();
      return;
    }

    if (oldWidget.controller != widget.controller ||
        oldWidget.parseBackend != widget.parseBackend ||
        oldWidget.onParseError != widget.onParseError ||
        oldWidget.profile != widget.profile ||
        oldWidget.parseDebounce != widget.parseDebounce) {
      _parseScheduler?.dispose();
      _configureParseScheduler();
    }
  }

  @override
  void dispose() {
    _parseScheduler?.dispose();
    _eventSubscription?.cancel();
    _ownedController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    final editor = switch (widget.editingMode) {
      SovereignMarkdownEditingMode.source => SovereignEditableText(
        controller: controller,
        focusNode: widget.focusNode,
        style: widget.style,
        cursorColor: widget.cursorColor,
        backgroundCursorColor: widget.backgroundCursorColor,
        minLines: widget.minLines,
        maxLines: widget.maxLines,
        expands: widget.expands,
        autofocus: widget.autofocus,
        shortcuts: widget.shortcuts,
      ),
      SovereignMarkdownEditingMode.projected => SovereignProjectedEditableText(
        controller: controller,
        focusNode: widget.focusNode,
        style: widget.style,
        cursorColor: widget.cursorColor,
        backgroundCursorColor: widget.backgroundCursorColor,
        minLines: widget.minLines,
        maxLines: widget.maxLines,
        expands: widget.expands,
        autofocus: widget.autofocus,
        shortcuts: widget.shortcuts,
      ),
      SovereignMarkdownEditingMode.liveRendered =>
        SovereignLiveRenderedEditableText(
          controller: controller,
          focusNode: widget.focusNode,
          style: widget.style,
          cursorColor: widget.cursorColor,
          backgroundCursorColor: widget.backgroundCursorColor,
          minLines: widget.minLines,
          maxLines: widget.maxLines,
          expands: widget.expands,
          autofocus: widget.autofocus,
          shortcuts: widget.shortcuts,
        ),
    };

    if (!widget.showOverlayControls) {
      return SovereignMarkdownInteractions(
        controller: controller,
        config: widget.interactionConfig,
        editable: true,
        child: editor,
      );
    }

    return SovereignMarkdownInteractions(
      controller: controller,
      config: widget.interactionConfig,
      editable: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          SovereignRenderPlanOverlayControls(
            controller: controller,
            builder: widget.overlayControlBuilder,
            onPressed: widget.onOverlayTargetPressed,
          ),
          editor,
        ],
      ),
    );
  }

  void _ensureOwnedController() {
    if (widget.controller != null) return;
    _ownedController = _createOwnedController(widget.initialMarkdown ?? '');
  }

  SovereignFlutterController _createOwnedController(String markdown) {
    return SovereignFlutterController.fromMarkdown(
      markdown,
      extensions: widget.extensions,
    );
  }

  void _listenForDocumentChanges() {
    _eventSubscription = _controller.events.listen((event) {
      if (!event.markdownChanged) return;
      widget.onChanged?.call(_controller.markdown);
    });
  }

  void _configureParseScheduler() {
    final backend = widget.parseBackend ?? _resolveDefaultParseBackend();
    _parseScheduler = SovereignParseScheduler(
      controller: _controller,
      backend: backend,
      profile: widget.profile,
      debounce: widget.parseDebounce,
      onError: widget.onParseError,
    )..start();
  }

  SovereignMarkdownParseBackend _resolveDefaultParseBackend() {
    return SovereignNativeComrakParseBackend.requiredDefault();
  }
}
