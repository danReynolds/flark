import '../core/selection/flark_selection.dart';
import '../core/transaction/flark_source_range.dart';
import '../core/transaction/flark_transaction.dart';
import '../markdown/parse/flark_markdown_parse_result.dart';

enum FlarkHiddenRangeKind {
  markdownMarker,
  blockMarker,
  inlineMarker,
  escapeMarker,
  linkDestination,
  linkTitle,
  referenceDefinition,
  rawHtml,
  unknown,
}

enum FlarkReplacementRangeKind { htmlEntity, unknown }

final class FlarkHiddenRange {
  const FlarkHiddenRange({required this.range, required this.kind});

  final FlarkSourceRange range;
  final FlarkHiddenRangeKind kind;

  @override
  bool operator ==(Object other) {
    return other is FlarkHiddenRange &&
        other.range == range &&
        other.kind == kind;
  }

  @override
  int get hashCode => Object.hash(range, kind);
}

final class FlarkReplacementRange {
  const FlarkReplacementRange({
    required this.range,
    required this.kind,
    required this.replacementText,
  });

  final FlarkSourceRange range;
  final FlarkReplacementRangeKind kind;
  final String replacementText;

  @override
  bool operator ==(Object other) {
    return other is FlarkReplacementRange &&
        other.range == range &&
        other.kind == kind &&
        other.replacementText == replacementText;
  }

  @override
  int get hashCode => Object.hash(range, kind, replacementText);
}

enum FlarkProjectionAmbiguityKind {
  delimiterRun,
  linkReference,
  tableBoundary,
  rawHtml,
  unknown,
}

final class FlarkProjectionAmbiguityZone {
  const FlarkProjectionAmbiguityZone({
    required this.range,
    required this.kind,
    this.preferredAffinity = FlarkMapAffinity.downstream,
  });

  final FlarkSourceRange range;
  final FlarkProjectionAmbiguityKind kind;
  final FlarkMapAffinity preferredAffinity;

  int normalize(int sourceOffset) {
    if (sourceOffset <= range.start || sourceOffset >= range.end) {
      return sourceOffset;
    }
    return switch (preferredAffinity) {
      FlarkMapAffinity.upstream => range.start,
      FlarkMapAffinity.downstream => range.end,
    };
  }

  @override
  bool operator ==(Object other) {
    return other is FlarkProjectionAmbiguityZone &&
        other.range == range &&
        other.kind == kind &&
        other.preferredAffinity == preferredAffinity;
  }

  @override
  int get hashCode => Object.hash(range, kind, preferredAffinity);
}

final class FlarkProjectionPrediction {
  const FlarkProjectionPrediction({
    required this.projection,
    required this.touchedProjectionSensitiveRange,
    this.invalidatedRange,
  });

  final FlarkProjection projection;
  final bool touchedProjectionSensitiveRange;
  final FlarkSourceRange? invalidatedRange;
}

final class FlarkProjectionReconciliation {
  const FlarkProjectionReconciliation({
    required this.predicted,
    required this.authoritative,
    required this.hiddenRangesChanged,
    required this.replacementRangesChanged,
    required this.ambiguityZonesChanged,
    required this.displayLengthDelta,
  });

  final FlarkProjection predicted;
  final FlarkProjection authoritative;
  final bool hiddenRangesChanged;
  final bool replacementRangesChanged;
  final bool ambiguityZonesChanged;
  final int displayLengthDelta;

  bool get isStable =>
      !hiddenRangesChanged &&
      !replacementRangesChanged &&
      !ambiguityZonesChanged &&
      displayLengthDelta == 0;
}

final class FlarkCursorMask {
  factory FlarkCursorMask({
    required int textLength,
    Iterable<FlarkHiddenRange> hiddenRanges = const [],
    Iterable<FlarkReplacementRange> replacementRanges = const [],
  }) {
    final validatedHiddenRanges = _validatedHiddenRanges(
      textLength,
      hiddenRanges,
    );
    final validatedReplacementRanges = _validatedReplacementRanges(
      textLength,
      replacementRanges,
    );
    return FlarkCursorMask._(
      textLength: textLength,
      hiddenRanges: validatedHiddenRanges,
      replacementRanges: validatedReplacementRanges,
      projectionSpans: _validatedProjectionSpans(
        hiddenRanges: validatedHiddenRanges,
        replacementRanges: validatedReplacementRanges,
      ),
    );
  }

