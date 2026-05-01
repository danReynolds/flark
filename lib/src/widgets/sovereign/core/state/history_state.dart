import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

class HistoryState {
  const HistoryState({
    required this.lastOp,
    required this.lastOpTime,
    required this.currentUndoGroup,
    required this.undoBoundaryDepth,
    required this.commandTransactionDepth,
    required this.commandTransactionUndoGroupId,
    required this.forceUndoBoundaryForNextTextOp,
    required this.compositionStartValue,
    required this.canUndo,
    required this.canRedo,
  });

  final EditOp? lastOp;
  final DateTime? lastOpTime;
  final int currentUndoGroup;
  final int undoBoundaryDepth;
  final int commandTransactionDepth;
  final int? commandTransactionUndoGroupId;
  final bool forceUndoBoundaryForNextTextOp;
  final TextEditingValue? compositionStartValue;
  final bool canUndo;
  final bool canRedo;

  HistoryState copyWith({
    EditOp? lastOp,
    DateTime? lastOpTime,
    int? currentUndoGroup,
    int? undoBoundaryDepth,
    int? commandTransactionDepth,
    int? commandTransactionUndoGroupId,
    bool? forceUndoBoundaryForNextTextOp,
    TextEditingValue? compositionStartValue,
    bool? canUndo,
    bool? canRedo,
  }) {
    return HistoryState(
      lastOp: lastOp ?? this.lastOp,
      lastOpTime: lastOpTime ?? this.lastOpTime,
      currentUndoGroup: currentUndoGroup ?? this.currentUndoGroup,
      undoBoundaryDepth: undoBoundaryDepth ?? this.undoBoundaryDepth,
      commandTransactionDepth:
          commandTransactionDepth ?? this.commandTransactionDepth,
      commandTransactionUndoGroupId:
          commandTransactionUndoGroupId ?? this.commandTransactionUndoGroupId,
      forceUndoBoundaryForNextTextOp:
          forceUndoBoundaryForNextTextOp ?? this.forceUndoBoundaryForNextTextOp,
      compositionStartValue:
          compositionStartValue ?? this.compositionStartValue,
      canUndo: canUndo ?? this.canUndo,
      canRedo: canRedo ?? this.canRedo,
    );
  }
}
