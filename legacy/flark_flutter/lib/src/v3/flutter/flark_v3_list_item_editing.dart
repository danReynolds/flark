import 'package:flark/flark_adapter.dart';
import 'package:flutter/widgets.dart';

import 'flark_v3_hidden_line_prefix_editing.dart';

enum FlarkV3ListItemMarkerPresentationKind { bullet, parserText }

/// Paint-only marker supplied alongside parser-certified list-item geometry.
///
/// [parserText] is exact parser-authored display data. Flutter never derives it
/// by trimming or interpreting a hidden source prefix.
final class FlarkV3ListItemMarkerPresentation {
  const FlarkV3ListItemMarkerPresentation.bullet()
    : kind = FlarkV3ListItemMarkerPresentationKind.bullet,
      parserText = null,
      minimumGutterWidth = 16;

  factory FlarkV3ListItemMarkerPresentation.parserText({
    required String parserText,
    double minimumGutterWidth = 88,
  }) {
    if (parserText.isEmpty ||
        parserText.length > maximumParserTextUtf16 ||
        parserText.contains('\n') ||
        parserText.contains('\r')) {
      throw ArgumentError.value(
        parserText,
        'parserText',
        'must be one bounded parser-authored marker label',
      );
    }
    if (!minimumGutterWidth.isFinite || minimumGutterWidth < 0) {
      throw ArgumentError.value(
        minimumGutterWidth,
        'minimumGutterWidth',
        'must be finite and non-negative',
      );
    }
    return FlarkV3ListItemMarkerPresentation._(
      kind: FlarkV3ListItemMarkerPresentationKind.parserText,
      parserText: parserText,
      minimumGutterWidth: minimumGutterWidth,
    );
  }

  const FlarkV3ListItemMarkerPresentation._({
    required this.kind,
    required this.parserText,
    required this.minimumGutterWidth,
  });

  /// Covers an ordered marker with one through nine decimal digits plus its
  /// parser-authored delimiter without making Flutter recognize that syntax.
  static const int maximumParserTextUtf16 = 10;

  final FlarkV3ListItemMarkerPresentationKind kind;
  final String? parserText;
  final double minimumGutterWidth;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ListItemMarkerPresentation &&
      other.kind == kind &&
      other.parserText == parserText &&
      other.minimumGutterWidth == minimumGutterWidth;

  @override
  int get hashCode => Object.hash(kind, parserText, minimumGutterWidth);
}

/// Parser-provided mechanics for one certified tight list item.
///
/// The exact source prefixes are data, not syntax recognized by Flutter.
/// [emptyEnterExits] and [backspaceAtStartRemovesPrefix] are explicit because a
/// parser may withhold either local operation for a structurally complex item.
final class FlarkV3TightListItemConfiguration {
  factory FlarkV3TightListItemConfiguration({
    required String activeHiddenSourcePrefix,
    required String activeRemovableSourcePrefix,
    required int activeRemovableSourcePrefixOffsetUtf16,
    required String continuationSourcePrefix,
    required String canonicalLineEnding,
    required bool emptyEnterExits,
    required bool backspaceAtStartRemovesPrefix,
    FlarkV3ListItemMarkerPresentation markerPresentation =
        const FlarkV3ListItemMarkerPresentation.bullet(),
  }) {
    _validatePrefix(activeHiddenSourcePrefix, 'activeHiddenSourcePrefix');
    _validatePrefix(activeRemovableSourcePrefix, 'activeRemovableSourcePrefix');
    _validatePrefix(continuationSourcePrefix, 'continuationSourcePrefix');
    if (activeRemovableSourcePrefixOffsetUtf16 < 0 ||
        activeRemovableSourcePrefixOffsetUtf16 +
                activeRemovableSourcePrefix.length >
            activeHiddenSourcePrefix.length ||
        activeHiddenSourcePrefix.substring(
              activeRemovableSourcePrefixOffsetUtf16,
              activeRemovableSourcePrefixOffsetUtf16 +
                  activeRemovableSourcePrefix.length,
            ) !=
            activeRemovableSourcePrefix) {
      throw ArgumentError.value(
        activeRemovableSourcePrefixOffsetUtf16,
        'activeRemovableSourcePrefixOffsetUtf16',
        'must select the exact removable prefix inside '
            'activeHiddenSourcePrefix',
      );
    }
    _validateLineEnding(canonicalLineEnding);
    return FlarkV3TightListItemConfiguration._(
      activeHiddenSourcePrefix: activeHiddenSourcePrefix,
      activeRemovableSourcePrefix: activeRemovableSourcePrefix,
      activeRemovableSourcePrefixOffsetUtf16:
          activeRemovableSourcePrefixOffsetUtf16,
      continuationSourcePrefix: continuationSourcePrefix,
      canonicalLineEnding: canonicalLineEnding,
      emptyEnterExits: emptyEnterExits,
      backspaceAtStartRemovesPrefix: backspaceAtStartRemovesPrefix,
      markerPresentation: markerPresentation,
    );
  }

