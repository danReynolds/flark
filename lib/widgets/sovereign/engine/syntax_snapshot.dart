import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'syntax_types.dart';

/// Contract: all offsets are UTF-16 code-unit indices (Dart string offsets).
abstract interface class CursorValidationMask {
  int snapToSafeOffset(int requestedOffset);
}

@immutable
class PassthroughCursorValidationMask implements CursorValidationMask {
  final int textLength;

  const PassthroughCursorValidationMask({required this.textLength})
      : assert(textLength >= 0);

  @override
  int snapToSafeOffset(int requestedOffset) {
    return requestedOffset.clamp(0, textLength).toInt();
  }
}

@immutable
class HiddenRangeCursorValidationMask implements CursorValidationMask {
  final int textLength;
  final List<TextRange> hiddenRanges;

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

@immutable
class SyntaxDiagnostic {
  final int start;
  final int end;
  final String message;
  final String? code;
  final bool isError;

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

@immutable
class SyntaxSnapshot {
  final int revision;
  final List<BlockSpan> blocks;
  final List<InlineSpanToken> inlineTokens;
  final List<TextRange> markerRanges;
  final List<TextRange> exclusionRanges;
  final List<TextRange> ambiguityZones;
  final CursorValidationMask cursorMask;
  final List<SyntaxDiagnostic> diagnostics;

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

@immutable
class SyntaxPrediction {
  final int revision;
  final List<TextRange> markerRanges;
  final List<TextRange> exclusionRanges;
  final List<TextRange> ambiguityZones;
  final CursorValidationMask cursorMask;

  const SyntaxPrediction({
    required this.revision,
    required this.markerRanges,
    required this.exclusionRanges,
    required this.ambiguityZones,
    required this.cursorMask,
  });

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
