import 'package:flutter/foundation.dart';

import '../models/block_node.dart';
import '../models/sovereign_style.dart';

@immutable
class BlockSpan {
  final BlockType type;
  final int start;
  final int end;
  final Map<String, Object?> payload;

  const BlockSpan({
    required this.type,
    required this.start,
    required this.end,
    this.payload = const {},
  })  : assert(start >= 0),
        assert(end >= start);

  int get length => end - start;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is BlockSpan &&
          type == other.type &&
          start == other.start &&
          end == other.end &&
          mapEquals(payload, other.payload);

  @override
  int get hashCode => Object.hash(
        type,
        start,
        end,
        Object.hashAllUnordered(
          payload.entries.map((entry) => Object.hash(entry.key, entry.value)),
        ),
      );
}

@immutable
class InlineSpanToken {
  final SovereignStyle style;
  final int start;
  final int end;

  const InlineSpanToken({
    required this.style,
    required this.start,
    required this.end,
  })  : assert(start >= 0),
        assert(end >= start);

  int get length => end - start;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is InlineSpanToken &&
          style == other.style &&
          start == other.start &&
          end == other.end;

  @override
  int get hashCode => Object.hash(style, start, end);
}