  FlarkCursorMask._({
    required this.textLength,
    required Iterable<FlarkHiddenRange> hiddenRanges,
    required Iterable<FlarkReplacementRange> replacementRanges,
    required Iterable<_ProjectionSpan> projectionSpans,
  }) : hiddenRanges = List<FlarkHiddenRange>.unmodifiable(hiddenRanges),
       replacementRanges = List<FlarkReplacementRange>.unmodifiable(
         replacementRanges,
       ),
       _projectionSpans = List<_ProjectionSpan>.unmodifiable(projectionSpans);

  final int textLength;
  final List<FlarkHiddenRange> hiddenRanges;
  final List<FlarkReplacementRange> replacementRanges;
  final List<_ProjectionSpan> _projectionSpans;

  bool allows(int sourceOffset) {
    _checkOffset(sourceOffset);
    return !_isInsideHiddenRange(sourceOffset);
  }

  int normalize(
    int sourceOffset, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    _checkOffset(sourceOffset);
    final index = _spanStrictlyContaining(sourceOffset);
    if (index < 0) return sourceOffset;
    final range = _projectionSpans[index].range;
    return switch (affinity) {
      FlarkMapAffinity.upstream => range.start,
      FlarkMapAffinity.downstream => range.end,
    };
  }

  bool _isInsideHiddenRange(int sourceOffset) {
    return _spanStrictlyContaining(sourceOffset) >= 0;
  }

