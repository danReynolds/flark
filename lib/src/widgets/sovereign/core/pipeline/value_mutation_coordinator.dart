import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

abstract class SovereignValueMutationHost {
  TextEditingValue get currentValue;
  bool get isApplyingVerticalCaretMove;
  void clearPreferredVerticalCaretColumn();

  TextEditingValue? get compositionStartValue;
  set compositionStartValue(TextEditingValue? value);
  void forceUndoBoundary();

  TextEditingValue normalizeProjectedSelectAllDelete(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  );
  TextSelection clampSelectionToText(TextSelection selection, int textLength);
  TextSelection projectSelection({
    required TextSelection selection,
    required TextSelection previousSelection,
  });
  TextSelection snapSelectionWithCursorMask(
    TextSelection selection, {
    required int textLength,
  });
  TextEditingValue applyEditTransformPipeline(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  );
  EditOp createOp(
    TextEditingValue oldVal,
    TextEditingValue newVal, {
    TextEditingValue? undoBeforeOverride,
  });
  void applyOp(EditOp op);
  void setControllerSuperValue(TextEditingValue value);
  bool normalizeCommittedSelectionAfterProjection();
  void updateProjection({TextEditingValue? overrideValue});
}

class SovereignValueMutationCoordinator {
  SovereignValueMutationCoordinator(this._host);

  final SovereignValueMutationHost _host;

  void applyIncomingValue(TextEditingValue newValue) {
    if (newValue == _host.currentValue) return;

    final oldValue = _host.currentValue;
    newValue = _host.normalizeProjectedSelectAllDelete(oldValue, newValue);
    if (!_host.isApplyingVerticalCaretMove &&
        (newValue.text != oldValue.text ||
            newValue.selection != oldValue.selection)) {
      _host.clearPreferredVerticalCaretColumn();
    }
    TextEditingValue? undoBeforeOverride;
    final textChanged = newValue.text != oldValue.text;

    if (newValue.composing.isValid) {
      // IME composition is pass-through.
    } else if (textChanged) {
      final clamped = _host.clampSelectionToText(
        newValue.selection,
        newValue.text.length,
      );
      if (clamped != newValue.selection) {
        newValue = newValue.copyWith(selection: clamped);
      }
    } else {
      final projected = _host.projectSelection(
        selection: newValue.selection,
        previousSelection: oldValue.selection,
      );
      if (projected != newValue.selection) {
        newValue = newValue.copyWith(selection: projected);
      }

      final snapped = _host.snapSelectionWithCursorMask(
        newValue.selection,
        textLength: newValue.text.length,
      );
      if (snapped != newValue.selection) {
        newValue = newValue.copyWith(selection: snapped);
      }
    }

    final oldComposing = oldValue.composing.isValid;
    final newComposing = newValue.composing.isValid;
    if (newComposing && !oldComposing) {
      _host.compositionStartValue = oldValue;
      _host.forceUndoBoundary();
    } else if (!newComposing && oldComposing) {
      undoBeforeOverride = _host.compositionStartValue ?? oldValue;
      _host.compositionStartValue = null;
      _host.forceUndoBoundary();
    } else if (!newComposing) {
      _host.compositionStartValue = null;
    }

    final projectedInputValue = newValue;
    newValue = _host.applyEditTransformPipeline(oldValue, newValue);
    if (newValue.text != projectedInputValue.text) {
      _host.forceUndoBoundary();
    }

    final op = _host.createOp(
      _host.currentValue,
      newValue,
      undoBeforeOverride: undoBeforeOverride,
    );
    _host.applyOp(op);
    _host.setControllerSuperValue(newValue);

    final selectionNormalized =
        _host.normalizeCommittedSelectionAfterProjection();
    if (op.kind == EditOpKind.selection && !selectionNormalized) {
      _host.updateProjection(overrideValue: op.after);
    }
  }
}
