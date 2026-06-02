enum SovereignMapAffinity {
  upstream,
  downstream,
}

final class SovereignSelection {
  const SovereignSelection({
    required this.baseOffset,
    required this.extentOffset,
  });

  const SovereignSelection.collapsed(int offset)
      : baseOffset = offset,
        extentOffset = offset;

  final int baseOffset;
  final int extentOffset;

  bool get isCollapsed => baseOffset == extentOffset;

  int get start => baseOffset < extentOffset ? baseOffset : extentOffset;

  int get end => baseOffset > extentOffset ? baseOffset : extentOffset;

  SovereignSelection copyWith({
    int? baseOffset,
    int? extentOffset,
  }) {
    return SovereignSelection(
      baseOffset: baseOffset ?? this.baseOffset,
      extentOffset: extentOffset ?? this.extentOffset,
    );
  }

  SovereignSelection validate(int textLength) {
    _checkOffset(baseOffset, textLength, 'baseOffset');
    _checkOffset(extentOffset, textLength, 'extentOffset');
    return this;
  }

  static void _checkOffset(int offset, int textLength, String name) {
    if (offset < 0 || offset > textLength) {
      throw RangeError.range(offset, 0, textLength, name);
    }
  }

  @override
  bool operator ==(Object other) {
    return other is SovereignSelection &&
        other.baseOffset == baseOffset &&
        other.extentOffset == extentOffset;
  }

  @override
  int get hashCode => Object.hash(baseOffset, extentOffset);

  @override
  String toString() {
    if (isCollapsed) return 'SovereignSelection.collapsed($baseOffset)';
    return 'SovereignSelection($baseOffset, $extentOffset)';
  }
}
