import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

class UndoStack {
  final List<EditOp> _undo = [];
  final List<EditOp> _redo = [];

  static const int kMaxStack = 500;

  void push(EditOp op) {
    // Selection ops are ephemeral and don't clear redo or enter undo history usually?
    // Spec: "SelectionOp... does not enter undo history".
    if (op.kind == EditOpKind.selection) return;

    _redo.clear();
    _undo.add(op);

    if (_undo.length > kMaxStack) {
      _undo.removeAt(0);
    }
  }

  /// Returns the list of ops to revert (LIFO order).
  /// Controller should apply unique inverses for each.
  List<EditOp> popUndo() {
    if (_undo.isEmpty) return [];

    final lastOp = _undo.last;
    final group = lastOp.undoGroupId;
    final opsToUndo = <EditOp>[];

    // Pop all ops with same group
    while (_undo.isNotEmpty && _undo.last.undoGroupId == group) {
      opsToUndo.add(_undo.removeLast());
    }

    // Add to redo stack (Store in File Order: [Op1, Op2])
    // The ops popped are [Op2, Op1] (Reverse File Order).
    // We want Redo stack to pop Op1 then Op2.
    // So Redo Stack should look like [..., Op2, Op1].
    // So we addAll(opsToUndo).
    _redo.addAll(opsToUndo);

    return opsToUndo;
  }

  /// Returns list of ops to re-apply (FIFO/File order).
  List<EditOp> popRedo() {
    if (_redo.isEmpty) return [];

    final lastOp = _redo.last;
    final group = lastOp.undoGroupId;
    final opsToRedo = <EditOp>[];

    while (_redo.isNotEmpty && _redo.last.undoGroupId == group) {
      opsToRedo.add(_redo.removeLast());
    }

    // opsToRedo is [Op1, Op2].
    // Add back to undo.
    // Undo wants [..., Op1, Op2].
    // Since opsToRedo is [Op1, Op2], addAll works.
    _undo.addAll(opsToRedo);

    return opsToRedo; // Apply Op1 then Op2.
  }

  bool get canUndo => _undo.isNotEmpty;
  bool get canRedo => _redo.isNotEmpty;

  void clearRedo() {
    _redo.clear();
  }

  void clear() {
    _undo.clear();
    _redo.clear();
  }
}
