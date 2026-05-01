import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_selection.dart';

class SovereignCommandContext {
  final SovereignController controller;
  final TextEditingValue value;
  final String text;
  final TextSelection selection;

  const SovereignCommandContext._({
    required this.controller,
    required this.value,
    required this.text,
    required this.selection,
  });

  factory SovereignCommandContext.fromController(
    SovereignController controller,
  ) {
    final value = controller.value;
    final text = value.text;
    final selection = safeSelectionForText(value.selection, text.length);
    return SovereignCommandContext._(
      controller: controller,
      value: value,
      text: text,
      selection: selection,
    );
  }

  bool get isComposing => value.composing.isValid;
}
