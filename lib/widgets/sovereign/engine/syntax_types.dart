import 'package:flutter/foundation.dart';

import '../models/block_node.dart';
import '../models/sovereign_style.dart';

/// Structural block span emitted by a syntax engine.
@immutable
class BlockSpan {
  /// Structural block kind.
  final BlockType type;

  /// Start offset in UTF-16 code units.
  final int start;

  /// End offset in UTF-16 code units.
  final int end;

  /// Optional block metadata, such as heading level or fence language.
  final Map<String, Object?> payload;

  /// Creates a structural block span over `[start, end)`.
  const BlockSpan({
    required this.type,
    required this.start,
    required this.end,
    this.payload = const {},
  })  : assert(start >= 0),
        assert(end >= start);

  /// Number of UTF-16 code units covered by this span.
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

/// Inline style token emitted by a syntax engine.
@immutable
class InlineSpanToken {
  /// Inline style applied to the span.
  final SovereignStyle style;

  /// Start offset in UTF-16 code units.
  final int start;

  /// End offset in UTF-16 code units.
  final int end;

  /// Creates an inline style token over `[start, end)`.
  const InlineSpanToken({
    required this.style,
    required this.start,
    required this.end,
  })  : assert(start >= 0),
        assert(end >= start);

  /// Number of UTF-16 code units covered by this token.
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
