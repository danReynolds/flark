import 'dart:convert';

import '../runtime/public/flark_v3_inline_facts.dart';
import 'flark_v3_inline_projection.dart' show FlarkV3InlineMarkerPolicy;

/// How one exhaustive interval of projected container text reaches display.
enum FlarkV3ProjectedInlineProjectionPieceKind { copy, hide, replace }

/// One exhaustive mapping interval in projected-container coordinates.
///
/// Projected offsets are relative to the independently certified container
/// projection (for example, marker-free block-quote text). They are never
/// physical document offsets.
final class FlarkV3ProjectedInlineProjectionPiece {
  const FlarkV3ProjectedInlineProjectionPiece._({
    required this.kind,
    required this.projectedStartUtf16,
    required this.projectedEndUtf16,
    required this.displayStartUtf16,
    required this.displayEndUtf16,
    required this.displayText,
  });

  final FlarkV3ProjectedInlineProjectionPieceKind kind;
  final int projectedStartUtf16;
  final int projectedEndUtf16;
  final int displayStartUtf16;
  final int displayEndUtf16;
  final String displayText;

  int get projectedLengthUtf16 => projectedEndUtf16 - projectedStartUtf16;
  int get displayLengthUtf16 => displayEndUtf16 - displayStartUtf16;
}

/// One visible display run backed by projected-container coordinates.
final class FlarkV3ProjectedInlineDisplayRun {
  const FlarkV3ProjectedInlineDisplayRun._({
    required this.text,
    required this.projectedStartUtf16,
    required this.projectedEndUtf16,
    required this.displayStartUtf16,
    required this.displayEndUtf16,
    required _ProjectedInlineSemanticStack semanticStack,
  }) : _semanticStack = semanticStack;

  final String text;
  final int projectedStartUtf16;
  final int projectedEndUtf16;
  final int displayStartUtf16;
  final int displayEndUtf16;
  final _ProjectedInlineSemanticStack _semanticStack;

  /// Active parser-certified style kinds, outermost first.
  ///
  /// The underlying stack is structurally shared between runs; this list is
  /// materialized only if a consumer asks for it.
  List<FlarkV3InlineFactKind> get semanticStyles =>
      _semanticStack.styleKindsOuterToInner;
}

/// A projected-text mismatch or non-canonical facts/projection combination.
final class FlarkV3ProjectedInlineProjectionException implements Exception {
  const FlarkV3ProjectedInlineProjectionException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3ProjectedInlineProjectionException($message)';
}

/// Pure-Dart inline presentation over a parser-certified container projection.
///
/// This layer does not recognize Markdown. It only applies exact projected
/// ranges supplied by [FlarkV3ProjectedInlineFacts]. A caller composes these
/// coordinates through its container source projection when physical source
/// mapping is required.
final class FlarkV3ProjectedInlineProjection {
  FlarkV3ProjectedInlineProjection._({
    required this.projectedText,
    required this.displayText,
    required List<FlarkV3ProjectedInlineProjectionPiece> pieces,
    required List<FlarkV3ProjectedInlineDisplayRun> runs,
  }) : pieces = List.unmodifiable(pieces),
       runs = List.unmodifiable(runs);

