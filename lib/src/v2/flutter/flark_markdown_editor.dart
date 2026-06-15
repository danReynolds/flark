import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core.dart' show FlarkExtensionSet;
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_command_actions.dart';
import 'flark_editable_text.dart';
import 'flark_editor_read_only_scope.dart';
import 'flark_markdown_theme.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_interactions.dart';
import 'flark_markdown_shortcuts.dart';
import 'flark_projected_editable_text.dart';
import 'flark_render_plan_overlay_controls.dart';

enum FlarkMarkdownEditingMode { source, liveRendered }

/// A markdown editor whose document is always canonical Markdown source.
final class FlarkMarkdownEditor extends StatefulWidget {
  const FlarkMarkdownEditor({
    super.key,
    this.controller,
    this.initialMarkdown,
    this.extensions,
    this.onChanged,
    this.parseBackend,
    this.onParseError,
    this.parseProfile,
    @Deprecated('Renamed to parseProfile; will be removed before 1.0.')
    this.profile,
    this.parseDebounce,
    this.editingMode = FlarkMarkdownEditingMode.liveRendered,
    this.readOnly = false,
    this.focusNode,
    this.style,
    this.theme,
    this.cursorColor,
    this.backgroundCursorColor = const Color(0x00000000),
    this.minLines,
    this.maxLines,
    this.expands = false,
    this.autofocus = false,
    this.shortcuts = const <ShortcutActivator, Intent>{},
    this.useDefaultShortcuts = true,
    this.showOverlayControls = false,
    this.overlayControlBuilder,
    this.onOverlayTargetPressed,
    this.interactionConfig = const FlarkMarkdownInteractionConfig(),
  }) : assert(
         controller == null || initialMarkdown == null,
         'Provide either controller or initialMarkdown, not both.',
       ),
       assert(
         parseProfile == null || profile == null,
         'Provide parseProfile only; profile is its deprecated alias.',
       ),
       assert(
         controller == null ||
             (extensions == null &&
                 parseBackend == null &&
                 onParseError == null &&
                 parseProfile == null &&
                 profile == null &&
                 parseDebounce == null),
         'When a controller is provided it owns parsing. Configure extensions, '
         'parseBackend, parseProfile, parseDebounce, and onParseError on the '
         'FlarkFlutterController instead.',
       );

  /// Controller for shared editor state.
  ///
  /// When omitted, the editor creates and owns a controller initialized from
  /// [initialMarkdown].
  final FlarkFlutterController? controller;

  /// Markdown used when this widget creates its own controller.
  ///
  /// This is initial-only for the widget state. Use a new widget [Key] to switch
  /// documents, or provide [controller] when document identity is managed
  /// outside the widget.
  final String? initialMarkdown;

  /// Runtime extensions for the owned controller.
  ///
  /// When [controller] is provided, configure extensions on that controller.
  final FlarkExtensionSet? extensions;

  /// Called after document-changing controller events.
  ///
  /// Delivery is asynchronous (a microtask after the edit is applied), unlike
  /// [TextField.onChanged]'s synchronous callback; read
  /// [FlarkFlutterController.markdown] for the always-current value.
  final ValueChanged<String>? onChanged;

  /// Parser backend used to adopt authoritative markdown structure.
  ///
  /// Used only when this widget creates its own controller from
  /// [initialMarkdown]. When a [controller] is supplied, configure the backend
  /// on the controller instead. When null, the packaged Comrak backend is
  /// required; load failures are surfaced directly instead of falling back to a
  /// second markdown implementation.
  final FlarkMarkdownParseBackend? parseBackend;

  /// Called when a scheduled background parse fails.
  ///
  /// Used only for the widget-owned controller. With a supplied [controller],
  /// configure `onParseError` on the controller.
  final void Function(Object error, StackTrace stackTrace)? onParseError;

  /// Markdown profile for the widget-owned controller. Defaults to GFM.
  final FlarkMarkdownProfile? parseProfile;

  /// Deprecated alias of [parseProfile].
  @Deprecated('Renamed to parseProfile; will be removed before 1.0.')
  final FlarkMarkdownProfile? profile;

  // ignore: deprecated_member_use_from_same_package
  FlarkMarkdownProfile? get _effectiveParseProfile => parseProfile ?? profile;

  /// Parse debounce for the widget-owned controller. Defaults to 80ms.
  final Duration? parseDebounce;
  final FlarkMarkdownEditingMode editingMode;

  /// Whether the editor displays the document without accepting edits.
  ///
  /// Selection and caret navigation remain available; text input, structural
  /// commands, keyboard shortcuts, checkbox toggles, and link/code-fence
  /// mutations are disabled.
  final bool readOnly;
  final FocusNode? focusNode;
  final TextStyle? style;

  /// Colors for markdown chrome (code fences, quotes, links, tables…).
  ///
  /// When null, the ambient [FlarkMarkdownTheme] applies, falling back to a
  /// platform-brightness default.
  final FlarkMarkdownThemeData? theme;
  final Color? cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;

  /// Additional keyboard shortcuts mapped to Markdown command intents.
  ///
  /// Build intents with [FlarkMarkdownShortcuts] (e.g.
  /// `FlarkMarkdownShortcuts.toggleStrong()`) rather than constructing
  /// [FlarkCommandIntent] directly. These merge over [useDefaultShortcuts] and
  /// override any default binding for the same activator.
  final Map<ShortcutActivator, Intent> shortcuts;