  /// Index of the (unique) span with `start < sourceOffset < end`, or `-1`.
  ///
  /// Spans are sorted by start and non-overlapping, so the only candidate is
  /// the rightmost span starting before [sourceOffset]; a boundary offset
  /// (`== start` or `== end`) is never considered inside.
  int _spanStrictlyContaining(int sourceOffset) {
    var low = 0;
    var high = _projectionSpans.length - 1;
    var predecessor = -1;
    while (low <= high) {
      final mid = low + ((high - low) >> 1);
      if (_projectionSpans[mid].range.start < sourceOffset) {
        predecessor = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    if (predecessor < 0) return -1;
    return sourceOffset < _projectionSpans[predecessor].range.end
        ? predecessor
        : -1;
  }

  void _checkOffset(int sourceOffset) {
    if (sourceOffset < 0 || sourceOffset > textLength) {
      throw RangeError.range(sourceOffset, 0, textLength, 'sourceOffset');
    }
  }
}

final class FlarkProjection {
  factory FlarkProjection({
    required int textLength,
    Iterable<FlarkHiddenRange> hiddenRanges = const [],
    Iterable<FlarkReplacementRange> replacementRanges = const [],
    Iterable<FlarkProjectionAmbiguityZone> ambiguityZones = const [],
  }) {
    final validatedHiddenRanges = _validatedHiddenRanges(
      textLength,
      hiddenRanges,
    );
    final validatedReplacementRanges = _validatedReplacementRanges(
      textLength,
      replacementRanges,
    );
    final projectionSpans = _validatedProjectionSpans(
      hiddenRanges: validatedHiddenRanges,
      replacementRanges: validatedReplacementRanges,
    );
    return FlarkProjection._(
      textLength: textLength,
      hiddenRanges: validatedHiddenRanges,
      replacementRanges: validatedReplacementRanges,
      projectionSpans: projectionSpans,
      ambiguityZones: _validatedAmbiguityZones(textLength, ambiguityZones),
    );
  }

  // Skips the redundant sort that the public constructor runs when callers can
  // promise the inputs are already sorted by start. Still verifies bounds and
  // non-overlap in one O(n) pass, so accidental mis-use surfaces loudly rather
  // than corrupting projection state. Predictive projections derive their
  // inputs from a validated source through a monotonic transaction mapping,
  // which preserves both invariants.
  factory FlarkProjection._sortedInputs({
    required int textLength,
    required List<FlarkHiddenRange> hiddenRanges,
    required List<FlarkReplacementRange> replacementRanges,
    required List<FlarkProjectionAmbiguityZone> ambiguityZones,
  }) {
    _verifySortedNonOverlappingInBounds(
      textLength,
      hiddenRanges,
      replacementRanges,
    );
    for (final zone in ambiguityZones) {
      zone.range.validate(textLength);
    }
    final projectionSpans = _buildProjectionSpans(
      hiddenRanges: hiddenRanges,
      replacementRanges: replacementRanges,
    );
    return FlarkProjection._(
      textLength: textLength,
      hiddenRanges: hiddenRanges,
      replacementRanges: replacementRanges,
      projectionSpans: projectionSpans,
      ambiguityZones: ambiguityZones,
    );
  }

  FlarkProjection._({
    required this.textLength,
    required Iterable<FlarkHiddenRange> hiddenRanges,
    required Iterable<FlarkReplacementRange> replacementRanges,
    required Iterable<_ProjectionSpan> projectionSpans,
    required Iterable<FlarkProjectionAmbiguityZone> ambiguityZones,
  }) : hiddenRanges = List<FlarkHiddenRange>.unmodifiable(hiddenRanges),
       replacementRanges = List<FlarkReplacementRange>.unmodifiable(
         replacementRanges,
       ),
       _projectionSpans = List<_ProjectionSpan>.unmodifiable(projectionSpans),
       ambiguityZones = List<FlarkProjectionAmbiguityZone>.unmodifiable(
         ambiguityZones,
       ),
       _spanSourceDeltaPrefix = List<int>.unmodifiable(
         _buildSpanSourceDeltaPrefix(projectionSpans),
       ),
       _spanDisplayStarts = List<int>.unmodifiable(
         _buildSpanDisplayStarts(projectionSpans),
       ),
       cursorMask = FlarkCursorMask._(
         textLength: textLength,
         hiddenRanges: hiddenRanges,
         replacementRanges: replacementRanges,
         projectionSpans: projectionSpans,
       );

  factory FlarkProjection.fromParseResult(
    FlarkMarkdownParseResult parseResult,
  ) {
    return FlarkProjection(
      textLength: parseResult.sourceTextLength,
      hiddenRanges: parseResult.hiddenRanges.map(
        (hiddenRange) => FlarkHiddenRange(
          range: hiddenRange.sourceRange,
          kind: _projectionHiddenRangeKind(hiddenRange.kind),
        ),
      ),
      replacementRanges: parseResult.replacementRanges.map(
        (replacementRange) => FlarkReplacementRange(
          range: replacementRange.sourceRange,
          kind: _projectionReplacementRangeKind(replacementRange.kind),
          replacementText: replacementRange.replacementText,
        ),
      ),
      ambiguityZones: parseResult.ambiguityZones.map(
        (zone) => FlarkProjectionAmbiguityZone(
          range: zone.sourceRange,
          kind: _projectionAmbiguityKind(zone.kind),
          preferredAffinity: zone.preferredAffinity,
        ),
      ),
    );
  }

  final int textLength;
  final List<FlarkHiddenRange> hiddenRanges;
  final List<FlarkReplacementRange> replacementRanges;
  final List<FlarkProjectionAmbiguityZone> ambiguityZones;
  final List<_ProjectionSpan> _projectionSpans;
  final List<int> _spanSourceDeltaPrefix;
  final List<int> _spanDisplayStarts;
  final FlarkCursorMask cursorMask;

  int sourceToDisplayOffset(int sourceOffset) {
    _checkOffset(sourceOffset);
    final spanIndex = _lastSpanStartingBefore(sourceOffset);
    if (spanIndex < 0) return sourceOffset;

    final span = _projectionSpans[spanIndex];
    final displayStart = _spanDisplayStarts[spanIndex];
    if (sourceOffset < span.range.end) return displayStart;

    final sourceDelta = _spanSourceDeltaPrefix[spanIndex];
    return sourceOffset - sourceDelta;
  }

  int displayToSourceOffset(
    int displayOffset, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    if (displayOffset < 0 || displayOffset > displayLength) {
      throw RangeError.range(displayOffset, 0, displayLength, 'displayOffset');
    }

    final spanIndex = _lastSpanAtOrBeforeDisplayOffset(displayOffset);
    if (spanIndex < 0) return displayOffset;

    final span = _projectionSpans[spanIndex];
    final displayStart = _spanDisplayStarts[spanIndex];
    final displayEnd = displayStart + span.displayLength;

    if (span.isHidden) {
      if (displayOffset == displayStart) {
        final sourceOffset = affinity == FlarkMapAffinity.upstream
            ? span.range.start
            : span.range.end;
        return _clampInt(sourceOffset, 0, textLength);
      }
      final sourceDelta = _spanSourceDeltaPrefix[spanIndex];
      return _clampInt(displayOffset + sourceDelta, 0, textLength);
    }

    if (displayOffset <= displayEnd) {
      if (displayOffset == displayStart) return span.range.start;
      if (displayOffset == displayEnd) return span.range.end;
      return switch (affinity) {
        FlarkMapAffinity.upstream => span.range.start,
        FlarkMapAffinity.downstream => span.range.end,
      };
    }

    final sourceDelta = _spanSourceDeltaPrefix[spanIndex];
    return _clampInt(displayOffset + sourceDelta, 0, textLength);
  }

  String projectText(String sourceText) {
    if (sourceText.length != textLength) {
      throw ArgumentError.value(
        sourceText,
        'sourceText',
        'Source text length must match projection textLength.',
      );
    }

    final buffer = StringBuffer();
    var cursor = 0;
    for (final span in _projectionSpans) {
      buffer.write(sourceText.substring(cursor, span.range.start));
      buffer.write(span.replacementText);
      cursor = span.range.end;
    }
    buffer.write(sourceText.substring(cursor));
    return buffer.toString();
  }

  FlarkSelection sourceSelectionToDisplay(
    FlarkSelection sourceSelection, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    return FlarkSelection(
      baseOffset: sourceToDisplayOffset(
        cursorMask.normalize(sourceSelection.baseOffset, affinity: affinity),
      ),
      extentOffset: sourceToDisplayOffset(
        cursorMask.normalize(sourceSelection.extentOffset, affinity: affinity),
      ),
    );
  }

  FlarkSelection displaySelectionToSource(
    FlarkSelection displaySelection, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    return FlarkSelection(
      baseOffset: displayToSourceOffset(
        displaySelection.baseOffset,
        affinity: affinity,
      ),
      extentOffset: displayToSourceOffset(
        displaySelection.extentOffset,
        affinity: affinity,
      ),
    );
  }

  int normalizeAmbiguousOffset(int sourceOffset) {
    _checkOffset(sourceOffset);
    for (final zone in ambiguityZones) {
      final normalized = zone.normalize(sourceOffset);
      if (normalized != sourceOffset) return normalized;
    }
    return sourceOffset;
  }

  FlarkProjectionPrediction predictAfter(
    FlarkTransaction transaction, {
    required int textLengthAfter,
  }) {
    var touchedSensitiveRange = false;
    final mappedHiddenRanges = <FlarkHiddenRange>[];
    for (final hiddenRange in hiddenRanges) {
      if (!touchedSensitiveRange && _touchesAny(transaction, hiddenRange.range)) {
        touchedSensitiveRange = true;
      }
      final mapped = _mapHiddenRange(transaction, hiddenRange);
      if (!mapped.range.isCollapsed) mappedHiddenRanges.add(mapped);
    }
    final mappedReplacementRanges = <FlarkReplacementRange>[];
    for (final replacementRange in replacementRanges) {
      if (!touchedSensitiveRange &&
          _touchesAny(transaction, replacementRange.range)) {
        touchedSensitiveRange = true;
      }
      final mapped = _mapReplacementRange(transaction, replacementRange);
      if (!mapped.range.isCollapsed) mappedReplacementRanges.add(mapped);
    }
    final mappedAmbiguityZones = <FlarkProjectionAmbiguityZone>[];
    for (final zone in ambiguityZones) {
      if (!touchedSensitiveRange && _touchesAny(transaction, zone.range)) {
        touchedSensitiveRange = true;
      }
      final mapped = _mapAmbiguityZone(transaction, zone);
      if (!mapped.range.isCollapsed) mappedAmbiguityZones.add(mapped);
    }

    return FlarkProjectionPrediction(
      projection: FlarkProjection._sortedInputs(
        textLength: textLengthAfter,
        hiddenRanges: mappedHiddenRanges,
        replacementRanges: mappedReplacementRanges,
        ambiguityZones: mappedAmbiguityZones,
      ),
      touchedProjectionSensitiveRange: touchedSensitiveRange,
      invalidatedRange: _transactionInvalidatedRange(transaction),
    );
  }

  FlarkProjectionReconciliation reconcileWith(FlarkProjection authoritative) {
    return FlarkProjectionReconciliation(
      predicted: this,
      authoritative: authoritative,
      hiddenRangesChanged: !_sameList(hiddenRanges, authoritative.hiddenRanges),
      replacementRangesChanged: !_sameList(
        replacementRanges,
        authoritative.replacementRanges,
      ),
      ambiguityZonesChanged: !_sameList(
        ambiguityZones,
        authoritative.ambiguityZones,
      ),
      displayLengthDelta: authoritative.displayLength - displayLength,
    );
  }

  int get displayLength {
    var sourceDelta = 0;
    for (final span in _projectionSpans) {
      sourceDelta += span.sourceDelta;
    }
    return textLength - sourceDelta;
  }

  void _checkOffset(int sourceOffset) {
    if (sourceOffset < 0 || sourceOffset > textLength) {
      throw RangeError.range(sourceOffset, 0, textLength, 'sourceOffset');
    }
  }

  int _lastSpanStartingBefore(int sourceOffset) {
    var low = 0;
    var high = _projectionSpans.length;
    while (low < high) {
      final middle = (low + high) >> 1;
      if (_projectionSpans[middle].range.start < sourceOffset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low - 1;
  }

  int _lastSpanAtOrBeforeDisplayOffset(int displayOffset) {
    var low = 0;
    var high = _spanDisplayStarts.length;
    while (low < high) {
      final middle = (low + high) >> 1;
      if (_spanDisplayStarts[middle] <= displayOffset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low - 1;
  }
}

FlarkHiddenRangeKind _projectionHiddenRangeKind(
  FlarkMarkdownHiddenRangeKind kind,
) {
  return switch (kind) {
    FlarkMarkdownHiddenRangeKind.markdownMarker =>
      FlarkHiddenRangeKind.markdownMarker,
    FlarkMarkdownHiddenRangeKind.blockMarker =>
      FlarkHiddenRangeKind.blockMarker,
    FlarkMarkdownHiddenRangeKind.inlineMarker =>
      FlarkHiddenRangeKind.inlineMarker,
    FlarkMarkdownHiddenRangeKind.escapeMarker =>
      FlarkHiddenRangeKind.escapeMarker,
    FlarkMarkdownHiddenRangeKind.linkDestination =>
      FlarkHiddenRangeKind.linkDestination,
    FlarkMarkdownHiddenRangeKind.linkTitle => FlarkHiddenRangeKind.linkTitle,
    FlarkMarkdownHiddenRangeKind.referenceDefinition =>
      FlarkHiddenRangeKind.referenceDefinition,
    FlarkMarkdownHiddenRangeKind.rawHtml => FlarkHiddenRangeKind.rawHtml,
    FlarkMarkdownHiddenRangeKind.unknown => FlarkHiddenRangeKind.unknown,
  };
}

FlarkReplacementRangeKind _projectionReplacementRangeKind(
  FlarkMarkdownReplacementRangeKind kind,
) {
  return switch (kind) {
    FlarkMarkdownReplacementRangeKind.htmlEntity =>
      FlarkReplacementRangeKind.htmlEntity,
    FlarkMarkdownReplacementRangeKind.unknown =>
      FlarkReplacementRangeKind.unknown,
  };
}

FlarkProjectionAmbiguityKind _projectionAmbiguityKind(
  FlarkMarkdownAmbiguityKind kind,
) {
  return switch (kind) {
    FlarkMarkdownAmbiguityKind.delimiterRun =>
      FlarkProjectionAmbiguityKind.delimiterRun,
    FlarkMarkdownAmbiguityKind.linkReference =>
      FlarkProjectionAmbiguityKind.linkReference,
    FlarkMarkdownAmbiguityKind.tableBoundary =>
      FlarkProjectionAmbiguityKind.tableBoundary,
    FlarkMarkdownAmbiguityKind.rawHtml => FlarkProjectionAmbiguityKind.rawHtml,
    FlarkMarkdownAmbiguityKind.unknown => FlarkProjectionAmbiguityKind.unknown,
  };
}

List<FlarkHiddenRange> _sortedHiddenRanges(
  Iterable<FlarkHiddenRange> hiddenRanges,
) {
  final sorted = [...hiddenRanges]
    ..sort((a, b) => a.range.start.compareTo(b.range.start));
  var previousEnd = 0;
  for (final hiddenRange in sorted) {
    _validateRangeShape(hiddenRange.range);
    if (hiddenRange.range.start < previousEnd) {
      throw StateError('Projection hidden ranges cannot overlap.');
    }
    previousEnd = hiddenRange.range.end;
  }
  return sorted;
}

void _validateRangeShape(FlarkSourceRange range) {
  if (range.start < 0) {
    throw RangeError.range(range.start, 0, null, 'start');
  }
  if (range.end < range.start) {
    throw RangeError.range(range.end, range.start, null, 'end');
  }
}

List<FlarkHiddenRange> _validatedHiddenRanges(
  int textLength,
  Iterable<FlarkHiddenRange> hiddenRanges,
) {
  final sorted = _sortedHiddenRanges(hiddenRanges);
  for (final hiddenRange in sorted) {
    hiddenRange.range.validate(textLength);
  }
  return sorted;
}

List<FlarkReplacementRange> _sortedReplacementRanges(
  Iterable<FlarkReplacementRange> replacementRanges,
) {
  final sorted = [...replacementRanges]
    ..sort((a, b) => a.range.start.compareTo(b.range.start));
  var previousEnd = 0;
  for (final replacementRange in sorted) {
    _validateRangeShape(replacementRange.range);
    if (replacementRange.range.start < previousEnd) {
      throw StateError('Projection replacement ranges cannot overlap.');
    }
    previousEnd = replacementRange.range.end;
  }
  return sorted;
}

List<FlarkReplacementRange> _validatedReplacementRanges(
  int textLength,
  Iterable<FlarkReplacementRange> replacementRanges,
) {
  final sorted = _sortedReplacementRanges(replacementRanges);
  for (final replacementRange in sorted) {
    replacementRange.range.validate(textLength);
  }
  return sorted;
}

List<_ProjectionSpan> _validatedProjectionSpans({
  required Iterable<FlarkHiddenRange> hiddenRanges,
  required Iterable<FlarkReplacementRange> replacementRanges,
}) {
  final spans = _buildProjectionSpans(
    hiddenRanges: hiddenRanges,
    replacementRanges: replacementRanges,
  );
  var previousEnd = 0;
  for (final span in spans) {
    if (span.range.start < previousEnd) {
      throw StateError('Projection ranges cannot overlap.');
    }
    previousEnd = span.range.end;
  }
  return spans;
}

List<_ProjectionSpan> _buildProjectionSpans({
  required Iterable<FlarkHiddenRange> hiddenRanges,
  required Iterable<FlarkReplacementRange> replacementRanges,
}) {
  return [
    for (final hiddenRange in hiddenRanges)
      _ProjectionSpan(range: hiddenRange.range, replacementText: ''),
    for (final replacementRange in replacementRanges)
      _ProjectionSpan(
        range: replacementRange.range,
        replacementText: replacementRange.replacementText,
      ),
  ]..sort((a, b) {
    final start = a.range.start.compareTo(b.range.start);
    if (start != 0) return start;
    return a.range.end.compareTo(b.range.end);
  });
}

List<FlarkProjectionAmbiguityZone> _validatedAmbiguityZones(
  int textLength,
  Iterable<FlarkProjectionAmbiguityZone> ambiguityZones,
) {
  final zones = [...ambiguityZones];
  for (final zone in zones) {
    zone.range.validate(textLength);
  }
  return zones;
}

FlarkHiddenRange _mapHiddenRange(
  FlarkTransaction transaction,
  FlarkHiddenRange hiddenRange,
) {
  return FlarkHiddenRange(
    range: _mapRange(transaction, hiddenRange.range),
    kind: hiddenRange.kind,
  );
}

FlarkReplacementRange _mapReplacementRange(
  FlarkTransaction transaction,
  FlarkReplacementRange replacementRange,
) {
  return FlarkReplacementRange(
    range: _mapRange(transaction, replacementRange.range),
    kind: replacementRange.kind,
    replacementText: replacementRange.replacementText,
  );
}

FlarkProjectionAmbiguityZone _mapAmbiguityZone(
  FlarkTransaction transaction,
  FlarkProjectionAmbiguityZone zone,
) {
  return FlarkProjectionAmbiguityZone(
    range: _mapRange(transaction, zone.range),
    kind: zone.kind,
    preferredAffinity: zone.preferredAffinity,
  );
}

FlarkSourceRange _mapRange(
  FlarkTransaction transaction,
  FlarkSourceRange range,
) {
  final mappedStart = transaction.mapOffset(
    range.start,
    affinity: FlarkMapAffinity.downstream,
  );
  final mappedEnd = transaction.mapOffset(
    range.end,
    affinity: FlarkMapAffinity.upstream,
  );
  if (mappedStart > mappedEnd) {
    return FlarkSourceRange(mappedEnd, mappedEnd);
  }
  return FlarkSourceRange(mappedStart, mappedEnd);
}

FlarkSourceRange? _transactionInvalidatedRange(FlarkTransaction transaction) {
  final metadataRange = transaction.metadata.projectionInvalidationRange;
  if (metadataRange != null) return metadataRange;
  FlarkSourceRange? invalidated;
  for (final operation in transaction.operations) {
    invalidated = invalidated == null
        ? operation.replacedRange
        : invalidated.union(operation.replacedRange);
  }
  return invalidated;
}

bool _sameList<T>(List<T> left, List<T> right) {
  if (left.length != right.length) return false;
  for (var i = 0; i < left.length; i++) {
    if (left[i] != right[i]) return false;
  }
  return true;
}

bool _operationTouchesRange(
  FlarkSourceRange operationRange,
  FlarkSourceRange sensitiveRange,
) {
  if (operationRange.isCollapsed) {
    return operationRange.start > sensitiveRange.start &&
        operationRange.start < sensitiveRange.end;
  }
  return operationRange.intersects(sensitiveRange);
}

bool _touchesAny(FlarkTransaction transaction, FlarkSourceRange sensitive) {
  for (final operation in transaction.operations) {
    if (_operationTouchesRange(operation.replacedRange, sensitive)) {
      return true;
    }
  }
  return false;
}

void _verifySortedNonOverlappingInBounds(
  int textLength,
  List<FlarkHiddenRange> hiddenRanges,
  List<FlarkReplacementRange> replacementRanges,
) {
  var hiddenPrevEnd = 0;
  for (final hidden in hiddenRanges) {
    final range = hidden.range;
    _validateRangeShape(range);
    if (range.end > textLength) {
      throw RangeError.range(range.end, 0, textLength, 'end');
    }
    if (range.start < hiddenPrevEnd) {
      throw StateError('Projection hidden ranges must be sorted and disjoint.');
    }
    hiddenPrevEnd = range.end;
  }
  var replacementPrevEnd = 0;
  for (final replacement in replacementRanges) {
    final range = replacement.range;
    _validateRangeShape(range);
    if (range.end > textLength) {
      throw RangeError.range(range.end, 0, textLength, 'end');
    }
    if (range.start < replacementPrevEnd) {
      throw StateError(
        'Projection replacement ranges must be sorted and disjoint.',
      );
    }
    replacementPrevEnd = range.end;
  }
  // Merge-walk the two pre-sorted lists to catch hidden-vs-replacement overlap
  // without re-sorting either input.
  var hiddenIndex = 0;
  var replacementIndex = 0;
  while (hiddenIndex < hiddenRanges.length &&
      replacementIndex < replacementRanges.length) {
    final hidden = hiddenRanges[hiddenIndex].range;
    final replacement = replacementRanges[replacementIndex].range;
    if (hidden.start < replacement.start) {
      if (hidden.end > replacement.start) {
        throw StateError('Projection hidden and replacement ranges overlap.');
      }
      hiddenIndex++;
    } else {
      if (replacement.end > hidden.start) {
        throw StateError('Projection hidden and replacement ranges overlap.');
      }
      replacementIndex++;
    }
  }
}

int _clampInt(int value, int min, int max) {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

Iterable<int> _buildSpanSourceDeltaPrefix(
  Iterable<_ProjectionSpan> projectionSpans,
) sync* {
  var sourceDelta = 0;
  for (final span in projectionSpans) {
    sourceDelta += span.sourceDelta;
    yield sourceDelta;
  }
}

Iterable<int> _buildSpanDisplayStarts(
  Iterable<_ProjectionSpan> projectionSpans,
) sync* {
  var sourceDeltaBefore = 0;
  for (final span in projectionSpans) {
    yield span.range.start - sourceDeltaBefore;
    sourceDeltaBefore += span.sourceDelta;
  }
}

final class _ProjectionSpan {
  const _ProjectionSpan({required this.range, required this.replacementText});

  final FlarkSourceRange range;
  final String replacementText;

  bool get isHidden => replacementText.isEmpty;
  int get displayLength => replacementText.length;
  int get sourceDelta => range.length - displayLength;
}
