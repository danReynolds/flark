import 'package:flutter/material.dart';

import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';
import 'read_only_task_checkbox_overlay.dart';

class SovereignReadOnlyTaskCheckboxVisualLayer extends StatelessWidget {
  const SovereignReadOnlyTaskCheckboxVisualLayer({
    super.key,
    required this.visuals,
    required this.theme,
    required this.padding,
  });

  static const Key visualKey = Key('SovereignMarkdownViewTaskCheckboxVisual');

  final List<SovereignReadOnlyTaskCheckboxVisualData> visuals;
  final SovereignTaskCheckboxTheme theme;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    if (visuals.isEmpty) return const SizedBox.shrink();

    return Positioned.fill(
      child: IgnorePointer(
        child: Padding(
          padding: padding,
          child: Stack(
            children: [
              for (final visual in visuals)
                _SovereignReadOnlyTaskCheckboxVisual(
                  visual: visual,
                  theme: theme,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SovereignReadOnlyTaskCheckboxVisual extends StatelessWidget {
  const _SovereignReadOnlyTaskCheckboxVisual({
    required this.visual,
    required this.theme,
  });

  final SovereignReadOnlyTaskCheckboxVisualData visual;
  final SovereignTaskCheckboxTheme theme;

  @override
  Widget build(BuildContext context) {
    final boxSize = theme.size.clamp(8.0, 28.0);
    final left = visual.markerRect.left + theme.horizontalInset;
    final top = visual.markerRect.top +
        ((visual.markerRect.height - boxSize) / 2) +
        theme.verticalInset;
    final fill =
        visual.isChecked ? theme.checkedFillColor : theme.uncheckedFillColor;
    final border = visual.isChecked
        ? theme.checkedBorderColor
        : theme.uncheckedBorderColor;

    return Positioned(
      left: left,
      top: top,
      width: boxSize,
      height: boxSize,
      child: DecoratedBox(
        key: SovereignReadOnlyTaskCheckboxVisualLayer.visualKey,
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(theme.borderRadius),
          border: Border.all(color: border, width: theme.borderWidth),
        ),
        child: visual.isChecked
            ? Center(
                child: Icon(
                  Icons.check_rounded,
                  size: theme.checkIconSize,
                  color: theme.checkColor,
                ),
              )
            : const SizedBox.shrink(),
      ),
    );
  }
}
