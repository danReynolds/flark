import 'package:flutter/services.dart';

import 'input_intent_models.dart';
import '../structure/models/fence_context.dart';

abstract class SovereignEnterIntentHost {
  TextEditingValue get value;
  TextSelection get selection;

  void commitProgrammaticTextEdit(TextEditingValue newValue);
  bool tryHandleIndentedCodeBlockEnter(String text, int caret);
  FenceContext? fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  });
  FenceEnterExitResult? computeFenceExitOnEnter({
    required String text,
    required int caret,
    required FenceContext context,
  });
}

class SovereignEnterIntentHandler {
  SovereignEnterIntentHandler(this._host);

  final SovereignEnterIntentHost _host;

  void handleEnter() {
    if (_host.value.composing.isValid) return;

    final sel = _host.selection;
    if (!sel.isValid) return;

    final text = _host.value.text;
    final start = sel.start;
    final end = sel.end;

    if (sel.isCollapsed && _host.tryHandleIndentedCodeBlockEnter(text, start)) {
      return;
    }

    final newText = text.replaceRange(start, end, '\n');
    final newSelection = TextSelection.collapsed(offset: start + 1);

    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: newSelection,
        composing: TextRange.empty,
      ),
    );
  }

  bool tryExitFencedCodeOnEnter() {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final caret = sel.baseOffset;
    final text = _host.value.text;
    if (caret < 0 || caret > text.length) return false;
    final context = _host.fenceContextForCaret(
      text,
      caret,
      includeUnclosedEof: true,
    );
    if (context == null) return false;
    final exit = _host.computeFenceExitOnEnter(
      text: text,
      caret: caret,
      context: context,
    );
    if (exit == null) return false;

    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: exit.text,
        selection: TextSelection.collapsed(offset: exit.caret),
        composing: TextRange.empty,
      ),
    );
    return true;
  }
}
