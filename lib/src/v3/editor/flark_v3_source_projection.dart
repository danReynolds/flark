import 'dart:typed_data';

import '../host/flark_v3_host_protocol.dart';

/// How one parser-authored source range contributes to a live projection.
enum FlarkV3SourceProjectionPieceKind { copy, hide, replace }

/// Which exact source caret wins when hidden source shares one display caret.
enum FlarkV3SourceProjectionAffinity { upstream, downstream }

/// One exhaustive, source-backed piece of a bounded live projection.
///
/// Pieces are absolute document UTF-16 ranges. A complete projection requires
/// ordered, non-empty pieces that cover its source range without gaps or
/// overlaps. [copy] contributes the exact source substring, [hide] contributes
/// no display text, and [replace] contributes explicit parser-authored display
/// text without interpreting the source.
final class FlarkV3SourceProjectionPiece {
  const FlarkV3SourceProjectionPiece.copy({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
  }) : kind = FlarkV3SourceProjectionPieceKind.copy,
       displayText = null;

  const FlarkV3SourceProjectionPiece.hide({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
  }) : kind = FlarkV3SourceProjectionPieceKind.hide,
       displayText = null;

  /// Replaces one exact source span with explicit semantic display text.
  ///
  /// This is a mechanical projection primitive, not a Markdown recognizer.
  /// Parser adapters can use it for source/display normalization such as
  /// NUL -> U+FFFD or CR/CRLF -> LF while retaining exact source coordinates.
  const FlarkV3SourceProjectionPiece.replace({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required String this.displayText,
  }) : kind = FlarkV3SourceProjectionPieceKind.replace;

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final FlarkV3SourceProjectionPieceKind kind;
  final String? displayText;

  int get sourceLengthUtf16 => sourceEndUtf16 - sourceStartUtf16;
  int get displayLengthUtf16 => switch (kind) {
    FlarkV3SourceProjectionPieceKind.copy => sourceLengthUtf16,
    FlarkV3SourceProjectionPieceKind.hide => 0,
    FlarkV3SourceProjectionPieceKind.replace => displayText!.length,
  };
  bool get isCopied => kind == FlarkV3SourceProjectionPieceKind.copy;
  bool get isHidden => kind == FlarkV3SourceProjectionPieceKind.hide;
  bool get isReplaced => kind == FlarkV3SourceProjectionPieceKind.replace;

  FlarkV3SourceProjectionPiece slice(int startUtf16, int endUtf16) {
    if (kind == FlarkV3SourceProjectionPieceKind.replace &&
        (startUtf16 != sourceStartUtf16 || endUtf16 != sourceEndUtf16)) {
      throw StateError(
        'A parser-authored replacement piece cannot be split mechanically.',
      );
    }
    return switch (kind) {
      FlarkV3SourceProjectionPieceKind.copy =>
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: startUtf16,
          sourceEndUtf16: endUtf16,
        ),
      FlarkV3SourceProjectionPieceKind.hide =>
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: startUtf16,
          sourceEndUtf16: endUtf16,
        ),
      FlarkV3SourceProjectionPieceKind.replace =>
        FlarkV3SourceProjectionPiece.replace(
          sourceStartUtf16: startUtf16,
          sourceEndUtf16: endUtf16,
          displayText: displayText!,
        ),
    };
  }

  FlarkV3SourceProjectionPiece shift(int deltaUtf16) => switch (kind) {
    FlarkV3SourceProjectionPieceKind.copy => FlarkV3SourceProjectionPiece.copy(
      sourceStartUtf16: sourceStartUtf16 + deltaUtf16,
      sourceEndUtf16: sourceEndUtf16 + deltaUtf16,
    ),
    FlarkV3SourceProjectionPieceKind.hide => FlarkV3SourceProjectionPiece.hide(
      sourceStartUtf16: sourceStartUtf16 + deltaUtf16,
      sourceEndUtf16: sourceEndUtf16 + deltaUtf16,
    ),
    FlarkV3SourceProjectionPieceKind.replace =>
      FlarkV3SourceProjectionPiece.replace(
        sourceStartUtf16: sourceStartUtf16 + deltaUtf16,
        sourceEndUtf16: sourceEndUtf16 + deltaUtf16,
        displayText: displayText!,
      ),
  };

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceProjectionPiece &&
      other.sourceStartUtf16 == sourceStartUtf16 &&
      other.sourceEndUtf16 == sourceEndUtf16 &&
      other.kind == kind &&
      other.displayText == displayText;

  @override
  int get hashCode =>
      Object.hash(sourceStartUtf16, sourceEndUtf16, kind, displayText);
}

