import '../core/selection/sovereign_selection.dart';
import '../core/transaction/sovereign_source_range.dart';
import '../core/transaction/sovereign_transaction.dart';
import '../markdown/parse/sovereign_markdown_parse_result.dart';

enum SovereignHiddenRangeKind {
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

enum SovereignReplacementRangeKind { htmlEntity, unknown }

final class SovereignHiddenRange {
  const SovereignHiddenRange({required this.range, required this.kind});

  final SovereignSourceRange range;
  final SovereignHiddenRangeKind kind;

  @override
  bool operator ==(Object other) {
    return other is SovereignHiddenRange &&
        other.range == range &&
        other.kind == kind;
  }

  @override
  int get hashCode => Object.hash(range, kind);
}

final class SovereignReplacementRange {
  const SovereignReplacementRange({
    required this.range,
    required this.kind,
    required this.replacementText,
  });

  final SovereignSourceRange range;
  final SovereignReplacementRangeKind kind;
  final String replacementText;

  @override
  bool operator ==(Object other) {
    return other is SovereignReplacementRange &&
        other.range == range &&
        other.kind == kind &&
        other.replacementText == replacementText;
  }

  @override
  int get hashCode => Object.hash(range, kind, replacementText);
}

enum SovereignProjectionAmbiguityKind {
  delimiterRun,
  linkReference,
  tableBoundary,
  rawHtml,
  unknown,
}

final class SovereignProjectionAmbiguityZone {
  const SovereignProjectionAmbiguityZone({
    required this.range,
    required this.kind,
    this.preferredAffinity = SovereignMapAffinity.downstream,
  });

  final SovereignSourceRange range;
  final SovereignProjectionAmbiguityKind kind;
  final SovereignMapAffinity preferredAffinity;

  int normalize(int sourceOffset) {
    if (sourceOffset <= range.start || sourceOffset >= range.end) {
      return sourceOffset;
    }
    return switch (preferredAffinity) {
      SovereignMapAffinity.upstream => range.start,
      SovereignMapAffinity.downstream => range.end,
    };
  }

  @override
  bool operator ==(Object other) {
    return other is SovereignProjectionAmbiguityZone &&
        other.range == range &&
        other.kind == kind &&
        other.preferredAffinity == preferredAffinity;
  }

  @override
  int get hashCode => Object.hash(range, kind, preferredAffinity);
}

final class SovereignProjectionPrediction {
  const SovereignProjectionPrediction({
    required this.projection,
    required this.touchedProjectionSensitiveRange,
    this.invalidatedRange,
  });

  final SovereignProjection projection;
  final bool touchedProjectionSensitiveRange;
  final SovereignSourceRange? invalidatedRange;
}

final class SovereignProjectionReconciliation {
  const SovereignProjectionReconciliation({
    required this.predicted,
    required this.authoritative,
    required this.hiddenRangesChanged,
    required this.replacementRangesChanged,
    required this.ambiguityZonesChanged,
    required this.displayLengthDelta,
  });

  final SovereignProjection predicted;
  final SovereignProjection authoritative;
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

final class SovereignCursorMask {
  factory SovereignCursorMask({
    required int textLength,
    Iterable<SovereignHiddenRange> hiddenRanges = const [],
    Iterable<SovereignReplacementRange> replacementRanges = const [],
  }) {
    final validatedHiddenRanges = _validatedHiddenRanges(
      textLength,
      hiddenRanges,
    );
    final validatedReplacementRanges = _validatedReplacementRanges(
      textLength,
      replacementRanges,
    );
    return SovereignCursorMask._(
      textLength: textLength,
      hiddenRanges: validatedHiddenRanges,
      replacementRanges: validatedReplacementRanges,
      projectionSpans: _validatedProjectionSpans(
        hiddenRanges: validatedHiddenRanges,
        replacementRanges: validatedReplacementRanges,
      ),
    );
  }

  SovereignCursorMask._({
    required this.textLength,
    required Iterable<SovereignHiddenRange> hiddenRanges,
    required Iterable<SovereignReplacementRange> replacementRanges,
    required Iterable<_ProjectionSpan> projectionSpans,
  }) : hiddenRanges = List<SovereignHiddenRange>.unmodifiable(hiddenRanges),
       replacementRanges = List<SovereignReplacementRange>.unmodifiable(
         replacementRanges,
       ),
       _projectionSpans = List<_ProjectionSpan>.unmodifiable(projectionSpans);

