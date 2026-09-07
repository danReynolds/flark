import 'package:characters/characters.dart';

/// A bounded viewport over exact source. Paging is presentation only: the
/// controller still owns the complete source and global selection offsets.
class SourceWindow {
  const SourceWindow(
    this.start,
    this.end,
    this.index,
    this.starts,
    this.length,
  );
  final int start, end, index, length;
  final List<int> starts;
  int get count => starts.length;
  int? get previous => index > 0 ? starts[index - 1] : null;
  int? get next => index + 1 < starts.length ? starts[index + 1] : null;

  factory SourceWindow.at(String source, int caret) {
    final starts = <int>[0];
    var from = 0, lines = 0;
    for (var i = 0; i < source.length; i++) {
      final unit = source.codeUnitAt(i);
      if (unit == 13 &&
          i + 1 < source.length &&
          source.codeUnitAt(i + 1) == 10) {
        i++;
        lines++;
      } else if (unit == 10) {
        lines++;
      } else if (unit >= 0xd800 && unit <= 0xdbff && i + 1 < source.length) {
        i++;
      }
      if ((i + 1 - from >= 4096 || lines >= 128) && i + 1 < source.length) {
        final boundary = CharacterRange.at(source, i + 1);
        if (boundary.isNotEmpty && boundary.stringBeforeLength > from) {
          i = boundary.stringBeforeLength - 1;
        }
        from = i + 1;
        starts.add(from);
        lines = 0;
      }
    }
    var index = 0;
    while (index + 1 < starts.length && starts[index + 1] <= caret) {
      index++;
    }
    return SourceWindow(
      starts[index],
      index + 1 < starts.length ? starts[index + 1] : source.length,
      index,
      List.unmodifiable(starts),
      source.length,
    );
  }
}
