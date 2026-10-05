part of 'editor.dart';

/// The immutable state a host receives from [FlarkEditor]. A live snapshot
/// owns a parsed document and projection; a source snapshot deliberately does
/// not, so code that handles both modes cannot accidentally paint stale data.
sealed class FlarkEditorSnapshot {
  const FlarkEditorSnapshot();

  String get source;
  FlarkSelection get selection;
}

final class FlarkLiveSnapshot extends FlarkEditorSnapshot {
  const FlarkLiveSnapshot._(this.document);

  final FlarkDocument document;

  @override
  String get source => document.source;

  @override
  FlarkSelection get selection => document.selection;

  Projection get projection => document.projection;
}

final class FlarkSourceSnapshot extends FlarkEditorSnapshot {
  factory FlarkSourceSnapshot._(String source, FlarkSelection selection) =>
      FlarkSourceSnapshot._raw(
        source,
        FlarkSelection(
          _legalSourceOffset(source, selection.base),
          _legalSourceOffset(source, selection.extent),
        ),
      );

  const FlarkSourceSnapshot._raw(this.source, this.selection);

  @override
  final String source;

  @override
  final FlarkSelection selection;

  FlarkSourceSnapshot _withSelection(FlarkSelection selection) =>
      FlarkSourceSnapshot._(source, selection);

  int _legalize(int offset) => _legalSourceOffset(source, offset);

  int _previousGraphemeBoundary(int offset) {
    final target = _legalize(offset);
    if (target == 0) return 0;
    final range = CharacterRange.at(source, target);
    range.moveBack();
    return range.stringBeforeLength;
  }

  int _nextGraphemeBoundary(int offset) {
    final target = _legalize(offset);
    if (target == source.length) return target;
    final range = CharacterRange.at(source, target);
    range.moveNext();
    return source.length - range.stringAfterLength;
  }
}

int _legalSourceOffset(String source, int offset) {
  final target = offset.clamp(0, source.length);
  if (target == 0 || target == source.length) return target;
  final range = CharacterRange.at(source, target);
  if (range.isEmpty) return target;
  final before = range.stringBeforeLength;
  final after = source.length - range.stringAfterLength;
  return after - target < target - before ? after : before;
}
