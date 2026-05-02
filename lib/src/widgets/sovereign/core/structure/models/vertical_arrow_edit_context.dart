import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

class VerticalArrowEditContext {
  const VerticalArrowEditContext({
    required this.text,
    required this.oldCaret,
    required this.newCaret,
    required this.oldLine,
    required this.newLine,
  });

  final String text;
  final int oldCaret;
  final int newCaret;
  final int oldLine;
  final int newLine;

  static VerticalArrowEditContext? detect({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required bool movingDown,
  }) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return null;
    }
    if (oldValue.text != newValue.text) return null;

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return null;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return null;

    final text = oldValue.text;
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret < 0 || oldCaret > text.length) return null;
    if (newCaret < 0 || newCaret > text.length) return null;
    if (oldCaret == newCaret) return null;

    final oldLine = lineIndex.lineAtOffset(oldCaret);
    final newLine = lineIndex.lineAtOffset(newCaret);
    final expectedLine = movingDown ? oldLine + 1 : oldLine - 1;
    if (newLine != expectedLine) return null;

    return VerticalArrowEditContext(
      text: text,
      oldCaret: oldCaret,
      newCaret: newCaret,
      oldLine: oldLine,
      newLine: newLine,
    );
  }
}
