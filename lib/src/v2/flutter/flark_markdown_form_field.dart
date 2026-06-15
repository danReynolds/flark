import 'package:flutter/widgets.dart';

import '../core/core.dart'
    show
        FlarkDocument,
        FlarkExtensionSet,
        FlarkSelection,
        FlarkSourceOperation,
        FlarkSourceRange,
        FlarkTransaction;
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_editor.dart';
import 'flark_markdown_theme.dart';
import 'flark_markdown_interactions.dart';
import 'flark_render_plan_overlay_controls.dart';

/// A [FormField] wrapping [FlarkMarkdownEditor], wired into Flutter `Form`
/// validation, saving, and reset flows.
final class FlarkMarkdownEditorFormField extends FormField<String> {
  FlarkMarkdownEditorFormField({
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
    this.errorStyle,
    super.errorBuilder,
    super.onSaved,
    super.validator,
    super.autovalidateMode,
    super.restorationId,
    super.enabled,
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
       ),
       super(
         initialValue: controller?.markdown ?? initialMarkdown ?? '',
         builder: (field) {
           return (field as _FlarkMarkdownEditorFormFieldState)._build(
             field.context,
           );
         },
       );

  final FlarkFlutterController? controller;
  final String? initialMarkdown;
  final FlarkExtensionSet? extensions;
  final ValueChanged<String>? onChanged;
  final FlarkMarkdownParseBackend? parseBackend;
  final void Function(Object error, StackTrace stackTrace)? onParseError;
  final FlarkMarkdownProfile? parseProfile;

  /// Deprecated alias of [parseProfile].
  @Deprecated('Renamed to parseProfile; will be removed before 1.0.')
  final FlarkMarkdownProfile? profile;

  // ignore: deprecated_member_use_from_same_package
  FlarkMarkdownProfile? get _effectiveParseProfile => parseProfile ?? profile;
  final Duration? parseDebounce;
  final FlarkMarkdownEditingMode editingMode;
  final FocusNode? focusNode;
  final TextStyle? style;

  /// Colors for markdown chrome, forwarded to the inner editor.
  final FlarkMarkdownThemeData? theme;
  final Color? cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, Intent> shortcuts;
  final bool useDefaultShortcuts;
  final bool showOverlayControls;
  final FlarkOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<FlarkRenderOverlayTarget>? onOverlayTargetPressed;
  final FlarkMarkdownInteractionConfig interactionConfig;
  final TextStyle? errorStyle;

  @override
  FormFieldState<String> createState() => _FlarkMarkdownEditorFormFieldState();
}

/// Deprecated name of [FlarkMarkdownEditorFormField].
@Deprecated(
  'Renamed to FlarkMarkdownEditorFormField; will be removed before 1.0.',
)
typedef MarkdownEditorFormField = FlarkMarkdownEditorFormField;

final class _FlarkMarkdownEditorFormFieldState extends FormFieldState<String> {
  FlarkFlutterController? _ownedController;

  @override
  FlarkMarkdownEditorFormField get widget {
    return super.widget as FlarkMarkdownEditorFormField;
  }

  FlarkFlutterController get _controller {
    return widget.controller ?? _ownedController!;
  }

  @override
  void initState() {
    super.initState();
    _ensureOwnedController();
  }

  @override
  void didUpdateWidget(FlarkMarkdownEditorFormField oldWidget) {
    super.didUpdateWidget(oldWidget);
    final controllerChanged = oldWidget.controller != widget.controller;
    final ownedExtensionsChanged =
        widget.controller == null && oldWidget.extensions != widget.extensions;

    if (controllerChanged || ownedExtensionsChanged) {
      final currentMarkdown =
          oldWidget.controller?.markdown ?? _ownedController?.markdown ?? value;
      _ownedController?.dispose();
      _ownedController = null;
      _ensureOwnedController(currentMarkdown);
      if (widget.controller != null && value != widget.controller!.markdown) {
        setValue(widget.controller!.markdown);
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
  void reset() {
    super.reset();
    _replaceControllerMarkdown(value ?? '');
  }

  Widget _build(BuildContext context) {
    final editor = FlarkMarkdownEditor(
      controller: _controller,
      readOnly: !widget.enabled,
      onChanged: _handleMarkdownChanged,
      editingMode: widget.editingMode,
      focusNode: widget.focusNode,
      style: widget.style,
      theme: widget.theme,
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      expands: widget.expands,
      autofocus: widget.autofocus,
      shortcuts: widget.shortcuts,
      useDefaultShortcuts: widget.useDefaultShortcuts,
      showOverlayControls: widget.showOverlayControls,
      overlayControlBuilder: widget.overlayControlBuilder,
      onOverlayTargetPressed: widget.onOverlayTargetPressed,
      interactionConfig: widget.interactionConfig,
    );

    if (!hasError) return editor;

    final errorText = this.errorText;
    if (errorText == null) return editor;
    final error =
        widget.errorBuilder?.call(context, errorText) ??
        Padding(
          padding: const EdgeInsets.only(top: 6),
          child: Text(
            errorText,
            style:
                widget.errorStyle ??
                TextStyle(
                  color: (widget.theme ?? FlarkMarkdownTheme.of(context))
                      .errorTextColor,
                  fontSize: 12,
                ),
          ),
        );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [editor, error],
    );
  }

  void _ensureOwnedController([String? markdown]) {
    if (widget.controller != null) return;
    _ownedController = FlarkFlutterController.fromMarkdown(
      markdown ?? value ?? '',
      extensions: widget.extensions,
      parseBackend: widget.parseBackend,
      parseProfile:
          widget._effectiveParseProfile ?? FlarkMarkdownProfile.commonMarkGfm,
      parseDebounce: widget.parseDebounce ?? const Duration(milliseconds: 80),
      onParseError: widget.onParseError,
    );
  }

  void _handleMarkdownChanged(String markdown) {
    if (markdown != value) didChange(markdown);
    widget.onChanged?.call(markdown);
  }

  void _replaceControllerMarkdown(String markdown) {
    final controller = _controller;
    // A reset value comes from app input and may carry CRLF line endings;
    // documents are LF-normalized at ingest, so normalize here too or the
    // equality guard never matches and the replace reinjects `\r` into a
    // document whose line math assumes it cannot be there.
    final normalized = FlarkDocument.normalizeLineEndings(markdown);
    final current = controller.markdown;
    if (current == normalized) return;
    controller.applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(0, current.length),
          replacementText: normalized,
        ),
        selectionBefore: controller.selection,
        selectionAfter: FlarkSelection.collapsed(normalized.length),
        userEvent: 'form.reset',
      ),
    );
  }
}
