import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';

import 'command_context.dart';
import 'command_selection.dart';

typedef SovereignCommandMutation = ({
  String text,
  TextSelection selection,
  TextRange composing,
});

SovereignCommandResult commitCommandMutation(
  SovereignCommandContext context,
  SovereignCommandMutation mutation, {
  SovereignCommandReasonCode noOpReasonCode =
      SovereignCommandReasonCode.noChange,
}) {
  final nextText = mutation.text;
  final nextSelection = clampSelectionToText(
    mutation.selection,
    nextText.length,
  );
  final nextComposing = mutation.composing;

  final current = context.controller.value;
  final nextValue = current.copyWith(
    text: nextText,
    selection: nextSelection,
    composing: nextComposing,
  );
  if (nextValue == current) {
    return SovereignCommandNoOp.code(noOpReasonCode);
  }

  context.controller.value = nextValue;
  return SovereignCommandApplied(nextSelection);
}
