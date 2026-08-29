/// Direction used when one rendered caret position borders hidden source.
enum FlarkTextAffinity { upstream, downstream }

/// Immutable UTF-16 range used by a portable editor input snapshot.
final class FlarkTextRange {
  const FlarkTextRange({required this.start, required this.end});

  static const empty = FlarkTextRange(start: -1, end: -1);

  final int start;
  final int end;

  bool get isValid => start >= 0 && end >= start;
  bool get isCollapsed => start == end;

  @override
  bool operator ==(Object other) =>
      other is FlarkTextRange && other.start == start && other.end == end;

  @override
  int get hashCode => Object.hash(start, end);
}

/// Immutable selection expressed in UTF-16 offsets inside one bounded value.
final class FlarkTextSelection {
  const FlarkTextSelection({
    required this.baseOffset,
    required this.extentOffset,
    this.affinity = FlarkTextAffinity.downstream,
    this.isDirectional = false,
  });

  const FlarkTextSelection.collapsed({
    required int offset,
    this.affinity = FlarkTextAffinity.downstream,
  }) : baseOffset = offset,
       extentOffset = offset,
       isDirectional = false;

  final int baseOffset;
  final int extentOffset;
  final FlarkTextAffinity affinity;
  final bool isDirectional;

  int get start => baseOffset < extentOffset ? baseOffset : extentOffset;
  int get end => baseOffset < extentOffset ? extentOffset : baseOffset;
  bool get isValid => baseOffset >= 0 && extentOffset >= 0;
  bool get isCollapsed => baseOffset == extentOffset;

  @override
  bool operator ==(Object other) =>
      other is FlarkTextSelection &&
      other.baseOffset == baseOffset &&
      other.extentOffset == extentOffset &&
      other.affinity == affinity &&
      other.isDirectional == isDirectional;

  @override
  int get hashCode =>
      Object.hash(baseOffset, extentOffset, affinity, isDirectional);
}

/// Bounded input value published to a frontend with an editor snapshot.
///
/// It deliberately mirrors only the framework-neutral facts a platform text
/// adapter needs; it is not a second document or platform input object.
final class FlarkEditorInputValue {
  const FlarkEditorInputValue({
    this.text = '',
    this.selection = const FlarkTextSelection.collapsed(offset: -1),
    this.composing = FlarkTextRange.empty,
  });

  final String text;
  final FlarkTextSelection selection;
  final FlarkTextRange composing;

  @override
  bool operator ==(Object other) =>
      other is FlarkEditorInputValue &&
      other.text == text &&
      other.selection == selection &&
      other.composing == composing;

  @override
  int get hashCode => Object.hash(text, selection, composing);
}