/// A bounded canonical replacement and its exact display projection.
///
/// The replacement projection is relative to source offset zero. This makes
/// hidden parser-authored insertion text explicit: for example, a displayed
/// newline may have canonical source `\n    ` with one copied newline piece
/// followed by one hidden indentation piece.
final class FlarkV3SourceProjectionReplacement {
  factory FlarkV3SourceProjectionReplacement.identity(String replacement) =>
      FlarkV3SourceProjectionReplacement.projected(
        sourceReplacement: replacement,
        pieces: replacement.isEmpty
            ? const <FlarkV3SourceProjectionPiece>[]
            : [
                FlarkV3SourceProjectionPiece.copy(
                  sourceStartUtf16: 0,
                  sourceEndUtf16: replacement.length,
                ),
              ],
      );

  factory FlarkV3SourceProjectionReplacement.projected({
    required String sourceReplacement,
    required List<FlarkV3SourceProjectionPiece> pieces,
  }) => FlarkV3SourceProjectionReplacement._(
    FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: 0,
      sourceText: sourceReplacement,
      pieces: pieces,
    ),
  );

  const FlarkV3SourceProjectionReplacement._(this.projection);

  final FlarkV3SourceProjection projection;

  String get sourceReplacement => projection.sourceText;
  String get displayReplacement => projection.displayText;
  List<FlarkV3SourceProjectionPiece> get pieces => projection.pieces;
}

/// One display edit widened over complete parser-authored replacement pieces.
///
/// A replacement piece is one indivisible source token even when its cooked
/// display text contains more than one Unicode scalar. Editing only part of
/// that display therefore consumes the complete source piece and preserves any
/// untouched cooked prefix or suffix as literal replacement text.
final class FlarkV3SourceProjectionDisplayEdit {
  const FlarkV3SourceProjectionDisplayEdit({
    required this.displayStartUtf16,
    required this.displayEndUtf16,
    required this.replacement,
  });

  final int displayStartUtf16;
  final int displayEndUtf16;
  final String replacement;
}

/// Parser-neutral edit input for one bounded source projection.
///
/// This request contains coordinates and parser-authored projection pieces
/// only. A policy may transform display input into canonical source input, but
/// it must not recognize Markdown from source text.
final class FlarkV3SourceProjectionEditRequest {
  const FlarkV3SourceProjectionEditRequest({
    required this.projection,
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.displayStartUtf16,
    required this.displayEndUtf16,
    required this.displayReplacement,
    this.preauthorizedHiddenDeletion = false,
    this.preauthorizedHiddenInsertion = false,
    this.preauthorizedHiddenReplacement = false,
  });

  final FlarkV3SourceProjection projection;
  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final int displayStartUtf16;
  final int displayEndUtf16;
  final String displayReplacement;

  /// Whether a typed higher layer already authorized deletion of hidden
  /// source, such as complete parser-certified inline delimiter pairs.
  final bool preauthorizedHiddenDeletion;

  /// Whether a typed higher layer authorized inserting at a certified boundary
  /// that lies inside a merged hidden-source chain.
  ///
  /// This permits, for example, insertion immediately before a parser-certified
  /// escaped-punctuation atom nested directly after a hidden strong opener. It
  /// does not authorize replacing or deleting hidden source.
  final bool preauthorizedHiddenInsertion;

  /// Whether a typed higher layer already authorized replacing hidden source.
  ///
  /// This is deliberately distinct from deletion. It is used for
  /// parser-certified atomic projections whose visible content replaces the
  /// complete hidden-plus-visible source atom. The policy receives no
  /// Markdown text classifier and cannot mint this authorization itself.
  final bool preauthorizedHiddenReplacement;