  const FlarkV3TightListItemConfiguration._({
    required this.activeHiddenSourcePrefix,
    required this.activeRemovableSourcePrefix,
    required this.activeRemovableSourcePrefixOffsetUtf16,
    required this.continuationSourcePrefix,
    required this.canonicalLineEnding,
    required this.emptyEnterExits,
    required this.backspaceAtStartRemovesPrefix,
    required this.markerPresentation,
  });

  static const int maximumSourcePrefixUtf16 =
      FlarkV3HiddenLinePrefixEditPolicy.maximumCanonicalContinuationPrefixUtf16;

  /// Exact parser-authored hidden prefix for the currently certified item.
  ///
  /// This can include protected source such as a document-start BOM because it
  /// describes complete projection coverage.
  final String activeHiddenSourcePrefix;

  /// Exact subrange of [activeHiddenSourcePrefix] that local item commands may
  /// remove.
  ///
  /// Keeping this separate is what lets Backspace and empty Enter preserve a
  /// protected BOM without Flutter recognizing one.
  final String activeRemovableSourcePrefix;

  /// UTF-16 offset of [activeRemovableSourcePrefix] inside the complete hidden
  /// prefix.
  ///
  /// The removable parser-authored cut may have protected source on either
  /// side: a BOF BOM before it, terminal author whitespace after it, or both.
  final int activeRemovableSourcePrefixOffsetUtf16;

  /// Exact hidden prefix to insert for a user-created sibling item.
  ///
  /// This can intentionally differ from [activeRemovableSourcePrefix]. Flutter
  /// does not inspect either value to decide which Markdown marker it
  /// represents.
  final String continuationSourcePrefix;

  /// Exact canonical source line ending. Display space always uses `\n`.
  final String canonicalLineEnding;

  /// Whether Enter on an empty item may replace its removable prefix with a
  /// line ending, leaving a plain line outside the list.
  final bool emptyEnterExits;

  /// Whether Backspace at display-column zero may remove the exact hidden item
  /// prefix without deleting visible text.
  final bool backspaceAtStartRemovesPrefix;

  /// Exact paint instruction kept separate from edit-prefix authority.
  final FlarkV3ListItemMarkerPresentation markerPresentation;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3TightListItemConfiguration &&
      other.activeHiddenSourcePrefix == activeHiddenSourcePrefix &&
      other.activeRemovableSourcePrefix == activeRemovableSourcePrefix &&
      other.activeRemovableSourcePrefixOffsetUtf16 ==
          activeRemovableSourcePrefixOffsetUtf16 &&
      other.continuationSourcePrefix == continuationSourcePrefix &&
      other.canonicalLineEnding == canonicalLineEnding &&
      other.emptyEnterExits == emptyEnterExits &&
      other.backspaceAtStartRemovesPrefix == backspaceAtStartRemovesPrefix &&
      other.markerPresentation == markerPresentation;

  @override
  int get hashCode => Object.hash(
    activeHiddenSourcePrefix,
    activeRemovableSourcePrefix,
    activeRemovableSourcePrefixOffsetUtf16,
    continuationSourcePrefix,
    canonicalLineEnding,
    emptyEnterExits,
    backspaceAtStartRemovesPrefix,
    markerPresentation,
  );
}

