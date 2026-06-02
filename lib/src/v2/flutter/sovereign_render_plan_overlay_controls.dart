import 'package:flutter/widgets.dart';

import '../render_plan/render_plan.dart';
import 'sovereign_flutter_controller.dart';

typedef SovereignOverlayTargetWidgetBuilder = Widget Function(
  BuildContext context,
  SovereignRenderOverlayTarget target,
);

final class SovereignRenderPlanOverlayControls extends StatelessWidget {
  const SovereignRenderPlanOverlayControls({
    super.key,
    required this.controller,
    this.builder,
    this.onPressed,
    this.spacing = 6,
    this.runSpacing = 6,
  });

  final SovereignFlutterController controller;
  final SovereignOverlayTargetWidgetBuilder? builder;
  final ValueChanged<SovereignRenderOverlayTarget>? onPressed;
  final double spacing;
  final double runSpacing;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        if (!controller.hasAuthoritativeRenderPlan) {
          return const SizedBox.shrink();
        }

        final targets = controller.renderPlan.overlayPlan().targets;
        if (targets.isEmpty) return const SizedBox.shrink();

        return Wrap(
          spacing: spacing,
          runSpacing: runSpacing,
          children: [
            for (final target in targets)
              builder?.call(context, target) ??
                  _DefaultOverlayTargetControl(
                    target: target,
                    onPressed: onPressed == null
                        ? null
                        : () => onPressed!.call(target),
                  ),
          ],
        );
      },
    );
  }
}

final class _DefaultOverlayTargetControl extends StatelessWidget {
  const _DefaultOverlayTargetControl({
    required this.target,
    this.onPressed,
  });

  final SovereignRenderOverlayTarget target;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style;
    final label = _targetLabel(target);
    final value = _targetValue(target);
    final child = DecoratedBox(
      decoration: BoxDecoration(
        border: Border.all(color: const Color(0xFFB8C1CC)),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Text(
          value.isEmpty ? label : '$label: $value',
          style: style.copyWith(fontSize: (style.fontSize ?? 14) - 1),
        ),
      ),
    );

    return Semantics(
      button: onPressed != null,
      label: label,
      value: value,
      child: MouseRegion(
        cursor:
            onPressed == null ? MouseCursor.defer : SystemMouseCursors.click,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: onPressed,
          child: child,
        ),
      ),
    );
  }
}

String _targetLabel(SovereignRenderOverlayTarget target) {
  return switch (target.kind) {
    SovereignRenderOverlayKind.link => 'Link',
    SovereignRenderOverlayKind.image => 'Image',
    SovereignRenderOverlayKind.taskListItem => 'Task',
    SovereignRenderOverlayKind.table => 'Table',
    SovereignRenderOverlayKind.codeBlock => 'Code',
  };
}

String _targetValue(SovereignRenderOverlayTarget target) {
  return switch (target.kind) {
    SovereignRenderOverlayKind.link => target.action?.destination ?? '',
    SovereignRenderOverlayKind.image =>
      target.action?.label ?? target.action?.destination ?? '',
    SovereignRenderOverlayKind.taskListItem =>
      target.taskListItem?.checked == true ? 'checked' : 'unchecked',
    SovereignRenderOverlayKind.table =>
      '${target.table?.columnAlignments.length ?? 0} columns',
    SovereignRenderOverlayKind.codeBlock =>
      target.codeBlock?.language ?? 'plain text',
  };
}
