part of 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

extension _SovereignEditorTaskCheckboxOverlay on _SovereignEditorState {
  Widget _buildTaskCheckboxTapTargets() {
    if (widget.controller.value.composing.isValid) {
      return const SizedBox.shrink();
    }
    final taskCheckboxTheme = _editorThemeData.taskCheckbox;
    if (_taskCheckboxTargetsTemporarilyHidden) {
      if (_cachedTaskCheckboxTargets.isEmpty) return const SizedBox.shrink();
      return _buildTaskCheckboxTargetsLayer(
        targets: _cachedTaskCheckboxTargets,
        taskCheckboxTheme: taskCheckboxTheme,
      );
    }

    final stackContext = _editorLayersKey.currentContext;
    final stackRender = stackContext?.findRenderObject();
    if (stackRender is! RenderBox) {
      _scheduleTaskCheckboxTargetsRefresh();
      return const SizedBox.shrink();
    }

    final targets = _computeTaskCheckboxTapTargets(stackRender);
    _cachedTaskCheckboxTargets = targets;
    if (targets.isEmpty) return const SizedBox.shrink();

    return _buildTaskCheckboxTargetsLayer(
      targets: targets,
      taskCheckboxTheme: taskCheckboxTheme,
    );
  }

  Widget _buildTaskCheckboxTargetsLayer({
    required List<_TaskCheckboxTapTargetData> targets,
    required SovereignTaskCheckboxTheme taskCheckboxTheme,
  }) {
    return Positioned.fill(
      child: Stack(
        children: [
          for (final target in targets)
            Positioned.fromRect(
              rect: target.hitRect,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: () {
                  final toggled = widget.controller.toggleTaskCheckboxAtOffset(
                    target.checkboxRange.start,
                    insertIfList: false,
                  );
                  if (toggled) {
                    _effectiveFocusNode.requestFocus();
                    _scheduleTaskCheckboxTargetsRefresh();
                    _scheduleLinkActionsOverlaySync();
                  }
                },
                child: Semantics(
                  button: true,
                  label: target.isChecked
                      ? 'Uncheck task item'
                      : 'Check task item',
                  child: MouseRegion(
                    cursor: SystemMouseCursors.click,
                    child: Stack(
                      children: [
                        const SizedBox(
                          key: _kTaskCheckboxTapTargetKey,
                          width: double.infinity,
                          height: double.infinity,
                        ),
                        if (taskCheckboxTheme.useCustomOverlay)
                          _buildTaskCheckboxVisual(
                            target: target,
                            theme: taskCheckboxTheme,
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }

  List<_TaskCheckboxTapTargetData> _computeTaskCheckboxTapTargets(
    RenderBox targetRenderBox,
  ) {
    final text = widget.controller.text;
    if (text.isEmpty) return const [];

    final lineIndex = widget.controller.decoration.lineIndex;
    if (lineIndex.lineCount <= 0) return const [];

    final out = <_TaskCheckboxTapTargetData>[];
    for (var line = 0; line < lineIndex.lineCount; line++) {
      final checkboxRange = widget.controller.taskCheckboxMarkerRangeForLine(
        line,
      );
      if (checkboxRange == null) continue;
      final visualRange =
          widget.controller.taskCheckboxVisualRangeForLine(line) ??
              checkboxRange;
      if (_isOffsetInsideCodeBlock(visualRange.start)) continue;

      final startRect = _caretRectInTargetSpaceForOffset(
        visualRange.start,
        targetRenderBox: targetRenderBox,
      );
      final endRect = _caretRectInTargetSpaceForOffset(
        visualRange.end,
        targetRenderBox: targetRenderBox,
        affinity: TextAffinity.upstream,
      );
      if (startRect == null || endRect == null) continue;

      final top = startRect.top;
      final bottom =
          (startRect.bottom > top ? startRect.bottom : top + _lineHeightPixels);
      final left = startRect.left;
      final right = (endRect.left > left ? endRect.left : left + 26.0);
      final markerRect = Rect.fromLTRB(left, top, right, bottom);
      final hitRect = markerRect.inflate(4);

      final markerText = text.substring(checkboxRange.start, checkboxRange.end);
      final isChecked = markerText.length >= 2 &&
          (markerText.codeUnitAt(1) == 120 || markerText.codeUnitAt(1) == 88);

      out.add(
        _TaskCheckboxTapTargetData(
          hitRect: hitRect,
          markerRect: markerRect,
          checkboxRange: checkboxRange,
          isChecked: isChecked,
        ),
      );
    }
    return out;
  }

  Widget _buildTaskCheckboxVisual({
    required _TaskCheckboxTapTargetData target,
    required SovereignTaskCheckboxTheme theme,
  }) {
    final boxSize = theme.size.clamp(8.0, 28.0);
    final markerOffset = Offset(
      target.markerRect.left - target.hitRect.left,
      target.markerRect.top - target.hitRect.top,
    );
    final maxLeft = (target.hitRect.width - boxSize).clamp(
      0.0,
      double.infinity,
    );
    final maxTop = (target.hitRect.height - boxSize).clamp(
      0.0,
      double.infinity,
    );
    final left = (markerOffset.dx + theme.horizontalInset).clamp(0.0, maxLeft);
    final centeredTop =
        markerOffset.dy + ((target.markerRect.height - boxSize) / 2);
    final top = (centeredTop + theme.verticalInset).clamp(0.0, maxTop);

    final fill =
        target.isChecked ? theme.checkedFillColor : theme.uncheckedFillColor;
    final border = target.isChecked
        ? theme.checkedBorderColor
        : theme.uncheckedBorderColor;

    return Positioned(
      left: left,
      top: top,
      width: boxSize,
      height: boxSize,
      child: DecoratedBox(
        key: _kTaskCheckboxVisualKey,
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(theme.borderRadius),
          border: Border.all(color: border, width: theme.borderWidth),
        ),
        child: target.isChecked
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

class _TaskCheckboxTapTargetData {
  final Rect hitRect;
  final Rect markerRect;
  final TextRange checkboxRange;
  final bool isChecked;

  const _TaskCheckboxTapTargetData({
    required this.hitRect,
    required this.markerRect,
    required this.checkboxRange,
    required this.isChecked,
  });
}