/// Source-compatible name for the original bullet-only configuration.
typedef FlarkV3TightBulletListItemConfiguration =
    FlarkV3TightListItemConfiguration;

/// Marker-hidden edit mechanics for one parser-certified tight list item.
///
/// The policy handles only exact prefixes named by [configuration] and exact
/// hidden projection pieces. It does not recognize markers, indentation, or
/// any other Markdown syntax. Because a hidden prefix produces no platform
/// deletion delta, hardware Backspace at display-column zero is represented by
/// an explicit collapsed edit with an empty display replacement.
final class FlarkV3TightListItemEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  FlarkV3TightListItemEditPolicy({required this.configuration})
    : _linePrefixDelegate = FlarkV3HiddenLinePrefixEditPolicy(
        canonicalContinuationPrefix: configuration.continuationSourcePrefix,
        canonicalLineEnding: configuration.canonicalLineEnding,
        projectionLabel: 'Tight list item',
      );

  final FlarkV3TightListItemConfiguration configuration;
  final FlarkV3HiddenLinePrefixEditPolicy _linePrefixDelegate;

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) {
    _validateCoordinateClosure(request);
    final line = _displayLineContaining(
      request.projection.displayText,
      request.displayStartUtf16,
    );
    final prefix = _authorizedPrefixAtDisplayLineStart(
      request.projection,
      line.startUtf16,
    );

    if (prefix == null) {
      final mechanicallyUnlisted = _isMechanicallyUnlistedFirstLine(
        request.projection,
        line.startUtf16,
      );
      if (!mechanicallyUnlisted &&
          _hasUnrecognizedHiddenPhysicalLinePrefix(
            request.projection,
            line.startUtf16,
          )) {
        throw StateError(
          'Tight list-item configuration does not match the parser-hidden '
          'line prefix.',
        );
      }
      // A preceding Backspace may already have removed the marker while exact
      // parser recertification is in flight. In that mechanically unlisted
      // line, ordinary edits must behave as source-backed text rather than
      // manufacturing another list marker.
      if (mechanicallyUnlisted &&
          request.sourceStartUtf16 == request.sourceEndUtf16) {
        // The trusted continuation anchor can sit between protected leading
        // and trailing hidden source after removing an interior marker cut.
        // A plain insertion at that exact source caret is safe even though the
        // generic source-backed policy cannot authorize a hidden boundary.
        return FlarkV3SourceProjectionEditPlan(
          sourceStartUtf16: request.sourceStartUtf16,
          sourceEndUtf16: request.sourceEndUtf16,
          replacement: FlarkV3SourceProjectionReplacement.identity(
            request.displayReplacement,
          ),
        );
      }
      return const FlarkV3SourceBackedProjectionEditPolicy().planEdit(request);
    }

    final isCollapsed = request.displayStartUtf16 == request.displayEndUtf16;
    final isBackspaceCommand =
        configuration.backspaceAtStartRemovesPrefix &&
        isCollapsed &&
        request.displayReplacement.isEmpty &&
        request.displayStartUtf16 == line.startUtf16;
    if (isBackspaceCommand) {
      return FlarkV3SourceProjectionEditPlan(
        sourceStartUtf16: prefix.startUtf16,
        sourceEndUtf16: prefix.endUtf16,
        replacement: FlarkV3SourceProjectionReplacement.identity(''),
      );
    }

    final isEmptyExit =
        configuration.emptyEnterExits &&
        isCollapsed &&
        request.displayReplacement == '\n' &&
        line.startUtf16 == line.endUtf16;
    if (isEmptyExit) {
      return FlarkV3SourceProjectionEditPlan(
        sourceStartUtf16: prefix.startUtf16,
        sourceEndUtf16: prefix.endUtf16,
        replacement: _displayLineEndingReplacement(
          configuration.canonicalLineEnding,
        ),
      );
    }

    return _linePrefixDelegate.planEdit(request);
  }

  bool _isMechanicallyUnlistedFirstLine(
    FlarkV3SourceProjection projection,
    int displayLineStartUtf16,
  ) {
    final physicalStart = _physicalLineStartForDisplayLine(
      projection,
      displayLineStartUtf16,
    );
    if (physicalStart != projection.sourceStartUtf16) return false;
    final removableStart = configuration.activeRemovableSourcePrefixOffsetUtf16;
    final removableEnd =
        removableStart + configuration.activeRemovableSourcePrefix.length;
    final protectedPrefix =
        configuration.activeHiddenSourcePrefix.substring(0, removableStart) +
        configuration.activeHiddenSourcePrefix.substring(removableEnd);
    final protectedEnd = physicalStart! + protectedPrefix.length;
    return protectedPrefix.isEmpty
        ? !_sourceOffsetIsInsideHiddenPiece(projection, physicalStart)
        : (_sourceRangeEquals(
                projection,
                physicalStart,
                protectedEnd,
                protectedPrefix,
              ) &&
              _rangeIsEntirelyHidden(projection, physicalStart, protectedEnd) &&
              !_sourceOffsetIsInsideHiddenPiece(projection, protectedEnd));
  }

  ({int startUtf16, int endUtf16})? _authorizedPrefixAtDisplayLineStart(
    FlarkV3SourceProjection projection,
    int displayLineStartUtf16,
  ) {
    final physicalStart = _physicalLineStartForDisplayLine(
      projection,
      displayLineStartUtf16,
    );
    if (physicalStart == null) return null;
    final firstPhysicalLine = physicalStart == projection.sourceStartUtf16;
    final hiddenPrefix = firstPhysicalLine
        ? configuration.activeHiddenSourcePrefix
        : configuration.continuationSourcePrefix;
    final hiddenEnd = physicalStart + hiddenPrefix.length;
    if (!_sourceRangeEquals(
          projection,
          physicalStart,
          hiddenEnd,
          hiddenPrefix,
        ) ||
        !_rangeIsEntirelyHidden(projection, physicalStart, hiddenEnd)) {
      return null;
    }
    final removableOffset = firstPhysicalLine
        ? configuration.activeRemovableSourcePrefixOffsetUtf16
        : 0;
    final removableLength = firstPhysicalLine
        ? configuration.activeRemovableSourcePrefix.length
        : configuration.continuationSourcePrefix.length;
    return (
      startUtf16: physicalStart + removableOffset,
      endUtf16: physicalStart + removableOffset + removableLength,
    );
  }

  bool _hasUnrecognizedHiddenPhysicalLinePrefix(
    FlarkV3SourceProjection projection,
    int displayLineStartUtf16,
  ) {
    final physicalStart = _physicalLineStartForDisplayLine(
      projection,
      displayLineStartUtf16,
    );
    if (physicalStart == null) return false;
    return _sourceOffsetIsInsideHiddenPiece(projection, physicalStart);
  }

  int? _physicalLineStartForDisplayLine(
    FlarkV3SourceProjection projection,
    int displayLineStartUtf16,
  ) {
    var relativeStart = 0;
    while (true) {
      final absoluteStart = projection.sourceStartUtf16 + relativeStart;
      if (projection.sourceToDisplayOffset(absoluteStart) ==
          displayLineStartUtf16) {
        return absoluteStart;
      }
      final nextLineEnding = projection.sourceText.indexOf(
        configuration.canonicalLineEnding,
        relativeStart,
      );
      if (nextLineEnding < 0) return null;
      relativeStart = nextLineEnding + configuration.canonicalLineEnding.length;
    }
  }
}

