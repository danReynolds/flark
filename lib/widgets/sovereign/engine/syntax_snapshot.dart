import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'syntax_types.dart';

/// Contract: all offsets are UTF-16 code-unit indices (Dart string offsets).
abstract interface class CursorValidationMask {
  /// Returns the nearest safe cursor offset for [requestedOffset].
  int snapToSafeOffset(int requestedOffset);
}

/// Cursor mask that accepts every offset inside the text bounds.
@immutable
class PassthroughCursorValidationMask implements CursorValidationMask {
  /// Text length used to clamp requested offsets.
  final int textLength;

  /// Creates a pass-through cursor mask for [textLength].
  const PassthroughCursorValidationMask({required this.textLength})
      : assert(textLength >= 0);

  @override
  int snapToSafeOffset(int requestedOffset) {
    return requestedOffset.clamp(0, textLength).toInt();
  }
}

/// Cursor mask that snaps offsets out of hidden markdown marker ranges.
@immutable
class HiddenRangeCursorValidationMask implements CursorValidationMask {
  /// Text length used to clamp requested offsets.
  final int textLength;

  /// Hidden half-open ranges that should not receive the caret.
  final List<TextRange> hiddenRanges;

  /// Creates a cursor mask for [hiddenRanges].
  HiddenRangeCursorValidationMask({
    required this.textLength,
    required List<TextRange> hiddenRanges,
  })  : assert(textLength >= 0),
        hiddenRanges = [...hiddenRanges]..sort((a, b) {
            final byStart = a.start.compareTo(b.start);
            if (byStart != 0) return byStart;
            return a.end.compareTo(b.end);
          });

  @override
  int snapToSafeOffset(int requestedOffset) {
    var candidate = requestedOffset.clamp(0, textLength).toInt();
    for (final range in hiddenRanges) {
      if (range.start >= range.end) continue;
      // Hidden ranges are half-open [start, end). Keep boundaries valid cursor
      // targets so caret can sit adjacent to hidden markers without snapping.
      if (candidate <= range.start || candidate >= range.end) continue;

      final left = range.start.clamp(0, textLength).toInt();
      final right = range.end.clamp(0, textLength).toInt();
      final leftDistance = (candidate - left).abs();
      final rightDistance = (right - candidate).abs();
      candidate = rightDistance <= leftDistance ? right : left;
    }
    return candidate.clamp(0, textLength).toInt();
  }
}

/// Diagnostic emitted by a syntax engine.
@immutable
class SyntaxDiagnostic {
  /// Start offset in UTF-16 code units.
  final int start;

  /// End offset in UTF-16 code units.
  final int end;

  /// Human-readable diagnostic message.
  final String message;

  /// Optional stable diagnostic code.
  final String? code;

  /// Whether this diagnostic represents an error instead of a warning.
  final bool isError;

  /// Creates a syntax diagnostic over `[start, end)`.
  const SyntaxDiagnostic({
    required this.start,
    required this.end,
    required this.message,
    this.code,
    this.isError = false,
  })  : assert(start >= 0),
        assert(end >= start);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SyntaxDiagnostic &&
          start == other.start &&
          end == other.end &&
          message == other.message &&
          code == other.code &&
          isError == other.isError;

  @override
  int get hashCode => Object.hash(start, end, message, code, isError);
}

/// Authoritative syntax metadata for a controller revision.
@immutable
class SyntaxSnapshot {
  /// Controller revision represented by this snapshot.
  final int revision;

  /// Structural block spans.
  final List<BlockSpan> blocks;

  /// Inline style spans.
  final List<InlineSpanToken> inlineTokens;

  /// Source marker ranges that may be visually collapsed.
  final List<TextRange> markerRanges;

  /// Ranges excluded from normal inline styling.
  final List<TextRange> exclusionRanges;

  /// Ranges where syntax is intentionally ambiguous or provisional.
  final List<TextRange> ambiguityZones;

  /// Cursor validation mask for projected source text.
  final CursorValidationMask cursorMask;

  /// Diagnostics emitted by the syntax engine.
  final List<SyntaxDiagnostic> diagnostics;

  /// Creates an authoritative syntax snapshot.
  const SyntaxSnapshot({
    required this.revision,
    required this.blocks,
    required this.inlineTokens,
    required this.markerRanges,
    required this.exclusionRanges,
    required this.ambiguityZones,
    required this.cursorMask,
    required this.diagnostics,
  });

  /// Creates an empty syntax snapshot.
  factory SyntaxSnapshot.empty({int revision = 0, int textLength = 0}) {
    return SyntaxSnapshot(
      revision: revision,
      blocks: const [],
      inlineTokens: const [],
      markerRanges: const [],
      exclusionRanges: const [],
      ambiguityZones: const [],
      cursorMask: PassthroughCursorValidationMask(textLength: textLength),
      diagnostics: const [],
    );
  }

  /// Stable hash for change detection across syntax payload fields.
  int get stableHash => Object.hash(
        revision,
        Object.hashAll(blocks),
        Object.hashAll(inlineTokens),
        _hashRanges(markerRanges),
        _hashRanges(exclusionRanges),
        _hashRanges(ambiguityZones),
        Object.hashAll(diagnostics),
      );

  static int _hashRanges(List<TextRange> ranges) {
    return Object.hashAll(ranges.map((r) => Object.hash(r.start, r.end)));
  }
}

/// Lightweight predictive syntax metadata for immediate editor feedback.
@immutable
class SyntaxPrediction {
  /// Controller revision represented by this prediction.
  final int revision;

  /// Source marker ranges that may be visually collapsed.
  final List<TextRange> markerRanges;

  /// Ranges excluded from normal inline styling.
  final List<TextRange> exclusionRanges;

  /// Ranges where syntax is intentionally ambiguous or provisional.
  final List<TextRange> ambiguityZones;

  /// Cursor validation mask for projected source text.
  final CursorValidationMask cursorMask;

  /// Creates a syntax prediction.
  const SyntaxPrediction({
    required this.revision,
    required this.markerRanges,
    required this.exclusionRanges,
    required this.ambiguityZones,
    required this.cursorMask,
  });

  /// Creates an empty syntax prediction.
  factory SyntaxPrediction.empty({int revision = 0, int textLength = 0}) {
    return SyntaxPrediction(
      revision: revision,
      markerRanges: const [],
      exclusionRanges: const [],
      ambiguityZones: const [],
      cursorMask: PassthroughCursorValidationMask(textLength: textLength),
    );
  }
}