  bool get intersectsHiddenSource =>
      projection.rangeIntersectsHiddenPiece(sourceStartUtf16, sourceEndUtf16);
}

/// Canonical source edit selected for one exact display-space edit.
final class FlarkV3SourceProjectionEditPlan {
  const FlarkV3SourceProjectionEditPlan({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.replacement,
  });

  factory FlarkV3SourceProjectionEditPlan.identity(
    FlarkV3SourceProjectionEditRequest request,
  ) => FlarkV3SourceProjectionEditPlan(
    sourceStartUtf16: request.sourceStartUtf16,
    sourceEndUtf16: request.sourceEndUtf16,
    replacement: FlarkV3SourceProjectionReplacement.identity(
      request.displayReplacement,
    ),
  );

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final FlarkV3SourceProjectionReplacement replacement;
}

/// Policy seam between platform display edits and canonical source edits.
///
/// Implementations operate only on typed projection pieces and explicit
/// parser-authored policy state. Markdown recognition does not belong here.
abstract interface class FlarkV3SourceProjectionEditPolicy {
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  );
}

/// Identity editing for source-backed projections.
///
/// Hidden source may be removed only when a typed higher layer has already
/// authorized that deletion. More specialized block policies can implement
/// [FlarkV3SourceProjectionEditPolicy] to authorize typed hidden gaps and
/// provide non-identity replacement projections.
final class FlarkV3SourceBackedProjectionEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  const FlarkV3SourceBackedProjectionEditPolicy();

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) {
    final hiddenEditAuthorized =
        request.sourceStartUtf16 == request.sourceEndUtf16 &&
            request.displayReplacement.isNotEmpty
        ? request.preauthorizedHiddenInsertion
        : request.displayReplacement.isEmpty
        ? request.preauthorizedHiddenDeletion
        : request.preauthorizedHiddenReplacement;
    if (request.intersectsHiddenSource && !hiddenEditAuthorized) {
      throw StateError(
        'Display edit crosses hidden source and requires a typed edit policy.',
      );
    }
    return FlarkV3SourceProjectionEditPlan.identity(request);
  }
}

