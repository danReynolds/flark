part of 'sovereign_controller.dart';

class _ControllerSovereignValueMutationHost
    implements SovereignValueMutationHost {
  _ControllerSovereignValueMutationHost(this._controller);

  final SovereignController _controller;

  @override
  TextEditingValue get currentValue => _controller.value;

  @override
  bool get isApplyingVerticalCaretMove =>
      _controller._isApplyingVerticalCaretMove;

  @override
  void clearPreferredVerticalCaretColumn() {
    _controller._preferredVerticalCaretColumn = null;
  }

  @override
  TextEditingValue? get compositionStartValue =>
      _controller._compositionStartValue;

  @override
  set compositionStartValue(TextEditingValue? value) =>
      _controller._compositionStartValue = value;

  @override
  void forceUndoBoundary() {
    _controller._forceUndoBoundaryForNextTextOp = true;
  }

  @override
  TextEditingValue normalizeProjectedSelectAllDelete(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._normalizeProjectedSelectAllDelete(oldValue, newValue);

  @override
  TextSelection clampSelectionToText(TextSelection selection, int textLength) {
    return SelectionMaskUtils.clampSelectionToText(selection, textLength);
  }

  @override
  TextSelection projectSelection({
    required TextSelection selection,
    required TextSelection previousSelection,
  }) =>
      _controller._projector.projectSelection(
        selection,
        previousSelection: previousSelection,
      );

  @override
  TextSelection snapSelectionWithCursorMask(
    TextSelection selection, {
    required int textLength,
  }) =>
      _controller._snapSelectionWithCursorMask(
        selection,
        textLength: textLength,
      );

  @override
  TextEditingValue applyEditTransformPipeline(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._applyEditTransformPipeline(oldValue, newValue);

  @override
  EditOp createOp(
    TextEditingValue oldVal,
    TextEditingValue newVal, {
    TextEditingValue? undoBeforeOverride,
  }) =>
      _controller._createOp(
        oldVal,
        newVal,
        undoBeforeOverride: undoBeforeOverride,
      );

  @override
  void applyOp(EditOp op) => _controller._applyOp(op);

  @override
  void setControllerSuperValue(TextEditingValue value) =>
      _controller._setControllerSuperValue(value);

  @override
  bool normalizeCommittedSelectionAfterProjection() =>
      _controller._normalizeCommittedSelectionAfterProjection();

  @override
  void updateProjection({TextEditingValue? overrideValue}) =>
      _controller._updateProjection(overrideValue: overrideValue);
}
