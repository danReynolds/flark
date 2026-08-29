import 'dart:math' as math;

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

int replacementResultLength({
  required String source,
  required int start,
  required int end,
  required String replacement,
}) => source.length - (end - start) + replacement.length;

/// Moves UTF-16 window cuts outward from the middle of a surrogate pair.
///
/// The returned half-open range is always contained by the requested range;
/// a scalar intersected by only one cut is excluded rather than split.
({int start, int end}) scalarAlignedUtf16Window(
  String text,
  int start,
  int end,
) {
  if (start < 0 || start > text.length) {
    throw RangeError.range(start, 0, text.length, 'start');
  }
  if (end < start || end > text.length) {
    throw RangeError.range(end, start, text.length, 'end');
  }
  var alignedStart = start;
  var alignedEnd = end;
  if (alignedStart > 0 &&
      alignedStart < text.length &&
      _isLowSurrogate(text.codeUnitAt(alignedStart)) &&
      _isHighSurrogate(text.codeUnitAt(alignedStart - 1))) {
    alignedStart += 1;
  }
  if (alignedEnd > alignedStart &&
      alignedEnd < text.length &&
      _isLowSurrogate(text.codeUnitAt(alignedEnd)) &&
      _isHighSurrogate(text.codeUnitAt(alignedEnd - 1))) {
    alignedEnd -= 1;
  }
  return (start: alignedStart, end: math.max(alignedStart, alignedEnd));
}

/// Returns a bounded, well-formed UTF-16 window around a replacement focus.
({int start, String text}) boundedReplacementWindow({
  required String source,
  required int start,
  required int end,
  required String replacement,
  required int focus,
  required int maximumCodeUnits,
}) {
  if (start < 0 || start > source.length) {
    throw RangeError.range(start, 0, source.length, 'start');
  }
  if (end < start || end > source.length) {
    throw RangeError.range(end, start, source.length, 'end');
  }
  if (maximumCodeUnits <= 0) {
    throw ArgumentError.value(
      maximumCodeUnits,
      'maximumCodeUnits',
      'must be positive',
    );
  }
  final nextLength = replacementResultLength(
    source: source,
    start: start,
    end: end,
    replacement: replacement,
  );
  final windowLength = math.min(nextLength, maximumCodeUnits);
  final windowStart = (focus - windowLength ~/ 2).clamp(
    0,
    nextLength - windowLength,
  );
  final windowEnd = windowStart + windowLength;
  final replacementEnd = start + replacement.length;
  final output = StringBuffer();

  void appendIntersection(
    String segment,
    int segmentStart,
    int sourceStart,
    int sourceEnd,
  ) {
    final segmentEnd = segmentStart + sourceEnd - sourceStart;
    final overlapStart = math.max(windowStart, segmentStart);
    final overlapEnd = math.min(windowEnd, segmentEnd);
    if (overlapStart >= overlapEnd) return;
    output.write(
      segment.substring(
        sourceStart + overlapStart - segmentStart,
        sourceStart + overlapEnd - segmentStart,
      ),
    );
  }

  appendIntersection(source, 0, 0, start);
  appendIntersection(replacement, start, 0, replacement.length);
  appendIntersection(source, replacementEnd, end, source.length);
  var bounded = output.toString();
  var boundedStart = windowStart;
  if (bounded.isNotEmpty && _isLowSurrogate(bounded.codeUnitAt(0))) {
    bounded = bounded.substring(1);
    boundedStart += 1;
  }
  if (bounded.isNotEmpty &&
      _isHighSurrogate(bounded.codeUnitAt(bounded.length - 1))) {
    bounded = bounded.substring(0, bounded.length - 1);
  }
  return (start: boundedStart, text: bounded);
}

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xd800 && codeUnit <= 0xdbff;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xdc00 && codeUnit <= 0xdfff;