/// Source-compatible name for the original bullet-only edit policy.
typedef FlarkV3TightBulletListItemEditPolicy = FlarkV3TightListItemEditPolicy;

/// Decorative gutter for one parser-certified list item.
///
/// [child] remains the sole editable subtree. The marker is paint-only so it
/// neither creates a second text client nor contributes duplicate semantics.
final class FlarkV3ListItemGutter extends StatelessWidget {
  const FlarkV3ListItemGutter({
    super.key,
    required this.configuration,
    required this.child,
    required this.markerColor,
    this.markerDiameter = 4.6,
    this.gutterWidth = 16,
    this.gap = 8,
    this.firstLineHeight = 16.8,
    this.markerTextStyle = const TextStyle(fontSize: 14),
  }) : assert(markerDiameter >= 0),
       assert(gutterWidth >= 0),
       assert(gap >= 0),
       assert(firstLineHeight >= 0);

  /// The same parser-derived item configuration used by the edit policy.
  ///
  /// A null value withholds the marker and its geometry while retaining the
  /// same child position. That stable inactive state prevents an
  /// [EditableText] child from being reparented while parser authority is in
  /// flight. A marker is therefore still impossible without parser-derived
  /// configuration.
  final FlarkV3TightListItemConfiguration? configuration;

  final Widget child;
  final Color markerColor;
  final double markerDiameter;
  final double gutterWidth;
  final double gap;
  final TextStyle markerTextStyle;