  factory FlarkV3ProjectedInlineProjection.fromValidatedFacts({
    required String projectedText,
    required FlarkV3ProjectedInlineFacts facts,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    _validateProjectedAuthority(projectedText, facts);

    final authoritative =
        facts.disposition ==
        FlarkV3ProjectedInlineFactsDisposition.authoritative;
    final hideCertifiedMarkers =
        authoritative &&
        markerPolicy == FlarkV3InlineMarkerPolicy.hideCertifiedMarkers;
    final hiddenMarkers = hideCertifiedMarkers
        ? _mergedProjectedMarkerRanges(facts.facts)
        : const <_ProjectedInlineRange>[];
    final boundaries = _projectedInlineBoundaries(facts);
    final characterReferencesByStart = <int, FlarkV3ProjectedInlineFact>{
      if (hideCertifiedMarkers)
        for (final fact in facts.facts)
          if (fact.kind == FlarkV3InlineFactKind.characterReference)
            fact.source.startUtf16: fact,
    };
    final hardLineBreakEndsByStart = <int, int>{
      if (hideCertifiedMarkers)
        for (final fact in facts.facts)
          if (fact.kind == FlarkV3InlineFactKind.hardLineBreak)
            fact.content.startUtf16: fact.content.endUtf16,
    };

    final pieces = <FlarkV3ProjectedInlineProjectionPiece>[];
    final runs = <FlarkV3ProjectedInlineDisplayRun>[];
    final display = StringBuffer();
    var displayOffset = 0;
    var hiddenIndex = 0;
    var nextFact = 0;
    var activeStack = _ProjectedInlineSemanticStack.empty;

    void advanceSemanticSweep(int boundary) {
      while (!activeStack.isEmpty &&
          activeStack.fact!.content.endUtf16 <= boundary) {
        activeStack = activeStack.parent!;
      }
      while (nextFact < facts.facts.length &&
          facts.facts[nextFact].content.startUtf16 <= boundary) {
        final fact = facts.facts[nextFact++];
        if (fact.content.endUtf16 > boundary) {
          activeStack = _ProjectedInlineSemanticStack.push(fact, activeStack);
        }
      }
    }

    for (var index = 0; index + 1 < boundaries.length; index += 1) {
      final start = boundaries[index];
      final end = boundaries[index + 1];
      advanceSemanticSweep(start);
      if (start == end) continue;

      while (hiddenIndex < hiddenMarkers.length &&
          hiddenMarkers[hiddenIndex].end <= start) {
        hiddenIndex += 1;
      }
      final hidden =
          hiddenIndex < hiddenMarkers.length &&
          hiddenMarkers[hiddenIndex].start <= start &&
          end <= hiddenMarkers[hiddenIndex].end;
      if (hidden) {
        pieces.add(
          FlarkV3ProjectedInlineProjectionPiece._(
            kind: FlarkV3ProjectedInlineProjectionPieceKind.hide,
            projectedStartUtf16: start,
            projectedEndUtf16: end,
            displayStartUtf16: displayOffset,
            displayEndUtf16: displayOffset,
            displayText: '',
          ),
        );
        continue;
      }

      final characterReference = characterReferencesByStart[start];
      final replacesCharacterReference =
          characterReference != null &&
          characterReference.source.endUtf16 == end;
      final normalizesHardLineBreak = hardLineBreakEndsByStart[start] == end;
      final text = replacesCharacterReference
          ? characterReference.characterReferenceValue!
          : normalizesHardLineBreak
          ? '\n'
          : projectedText.substring(start, end);
      final kind = replacesCharacterReference || normalizesHardLineBreak
          ? FlarkV3ProjectedInlineProjectionPieceKind.replace
          : FlarkV3ProjectedInlineProjectionPieceKind.copy;
      final displayEnd = displayOffset + text.length;
      pieces.add(
        FlarkV3ProjectedInlineProjectionPiece._(
          kind: kind,
          projectedStartUtf16: start,
          projectedEndUtf16: end,
          displayStartUtf16: displayOffset,
          displayEndUtf16: displayEnd,
          displayText: text,
        ),
      );
      runs.add(
        FlarkV3ProjectedInlineDisplayRun._(
          text: text,
          projectedStartUtf16: start,
          projectedEndUtf16: end,
          displayStartUtf16: displayOffset,
          displayEndUtf16: displayEnd,
          semanticStack: activeStack,
        ),
      );
      display.write(text);
      displayOffset = displayEnd;
    }
    advanceSemanticSweep(boundaries.last);

    return FlarkV3ProjectedInlineProjection._(
      projectedText: projectedText,
      displayText: display.toString(),
      pieces: pieces,
      runs: runs,
    );
  }

  /// Exact marker-free container text supplied to the inline parser.
  final String projectedText;

  /// Alias for generic projection consumers that call their input source text.
  String get sourceText => projectedText;

  /// Inline-presented text after certified Markdown markers/replacements.
  final String displayText;

  /// Exhaustive, ordered coverage of [projectedText].
  final List<FlarkV3ProjectedInlineProjectionPiece> pieces;

  /// Non-empty visible intervals and their parser-certified semantic styles.
  final List<FlarkV3ProjectedInlineDisplayRun> runs;

  int get projectedLengthUtf16 => projectedText.length;
  int get displayLengthUtf16 => displayText.length;
}

void _validateProjectedAuthority(
  String projectedText,
  FlarkV3ProjectedInlineFacts facts,
) {
  if (projectedText.length != facts.projectedUtf16Length ||
      utf8.encode(projectedText).length != facts.projectedUtf8Length ||
      facts.projectedSource.startUtf8 != 0 ||
      facts.projectedSource.startUtf16 != 0 ||
      (facts.disposition ==
              FlarkV3ProjectedInlineFactsDisposition.unsupported &&
          facts.facts.isNotEmpty)) {
    throw const FlarkV3ProjectedInlineProjectionException(
      'Projected text does not match its certified inline authority.',
    );
  }

  var previousStart = -1;
  var previousContentStart = -1;
  final open = <FlarkV3ProjectedInlineFact>[];
  for (final fact in facts.facts) {
    final source = fact.source;
    final content = fact.content;
    if (source.startUtf16 < previousStart ||
        content.startUtf16 < previousContentStart ||
        source.startUtf16 < 0 ||
        source.endUtf16 > projectedText.length ||
        source.startUtf16 != fact.opener.startUtf16 ||
        fact.opener.endUtf16 != content.startUtf16 ||
        content.endUtf16 != fact.closer.startUtf16 ||
        fact.closer.endUtf16 != source.endUtf16) {
      throw const FlarkV3ProjectedInlineProjectionException(
        'Projected inline fact ranges are not canonical.',
      );
    }
    previousStart = source.startUtf16;
    previousContentStart = content.startUtf16;
    while (open.isNotEmpty && source.startUtf16 >= open.last.source.endUtf16) {
      open.removeLast();
    }
    if (open.isNotEmpty &&
        (source.startUtf16 < open.last.content.startUtf16 ||
            source.endUtf16 > open.last.content.endUtf16)) {
      throw const FlarkV3ProjectedInlineProjectionException(
        'Projected inline fact ranges cross non-canonically.',
      );
    }
    open.add(fact);
  }
}

List<int> _projectedInlineBoundaries(FlarkV3ProjectedInlineFacts facts) {
  final boundaries = <int>{0, facts.projectedUtf16Length};
  for (final fact in facts.facts) {
    boundaries
      ..add(fact.source.startUtf16)
      ..add(fact.source.endUtf16)
      ..add(fact.content.startUtf16)
      ..add(fact.content.endUtf16)
      ..add(fact.opener.startUtf16)
      ..add(fact.opener.endUtf16)
      ..add(fact.closer.startUtf16)
      ..add(fact.closer.endUtf16);
  }
  return boundaries.toList()..sort();
}

List<_ProjectedInlineRange> _mergedProjectedMarkerRanges(
  List<FlarkV3ProjectedInlineFact> facts,
) {
  final ranges =
      <_ProjectedInlineRange>[
        for (final fact in facts) ...[
          if (!fact.opener.isCollapsed)
            _ProjectedInlineRange(fact.opener.startUtf16, fact.opener.endUtf16),
          if (!fact.closer.isCollapsed)
            _ProjectedInlineRange(fact.closer.startUtf16, fact.closer.endUtf16),
        ],
      ]..sort((left, right) {
        final byStart = left.start.compareTo(right.start);
        return byStart != 0 ? byStart : left.end.compareTo(right.end);
      });
  final merged = <_ProjectedInlineRange>[];
  for (final range in ranges) {
    if (merged.isEmpty || range.start > merged.last.end) {
      merged.add(range);
    } else if (range.end > merged.last.end) {
      merged[merged.length - 1] = _ProjectedInlineRange(
        merged.last.start,
        range.end,
      );
    }
  }
  return merged;
}

bool _isProjectedInlineStyle(FlarkV3InlineFactKind kind) => switch (kind) {
  FlarkV3InlineFactKind.emphasis ||
  FlarkV3InlineFactKind.strong ||
  FlarkV3InlineFactKind.code ||
  FlarkV3InlineFactKind.strikethrough => true,
  _ => false,
};

final class _ProjectedInlineRange {
  const _ProjectedInlineRange(this.start, this.end);

  final int start;
  final int end;
}

final class _ProjectedInlineSemanticStack {
  const _ProjectedInlineSemanticStack._(this.fact, this.parent, this.depth);

  static const empty = _ProjectedInlineSemanticStack._(null, null, 0);

  factory _ProjectedInlineSemanticStack.push(
    FlarkV3ProjectedInlineFact fact,
    _ProjectedInlineSemanticStack parent,
  ) => _ProjectedInlineSemanticStack._(fact, parent, parent.depth + 1);

  final FlarkV3ProjectedInlineFact? fact;
  final _ProjectedInlineSemanticStack? parent;
  final int depth;

  bool get isEmpty => depth == 0;

  List<FlarkV3InlineFactKind> get styleKindsOuterToInner {
    final innerToOuter = <FlarkV3InlineFactKind>[];
    var cursor = this;
    while (!cursor.isEmpty) {
      final kind = cursor.fact!.kind;
      if (_isProjectedInlineStyle(kind)) innerToOuter.add(kind);
      cursor = cursor.parent!;
    }
    return List.unmodifiable(innerToOuter.reversed);
  }
}
