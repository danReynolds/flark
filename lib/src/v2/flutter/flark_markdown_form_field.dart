import 'package:flutter/widgets.dart';

import '../core/core.dart'
    show
        FlarkExtensionSet,
        FlarkSelection,
        FlarkSourceOperation,
        FlarkSourceRange,
        FlarkTransaction;
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_command_actions.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_editor.dart';
import 'flark_markdown_interactions.dart';
import 'flark_render_plan_overlay_controls.dart';

final class MarkdownEditorFormField extends FormField<String> {
  MarkdownEditorFormField({
    super.key,
    this.controller,
    this.initialMarkdown,
    this.extensions,
    this.onChanged,
    this.parseBackend,
    this.onParseError,
    this.profile,
    this.parseDebounce,
    this.editingMode = FlarkMarkdownEditingMode.projected,
    this.focusNode,
    this.style,
    this.cursorColor,
    this.backgroundCursorColor = const Color(0x00000000),
    this.minLines,
    this.maxLines,
    this.expands = false,
    this.autofocus = false,
    this.shortcuts = const <ShortcutActivator, FlarkCommandIntent>{},
    this.useDefaultShortcuts = true,
    this.showOverlayControls = false,
    this.overlayControlBuilder,
    this.onOverlayTargetPressed,
    this.interactionConfig = const FlarkMarkdownInteractionConfig(),
    this.errorStyle = const TextStyle(color: Color(0xFFB3261E), fontSize: 12),
    super.errorBuilder,
    super.onSaved,
    super.validator,
    super.autovalidateMode,
    super.restorationId,
  }) : assert(
         controller == null || initialMarkdown == null,
         'Provide either controller or initialMarkdown, not both.',
       ),
       assert(
         controller == null ||
             (extensions == null &&
                 parseBackend == null &&
                 onParseError == null &&
                 profile == null &&
                 parseDebounce == null),
         'When a controller is provided it owns parsing. Configure extensions, '
         'parseBackend, profile, parseDebounce, and onParseError on the '
         'FlarkFlutterController instead.',
       ),
       super(
         initialValue: controller?.markdown ?? initialMarkdown ?? '',
         builder: (field) {
           return (field as _MarkdownEditorFormFieldState)._build(
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
  final FlarkMarkdownProfile? profile;
  final Duration? parseDebounce;
  final FlarkMarkdownEditingMode editingMode;
  final FocusNode? focusNode;
  final TextStyle? style;
  final Color? cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, FlarkCommandIntent> shortcuts;
  final bool useDefaultShortcuts;
  final bool showOverlayControls;
  final FlarkOverlayTargetWidgetBuilder? overlayControlBuilder;
  final ValueChanged<FlarkRenderOverlayTarget>? onOverlayTargetPressed;
  final FlarkMarkdownInteractionConfig interactionConfig;
  final TextStyle? errorStyle;

  @override
  FormFieldState<String> createState() => _MarkdownEditorFormFieldState();
}

final class _MarkdownEditorFormFieldState extends FormFieldState<String> {
  FlarkFlutterController? _ownedController;

  @override
  MarkdownEditorFormField get widget {
    return super.widget as MarkdownEditorFormField;
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
  void didUpdateWidget(MarkdownEditorFormField oldWidget) {
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
  void reset() {
    super.reset();
    _replaceControllerMarkdown(value ?? '');
  }

  Widget _build(BuildContext context) {
    final editor = MarkdownEditor(
      controller: _controller,
      onChanged: _handleMarkdownChanged,
      editingMode: widget.editingMode,
      focusNode: widget.focusNode,
      style: widget.style,
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
          child: Text(errorText, style: widget.errorStyle),
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
      parseProfile: widget.profile ?? FlarkMarkdownProfile.commonMarkGfm,
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
    final current = controller.markdown;
    if (current == markdown) return;
    controller.applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(0, current.length),
          replacementText: markdown,
        ),
        selectionBefore: controller.selection,
        selectionAfter: FlarkSelection.collapsed(markdown.length),
        userEvent: 'form.reset',
      ),
    );
  }
}