  /// Whether to install [FlarkMarkdownShortcuts.defaults] (bold/italic/code/
  /// strikethrough). User-provided [shortcuts] override matching defaults.
  final bool useDefaultShortcuts;
  final bool showOverlayControls;
  final FlarkOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<FlarkRenderOverlayTarget>? onOverlayTargetPressed;
  final FlarkMarkdownInteractionConfig interactionConfig;

  @override
  State<FlarkMarkdownEditor> createState() {
    return _FlarkMarkdownEditorState();
  }
}

/// Deprecated name of [FlarkMarkdownEditor].
@Deprecated('Renamed to FlarkMarkdownEditor; will be removed before 1.0.')
typedef MarkdownEditor = FlarkMarkdownEditor;

final class _FlarkMarkdownEditorState extends State<FlarkMarkdownEditor> {
  FlarkFlutterController? _ownedController;
  StreamSubscription<String>? _eventSubscription;

  FlarkFlutterController get _controller {
    return widget.controller ?? _ownedController!;
  }

  @override
  void initState() {
    super.initState();
    _ensureOwnedController();
    _listenForDocumentChanges();
    _controller.attachParsingSurface();
  }

  @override
  void didUpdateWidget(FlarkMarkdownEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    final controllerChanged = oldWidget.controller != widget.controller;
    final ownedExtensionsChanged =
        widget.controller == null && oldWidget.extensions != widget.extensions;

    if (controllerChanged || ownedExtensionsChanged) {
      final previousController = oldWidget.controller ?? _ownedController!;
      _eventSubscription?.cancel();
      previousController.detachParsingSurface();
      if (widget.controller == null) {
        _ownedController?.dispose();
        _ownedController = _createOwnedController(previousController.markdown);
      } else {
        _ownedController?.dispose();
        _ownedController = null;
      }
      _listenForDocumentChanges();
      _controller.attachParsingSurface();
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
    _eventSubscription?.cancel();
    if (widget.controller != null) {
      widget.controller!.detachParsingSurface();
    }
    _ownedController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    final theme = widget.theme ?? FlarkMarkdownTheme.of(context);
    final cursorColor = _effectiveCursorColor(
      context,
      widget.cursorColor,
      theme,
    );
    final shortcuts = _effectiveShortcuts();
    final editor = switch (widget.editingMode) {
      FlarkMarkdownEditingMode.source => FlarkEditableText(
        controller: controller,
        focusNode: widget.focusNode,
        style: widget.style,
        cursorColor: cursorColor,
        backgroundCursorColor: widget.backgroundCursorColor,
        minLines: widget.minLines,
        maxLines: widget.maxLines,
        expands: widget.expands,
        autofocus: widget.autofocus,
        shortcuts: shortcuts,
      ),
      FlarkMarkdownEditingMode.liveRendered => FlarkLiveRenderedEditableText(
        controller: controller,
        focusNode: widget.focusNode,
        style: widget.style,
        cursorColor: cursorColor,
        backgroundCursorColor: widget.backgroundCursorColor,
        minLines: widget.minLines,
        maxLines: widget.maxLines,
        expands: widget.expands,
        autofocus: widget.autofocus,
        shortcuts: shortcuts,
      ),
    };

    final Widget surface;
    if (!widget.showOverlayControls) {
      surface = editor;
    } else {
      surface = Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          FlarkRenderPlanOverlayControls(
            controller: controller,
            builder: widget.overlayControlBuilder,
            onPressed: widget.onOverlayTargetPressed,
          ),
          editor,
        ],
      );
    }

    Widget result = FlarkEditorReadOnlyScope(
      readOnly: widget.readOnly,
      child: FlarkMarkdownInteractions(
        controller: controller,
        config: widget.interactionConfig,
        editable: !widget.readOnly,
        child: surface,
      ),
    );
    if (widget.theme != null) {
      result = FlarkMarkdownTheme(data: theme, child: result);
    }
    return result;
  }

  void _ensureOwnedController() {
    if (widget.controller != null) return;
    _ownedController = _createOwnedController(widget.initialMarkdown ?? '');
  }

  FlarkFlutterController _createOwnedController(String markdown) {
    return FlarkFlutterController.fromMarkdown(
      markdown,
      extensions: widget.extensions,
      parseBackend: widget.parseBackend,
      parseProfile:
          widget._effectiveParseProfile ?? FlarkMarkdownProfile.commonMarkGfm,
      parseDebounce: widget.parseDebounce ?? const Duration(milliseconds: 80),
      onParseError: widget.onParseError,
    );
  }

  Map<ShortcutActivator, Intent> _effectiveShortcuts() {
    if (widget.readOnly) return const {};
    if (!widget.useDefaultShortcuts) return widget.shortcuts;
    return <ShortcutActivator, Intent>{
      ...FlarkMarkdownShortcuts.defaults(),
      ...widget.shortcuts,
    };
  }

  void _listenForDocumentChanges() {
    _eventSubscription = _controller.markdownChanges.listen((markdown) {
      widget.onChanged?.call(markdown);
    });
  }
}

Color _effectiveCursorColor(
  BuildContext context,
  Color? explicitColor,
  FlarkMarkdownThemeData theme,
) {
  return explicitColor ??
      DefaultSelectionStyle.of(context).cursorColor ??
      theme.cursorColor;
}
