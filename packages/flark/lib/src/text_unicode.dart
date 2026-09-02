/// Returns the UTF-16 code-unit offset of the first unpaired surrogate, or
/// null when [source] is well formed.
int? firstInvalidUtf16Offset(String source) {
  var offset = 0;
  while (offset < source.length) {
    final unit = source.codeUnitAt(offset);
    if (unit >= 0xdc00 && unit <= 0xdfff) return offset;
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (offset + 1 >= source.length) return offset;
      final next = source.codeUnitAt(offset + 1);
      if (next < 0xdc00 || next > 0xdfff) return offset;
      offset += 2;
      continue;
    }
    offset += 1;
  }
  return null;
}
