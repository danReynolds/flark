import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

FlarkTextAffinity portableTextAffinity(TextAffinity affinity) =>
    affinity == TextAffinity.upstream
    ? FlarkTextAffinity.upstream
    : FlarkTextAffinity.downstream;

TextAffinity flutterTextAffinity(FlarkTextAffinity affinity) =>
    affinity == FlarkTextAffinity.upstream
    ? TextAffinity.upstream
    : TextAffinity.downstream;

FlarkTextSelection portableTextSelection(TextSelection selection) =>
    FlarkTextSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
      affinity: portableTextAffinity(selection.affinity),
      isDirectional: selection.isDirectional,
    );

TextSelection flutterTextSelection(FlarkTextSelection selection) =>
    TextSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
      affinity: flutterTextAffinity(selection.affinity),
      isDirectional: selection.isDirectional,
    );

FlarkEditorInputValue portableEditorInputValue(TextEditingValue value) =>
    FlarkEditorInputValue(
      text: value.text,
      selection: portableTextSelection(value.selection),
      composing: FlarkTextRange(
        start: value.composing.start,
        end: value.composing.end,
      ),
    );