  final int textLength;
  final List<SovereignHiddenRange> hiddenRanges;
  final List<SovereignReplacementRange> replacementRanges;
  final List<_ProjectionSpan> _projectionSpans;

  bool allows(int sourceOffset) {
    _checkOffset(sourceOffset);
    return !_isInsideHiddenRange(sourceOffset);
  }

  int normalize(
    int sourceOffset, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    _checkOffset(sourceOffset);
    for (final span in _projectionSpans) {
      final range = span.range;
      if (sourceOffset <= range.start) return sourceOffset;
      if (sourceOffset < range.end) {
        return switch (affinity) {
          SovereignMapAffinity.upstream => range.start,
          SovereignMapAffinity.downstream => range.end,
        };
      }
    }
    return sourceOffset;
  }

  bool _isInsideHiddenRange(int sourceOffset) {
    for (final span in _projectionSpans) {
      final range = span.range;
      if (sourceOffset <= range.start) continue;
      if (sourceOffset < range.end) return true;
      if (sourceOffset >= range.end) continue;
    }
    return false;
  }

  void _checkOffset(int sourceOffset) {
    if (sourceOffset < 0 || sourceOffset > textLength) {
      throw RangeError.range(sourceOffset, 0, textLength, 'sourceOffset');
    }
  }
}

final class SovereignProjection {
  factory SovereignProjection({
    required int textLength,
    Iterable<SovereignHiddenRange> hiddenRanges = const [],
    Iterable<SovereignReplacementRange> replacementRanges = const [],
    Iterable<SovereignProjectionAmbiguityZone> ambiguityZones = const [],
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
    return SovereignProjection._(
      textLength: textLength,
      hiddenRanges: validatedHiddenRanges,
      replacementRanges: validatedReplacementRanges,
      projectionSpans: projectionSpans,
      ambiguityZones: _validatedAmbiguityZones(textLength, ambiguityZones),
    );
  }

  SovereignProjection._({
    required this.textLength,
    required Iterable<SovereignHiddenRange> hiddenRanges,
    required Iterable<SovereignReplacementRange> replacementRanges,
    required Iterable<_ProjectionSpan> projectionSpans,
    required Iterable<SovereignProjectionAmbiguityZone> ambiguityZones,
  }) : hiddenRanges = List<SovereignHiddenRange>.unmodifiable(hiddenRanges),
       replacementRanges = List<SovereignReplacementRange>.unmodifiable(
         replacementRanges,
       ),
       _projectionSpans = List<_ProjectionSpan>.unmodifiable(projectionSpans),
       ambiguityZones = List<SovereignProjectionAmbiguityZone>.unmodifiable(
         ambiguityZones,
       ),
       _spanSourceDeltaPrefix = List<int>.unmodifiable(
         _buildSpanSourceDeltaPrefix(projectionSpans),
       ),
       _spanDisplayStarts = List<int>.unmodifiable(
         _buildSpanDisplayStarts(projectionSpans),
       ),
       cursorMask = SovereignCursorMask._(
         textLength: textLength,
         hiddenRanges: hiddenRanges,
         replacementRanges: replacementRanges,
         projectionSpans: projectionSpans,
       );

  factory SovereignProjection.fromParseResult(
    SovereignMarkdownParseResult parseResult,
  ) {
    return SovereignProjection(
      textLength: parseResult.sourceTextLength,
      hiddenRanges: parseResult.hiddenRanges.map(
        (hiddenRange) => SovereignHiddenRange(
          range: hiddenRange.sourceRange,
          kind: _projectionHiddenRangeKind(hiddenRange.kind),
        ),
      ),
      replacementRanges: parseResult.replacementRanges.map(
        (replacementRange) => SovereignReplacementRange(
          range: replacementRange.sourceRange,
          kind: _projectionReplacementRangeKind(replacementRange.kind),
          replacementText: replacementRange.replacementText,
        ),
      ),
      ambiguityZones: parseResult.ambiguityZones.map(
        (zone) => SovereignProjectionAmbiguityZone(
          range: zone.sourceRange,
          kind: _projectionAmbiguityKind(zone.kind),
          preferredAffinity: zone.preferredAffinity,
        ),
      ),
    );
  }

  final int textLength;
  final List<SovereignHiddenRange> hiddenRanges;
  final List<SovereignReplacementRange> replacementRanges;
  final List<SovereignProjectionAmbiguityZone> ambiguityZones;
  final List<_ProjectionSpan> _projectionSpans;
  final List<int> _spanSourceDeltaPrefix;
  final List<int> _spanDisplayStarts;
  final SovereignCursorMask cursorMask;

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
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
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
        final sourceOffset = affinity == SovereignMapAffinity.upstream
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
        SovereignMapAffinity.upstream => span.range.start,
        SovereignMapAffinity.downstream => span.range.end,
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

  SovereignSelection sourceSelectionToDisplay(
    SovereignSelection sourceSelection, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    return SovereignSelection(
      baseOffset: sourceToDisplayOffset(
        cursorMask.normalize(sourceSelection.baseOffset, affinity: affinity),
      ),
      extentOffset: sourceToDisplayOffset(
        cursorMask.normalize(sourceSelection.extentOffset, affinity: affinity),
      ),
    );
  }

  SovereignSelection displaySelectionToSource(
    SovereignSelection displaySelection, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    return SovereignSelection(
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

  SovereignProjectionPrediction predictAfter(
    SovereignTransaction transaction, {
    required int textLengthAfter,
  }) {
    final sensitiveRanges = [
      for (final hiddenRange in hiddenRanges) hiddenRange.range,
      for (final replacementRange in replacementRanges) replacementRange.range,
      for (final zone in ambiguityZones) zone.range,
    ];
    final touchedSensitiveRange = transaction.operations.any((operation) {
      return sensitiveRanges.any(
        (range) => _operationTouchesRange(operation.replacedRange, range),
      );
    });
    final invalidatedRange = _transactionInvalidatedRange(transaction);

    return SovereignProjectionPrediction(
      projection: SovereignProjection(
        textLength: textLengthAfter,
        hiddenRanges: hiddenRanges
            .map((range) => _mapHiddenRange(transaction, range))
            .where((range) => !range.range.isCollapsed),
        replacementRanges: replacementRanges
            .map((range) => _mapReplacementRange(transaction, range))
            .where((range) => !range.range.isCollapsed),
        ambiguityZones: ambiguityZones
            .map((zone) => _mapAmbiguityZone(transaction, zone))
            .where((zone) => !zone.range.isCollapsed),
      ),
      touchedProjectionSensitiveRange: touchedSensitiveRange,
      invalidatedRange: invalidatedRange,
    );
  }

  SovereignProjectionReconciliation reconcileWith(
    SovereignProjection authoritative,
  ) {
    return SovereignProjectionReconciliation(
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

SovereignHiddenRangeKind _projectionHiddenRangeKind(
  SovereignMarkdownHiddenRangeKind kind,
) {
  return switch (kind) {
    SovereignMarkdownHiddenRangeKind.markdownMarker =>
      SovereignHiddenRangeKind.markdownMarker,
    SovereignMarkdownHiddenRangeKind.blockMarker =>
      SovereignHiddenRangeKind.blockMarker,
    SovereignMarkdownHiddenRangeKind.inlineMarker =>
      SovereignHiddenRangeKind.inlineMarker,
    SovereignMarkdownHiddenRangeKind.escapeMarker =>
      SovereignHiddenRangeKind.escapeMarker,
    SovereignMarkdownHiddenRangeKind.linkDestination =>
      SovereignHiddenRangeKind.linkDestination,
    SovereignMarkdownHiddenRangeKind.linkTitle =>
      SovereignHiddenRangeKind.linkTitle,
    SovereignMarkdownHiddenRangeKind.referenceDefinition =>
      SovereignHiddenRangeKind.referenceDefinition,
    SovereignMarkdownHiddenRangeKind.rawHtml =>
      SovereignHiddenRangeKind.rawHtml,
    SovereignMarkdownHiddenRangeKind.unknown =>
      SovereignHiddenRangeKind.unknown,
  };
}

SovereignReplacementRangeKind _projectionReplacementRangeKind(
  SovereignMarkdownReplacementRangeKind kind,
) {
  return switch (kind) {
    SovereignMarkdownReplacementRangeKind.htmlEntity =>
      SovereignReplacementRangeKind.htmlEntity,
    SovereignMarkdownReplacementRangeKind.unknown =>
      SovereignReplacementRangeKind.unknown,
  };
}

SovereignProjectionAmbiguityKind _projectionAmbiguityKind(
  SovereignMarkdownAmbiguityKind kind,
) {
  return switch (kind) {
    SovereignMarkdownAmbiguityKind.delimiterRun =>
      SovereignProjectionAmbiguityKind.delimiterRun,
    SovereignMarkdownAmbiguityKind.linkReference =>
      SovereignProjectionAmbiguityKind.linkReference,
    SovereignMarkdownAmbiguityKind.tableBoundary =>
      SovereignProjectionAmbiguityKind.tableBoundary,
    SovereignMarkdownAmbiguityKind.rawHtml =>
      SovereignProjectionAmbiguityKind.rawHtml,
    SovereignMarkdownAmbiguityKind.unknown =>
      SovereignProjectionAmbiguityKind.unknown,
  };
}

List<SovereignHiddenRange> _sortedHiddenRanges(
  Iterable<SovereignHiddenRange> hiddenRanges,
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

void _validateRangeShape(SovereignSourceRange range) {
  if (range.start < 0) {
    throw RangeError.range(range.start, 0, null, 'start');
  }
  if (range.end < range.start) {
    throw RangeError.range(range.end, range.start, null, 'end');
  }
}

List<SovereignHiddenRange> _validatedHiddenRanges(
  int textLength,
  Iterable<SovereignHiddenRange> hiddenRanges,
) {
  final sorted = _sortedHiddenRanges(hiddenRanges);
  for (final hiddenRange in sorted) {
    hiddenRange.range.validate(textLength);
  }
  return sorted;
}

List<SovereignReplacementRange> _sortedReplacementRanges(
  Iterable<SovereignReplacementRange> replacementRanges,
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

List<SovereignReplacementRange> _validatedReplacementRanges(
  int textLength,
  Iterable<SovereignReplacementRange> replacementRanges,
) {
  final sorted = _sortedReplacementRanges(replacementRanges);
  for (final replacementRange in sorted) {
    replacementRange.range.validate(textLength);
  }
  return sorted;
}

List<_ProjectionSpan> _validatedProjectionSpans({
  required Iterable<SovereignHiddenRange> hiddenRanges,
  required Iterable<SovereignReplacementRange> replacementRanges,
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
  required Iterable<SovereignHiddenRange> hiddenRanges,
  required Iterable<SovereignReplacementRange> replacementRanges,
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

List<SovereignProjectionAmbiguityZone> _validatedAmbiguityZones(
  int textLength,
  Iterable<SovereignProjectionAmbiguityZone> ambiguityZones,
) {
  final zones = [...ambiguityZones];
  for (final zone in zones) {
    zone.range.validate(textLength);
  }
  return zones;
}

SovereignHiddenRange _mapHiddenRange(
  SovereignTransaction transaction,
  SovereignHiddenRange hiddenRange,
) {
  return SovereignHiddenRange(
    range: _mapRange(transaction, hiddenRange.range),
    kind: hiddenRange.kind,
  );
}

SovereignReplacementRange _mapReplacementRange(
  SovereignTransaction transaction,
  SovereignReplacementRange replacementRange,
) {
  return SovereignReplacementRange(
    range: _mapRange(transaction, replacementRange.range),
    kind: replacementRange.kind,
    replacementText: replacementRange.replacementText,
  );
}

SovereignProjectionAmbiguityZone _mapAmbiguityZone(
  SovereignTransaction transaction,
  SovereignProjectionAmbiguityZone zone,
) {
  return SovereignProjectionAmbiguityZone(
    range: _mapRange(transaction, zone.range),
    kind: zone.kind,
    preferredAffinity: zone.preferredAffinity,
  );
}

SovereignSourceRange _mapRange(
  SovereignTransaction transaction,
  SovereignSourceRange range,
) {
  final mappedStart = transaction.mapOffset(
    range.start,
    affinity: SovereignMapAffinity.downstream,
  );
  final mappedEnd = transaction.mapOffset(
    range.end,
    affinity: SovereignMapAffinity.upstream,
  );
  if (mappedStart > mappedEnd) {
    return SovereignSourceRange(mappedEnd, mappedEnd);
  }
  return SovereignSourceRange(mappedStart, mappedEnd);
}

SovereignSourceRange? _transactionInvalidatedRange(
  SovereignTransaction transaction,
) {
  final metadataRange = transaction.metadata.projectionInvalidationRange;
  if (metadataRange != null) return metadataRange;
  SovereignSourceRange? invalidated;
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
  SovereignSourceRange operationRange,
  SovereignSourceRange sensitiveRange,
) {
  if (operationRange.isCollapsed) {
    return operationRange.start > sensitiveRange.start &&
        operationRange.start < sensitiveRange.end;
  }
  return operationRange.intersects(sensitiveRange);
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

  final SovereignSourceRange range;
  final String replacementText;

  bool get isHidden => replacementText.isEmpty;
  int get displayLength => replacementText.length;
  int get sourceDelta => range.length - displayLength;
}
