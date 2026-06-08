import '../render_plan/render_plan.dart';

/// Assigns stable identities to live-rendered blocks across re-parses.
///
/// Flark re-parses the whole Markdown source on every edit, producing a fresh
/// block list with no inherent identity continuity — and the previous
/// position-based id (`type:sourceRange.start`) shifts whenever earlier content
/// changes length. This reconciler matches each new block to the previous block
/// it represents, so an unchanged block keeps its id even after offsets shift,
/// and the *edited* block keeps its id (preserving its widget State/focus/IME)
/// even though its content changed.
///
/// Matching is offset-independent and runs in two passes over the previous
/// block list:
///
/// 1. **Exact content match** — `type + display-text slice` — claims unchanged
///    blocks (before and after the edit point).
/// 2. **Type + position inherit** — an unmatched new block inherits the id of
///    the next unclaimed previous block of the same type, in order. This is what
///    lets the edited block (same type, changed text) keep its id.
///
/// Anything still unmatched (a genuinely new block) gets a fresh id. Blocks that
/// already carry an `attributes['stableId']` (synthetic gap hosts) bypass
/// reconciliation and keep their existing id scheme unchanged.
///
/// This is the Stage 1 foundation for per-block rebuild isolation; it does not
/// by itself change rebuild cost. See
/// `doc/architecture/live_rendered_rebuild_isolation.md`.
final class FlarkLiveBlockReconciler {
  List<_PrevEntry> _previous = const <_PrevEntry>[];
  int _nextId = 0;

  /// Returns a stable id per block in [blocks], in order. [displayText] is the
  /// current projected document text used to slice each block's content.
  List<String> assignIds(List<FlarkRenderBlock> blocks, String displayText) {
    final ids = List<String?>.filled(blocks.length, null);

    // Synthetic gap-host blocks keep their existing offset-keyed id scheme and
    // are excluded from reconciliation.
    final realIndices = <int>[];
    for (var i = 0; i < blocks.length; i += 1) {
      final stableId = blocks[i].attributes['stableId'];
      if (stableId is String) {
        ids[i] = 'live-block:$stableId';
      } else {
        realIndices.add(i);
      }
    }

    final keys = [
      for (final i in realIndices) _contentKey(blocks[i], displayText),
    ];
    final types = [for (final i in realIndices) blocks[i].type];
    final resolved = List<String?>.filled(realIndices.length, null);
    final claimed = List<bool>.filled(_previous.length, false);

    // Pass 1: exact content-key match (greedy, first unclaimed per key).
    final prevByKey = <String, List<int>>{};
    for (var p = 0; p < _previous.length; p += 1) {
      (prevByKey[_previous[p].key] ??= <int>[]).add(p);
    }
    for (var k = 0; k < keys.length; k += 1) {
      final bucket = prevByKey[keys[k]];
      if (bucket == null) continue;
      for (final p in bucket) {
        if (claimed[p]) continue;
        claimed[p] = true;
        resolved[k] = _previous[p].id;
        break;
      }
    }

    // Pass 2: unmatched new block inherits the next unclaimed previous block of
    // the same type, in order (the edited block keeps its id).
    var cursor = 0;
    for (var k = 0; k < resolved.length; k += 1) {
      if (resolved[k] != null) continue;
      while (cursor < _previous.length &&
          (claimed[cursor] || _previous[cursor].type != types[k])) {
        cursor += 1;
      }
      if (cursor < _previous.length) {
        claimed[cursor] = true;
        resolved[k] = _previous[cursor].id;
        cursor += 1;
      }
    }

    // Pass 3: genuinely new blocks get fresh ids.
    for (var k = 0; k < resolved.length; k += 1) {
      resolved[k] ??= 'live-block:#${_nextId++}';
    }

    for (var k = 0; k < realIndices.length; k += 1) {
      ids[realIndices[k]] = resolved[k];
    }

    _previous = [
      for (var k = 0; k < realIndices.length; k += 1)
        _PrevEntry(key: keys[k], type: types[k], id: resolved[k]!),
    ];

    return ids.cast<String>();
  }

  String _contentKey(FlarkRenderBlock block, String displayText) {
    final range = block.displayRange;
    final text =
        (range.start >= 0 &&
            range.end <= displayText.length &&
            range.start <= range.end)
        ? displayText.substring(range.start, range.end)
        : '';
    return 'c:${block.type}:$text';
  }
}

final class _PrevEntry {
  const _PrevEntry({required this.key, required this.type, required this.id});

  final String key;
  final String type;
  final String id;
}
