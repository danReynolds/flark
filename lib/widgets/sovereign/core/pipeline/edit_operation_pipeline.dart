import 'package:flutter/services.dart';

import '../../models/edit_op.dart';
import 'edit_differ.dart';
import 'undo_grouping_policy.dart';

class EditOperationResult {
  const EditOperationResult({
    required this.op,
    required this.nextOpId,
    required this.undoGroupingState,
  });

  final EditOp op;
  final int nextOpId;
  final UndoGroupingState undoGroupingState;
}

class EditOperationPipeline {
  const EditOperationPipeline._();

  static EditOperationResult create({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int nextOpId,
    required UndoGroupingState undoGroupingState,
    required DateTime now,
    TextEditingValue? undoBeforeOverride,
  }) {
    final diffOldValue = undoBeforeOverride ?? oldValue;
    final rawOp = EditDiffer.diff(
      oldVal: diffOldValue,
      newVal: newValue,
      nextOpId: nextOpId,
      isSmart: false,
      undoGroupId: 0,
    );

    final decision = UndoGroupingPolicy.assignUndoGroup(
      rawOp: rawOp,
      state: undoGroupingState,
      now: now,
    );
    return EditOperationResult(
      op: decision.op,
      nextOpId: nextOpId + 1,
      undoGroupingState: decision.state,
    );
  }
}