/// Immutable bounded source/display map for one parser-authored projection.
///
/// [sourceText] is the exact bounded canonical source snapshot. Display text is
/// derived solely by concatenating copied source slices and explicit
/// parser-authored replacement text.
final class FlarkV3SourceProjection {
  factory FlarkV3SourceProjection.fromSource({
    required int sourceStartUtf16,
    required String sourceText,
    required List<FlarkV3SourceProjectionPiece> pieces,
    FlarkV3SourceVersion? certifiedSourceVersion,
    int maximumSourceUtf16 = defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 = defaultMaximumDisplayUtf16,
  }) {
    if (sourceStartUtf16 < 0 ||
        maximumSourceUtf16 <= 0 ||
        maximumDisplayUtf16 <= 0) {
      throw RangeError('Source projection bounds are invalid.');
    }
    if (sourceText.length > maximumSourceUtf16) {
      throw RangeError('Source projection exceeds its bounded source window.');
    }
    final sourceEndUtf16 = sourceStartUtf16 + sourceText.length;
    if (certifiedSourceVersion != null &&
        sourceEndUtf16 > certifiedSourceVersion.metric.utf16) {
      throw RangeError(
        'Source projection exceeds its certified source authority.',
      );
    }
    final immutablePieces = List<FlarkV3SourceProjectionPiece>.unmodifiable(
      pieces,
    );
    _validatePieces(
      sourceStartUtf16: sourceStartUtf16,
      sourceEndUtf16: sourceEndUtf16,
      pieces: immutablePieces,
    );
    final display = StringBuffer();
    for (final piece in immutablePieces) {
      switch (piece.kind) {
        case FlarkV3SourceProjectionPieceKind.copy:
          display.write(
            sourceText.substring(
              piece.sourceStartUtf16 - sourceStartUtf16,
              piece.sourceEndUtf16 - sourceStartUtf16,
            ),
          );
        case FlarkV3SourceProjectionPieceKind.hide:
          break;
        case FlarkV3SourceProjectionPieceKind.replace:
          display.write(piece.displayText);
      }
    }
    final displayText = display.toString();
    if (displayText.length > maximumDisplayUtf16) {
      throw RangeError('Source projection exceeds its bounded display window.');
    }
    return FlarkV3SourceProjection._(
      sourceStartUtf16: sourceStartUtf16,
      sourceText: sourceText,
      displayText: displayText,
      pieces: immutablePieces,
      certifiedSourceVersion: certifiedSourceVersion,
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }

  FlarkV3SourceProjection._({
    required this.sourceStartUtf16,
    required this.sourceText,
    required this.displayText,
    required this.pieces,
    required this.certifiedSourceVersion,
    required this.maximumSourceUtf16,
    required this.maximumDisplayUtf16,
  }) : sourceEndUtf16 = sourceStartUtf16 + sourceText.length {
    final maps = _buildCoordinateMaps(
      sourceStartUtf16: sourceStartUtf16,
      sourceEndUtf16: sourceEndUtf16,
      displayLengthUtf16: displayText.length,
      pieces: pieces,
    );
    _sourceToDisplay = maps.sourceToDisplay;
    _displayToSourceUpstream = maps.displayToSourceUpstream;
    _displayToSourceDownstream = maps.displayToSourceDownstream;
  }

  static const int defaultMaximumSourceUtf16 = 8 * 1024;
  static const int defaultMaximumDisplayUtf16 = 8 * 1024;

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final String sourceText;
  final String displayText;
  final List<FlarkV3SourceProjectionPiece> pieces;
  final FlarkV3SourceVersion? certifiedSourceVersion;
  final int maximumSourceUtf16;
  final int maximumDisplayUtf16;

  late final Uint32List _sourceToDisplay;
  late final Uint32List _displayToSourceUpstream;
  late final Uint32List _displayToSourceDownstream;

  int get sourceLengthUtf16 => sourceText.length;
  int get displayLengthUtf16 => displayText.length;
  bool get isCertified => certifiedSourceVersion != null;

  FlarkV3SourceProjection asProvisional() {
    if (!isCertified) return this;
    return FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: sourceStartUtf16,
      sourceText: sourceText,
      pieces: pieces,
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }

  int sourceToDisplayOffset(int sourceOffsetUtf16) {
    if (sourceOffsetUtf16 < sourceStartUtf16 ||
        sourceOffsetUtf16 > sourceEndUtf16) {
      throw RangeError.range(
        sourceOffsetUtf16,
        sourceStartUtf16,
        sourceEndUtf16,
        'sourceOffsetUtf16',
      );
    }
    return _sourceToDisplay[sourceOffsetUtf16 - sourceStartUtf16];
  }

  int displayToSourceOffset(
    int displayOffsetUtf16, {
    required FlarkV3SourceProjectionAffinity affinity,
  }) {
    if (displayOffsetUtf16 < 0 || displayOffsetUtf16 > displayLengthUtf16) {
      throw RangeError.range(
        displayOffsetUtf16,
        0,
        displayLengthUtf16,
        'displayOffsetUtf16',
      );
    }
    final relative = affinity == FlarkV3SourceProjectionAffinity.upstream
        ? _displayToSourceUpstream[displayOffsetUtf16]
        : _displayToSourceDownstream[displayOffsetUtf16];
    return sourceStartUtf16 + relative;
  }

  bool isStrictlyInsideHiddenPiece(int sourceOffsetUtf16) {
    for (final piece in pieces) {
      if (piece.isHidden &&
          piece.sourceStartUtf16 < sourceOffsetUtf16 &&
          sourceOffsetUtf16 < piece.sourceEndUtf16) {
        return true;
      }
    }
    return false;
  }

  bool rangeIntersectsHiddenPiece(int sourceStart, int sourceEnd) => pieces.any(
    (piece) =>
        piece.isHidden &&
        piece.sourceStartUtf16 < sourceEnd &&
        piece.sourceEndUtf16 > sourceStart,
  );

  /// Widens a display edit over every intersected replacement piece.
  ///
  /// Insertions exactly at a replacement edge remain outside the source token.
  /// An insertion strictly inside cooked display text consumes the token and
  /// re-emits the cooked prefix, insertion, and suffix as literal source.
  /// Edits whose endpoints split a Unicode scalar are rejected so cooked text
  /// cannot be re-emitted as ill-formed UTF-16 source.
  FlarkV3SourceProjectionDisplayEdit expandDisplayEditOverReplacements({
    required int displayStartUtf16,
    required int displayEndUtf16,
    required String replacement,
  }) {
    if (displayStartUtf16 < 0 ||
        displayEndUtf16 < displayStartUtf16 ||
        displayEndUtf16 > displayLengthUtf16) {
      throw RangeError('Display edit escapes its source projection.');
    }

    var expandedStart = displayStartUtf16;
    var expandedEnd = displayEndUtf16;
    var displayCursor = 0;
    final isInsertion = displayStartUtf16 == displayEndUtf16;
    for (final piece in pieces) {
      final pieceStart = displayCursor;
      final pieceEnd = pieceStart + piece.displayLengthUtf16;
      displayCursor = pieceEnd;
      if (!piece.isReplaced) continue;
      final displayText = piece.displayText!;
      final startSplitsScalar =
          pieceStart < displayStartUtf16 &&
          displayStartUtf16 < pieceEnd &&
          !_isUnicodeScalarBoundary(
            displayText,
            displayStartUtf16 - pieceStart,
          );
      final endSplitsScalar =
          displayEndUtf16 != displayStartUtf16 &&
          pieceStart < displayEndUtf16 &&
          displayEndUtf16 < pieceEnd &&
          !_isUnicodeScalarBoundary(displayText, displayEndUtf16 - pieceStart);
      if (startSplitsScalar || endSplitsScalar) {
        throw StateError(
          'Display edit boundary splits a Unicode scalar in '
          'parser-authored replacement text.',
        );
      }
      final intersects = isInsertion
          ? pieceStart < displayStartUtf16 && displayStartUtf16 < pieceEnd
          : pieceStart < displayEndUtf16 && pieceEnd > displayStartUtf16;
      if (!intersects) continue;
      if (pieceStart < expandedStart) expandedStart = pieceStart;
      if (pieceEnd > expandedEnd) expandedEnd = pieceEnd;
    }

    if (expandedStart == displayStartUtf16 && expandedEnd == displayEndUtf16) {
      return FlarkV3SourceProjectionDisplayEdit(
        displayStartUtf16: displayStartUtf16,
        displayEndUtf16: displayEndUtf16,
        replacement: replacement,
      );
    }
    return FlarkV3SourceProjectionDisplayEdit(
      displayStartUtf16: expandedStart,
      displayEndUtf16: expandedEnd,
      replacement:
          '${displayText.substring(expandedStart, displayStartUtf16)}'
          '$replacement'
          '${displayText.substring(displayEndUtf16, expandedEnd)}',
    );
  }

  /// Applies one exact canonical replacement and returns a provisional map.
  ///
  /// The caller remains responsible for ensuring that the replacement's
  /// display projection matches the platform delta being handled.
  FlarkV3SourceProjection replaceSourceRange({
    required int sourceStartUtf16,
    required int sourceEndUtf16,
    required FlarkV3SourceProjectionReplacement replacement,
  }) {
    if (sourceStartUtf16 < this.sourceStartUtf16 ||
        sourceEndUtf16 < sourceStartUtf16 ||
        sourceEndUtf16 > this.sourceEndUtf16) {
      throw RangeError('Source replacement escapes its projection.');
    }
    final sourceDelta =
        replacement.sourceReplacement.length -
        (sourceEndUtf16 - sourceStartUtf16);
    final nextSourceLength = sourceLengthUtf16 + sourceDelta;
    if (nextSourceLength > maximumSourceUtf16) {
      throw RangeError('Source replacement exceeds its bounded projection.');
    }

    final output = <FlarkV3SourceProjectionPiece>[];
    var inserted = false;

    void insertReplacement() {
      if (inserted) return;
      inserted = true;
      for (final piece in replacement.pieces) {
        output.add(piece.shift(sourceStartUtf16));
      }
    }

    for (final piece in pieces) {
      if (piece.sourceEndUtf16 <= sourceStartUtf16) {
        output.add(piece);
        continue;
      }
      if (piece.sourceStartUtf16 >= sourceEndUtf16) {
        insertReplacement();
        output.add(piece.shift(sourceDelta));
        continue;
      }
      if (piece.sourceStartUtf16 < sourceStartUtf16) {
        output.add(piece.slice(piece.sourceStartUtf16, sourceStartUtf16));
      }
      insertReplacement();
      if (piece.sourceEndUtf16 > sourceEndUtf16) {
        output.add(
          piece.slice(sourceEndUtf16, piece.sourceEndUtf16).shift(sourceDelta),
        );
      }
    }
    insertReplacement();

    final relativeStart = sourceStartUtf16 - this.sourceStartUtf16;
    final relativeEnd = sourceEndUtf16 - this.sourceStartUtf16;
    final nextSource = sourceText.replaceRange(
      relativeStart,
      relativeEnd,
      replacement.sourceReplacement,
    );
    return FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: this.sourceStartUtf16,
      sourceText: nextSource,
      pieces: _normalizePieces(output),
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }
}

void _validatePieces({
  required int sourceStartUtf16,
  required int sourceEndUtf16,
  required List<FlarkV3SourceProjectionPiece> pieces,
}) {
  var cursor = sourceStartUtf16;
  for (final piece in pieces) {
    if (piece.sourceStartUtf16 != cursor ||
        piece.sourceEndUtf16 <= piece.sourceStartUtf16 ||
        piece.sourceEndUtf16 > sourceEndUtf16) {
      throw StateError(
        'Source projection pieces must exhaustively cover source.',
      );
    }
    if (piece.isReplaced && (piece.displayText?.isEmpty ?? true)) {
      throw StateError(
        'Replacement pieces require non-empty explicit display text.',
      );
    }
    cursor = piece.sourceEndUtf16;
  }
  if (cursor != sourceEndUtf16 ||
      (sourceStartUtf16 != sourceEndUtf16 && pieces.isEmpty)) {
    throw StateError('Source projection pieces do not cover their source.');
  }
}

List<FlarkV3SourceProjectionPiece> _normalizePieces(
  List<FlarkV3SourceProjectionPiece> pieces,
) {
  final output = <FlarkV3SourceProjectionPiece>[];
  for (final piece in pieces) {
    if (piece.sourceLengthUtf16 == 0) continue;
    if (output.isNotEmpty &&
        output.last.sourceEndUtf16 == piece.sourceStartUtf16 &&
        output.last.kind == piece.kind &&
        !piece.isReplaced) {
      final previous = output.removeLast();
      output.add(
        previous.slice(previous.sourceStartUtf16, piece.sourceEndUtf16),
      );
    } else {
      output.add(piece);
    }
  }
  return output;
}

typedef _CoordinateMaps = ({
  Uint32List sourceToDisplay,
  Uint32List displayToSourceUpstream,
  Uint32List displayToSourceDownstream,
});

_CoordinateMaps _buildCoordinateMaps({
  required int sourceStartUtf16,
  required int sourceEndUtf16,
  required int displayLengthUtf16,
  required List<FlarkV3SourceProjectionPiece> pieces,
}) {
  final sourceLength = sourceEndUtf16 - sourceStartUtf16;
  final sourceToDisplay = Uint32List(sourceLength + 1);
  final upstream = Uint32List(displayLengthUtf16 + 1);
  final downstream = Uint32List(displayLengthUtf16 + 1);
  final seen = Uint8List(displayLengthUtf16 + 1);
  var sourceCursor = sourceStartUtf16;
  var displayCursor = 0;

  void recordSource(int sourceOffset, int displayOffset) {
    final sourceRelative = sourceOffset - sourceStartUtf16;
    sourceToDisplay[sourceRelative] = displayOffset;
  }

  void recordInverse(int displayOffset, int sourceOffset) {
    final sourceRelative = sourceOffset - sourceStartUtf16;
    if (seen[displayOffset] == 0) {
      upstream[displayOffset] = sourceRelative;
      downstream[displayOffset] = sourceRelative;
      seen[displayOffset] = 1;
    } else {
      if (sourceRelative < upstream[displayOffset]) {
        upstream[displayOffset] = sourceRelative;
      }
      if (sourceRelative > downstream[displayOffset]) {
        downstream[displayOffset] = sourceRelative;
      }
    }
  }

  void record(int sourceOffset, int displayOffset) {
    recordSource(sourceOffset, displayOffset);
    recordInverse(displayOffset, sourceOffset);
  }

  if (pieces.isEmpty && sourceLength == 0 && displayLengthUtf16 == 0) {
    record(sourceStartUtf16, 0);
  }
  for (final piece in pieces) {
    if (piece.sourceStartUtf16 != sourceCursor ||
        piece.sourceEndUtf16 <= piece.sourceStartUtf16) {
      throw StateError('Source projection pieces do not exactly cover source.');
    }
    switch (piece.kind) {
      case FlarkV3SourceProjectionPieceKind.hide:
        for (
          var sourceOffset = piece.sourceStartUtf16;
          sourceOffset <= piece.sourceEndUtf16;
          sourceOffset += 1
        ) {
          record(sourceOffset, displayCursor);
        }
      case FlarkV3SourceProjectionPieceKind.copy:
        for (var offset = 0; offset <= piece.sourceLengthUtf16; offset += 1) {
          record(piece.sourceStartUtf16 + offset, displayCursor + offset);
        }
        displayCursor += piece.sourceLengthUtf16;
      case FlarkV3SourceProjectionPieceKind.replace:
        final sourceLength = piece.sourceLengthUtf16;
        final displayLength = piece.displayLengthUtf16;
        final scalarBoundaries = _unicodeScalarBoundaries(piece.displayText!);
        final scalarCount = scalarBoundaries.length - 1;
        for (
          var sourceOffset = 0;
          sourceOffset <= sourceLength;
          sourceOffset++
        ) {
          final scalarOffset = (sourceOffset * scalarCount) ~/ sourceLength;
          recordSource(
            piece.sourceStartUtf16 + sourceOffset,
            displayCursor + scalarBoundaries[scalarOffset],
          );
        }
        var scalarOffset = 0;
        for (
          var displayOffset = 0;
          displayOffset <= displayLength;
          displayOffset++
        ) {
          while (scalarBoundaries[scalarOffset] < displayOffset) {
            scalarOffset += 1;
          }
          final sourceOffset =
              (scalarOffset * sourceLength + scalarCount - 1) ~/ scalarCount;
          recordInverse(
            displayCursor + displayOffset,
            piece.sourceStartUtf16 + sourceOffset,
          );
        }
        displayCursor += displayLength;
    }
    sourceCursor = piece.sourceEndUtf16;
  }
  if (sourceCursor != sourceEndUtf16 ||
      displayCursor != displayLengthUtf16 ||
      seen.any((value) => value == 0)) {
    throw StateError('Source projection coordinate maps are not total.');
  }
  return (
    sourceToDisplay: sourceToDisplay,
    displayToSourceUpstream: upstream,
    displayToSourceDownstream: downstream,
  );
}

List<int> _unicodeScalarBoundaries(String text) {
  final boundaries = <int>[0];
  var offset = 0;
  for (final scalar in text.runes) {
    offset += scalar > 0xFFFF ? 2 : 1;
    boundaries.add(offset);
  }
  if (offset != text.length) {
    throw StateError('Replacement display text is not well-formed UTF-16.');
  }
  return boundaries;
}

bool _isUnicodeScalarBoundary(String text, int offsetUtf16) {
  if (offsetUtf16 <= 0 || offsetUtf16 >= text.length) return true;
  final previous = text.codeUnitAt(offsetUtf16 - 1);
  final next = text.codeUnitAt(offsetUtf16);
  final splitsSurrogatePair =
      0xD800 <= previous &&
      previous <= 0xDBFF &&
      0xDC00 <= next &&
      next <= 0xDFFF;
  return !splitsSurrogatePair;
}
