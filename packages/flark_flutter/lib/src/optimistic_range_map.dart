import 'dart:collection';
import 'dart:math' as math;

import 'package:flark/flark.dart';

final class FlarkOptimisticViewportEdit {
  const FlarkOptimisticViewportEdit({
    required this.start,
    required this.end,
    required this.replacementLength,
    this.preservesMappedRowFacts = true,
  });

  final int start;
  final int end;
  final int replacementLength;

  /// Whether rows outside this exact splice may keep predecessor semantics by
  /// source-range mapping alone.
  final bool preservesMappedRowFacts;

  int get delta => replacementLength - (end - start);
}

/// Owns the ordered optimistic splice map between the installed parser
/// revision and the source currently exposed by the editor.
final class FlarkOptimisticRangeMap
    extends IterableBase<FlarkOptimisticViewportEdit> {
  FlarkOptimisticRangeMap();

  FlarkOptimisticRangeMap.snapshot(
    Iterable<FlarkOptimisticViewportEdit> edits,
  ) {
    _edits.addAll(edits);
  }

  final List<FlarkOptimisticViewportEdit> _edits = [];

  @override
  Iterator<FlarkOptimisticViewportEdit> get iterator => _edits.iterator;

  void add(FlarkOptimisticViewportEdit edit) => _edits.add(edit);

  void clear() => _edits.clear();

  FlarkSourceRange mapRange(FlarkSourceRange base) {
    var start = base.start;
    var end = base.end;
    for (final edit in _edits) {
      if (end <= edit.start) continue;
      if (start >= edit.end) {
        start += edit.delta;
        end += edit.delta;
        continue;
      }
      start = math.min(start, edit.start);
      end = math.max(edit.start + edit.replacementLength, end + edit.delta);
    }
    return FlarkSourceRange(start, end);
  }

  /// Whether the range itself is mechanically unchanged after applying every
  /// optimistic splice before it.
  bool leavesRangeUnchanged(FlarkSourceRange base) {
    if (_edits.isEmpty) return false;
    var start = base.start;
    var end = base.end;
    for (final edit in _edits) {
      if (edit.start == edit.end) {
        final touchesRange = start == end
            ? edit.start == start
            : start <= edit.start && edit.start < end;
        if (touchesRange) return false;
        if (start > edit.start) {
          start += edit.delta;
          end += edit.delta;
        }
        continue;
      }
      if (end <= edit.start) continue;
      if (start >= edit.end) {
        start += edit.delta;
        end += edit.delta;
        continue;
      }
      return false;
    }
    return true;
  }

  /// Whether all splices preserve and remain inside an editable container.
  bool staysWithin(FlarkSourceRange editable) {
    if (_edits.isEmpty) return false;
    var start = editable.start;
    var end = editable.end;
    for (final edit in _edits) {
      if (!edit.preservesMappedRowFacts ||
          edit.start < start ||
          edit.end > end) {
        return false;
      }
      end += edit.delta;
    }
    return true;
  }
}
