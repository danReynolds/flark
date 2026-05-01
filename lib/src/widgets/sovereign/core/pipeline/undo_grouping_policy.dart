import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

const Object _kUnsetUndoGroupingField = Object();

class UndoGroupingState {
  const UndoGroupingState({
    required this.currentUndoGroup,
    required this.commandTransactionUndoGroupId,
    required this.lastOp,
    required this.lastOpTime,
    required this.commandTransactionDepth,
    required this.undoBoundaryDepth,
    required this.forceUndoBoundaryForNextTextOp,
  });

  final int currentUndoGroup;
  final int? commandTransactionUndoGroupId;
  final EditOp? lastOp;
  final DateTime? lastOpTime;
  final int commandTransactionDepth;
  final int undoBoundaryDepth;
  final bool forceUndoBoundaryForNextTextOp;

  UndoGroupingState copyWith({
    int? currentUndoGroup,
    int? commandTransactionDepth,
    int? undoBoundaryDepth,
    Object? commandTransactionUndoGroupId = _kUnsetUndoGroupingField,
    Object? lastOp = _kUnsetUndoGroupingField,
    Object? lastOpTime = _kUnsetUndoGroupingField,
    bool? forceUndoBoundaryForNextTextOp,
  }) {
    return UndoGroupingState(
      currentUndoGroup: currentUndoGroup ?? this.currentUndoGroup,
      commandTransactionUndoGroupId:
          identical(commandTransactionUndoGroupId, _kUnsetUndoGroupingField)
              ? this.commandTransactionUndoGroupId
              : commandTransactionUndoGroupId as int?,
      lastOp: identical(lastOp, _kUnsetUndoGroupingField)
          ? this.lastOp
          : lastOp as EditOp?,
      lastOpTime: identical(lastOpTime, _kUnsetUndoGroupingField)
          ? this.lastOpTime
          : lastOpTime as DateTime?,
      commandTransactionDepth:
          commandTransactionDepth ?? this.commandTransactionDepth,
      undoBoundaryDepth: undoBoundaryDepth ?? this.undoBoundaryDepth,
      forceUndoBoundaryForNextTextOp:
          forceUndoBoundaryForNextTextOp ?? this.forceUndoBoundaryForNextTextOp,
    );
  }
}

class UndoGroupingDecision {
  const UndoGroupingDecision({required this.op, required this.state});

  final EditOp op;
  final UndoGroupingState state;
}

class UndoGroupingPolicy {
  const UndoGroupingPolicy._();

  static UndoGroupingDecision assignUndoGroup({
    required EditOp rawOp,
    required UndoGroupingState state,
    required DateTime now,
  }) {
    if (rawOp.kind == EditOpKind.selection) {
      return UndoGroupingDecision(op: rawOp, state: state);
    }

    final forceUndoBoundary =
        state.undoBoundaryDepth > 0 || state.forceUndoBoundaryForNextTextOp;

    if (state.commandTransactionDepth > 0) {
      final txGroupId =
          state.commandTransactionUndoGroupId ?? state.currentUndoGroup + 1;
      return UndoGroupingDecision(
        op: _withUndoGroup(rawOp, txGroupId),
        state: state.copyWith(
          currentUndoGroup: txGroupId,
          commandTransactionUndoGroupId: txGroupId,
          forceUndoBoundaryForNextTextOp: false,
          lastOp: rawOp,
          lastOpTime: now,
        ),
      );
    }

    var merge = false;
    if (!forceUndoBoundary &&
        state.lastOp != null &&
        state.lastOpTime != null) {
      final delta = now.difference(state.lastOpTime!).inMilliseconds;
      if (delta < 750 && rawOp.after.selection.isCollapsed) {
        final caret = rawOp.after.selection.baseOffset;
        final affected = state.lastOp!.affectedRange;
        if (caret >= affected.start - 1 &&
            caret <= affected.end + 1 &&
            !rawOp.insertedText.contains('\n') &&
            rawOp.insertedText.length < 5) {
          merge = true;
        }
      }
    }

    final nextGroup =
        merge ? state.currentUndoGroup : state.currentUndoGroup + 1;
    return UndoGroupingDecision(
      op: _withUndoGroup(rawOp, nextGroup),
      state: state.copyWith(
        currentUndoGroup: nextGroup,
        forceUndoBoundaryForNextTextOp: false,
        lastOp: rawOp,
        lastOpTime: now,
      ),
    );
  }

  static EditOp _withUndoGroup(EditOp op, int undoGroupId) {
    return EditOp(
      id: op.id,
      kind: op.kind,
      before: op.before,
      after: op.after,
      replacedRange: op.replacedRange,
      insertedText: op.insertedText,
      replacedText: op.replacedText,
      isSmartTransform: op.isSmartTransform,
      undoGroupId: undoGroupId,
    );
  }
}
