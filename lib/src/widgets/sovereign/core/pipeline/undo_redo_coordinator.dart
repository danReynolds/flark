import 'package:flutter/services.dart';

import 'undo_stack.dart';

abstract class SovereignUndoRedoHost {
  TextEditingValue get value;
  UndoStack get undoStack;

  void applyRestoration(TextEditingValue value);
}

class SovereignUndoRedoCoordinator {
  const SovereignUndoRedoCoordinator(this._host);

  final SovereignUndoRedoHost _host;

  void undo() {
    if (_host.value.composing.isValid) return;
    if (!_host.undoStack.canUndo) return;

    final ops = _host.undoStack.popUndo();
    if (ops.isEmpty) return;

    var accumulator = _host.value;
    for (final op in ops) {
      accumulator = op.before;
    }
    _host.applyRestoration(accumulator);
  }

  void redo() {
    if (_host.value.composing.isValid) return;
    if (!_host.undoStack.canRedo) return;

    final ops = _host.undoStack.popRedo();

    var accumulator = _host.value;
    for (final op in ops) {
      accumulator = op.after;
    }
    _host.applyRestoration(accumulator);
  }
}
