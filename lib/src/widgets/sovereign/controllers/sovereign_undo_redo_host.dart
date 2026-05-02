part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

class _ControllerSovereignUndoRedoHost implements SovereignUndoRedoHost {
  _ControllerSovereignUndoRedoHost(this._controller);

  final SovereignController _controller;

  @override
  TextEditingValue get value => _controller.value;

  @override
  UndoStack get undoStack => _controller._undoStack;

  @override
  void applyRestoration(TextEditingValue value) {
    _controller._applyRestoration(value);
  }
}
