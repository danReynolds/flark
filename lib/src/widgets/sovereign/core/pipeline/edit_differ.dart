import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

class EditDiffer {
  const EditDiffer._();

  static EditOp diff({
    required TextEditingValue oldVal,
    required TextEditingValue newVal,
    required int nextOpId,
    required bool isSmart,
    required int undoGroupId,
  }) {
    if (oldVal.text == newVal.text) {
      if (oldVal.selection != newVal.selection ||
          oldVal.composing != newVal.composing) {
        return EditOp(
          id: nextOpId,
          kind: EditOpKind.selection,
          before: oldVal,
          after: newVal,
          undoGroupId: undoGroupId,
        );
      }
      return EditOp(
        id: nextOpId,
        kind: EditOpKind.selection,
        before: oldVal,
        after: newVal,
        undoGroupId: undoGroupId,
      );
    }

    final oldText = oldVal.text;
    final newText = newVal.text;

    int prefix = 0;
    while (prefix < oldText.length &&
        prefix < newText.length &&
        oldText.codeUnitAt(prefix) == newText.codeUnitAt(prefix)) {
      prefix++;
    }

    int suffix = 0;
    final maxSuffix = (oldText.length - prefix) < (newText.length - prefix)
        ? (oldText.length - prefix)
        : (newText.length - prefix);

    while (suffix < maxSuffix &&
        oldText.codeUnitAt(oldText.length - 1 - suffix) ==
            newText.codeUnitAt(newText.length - 1 - suffix)) {
      suffix++;
    }

    final replacedStart = prefix;
    final replacedEnd = oldText.length - suffix;
    final replacedRange = TextRange(start: replacedStart, end: replacedEnd);

    final insertedText = newText.substring(prefix, newText.length - suffix);
    final replacedText = oldText.substring(prefix, oldText.length - suffix);

    var valid = true;
    if (replacedText !=
        oldText.substring(replacedRange.start, replacedRange.end)) {
      valid = false;
    }

    if (valid) {
      final reconstruction = oldText.replaceRange(
        replacedRange.start,
        replacedRange.end,
        insertedText,
      );
      if (reconstruction != newText) {
        valid = false;
      }
    }

    if (!valid) {
      return EditOp(
        id: nextOpId,
        kind: EditOpKind.text,
        before: oldVal,
        after: newVal,
        replacedRange: TextRange(start: 0, end: oldText.length),
        replacedText: oldText,
        insertedText: newText,
        undoGroupId: undoGroupId,
        isSmartTransform: isSmart,
      );
    }

    return EditOp(
      id: nextOpId,
      kind: EditOpKind.text,
      before: oldVal,
      after: newVal,
      replacedRange: replacedRange,
      replacedText: replacedText,
      insertedText: insertedText,
      isSmartTransform: isSmart,
      undoGroupId: undoGroupId,
    );
  }
}
