import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_image_popover.dart';
import 'flark_inline_image.dart';
import 'flark_link_popover.dart';

typedef FlarkLinkOpenCallback = void Function(String destination);
typedef FlarkLinkEditCallback =
    void Function(BuildContext context, FlarkRenderOverlayTarget target);

final class FlarkCodeLanguageOption {
  const FlarkCodeLanguageOption({required this.value, required this.label});

  final String value;
  final String label;
}

final class FlarkMarkdownInteractionConfig {
  const FlarkMarkdownInteractionConfig({
    this.codeLanguages = standardCodeLanguages,
    this.enableCodeFenceLanguagePicker = true,
    this.enableLinkMenus = true,
    this.enableTaskCheckboxToggles = true,
    this.enableImageMenus = true,
    this.onOpenLink,
    this.onEditLink,
    this.onOpenImage,
    this.onEditImage,
    this.linkActions,
    this.imageActions,
    this.imageBuilder,
  });

  static const standardCodeLanguages = <FlarkCodeLanguageOption>[
    FlarkCodeLanguageOption(value: '', label: 'Auto'),
    FlarkCodeLanguageOption(value: 'text', label: 'Plain text'),
    FlarkCodeLanguageOption(value: 'dart', label: 'Dart'),
    FlarkCodeLanguageOption(value: 'markdown', label: 'Markdown'),
    FlarkCodeLanguageOption(value: 'json', label: 'JSON'),
    FlarkCodeLanguageOption(value: 'yaml', label: 'YAML'),
    FlarkCodeLanguageOption(value: 'sql', label: 'SQL'),
    FlarkCodeLanguageOption(value: 'javascript', label: 'JavaScript'),
    FlarkCodeLanguageOption(value: 'typescript', label: 'TypeScript'),
    FlarkCodeLanguageOption(value: 'python', label: 'Python'),
    FlarkCodeLanguageOption(value: 'rust', label: 'Rust'),
    FlarkCodeLanguageOption(value: 'swift', label: 'Swift'),
    FlarkCodeLanguageOption(value: 'kotlin', label: 'Kotlin'),
    FlarkCodeLanguageOption(value: 'shell', label: 'Shell'),
  ];

  final List<FlarkCodeLanguageOption> codeLanguages;
  final bool enableCodeFenceLanguagePicker;
  final bool enableLinkMenus;
  final bool enableTaskCheckboxToggles;
  final bool enableImageMenus;
  final FlarkLinkOpenCallback? onOpenLink;
  final FlarkLinkEditCallback? onEditLink;

  /// Invoked to open an image's source URL (e.g. in a browser).
  final FlarkLinkOpenCallback? onOpenImage;

  /// Invoked to edit an image — the app shows UI and calls `applyImageEdit`.
  /// The target carries the current alt (`action.label`) and URL
  /// (`action.destination`).
  final FlarkLinkEditCallback? onEditImage;

  /// The actions shown in the inline link popover. Defaults to
  /// [FlarkLinkAction.defaults] (Open · Edit · Copy · Remove). Override to add,
  /// remove, or reorder — e.g. `[...FlarkLinkAction.defaults, myAction]`.
  final List<FlarkLinkAction>? linkActions;

  /// The actions shown in the inline image popover. Defaults to
  /// [FlarkImageAction.defaults] (Open · Edit · Copy · Remove).
  final List<FlarkImageAction>? imageActions;

  /// Resolves an inline image's URL into a widget (e.g. `Image.file`, an asset,
  /// or a cached/authenticated provider). When null, Flark renders http(s)
  /// images with [Image.network] and falls back to a labelled card otherwise.
  final FlarkInlineImageBuilder? imageBuilder;
}

final class FlarkMarkdownInteractions extends InheritedWidget {
  const FlarkMarkdownInteractions({
    super.key,
    required this.controller,
    required this.config,
    required this.editable,
    required super.child,
  });

  final FlarkFlutterController controller;
  final FlarkMarkdownInteractionConfig config;
  final bool editable;

  static FlarkMarkdownInteractions? maybeOf(BuildContext context) {
    return context
        .dependOnInheritedWidgetOfExactType<FlarkMarkdownInteractions>();
  }

  bool setCodeFenceLanguage(FlarkRenderBlock block, String language) {
    if (!editable || block.codeBlock == null) return false;
    return _handled(
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: FlarkSetFenceLanguagePayload(
          codeBlockRange: block.sourceRange,
          language: language,
        ),
      ),
    );
  }

  bool setTaskListChecked(FlarkRenderBlock block, bool checked) {
    if (!editable || block.taskListItem == null) return false;
    return _handled(
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.setTaskListChecked,
        payload: FlarkSetTaskListCheckedPayload(
          taskItemRange: block.sourceRange,
          checked: checked,
        ),
      ),
    );
  }

  void openLink(FlarkRenderOverlayTarget target) {
    openTarget(target);
  }

  void openTarget(FlarkRenderOverlayTarget target) {
    final destination = target.action?.destination;
    if (destination == null || destination.isEmpty) return;
    config.onOpenLink?.call(destination);
  }

  void editLink(BuildContext context, FlarkRenderOverlayTarget target) {
    editTarget(context, target);
  }

  void editTarget(BuildContext context, FlarkRenderOverlayTarget target) {
    if (!editable) return;
    controller.applySelection(
      FlarkSelection(
        baseOffset: target.sourceRange.start,
        extentOffset: target.sourceRange.end,
      ),
      userEvent: 'selection.inlineAction',
    );
    if (target.action?.kind == FlarkRenderInlineActionKind.link) {
      config.onEditLink?.call(context, target);
    }
  }

  void editImage(BuildContext context, FlarkRenderOverlayTarget target) {
    if (!editable) return;
    controller.applySelection(
      FlarkSelection(
        baseOffset: target.sourceRange.start,
        extentOffset: target.sourceRange.end,
      ),
      userEvent: 'selection.inlineAction',
    );
    if (target.action?.kind == FlarkRenderInlineActionKind.image) {
      config.onEditImage?.call(context, target);
    }
  }

  Future<void> copyLink(FlarkRenderOverlayTarget target) async {
    await copyTarget(target);
  }

  Future<void> copyTarget(FlarkRenderOverlayTarget target) async {
    final destination = target.action?.destination;
    if (destination == null || destination.isEmpty) return;
    await Clipboard.setData(ClipboardData(text: destination));
  }

  bool removeLink(FlarkRenderOverlayTarget target) {
    if (!editable || target.action?.kind != FlarkRenderInlineActionKind.link) {
      return false;
    }
    return _handled(
      controller.dispatch(
        command: FlarkMarkdownLinkCommands.removeLink,
        payload: FlarkRemoveLinkPayload(linkRange: target.sourceRange),
      ),
    );
  }

  bool _handled(FlarkEditorRuntimeResult result) {
    return result.commandResult.isHandled &&
        result.commandResult.transaction != null;
  }

  @override
  bool updateShouldNotify(FlarkMarkdownInteractions oldWidget) {
    return oldWidget.controller != controller ||
        oldWidget.config != config ||
        oldWidget.editable != editable;
  }
}
