import 'package:flark/flark_adapter.dart';

/// Grammar-neutral mechanics for projections that hide one canonical prefix
/// on every physical source line.
///
/// Parser-specific policies select this only after structural certification.
/// The class validates projection geometry and translates display newlines and
/// cross-line deletions; it never examines source text for Markdown syntax.
final class FlarkV3HiddenLinePrefixEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  factory FlarkV3HiddenLinePrefixEditPolicy({
    required String canonicalContinuationPrefix,
    required String canonicalLineEnding,
    required String projectionLabel,
  }) {
    if (canonicalContinuationPrefix.isEmpty ||
        canonicalContinuationPrefix.length >
            maximumCanonicalContinuationPrefixUtf16 ||
        canonicalContinuationPrefix.contains('\n') ||
        canonicalContinuationPrefix.contains('\r')) {
      throw ArgumentError.value(
        canonicalContinuationPrefix,
        'canonicalContinuationPrefix',
        'must be one bounded, non-empty source-line prefix',
      );
    }
    if (canonicalLineEnding != '\n' &&
        canonicalLineEnding != '\r' &&
        canonicalLineEnding != '\r\n') {
      throw ArgumentError.value(
        canonicalLineEnding,
        'canonicalLineEnding',
        r"must be '\n', '\r', or '\r\n'",
      );
    }
    return FlarkV3HiddenLinePrefixEditPolicy._(
      canonicalContinuationPrefix: canonicalContinuationPrefix,
      canonicalLineEnding: canonicalLineEnding,
      projectionLabel: projectionLabel,
    );
  }

  const FlarkV3HiddenLinePrefixEditPolicy._({
    required this.canonicalContinuationPrefix,
    required this.canonicalLineEnding,
    required this.projectionLabel,
  });

  /// Keeps editor-command configuration bounded independently of document size.
  static const int maximumCanonicalContinuationPrefixUtf16 = 64;

  /// Exact source prefix inserted after a user-created display newline.
  ///
  /// The parser supplies/selects this configuration; the policy does not infer
  /// it by examining source text.
  final String canonicalContinuationPrefix;

  /// Exact source line ending inserted for a displayed `\n`.
  final String canonicalLineEnding;

  final String projectionLabel;

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) {
    _validateCoordinateClosure(request);

    final startsInsideHidden = request.projection.isStrictlyInsideHiddenPiece(
      request.sourceStartUtf16,
    );
    final endsInsideHidden = request.projection.isStrictlyInsideHiddenPiece(
      request.sourceEndUtf16,
    );
    if (startsInsideHidden || endsInsideHidden) {
      throw StateError(
        '$projectionLabel edit terminates inside parser-hidden source.',
      );
    }

    if (request.displayReplacement == '\n') {
      if (request.intersectsHiddenSource ||
          _displayRangeContainsLineEnding(request)) {
        throw StateError(
          '$projectionLabel newline replacement crosses hidden or cross-line '
          'source.',
        );
      }
      return FlarkV3SourceProjectionEditPlan(
        sourceStartUtf16: request.sourceStartUtf16,
        sourceEndUtf16: request.sourceEndUtf16,
        replacement: _continuationReplacement(),
      );
    }

    if (request.displayReplacement.isEmpty &&
        request.displayStartUtf16 < request.displayEndUtf16) {
      final deletion = _planCrossLineDeletion(request);
      if (deletion != null) return deletion;
    }

    if (request.intersectsHiddenSource) {
      throw StateError(
        '$projectionLabel edit crosses parser-hidden source without an exact '
        'cross-line deletion.',
      );
    }
    return FlarkV3SourceProjectionEditPlan.identity(request);
  }

  FlarkV3SourceProjectionReplacement _continuationReplacement() {
    final lineEndingLength = canonicalLineEnding.length;
    final sourceReplacement =
        '$canonicalLineEnding$canonicalContinuationPrefix';
    return FlarkV3SourceProjectionReplacement.projected(
      sourceReplacement: sourceReplacement,
      pieces: [
        if (canonicalLineEnding == '\n')
          const FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 0,
            sourceEndUtf16: 1,
          )
        else
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 0,
            sourceEndUtf16: lineEndingLength,
            displayText: '\n',
          ),
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: lineEndingLength,
          sourceEndUtf16: sourceReplacement.length,
        ),
      ],
    );
  }

  FlarkV3SourceProjectionEditPlan? _planCrossLineDeletion(
    FlarkV3SourceProjectionEditRequest request,
  ) {
    if (!_displayRangeContainsLineEnding(request)) return null;

    final projection = request.projection;
    var sourceEnd = request.sourceEndUtf16;

    // A backspace/forward-delete of the visible line ending resolves to the
    // upstream side of the collapsed next-line prefix. Extend through that
    // exact parser-authored hidden piece so the canonical lines join too.
    var expanded = true;
    while (expanded) {
      expanded = false;
      for (final piece in projection.pieces) {
        if (!piece.isHidden || piece.sourceStartUtf16 != sourceEnd) continue;
        final displayOffset = projection.sourceToDisplayOffset(
          piece.sourceStartUtf16,
        );
        if (displayOffset == request.displayEndUtf16 &&
            _isImmediatelyAfterDisplayLineEnding(
              projection.displayText,
              displayOffset,
            )) {
          sourceEnd = piece.sourceEndUtf16;
          expanded = true;
          break;
        }
      }
    }

    final hiddenPieces = projection.pieces.where(
      (piece) =>
          piece.isHidden &&
          piece.sourceStartUtf16 < sourceEnd &&
          piece.sourceEndUtf16 > request.sourceStartUtf16,
    );
    var consumedHiddenPrefix = false;
    for (final piece in hiddenPieces) {
      if (piece.sourceStartUtf16 < request.sourceStartUtf16 ||
          piece.sourceEndUtf16 > sourceEnd) {
        throw StateError(
          'Cross-line deletion only permits complete hidden prefixes.',
        );
      }
      final collapsedStart = projection.sourceToDisplayOffset(
        piece.sourceStartUtf16,
      );
      final collapsedEnd = projection.sourceToDisplayOffset(
        piece.sourceEndUtf16,
      );
      if (collapsedStart != collapsedEnd ||
          collapsedStart <= request.displayStartUtf16 ||
          collapsedStart > request.displayEndUtf16 ||
          !_isImmediatelyAfterDisplayLineEnding(
            projection.displayText,
            collapsedStart,
          )) {
        throw StateError(
          'Cross-line deletion encountered hidden source that is not an '
          'exact next-line prefix.',
        );
      }
      consumedHiddenPrefix = true;
    }
    if (!consumedHiddenPrefix) return null;

    return FlarkV3SourceProjectionEditPlan(
      sourceStartUtf16: request.sourceStartUtf16,
      sourceEndUtf16: sourceEnd,
      replacement: FlarkV3SourceProjectionReplacement.identity(''),
    );
  }

  void _validateCoordinateClosure(FlarkV3SourceProjectionEditRequest request) {
    final projection = request.projection;
    if (request.sourceStartUtf16 < projection.sourceStartUtf16 ||
        request.sourceEndUtf16 < request.sourceStartUtf16 ||
        request.sourceEndUtf16 > projection.sourceEndUtf16 ||
        request.displayStartUtf16 < 0 ||
        request.displayEndUtf16 < request.displayStartUtf16 ||
        request.displayEndUtf16 > projection.displayLengthUtf16) {
      throw RangeError('$projectionLabel edit escapes its source projection.');
    }
    if (projection.sourceToDisplayOffset(request.sourceStartUtf16) !=
            request.displayStartUtf16 ||
        projection.sourceToDisplayOffset(request.sourceEndUtf16) !=
            request.displayEndUtf16) {
      throw StateError(
        '$projectionLabel edit does not close over exact projection '
        'boundaries.',
      );
    }
  }
}

bool _displayRangeContainsLineEnding(
  FlarkV3SourceProjectionEditRequest request,
) {
  final display = request.projection.displayText;
  for (
    var offset = request.displayStartUtf16;
    offset < request.displayEndUtf16;
    offset += 1
  ) {
    final codeUnit = display.codeUnitAt(offset);
    if (codeUnit == 0x0A || codeUnit == 0x0D) return true;
  }
  return false;
}

bool _isImmediatelyAfterDisplayLineEnding(String display, int offsetUtf16) {
  if (offsetUtf16 <= 0 || offsetUtf16 > display.length) return false;
  final previous = display.codeUnitAt(offsetUtf16 - 1);
  return previous == 0x0A || previous == 0x0D;
}
