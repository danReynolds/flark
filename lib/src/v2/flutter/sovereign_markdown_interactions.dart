import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'sovereign_flutter_controller.dart';

typedef SovereignLinkOpenCallback = void Function(String destination);
typedef SovereignLinkEditCallback = void Function(
  BuildContext context,
  SovereignRenderOverlayTarget target,
);

final class SovereignCodeLanguageOption {
  const SovereignCodeLanguageOption({
    required this.value,
    required this.label,
  });

  final String value;
  final String label;
}

final class SovereignMarkdownInteractionConfig {
  const SovereignMarkdownInteractionConfig({
    this.codeLanguages = standardCodeLanguages,
    this.enableCodeFenceLanguagePicker = true,
    this.enableLinkMenus = true,
    this.enableTaskCheckboxToggles = true,
    this.onOpenLink,
    this.onEditLink,
  });

  static const standardCodeLanguages = <SovereignCodeLanguageOption>[
    SovereignCodeLanguageOption(value: '', label: 'Auto'),
    SovereignCodeLanguageOption(value: 'text', label: 'Plain text'),
    SovereignCodeLanguageOption(value: 'dart', label: 'Dart'),
    SovereignCodeLanguageOption(value: 'markdown', label: 'Markdown'),
    SovereignCodeLanguageOption(value: 'json', label: 'JSON'),
    SovereignCodeLanguageOption(value: 'yaml', label: 'YAML'),
    SovereignCodeLanguageOption(value: 'sql', label: 'SQL'),
    SovereignCodeLanguageOption(value: 'javascript', label: 'JavaScript'),
    SovereignCodeLanguageOption(value: 'typescript', label: 'TypeScript'),
    SovereignCodeLanguageOption(value: 'python', label: 'Python'),
    SovereignCodeLanguageOption(value: 'rust', label: 'Rust'),
    SovereignCodeLanguageOption(value: 'swift', label: 'Swift'),
    SovereignCodeLanguageOption(value: 'kotlin', label: 'Kotlin'),
    SovereignCodeLanguageOption(value: 'shell', label: 'Shell'),
  ];

  final List<SovereignCodeLanguageOption> codeLanguages;
  final bool enableCodeFenceLanguagePicker;
  final bool enableLinkMenus;
  final bool enableTaskCheckboxToggles;
  final SovereignLinkOpenCallback? onOpenLink;
  final SovereignLinkEditCallback? onEditLink;
}

final class SovereignMarkdownInteractions extends InheritedWidget {
  const SovereignMarkdownInteractions({
    super.key,
    required this.controller,
    required this.config,
    required this.editable,
    required super.child,
  });

  final SovereignFlutterController controller;
  final SovereignMarkdownInteractionConfig config;
  final bool editable;

  static SovereignMarkdownInteractions? maybeOf(BuildContext context) {
    return context
        .dependOnInheritedWidgetOfExactType<SovereignMarkdownInteractions>();
  }

  bool setCodeFenceLanguage(
    SovereignRenderBlock block,
    String language,
  ) {
    if (!editable || block.codeBlock == null) return false;
    return _handled(
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: SovereignSetFenceLanguagePayload(
          codeBlockRange: block.sourceRange,
          language: language,
        ),
      ),
    );
  }

  bool setTaskListChecked(
    SovereignRenderBlock block,
    bool checked,
  ) {
    if (!editable || block.taskListItem == null) return false;
    return _handled(
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.setTaskListChecked,
        payload: SovereignSetTaskListCheckedPayload(
          taskItemRange: block.sourceRange,
          checked: checked,
        ),
      ),
    );
  }

  void openLink(SovereignRenderOverlayTarget target) {
    openTarget(target);
  }

  void openTarget(SovereignRenderOverlayTarget target) {
    final destination = target.action?.destination;
    if (destination == null || destination.isEmpty) return;
    config.onOpenLink?.call(destination);
  }

  void editLink(BuildContext context, SovereignRenderOverlayTarget target) {
    editTarget(context, target);
  }

  void editTarget(BuildContext context, SovereignRenderOverlayTarget target) {
    if (!editable) return;
    controller.applySelection(
      SovereignSelection(
        baseOffset: target.sourceRange.start,
        extentOffset: target.sourceRange.end,
      ),
      userEvent: 'selection.inlineAction',
    );
    if (target.action?.kind == SovereignRenderInlineActionKind.link) {
      config.onEditLink?.call(context, target);
    }
  }

  Future<void> copyLink(SovereignRenderOverlayTarget target) async {
    await copyTarget(target);
  }

  Future<void> copyTarget(SovereignRenderOverlayTarget target) async {
    final destination = target.action?.destination;
    if (destination == null || destination.isEmpty) return;
    await Clipboard.setData(ClipboardData(text: destination));
  }

  bool removeLink(SovereignRenderOverlayTarget target) {
    if (!editable ||
        target.action?.kind != SovereignRenderInlineActionKind.link) {
      return false;
    }
    return _handled(
      controller.dispatch(
        command: SovereignMarkdownLinkCommands.removeLink,
        payload: SovereignRemoveLinkPayload(linkRange: target.sourceRange),
      ),
    );
  }

  bool _handled(SovereignEditorRuntimeResult result) {
    return result.commandResult.isHandled &&
        result.commandResult.transaction != null;
  }

  @override
  bool updateShouldNotify(SovereignMarkdownInteractions oldWidget) {
    return oldWidget.controller != controller ||
        oldWidget.config != config ||
        oldWidget.editable != editable;
  }
}