  /// Height of the editable's first line, used only to center the marker.
  final double firstLineHeight;

  @override
  Widget build(BuildContext context) {
    final marker = configuration?.markerPresentation;
    final active = marker != null;
    final activeGutter = switch (marker?.kind) {
      FlarkV3ListItemMarkerPresentationKind.bullet => SizedBox(
        key: const Key('flark-v3-bullet-list-item-gutter'),
        width: gutterWidth,
        height: firstLineHeight,
        child: ExcludeSemantics(
          child: KeyedSubtree(
            key: const Key('flark-v3-list-item-marker'),
            child: SizedBox(
              key: const Key('flark-v3-bullet-list-item-marker'),
              width: gutterWidth,
              height: firstLineHeight,
              child: CustomPaint(
                painter: _FlarkV3BulletPainter(
                  color: markerColor,
                  diameter: markerDiameter,
                ),
              ),
            ),
          ),
        ),
      ),
      FlarkV3ListItemMarkerPresentationKind.parserText => ConstrainedBox(
        key: const Key('flark-v3-ordered-list-item-gutter'),
        constraints: BoxConstraints(minWidth: marker!.minimumGutterWidth),
        child: SizedBox(
          height: firstLineHeight,
          child: ExcludeSemantics(
            child: Align(
              alignment: AlignmentDirectional.centerEnd,
              child: KeyedSubtree(
                key: const Key('flark-v3-list-item-marker'),
                child: RichText(
                  key: const Key('flark-v3-ordered-list-item-marker'),
                  maxLines: 1,
                  softWrap: false,
                  textDirection: Directionality.of(context),
                  text: TextSpan(
                    text: marker.parserText!,
                    style: markerTextStyle.copyWith(color: markerColor),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
      null => const SizedBox.shrink(),
    };
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        KeyedSubtree(
          key: active ? const Key('flark-v3-list-item-gutter') : null,
          child: activeGutter,
        ),
        SizedBox(width: active ? gap : 0),
        Expanded(child: child),
      ],
    );
  }
}

/// Source-compatible name for the original bullet-only gutter.
typedef FlarkV3BulletListItemGutter = FlarkV3ListItemGutter;

final class _FlarkV3BulletPainter extends CustomPainter {
  const _FlarkV3BulletPainter({required this.color, required this.diameter});

  final Color color;
  final double diameter;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawCircle(
      Offset(size.width / 2, size.height / 2),
      diameter / 2,
      Paint()..color = color,
    );
  }

  @override
  bool shouldRepaint(_FlarkV3BulletPainter oldDelegate) =>
      oldDelegate.color != color || oldDelegate.diameter != diameter;
}

({int startUtf16, int endUtf16}) _displayLineContaining(
  String display,
  int offsetUtf16,
) {
  if (offsetUtf16 < 0 || offsetUtf16 > display.length) {
    throw RangeError.range(offsetUtf16, 0, display.length, 'offsetUtf16');
  }
  final previousLineEnding = offsetUtf16 == 0
      ? -1
      : display.lastIndexOf('\n', offsetUtf16 - 1);
  final nextLineEnding = display.indexOf('\n', offsetUtf16);
  return (
    startUtf16: previousLineEnding + 1,
    endUtf16: nextLineEnding < 0 ? display.length : nextLineEnding,
  );
}

bool _sourceRangeEquals(
  FlarkV3SourceProjection projection,
  int sourceStartUtf16,
  int sourceEndUtf16,
  String expected,
) {
  if (sourceStartUtf16 < projection.sourceStartUtf16 ||
      sourceEndUtf16 < sourceStartUtf16 ||
      sourceEndUtf16 > projection.sourceEndUtf16) {
    return false;
  }
  return projection.sourceText.substring(
        sourceStartUtf16 - projection.sourceStartUtf16,
        sourceEndUtf16 - projection.sourceStartUtf16,
      ) ==
      expected;
}

bool _rangeIsEntirelyHidden(
  FlarkV3SourceProjection projection,
  int sourceStartUtf16,
  int sourceEndUtf16,
) {
  var cursor = sourceStartUtf16;
  for (final piece in projection.pieces) {
    if (!piece.isHidden || piece.sourceEndUtf16 <= cursor) continue;
    if (piece.sourceStartUtf16 > cursor) return false;
    cursor = piece.sourceEndUtf16.clamp(cursor, sourceEndUtf16);
    if (cursor == sourceEndUtf16) return true;
  }
  return cursor == sourceEndUtf16;
}

bool _sourceOffsetIsInsideHiddenPiece(
  FlarkV3SourceProjection projection,
  int sourceOffsetUtf16,
) => projection.pieces.any(
  (piece) =>
      piece.isHidden &&
      piece.sourceStartUtf16 <= sourceOffsetUtf16 &&
      sourceOffsetUtf16 < piece.sourceEndUtf16,
);

FlarkV3SourceProjectionReplacement _displayLineEndingReplacement(
  String canonicalLineEnding,
) => FlarkV3SourceProjectionReplacement.projected(
  sourceReplacement: canonicalLineEnding,
  pieces: [
    if (canonicalLineEnding == '\n')
      const FlarkV3SourceProjectionPiece.copy(
        sourceStartUtf16: 0,
        sourceEndUtf16: 1,
      )
    else
      FlarkV3SourceProjectionPiece.replace(
        sourceStartUtf16: 0,
        sourceEndUtf16: canonicalLineEnding.length,
        displayText: '\n',
      ),
  ],
);

void _validateCoordinateClosure(FlarkV3SourceProjectionEditRequest request) {
  final projection = request.projection;
  if (request.sourceStartUtf16 < projection.sourceStartUtf16 ||
      request.sourceEndUtf16 < request.sourceStartUtf16 ||
      request.sourceEndUtf16 > projection.sourceEndUtf16 ||
      request.displayStartUtf16 < 0 ||
      request.displayEndUtf16 < request.displayStartUtf16 ||
      request.displayEndUtf16 > projection.displayLengthUtf16) {
    throw RangeError('Tight list-item edit escapes its source projection.');
  }
  if (projection.sourceToDisplayOffset(request.sourceStartUtf16) !=
          request.displayStartUtf16 ||
      projection.sourceToDisplayOffset(request.sourceEndUtf16) !=
          request.displayEndUtf16) {
    throw StateError(
      'Tight list-item edit does not close over exact projection boundaries.',
    );
  }
}

void _validatePrefix(String prefix, String parameterName) {
  if (prefix.isEmpty ||
      prefix.length >
          FlarkV3TightListItemConfiguration.maximumSourcePrefixUtf16 ||
      prefix.contains('\n') ||
      prefix.contains('\r')) {
    throw ArgumentError.value(
      prefix,
      parameterName,
      'must be one bounded, non-empty source-line prefix',
    );
  }
}

void _validateLineEnding(String lineEnding) {
  if (lineEnding != '\n' && lineEnding != '\r' && lineEnding != '\r\n') {
    throw ArgumentError.value(
      lineEnding,
      'canonicalLineEnding',
      r"must be '\n', '\r', or '\r\n'",
    );
  }
}
