import 'package:flark/flark_adapter.dart';
import 'package:flutter/widgets.dart';

/// One display-space edit resolved against a bounded projected input lease.
///
/// The platform value remains entirely in display coordinates. Source ranges
/// are absolute document UTF-16 coordinates and are the only coordinates that
/// may be committed to the canonical document.
final class FlarkV3ProjectedInputEdit {
  const FlarkV3ProjectedInputEdit._({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.sourceReplacement,
    required this.displayReplacement,
    required this.sourceSelection,
    required this.sourceComposing,
    required this.displayValue,
    required this.nextLease,
  });

  final int sourceStartUtf16;
  final int sourceEndUtf16;

  /// Exact replacement committed to canonical Markdown source.
  final String sourceReplacement;

  /// Exact replacement already echoed in the platform display value.
  final String displayReplacement;

  /// Compatibility name for the canonical replacement.
  ///
  /// Existing inline projections use identical source/display replacements.
  String get replacement => sourceReplacement;
  final TextSelection sourceSelection;
  final TextRange sourceComposing;
  final TextEditingValue displayValue;
  final FlarkV3ProjectedInputLease nextLease;
}

/// Source-only delimiter cleanup after a deferred platform delta batch.
///
/// [sourceEdits] are ordered in the coordinate space produced by the previous
/// edit. They remove only parser-certified empty pairs and never change the
/// platform display value.
final class FlarkV3ProjectedInputCleanup {
  FlarkV3ProjectedInputCleanup._({
    required List<FlarkV3SourceEdit> sourceEdits,
    required this.sourceSelection,
    required this.sourceComposing,
    required this.nextLease,
  }) : sourceEdits = List<FlarkV3SourceEdit>.unmodifiable(sourceEdits);

  final List<FlarkV3SourceEdit> sourceEdits;
  final TextSelection sourceSelection;
  final TextRange sourceComposing;
  final FlarkV3ProjectedInputLease nextLease;
}

/// Immutable bounded source/display map for one input-island snapshot.
///
/// The source leaf is bounded to the parser's 8 KiB inline-fact envelope.
/// Mapping tables are therefore bounded independently of document size. They
/// are replaced mechanically after display edits and parser certification;
/// this never recognizes Markdown or manufactures parser authority.
final class FlarkV3ProjectedInputLease {
  FlarkV3ProjectedInputLease._({
    required FlarkV3SourceProjection sourceProjection,
    required List<_ProjectedPiece> pieces,
    required FlarkV3InlineDelimiterTopology? delimiterTopology,
    required FlarkV3SourceProjectionEditPolicy editPolicy,
    required _ProjectedContinuationAnchor? continuationAnchor,
  }) : _sourceProjection = sourceProjection,
       _pieces = List<_ProjectedPiece>.unmodifiable(pieces),
       _delimiterTopology = delimiterTopology,
       _editPolicy = editPolicy,
       _continuationAnchor = continuationAnchor {
    _validatePresentationPieces(
      sourceProjection: _sourceProjection,
      pieces: _pieces,
    );
    final anchor = _continuationAnchor;
    if (anchor != null &&
        (anchor.sourceOffsetUtf16 < sourceStartUtf16 ||
            anchor.sourceOffsetUtf16 > sourceEndUtf16 ||
            anchor.displayOffsetUtf16 < 0 ||
            anchor.displayOffsetUtf16 > displayText.length ||
            sourceToDisplayOffset(anchor.sourceOffsetUtf16) !=
                anchor.displayOffsetUtf16)) {
      throw RangeError(
        'Projected continuation anchor escapes its exact source/display map.',
      );
    }
  }

  /// Adapts one parser-authored source projection without inline semantics.
  factory FlarkV3ProjectedInputLease.fromSourceProjection(
    FlarkV3SourceProjection projection, {
    FlarkV3SourceProjectionEditPolicy editPolicy =
        const FlarkV3SourceBackedProjectionEditPolicy(),
  }) => FlarkV3ProjectedInputLease._(
    sourceProjection: projection,
    pieces: [
      for (final piece in projection.pieces)
        _ProjectedPiece.fromSourcePiece(
          piece,
          const <FlarkV3InlineFactKind>[],
          null,
        ),
    ],
    delimiterTopology: null,
    editPolicy: editPolicy,
    continuationAnchor: null,
  );

  factory FlarkV3ProjectedInputLease.fromAuthoritative(
    FlarkV3AuthoritativeInlineIslandPresentation authoritative,
  ) {
    final projection = authoritative.projection;
    final pieces = _projectedPiecesFromInlineProjection(projection);
    final sourceProjection = projection.sourceProjection;
    return FlarkV3ProjectedInputLease._(
      sourceProjection: sourceProjection,
      pieces: pieces,
      delimiterTopology: projection.delimiterTopology,
      editPolicy: const FlarkV3SourceBackedProjectionEditPolicy(),
      continuationAnchor: null,
    );
  }

  /// Adapts one already-validated parser inline projection directly.
  ///
  /// This is used when the viewport materializer, rather than the focused
  /// point-query coordinator, owns the same exact source/display authority.
  factory FlarkV3ProjectedInputLease.fromInlineProjection(
    FlarkV3InlineProjection projection,
  ) => FlarkV3ProjectedInputLease._(
    sourceProjection: projection.sourceProjection,
    pieces: _projectedPiecesFromInlineProjection(projection),
    delimiterTopology: projection.delimiterTopology,
    editPolicy: const FlarkV3SourceBackedProjectionEditPolicy(),
    continuationAnchor: null,
  );

  /// Overlays one parser-certified inline leaf inside an independently
  /// parser-authored structural projection.
  ///
  /// The outer projection remains authoritative for hidden prefixes, line
  /// endings, and structural edit policy. The inline presentation may only
  /// replace an exact copied source subrange; it cannot reinterpret a hidden
  /// or normalized structural piece.
  factory FlarkV3ProjectedInputLease.fromSourceProjectionWithAuthoritativeInline(
    FlarkV3SourceProjection sourceProjection,
    FlarkV3AuthoritativeInlineIslandPresentation authoritative, {
    FlarkV3SourceProjectionEditPolicy editPolicy =
        const FlarkV3SourceBackedProjectionEditPolicy(),
  }) {
    final inline = authoritative.projection;
    final inlineStart = inline.sourceStartUtf16;
    final inlineEnd = inline.sourceEndUtf16;
    if (sourceProjection.certifiedSourceVersion !=
            authoritative.facts.sourceVersion ||
        inlineStart < sourceProjection.sourceStartUtf16 ||
        inlineEnd > sourceProjection.sourceEndUtf16 ||
        inlineStart >= inlineEnd ||
        sourceProjection.sourceText.substring(
              inlineStart - sourceProjection.sourceStartUtf16,
              inlineEnd - sourceProjection.sourceStartUtf16,
            ) !=
            inline.sourceText) {
      throw StateError(
        'Inline authority does not match its enclosing source projection.',
      );
    }
    for (final piece in sourceProjection.pieces) {
      if (piece.sourceEndUtf16 <= inlineStart ||
          piece.sourceStartUtf16 >= inlineEnd) {
        continue;
      }
      if (!piece.isCopied) {
        throw StateError(
          'Inline authority intersects a non-copy structural projection piece.',
        );
      }
    }

    final inlinePieces = _projectedPiecesFromInlineProjection(inline);

    final pieces = <_ProjectedPiece>[];
    var insertedInline = false;
    void insertInline() {
      if (insertedInline) return;
      insertedInline = true;
      pieces.addAll(inlinePieces);
    }

    for (final piece in sourceProjection.pieces) {
      if (piece.sourceEndUtf16 <= inlineStart) {
        pieces.add(
          _ProjectedPiece.fromSourcePiece(
            piece,
            const <FlarkV3InlineFactKind>[],
            null,
          ),
        );
        continue;
      }
      if (piece.sourceStartUtf16 >= inlineEnd) {
        insertInline();
        pieces.add(
          _ProjectedPiece.fromSourcePiece(
            piece,
            const <FlarkV3InlineFactKind>[],
            null,
          ),
        );
        continue;
      }
      if (piece.sourceStartUtf16 < inlineStart) {
        pieces.add(
          _ProjectedPiece.fromSourcePiece(
            piece.slice(piece.sourceStartUtf16, inlineStart),
            const <FlarkV3InlineFactKind>[],
            null,
          ),
        );
      }
      insertInline();
      if (piece.sourceEndUtf16 > inlineEnd) {
        pieces.add(
          _ProjectedPiece.fromSourcePiece(
            piece.slice(inlineEnd, piece.sourceEndUtf16),
            const <FlarkV3InlineFactKind>[],
            null,
          ),
        );
      }
    }
    insertInline();
    final mergedPieces = _normalizePieces(pieces);
    final mergedProjection = FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: sourceProjection.sourceStartUtf16,
      sourceText: sourceProjection.sourceText,
      pieces: [for (final piece in mergedPieces) piece.sourcePiece],
      certifiedSourceVersion: sourceProjection.certifiedSourceVersion,
      maximumSourceUtf16: sourceProjection.maximumSourceUtf16,
      maximumDisplayUtf16: sourceProjection.maximumDisplayUtf16,
    );
    return FlarkV3ProjectedInputLease._(
      sourceProjection: mergedProjection,
      pieces: mergedPieces,
      delimiterTopology: inline.delimiterTopology,
      editPolicy: editPolicy,
      continuationAnchor: null,
    );
  }

  /// Composes inline facts expressed in marker-free container coordinates
  /// through the container's independently certified physical source map.
  ///
  /// Block-quote prefixes remain owned by [sourceProjection]. Inline markers
  /// and styles remain owned by [projectedInline]. This join is purely
  /// geometric: it does not inspect either source string for Markdown.
  factory FlarkV3ProjectedInputLease.fromSourceProjectionWithProjectedInline(
    FlarkV3SourceProjection sourceProjection,
    FlarkV3ProjectedInlineProjection projectedInline, {
    FlarkV3SourceProjectionEditPolicy editPolicy =
        const FlarkV3SourceBackedProjectionEditPolicy(),
  }) {
    if (sourceProjection.displayText != projectedInline.projectedText) {
      throw StateError(
        'Projected inline authority does not match its container projection.',
      );
    }
    final pieces = _composeProjectedInlinePieces(
      sourceProjection,
      projectedInline,
    );
    final mergedProjection = FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: sourceProjection.sourceStartUtf16,
      sourceText: sourceProjection.sourceText,
      pieces: [for (final piece in pieces) piece.sourcePiece],
      certifiedSourceVersion: sourceProjection.certifiedSourceVersion,
      maximumSourceUtf16: sourceProjection.maximumSourceUtf16,
      maximumDisplayUtf16: sourceProjection.maximumDisplayUtf16,
    );
    if (mergedProjection.displayText != projectedInline.displayText) {
      throw StateError(
        'Projected inline composition diverged from its certified display.',
      );
    }
    return FlarkV3ProjectedInputLease._(
      sourceProjection: mergedProjection,
      pieces: pieces,
      delimiterTopology: null,
      editPolicy: editPolicy,
      continuationAnchor: null,
    );
  }

  final FlarkV3SourceProjection _sourceProjection;
  final List<_ProjectedPiece> _pieces;
  final FlarkV3InlineDelimiterTopology? _delimiterTopology;
  final FlarkV3SourceProjectionEditPolicy _editPolicy;
  final _ProjectedContinuationAnchor? _continuationAnchor;

  int get sourceStartUtf16 => _sourceProjection.sourceStartUtf16;
  int get sourceEndUtf16 => _sourceProjection.sourceEndUtf16;
  String get displayText => _sourceProjection.displayText;
  FlarkV3SourceVersion? get certifiedSourceVersion =>
      _sourceProjection.certifiedSourceVersion;
  int get sourceLengthUtf16 => _sourceProjection.sourceLengthUtf16;
  int get displayLengthUtf16 => _sourceProjection.displayLengthUtf16;
  bool get isCertified => _sourceProjection.isCertified;

  FlarkV3ProjectedInputLease asProvisional() {
    if (!isCertified) return this;
    return FlarkV3ProjectedInputLease._(
      sourceProjection: _sourceProjection.asProvisional(),
      pieces: _pieces,
      delimiterTopology: _delimiterTopology,
      editPolicy: _editPolicy,
      continuationAnchor: _continuationAnchor,
    );
  }

  int sourceToDisplayOffset(int sourceOffsetUtf16) =>
      _sourceProjection.sourceToDisplayOffset(sourceOffsetUtf16);

  int displayToSourceOffset(
    int displayOffsetUtf16, {
    required FlarkV3InlineProjectionAffinity affinity,
  }) {
    return _sourceProjection.displayToSourceOffset(
      displayOffsetUtf16,
      affinity: affinity == FlarkV3InlineProjectionAffinity.upstream
          ? FlarkV3SourceProjectionAffinity.upstream
          : FlarkV3SourceProjectionAffinity.downstream,
    );
  }

  TextSelection sourceSelectionToDisplay(TextSelection sourceSelection) {
    _validateSourceSelection(sourceSelection);
    return TextSelection(
      baseOffset: sourceToDisplayOffset(sourceSelection.baseOffset),
      extentOffset: sourceToDisplayOffset(sourceSelection.extentOffset),
      affinity: sourceSelection.affinity,
      isDirectional: sourceSelection.isDirectional,
    );
  }

  TextSelection displaySelectionToSource(
    TextSelection displaySelection, {
    TextSelection? preferredSourceSelection,
  }) {
    _validateDisplaySelection(displaySelection);
    if (preferredSourceSelection != null &&
        _isSourceSelectionInLease(preferredSourceSelection) &&
        sourceSelectionToDisplay(preferredSourceSelection) ==
            displaySelection) {
      return preferredSourceSelection;
    }
    if (displaySelection.isCollapsed) {
      final offset = displaySelection.extentOffset;
      final affinity = offset == displayLengthUtf16 && offset != 0
          ? FlarkV3InlineProjectionAffinity.upstream
          : FlarkV3InlineProjectionAffinity.downstream;
      return TextSelection.collapsed(
        offset: displayToSourceOffset(offset, affinity: affinity),
        affinity: displaySelection.affinity,
      );
    }

    final start = displayToSourceOffset(
      displaySelection.start,
      affinity: FlarkV3InlineProjectionAffinity.downstream,
    );
    final end = displayToSourceOffset(
      displaySelection.end,
      affinity: FlarkV3InlineProjectionAffinity.upstream,
    );
    if (start > end) {
      throw StateError('Display selection crosses an ambiguous marker chain.');
    }
    return displaySelection.baseOffset <= displaySelection.extentOffset
        ? TextSelection(
            baseOffset: start,
            extentOffset: end,
            affinity: displaySelection.affinity,
            isDirectional: displaySelection.isDirectional,
          )
        : TextSelection(
            baseOffset: end,
            extentOffset: start,
            affinity: displaySelection.affinity,
            isDirectional: displaySelection.isDirectional,
          );
  }

  TextRange displayComposingToSource(
    TextRange displayComposing, {
    TextRange? preferredSourceComposing,
  }) {
    if (!displayComposing.isValid) return TextRange.empty;
    if (displayComposing.start < 0 ||
        displayComposing.end < displayComposing.start ||
        displayComposing.end > displayLengthUtf16) {
      throw RangeError('Display composing range escapes the input lease.');
    }
    if (preferredSourceComposing != null &&
        preferredSourceComposing.isValid &&
        preferredSourceComposing.start >= sourceStartUtf16 &&
        preferredSourceComposing.end <= sourceEndUtf16 &&
        sourceToDisplayOffset(preferredSourceComposing.start) ==
            displayComposing.start &&
        sourceToDisplayOffset(preferredSourceComposing.end) ==
            displayComposing.end) {
      return preferredSourceComposing;
    }
    final start = displayToSourceOffset(
      displayComposing.start,
      affinity: FlarkV3InlineProjectionAffinity.downstream,
    );
    final end = displayToSourceOffset(
      displayComposing.end,
      affinity: FlarkV3InlineProjectionAffinity.upstream,
    );
    if (start > end) {
      throw StateError(
        'Display composition crosses an ambiguous marker chain.',
      );
    }
    return TextRange(start: start, end: end);
  }

  FlarkV3ProjectedInputEdit applyDisplayEdit({
    required int displayStartUtf16,
    required int displayEndUtf16,
    required String replacement,
    required TextEditingValue nextDisplayValue,
    required TextSelection preferredSourceSelection,
    required TextRange preferredSourceComposing,
    bool cleanupOrphanedDelimiters = true,
  }) {
    if (displayStartUtf16 < 0 ||
        displayEndUtf16 < displayStartUtf16 ||
        displayEndUtf16 > displayLengthUtf16) {
      throw RangeError('Display edit escapes the projected input lease.');
    }
    final expectedDisplay = displayText.replaceRange(
      displayStartUtf16,
      displayEndUtf16,
      replacement,
    );
    if (nextDisplayValue.text != expectedDisplay) {
      throw StateError(
        'Platform display value does not match its exact delta.',
      );
    }
    final expandedDisplayEdit = _sourceProjection
        .expandDisplayEditOverReplacements(
          displayStartUtf16: displayStartUtf16,
          displayEndUtf16: displayEndUtf16,
          replacement: replacement,
        );
    final effectiveDisplayStart = expandedDisplayEdit.displayStartUtf16;
    final effectiveDisplayEnd = expandedDisplayEdit.displayEndUtf16;
    final effectiveReplacement = expandedDisplayEdit.replacement;
    if (_sourceProjection.displayText.replaceRange(
          effectiveDisplayStart,
          effectiveDisplayEnd,
          effectiveReplacement,
        ) !=
        nextDisplayValue.text) {
      throw StateError(
        'Expanded replacement-piece edit changed the platform display result.',
      );
    }

    late int sourceStart;
    late int sourceEnd;
    final continuationAnchor = _continuationAnchor;
    final trustedAnchorMatches =
        continuationAnchor != null &&
        effectiveDisplayStart == effectiveDisplayEnd &&
        preferredSourceSelection.isCollapsed &&
        preferredSourceSelection.extentOffset ==
            continuationAnchor.sourceOffsetUtf16 &&
        effectiveDisplayStart == continuationAnchor.displayOffsetUtf16;
    final preferredMatches =
        _isSourceSelectionInLease(preferredSourceSelection) &&
        ((!_isStrictlyInsideHiddenPiece(preferredSourceSelection.start) &&
                !_isStrictlyInsideHiddenPiece(preferredSourceSelection.end)) ||
            trustedAnchorMatches) &&
        sourceToDisplayOffset(preferredSourceSelection.start) ==
            effectiveDisplayStart &&
        sourceToDisplayOffset(preferredSourceSelection.end) ==
            effectiveDisplayEnd;
    if (preferredMatches) {
      sourceStart = preferredSourceSelection.start;
      sourceEnd = preferredSourceSelection.end;
    } else if (effectiveDisplayStart == effectiveDisplayEnd) {
      sourceStart = _sourceInsertionOffsetAtDisplayBoundary(
        effectiveDisplayStart,
        affinity: preferredSourceSelection.affinity,
      );
      sourceEnd = sourceStart;
    } else {
      sourceStart = displayToSourceOffset(
        effectiveDisplayStart,
        affinity: FlarkV3InlineProjectionAffinity.downstream,
      );
      sourceEnd = displayToSourceOffset(
        effectiveDisplayEnd,
        affinity: FlarkV3InlineProjectionAffinity.upstream,
      );
    }
    if (sourceStart > sourceEnd) {
      throw StateError('Display edit cannot resolve to an exact source range.');
    }
    final semanticSourceCaret = sourceStart;
    final delimiterTopology = _delimiterTopology;
    FlarkV3InlineEditPlan? inlineEditPlan;
    if (delimiterTopology != null) {
      inlineEditPlan = delimiterTopology.planEdit(
        FlarkV3SourceEdit(
          startUtf16: sourceStart,
          endUtf16: sourceEnd,
          replacement: effectiveReplacement,
        ),
        cleanupOrphanedPairs: cleanupOrphanedDelimiters,
      );
      sourceStart = inlineEditPlan.sourceStartUtf16;
      sourceEnd = inlineEditPlan.sourceEndUtf16;
    }
    final editPlan = _editPolicy.planEdit(
      FlarkV3SourceProjectionEditRequest(
        projection: _sourceProjection,
        sourceStartUtf16: sourceStart,
        sourceEndUtf16: sourceEnd,
        displayStartUtf16: effectiveDisplayStart,
        displayEndUtf16: effectiveDisplayEnd,
        displayReplacement: effectiveReplacement,
        preauthorizedHiddenDeletion:
            effectiveReplacement.isEmpty &&
            inlineEditPlan?.removesCertifiedConstructs == true,
        preauthorizedHiddenInsertion:
            inlineEditPlan?.authorizesAtomicBoundaryInsertion == true,
        preauthorizedHiddenReplacement:
            effectiveReplacement.isNotEmpty &&
            inlineEditPlan?.removesAtomicInlineAtoms == true,
      ),
    );
    if (editPlan.sourceStartUtf16 < sourceStartUtf16 ||
        editPlan.sourceEndUtf16 < editPlan.sourceStartUtf16 ||
        editPlan.sourceEndUtf16 > sourceEndUtf16) {
      throw RangeError('Projection edit policy escaped its source lease.');
    }
    if (sourceToDisplayOffset(editPlan.sourceStartUtf16) !=
            effectiveDisplayStart ||
        sourceToDisplayOffset(editPlan.sourceEndUtf16) != effectiveDisplayEnd) {
      throw StateError(
        'Projection edit policy did not preserve exact display boundaries.',
      );
    }
    if (editPlan.replacement.displayReplacement != effectiveReplacement) {
      throw StateError(
        'Projection edit policy changed the platform display replacement.',
      );
    }
    sourceStart = editPlan.sourceStartUtf16;
    sourceEnd = editPlan.sourceEndUtf16;
    final removedDelimiterPairs =
        inlineEditPlan?.removesPairedDelimiters == true;
    final replacementStyles = removedDelimiterPairs
        ? const <FlarkV3InlineFactKind>[]
        : trustedAnchorMatches
        ? continuationAnchor.semanticStyles
        : _stylesAtSourceCaret(semanticSourceCaret);
    final replacementLinkKind = removedDelimiterPairs
        ? null
        : trustedAnchorMatches
        ? continuationAnchor.linkKind
        : _linkKindAtSourceCaret(semanticSourceCaret);
    final nextContinuationAnchor =
        editPlan.replacement.sourceReplacement.isEmpty &&
            sourceStart < sourceEnd &&
            !removedDelimiterPairs &&
            nextDisplayValue.selection.isCollapsed &&
            nextDisplayValue.selection.extentOffset == effectiveDisplayStart
        ? _ProjectedContinuationAnchor(
            sourceOffsetUtf16: sourceStart,
            displayOffsetUtf16: effectiveDisplayStart,
            semanticStyles: replacementStyles,
            linkKind: replacementLinkKind,
          )
        : null;

    final nextLease = _replaceSourceRange(
      sourceStartUtf16: sourceStart,
      sourceEndUtf16: sourceEnd,
      displayStartUtf16: effectiveDisplayStart,
      displayEndUtf16: effectiveDisplayEnd,
      replacement: editPlan.replacement,
      replacementStyles: replacementStyles,
      replacementLinkKind: replacementLinkKind,
      continuationAnchor: nextContinuationAnchor,
    );
    final insertedSourceEnd =
        sourceStart + editPlan.replacement.sourceReplacement.length;
    final preferredSelectionAfter =
        nextDisplayValue.selection.isCollapsed &&
            nextDisplayValue.selection.extentOffset ==
                effectiveDisplayStart +
                    editPlan.replacement.displayReplacement.length
        ? TextSelection.collapsed(
            offset: insertedSourceEnd,
            affinity: nextDisplayValue.selection.affinity,
          )
        : null;
    final sourceSelection = nextLease.displaySelectionToSource(
      nextDisplayValue.selection,
      preferredSourceSelection: preferredSelectionAfter,
    );
    final insertedComposing =
        nextDisplayValue.composing.isValid &&
            nextDisplayValue.composing.start >= effectiveDisplayStart &&
            nextDisplayValue.composing.end <=
                effectiveDisplayStart +
                    editPlan.replacement.displayReplacement.length
        ? TextRange(
            start:
                sourceStart +
                editPlan.replacement.projection.displayToSourceOffset(
                  nextDisplayValue.composing.start - effectiveDisplayStart,
                  affinity: FlarkV3SourceProjectionAffinity.downstream,
                ),
            end:
                sourceStart +
                editPlan.replacement.projection.displayToSourceOffset(
                  nextDisplayValue.composing.end - effectiveDisplayStart,
                  affinity: FlarkV3SourceProjectionAffinity.upstream,
                ),
          )
        : null;
    final sourceComposing = nextLease.displayComposingToSource(
      nextDisplayValue.composing,
      preferredSourceComposing: insertedComposing,
    );
    return FlarkV3ProjectedInputEdit._(
      sourceStartUtf16: sourceStart,
      sourceEndUtf16: sourceEnd,
      sourceReplacement: editPlan.replacement.sourceReplacement,
      displayReplacement: editPlan.replacement.displayReplacement,
      sourceSelection: sourceSelection,
      sourceComposing: sourceComposing,
      displayValue: nextDisplayValue,
      nextLease: nextLease,
    );
  }

  /// Removes parser-certified pairs left empty by deferred batch edits.
  ///
  /// Platform replacement callbacks commonly arrive as delete-then-insert.
  /// Per-delta cleanup would discard the pair before the insertion can reuse
  /// it, so batch callers defer cleanup and invoke this once at the end.
  FlarkV3ProjectedInputCleanup cleanupOrphanedDelimiters({
    required TextSelection sourceSelection,
    required TextRange sourceComposing,
  }) {
    _validateSourceSelection(sourceSelection);
    if (sourceComposing.isValid &&
        (sourceComposing.start < sourceStartUtf16 ||
            sourceComposing.end > sourceEndUtf16)) {
      throw RangeError('Source composition escapes the projected input lease.');
    }

    var lease = this;
    var selection = sourceSelection;
    var composing = sourceComposing;
    final edits = <FlarkV3SourceEdit>[];
    final delimiterTopology = _delimiterTopology;
    if (delimiterTopology == null) {
      return FlarkV3ProjectedInputCleanup._(
        sourceEdits: const <FlarkV3SourceEdit>[],
        sourceSelection: selection,
        sourceComposing: composing,
        nextLease: lease,
      );
    }
    final maximumCleanups = delimiterTopology.pairs.length;
    for (var cleanup = 0; cleanup < maximumCleanups; cleanup += 1) {
      final plans = lease._delimiterTopology!.planOrphanCleanup();
      if (plans.isEmpty) break;
      final plan = plans.first;
      final displayStart = lease.sourceToDisplayOffset(plan.sourceStartUtf16);
      final displayEnd = lease.sourceToDisplayOffset(plan.sourceEndUtf16);
      if (displayStart != displayEnd) {
        throw StateError(
          'Empty delimiter cleanup unexpectedly changes display text.',
        );
      }
      final edit = FlarkV3SourceEdit(
        startUtf16: plan.sourceStartUtf16,
        endUtf16: plan.sourceEndUtf16,
        replacement: '',
      );
      selection = _mapSelectionThroughSourceDeletion(selection, edit);
      composing = _mapRangeThroughSourceDeletion(composing, edit);
      lease = lease._replaceSourceRange(
        sourceStartUtf16: edit.startUtf16,
        sourceEndUtf16: edit.endUtf16,
        displayStartUtf16: displayStart,
        displayEndUtf16: displayEnd,
        replacement: FlarkV3SourceProjectionReplacement.identity(''),
        replacementStyles: const <FlarkV3InlineFactKind>[],
        replacementLinkKind: null,
        continuationAnchor: null,
      );
      edits.add(edit);
    }
    if (lease._delimiterTopology!.planOrphanCleanup().isNotEmpty) {
      throw StateError('Delimiter cleanup exceeded its bounded pair count.');
    }
    return FlarkV3ProjectedInputCleanup._(
      sourceEdits: edits,
      sourceSelection: selection,
      sourceComposing: composing,
      nextLease: lease,
    );
  }

  TextSpan buildTextSpan({
    required TextStyle baseStyle,
    required TextRange composing,
  }) {
    final children = <InlineSpan>[];
    var displayOffset = 0;
    for (final piece in _pieces) {
      if (piece.hidden) continue;
      final end = displayOffset + piece.displayLengthUtf16;
      _appendStyledRange(
        children,
        text: displayText,
        start: displayOffset,
        end: end,
        baseStyle: _semanticStyle(
          baseStyle,
          piece.semanticStyles,
          piece.linkKind,
        ),
        composing: composing,
      );
      displayOffset = end;
    }
    if (displayOffset != displayText.length) {
      throw StateError('Projected presentation does not cover display text.');
    }
    return TextSpan(style: baseStyle, children: children);
  }

  FlarkV3ProjectedInputLease _replaceSourceRange({
    required int sourceStartUtf16,
    required int sourceEndUtf16,
    required int displayStartUtf16,
    required int displayEndUtf16,
    required FlarkV3SourceProjectionReplacement replacement,
    required List<FlarkV3InlineFactKind> replacementStyles,
    required FlarkV3InlineLinkKind? replacementLinkKind,
    required _ProjectedContinuationAnchor? continuationAnchor,
  }) {
    final sourceDelta =
        replacement.sourceReplacement.length -
        (sourceEndUtf16 - sourceStartUtf16);
    final sourceEdit = FlarkV3SourceEdit(
      startUtf16: sourceStartUtf16,
      endUtf16: sourceEndUtf16,
      replacement: replacement.sourceReplacement,
    );
    final nextDelimiterTopology = _delimiterTopology?.afterEnclosingSourceEdit(
      sourceEdit,
    );
    final nextSourceProjection = _sourceProjection.replaceSourceRange(
      sourceStartUtf16: sourceStartUtf16,
      sourceEndUtf16: sourceEndUtf16,
      replacement: replacement,
    );
    final expectedDisplay = displayText.replaceRange(
      displayStartUtf16,
      displayEndUtf16,
      replacement.displayReplacement,
    );
    if (nextSourceProjection.displayText != expectedDisplay) {
      throw StateError(
        'Canonical replacement projection disagrees with the platform delta.',
      );
    }
    final output = <_ProjectedPiece>[];
    var inserted = false;

    void insertReplacement() {
      if (inserted) return;
      inserted = true;
      for (final piece in replacement.pieces) {
        output.add(
          _ProjectedPiece.fromSourcePiece(
            piece.shift(sourceStartUtf16),
            replacementStyles,
            replacementLinkKind,
          ),
        );
      }
    }

    for (final piece in _pieces) {
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

    final normalized = _normalizePieces(output);
    return FlarkV3ProjectedInputLease._(
      sourceProjection: nextSourceProjection,
      pieces: normalized,
      delimiterTopology: nextDelimiterTopology,
      editPolicy: _editPolicy,
      continuationAnchor: continuationAnchor,
    );
  }

  List<FlarkV3InlineFactKind> _stylesAtSourceCaret(int sourceOffsetUtf16) {
    for (final piece in _pieces) {
      if (!piece.hidden &&
          piece.sourceStartUtf16 <= sourceOffsetUtf16 &&
          sourceOffsetUtf16 <= piece.sourceEndUtf16) {
        return piece.semanticStyles;
      }
    }
    return const <FlarkV3InlineFactKind>[];
  }

  int _sourceInsertionOffsetAtDisplayBoundary(
    int displayOffsetUtf16, {
    required TextAffinity affinity,
  }) {
    final upstream = displayToSourceOffset(
      displayOffsetUtf16,
      affinity: FlarkV3InlineProjectionAffinity.upstream,
    );
    final downstream = displayToSourceOffset(
      displayOffsetUtf16,
      affinity: FlarkV3InlineProjectionAffinity.downstream,
    );
    if (upstream == downstream) return downstream;

    final upstreamStyled = _stylesAtSourceCaret(upstream).isNotEmpty;
    final downstreamStyled = _stylesAtSourceCaret(downstream).isNotEmpty;
    if (upstreamStyled != downstreamStyled) {
      // At one edge of a parser-certified styled run, keep ordinary typing in
      // that run. This selects after its hidden opener and before its hidden
      // closer without globally preferring either side of every hidden atom.
      return upstreamStyled ? upstream : downstream;
    }
    return affinity == TextAffinity.upstream ? upstream : downstream;
  }

  FlarkV3InlineLinkKind? _linkKindAtSourceCaret(int sourceOffsetUtf16) {
    for (final piece in _pieces) {
      if (!piece.hidden &&
          piece.sourceStartUtf16 <= sourceOffsetUtf16 &&
          sourceOffsetUtf16 <= piece.sourceEndUtf16) {
        return piece.linkKind;
      }
    }
    return null;
  }

  bool _isSourceSelectionInLease(TextSelection selection) =>
      selection.isValid &&
      selection.start >= sourceStartUtf16 &&
      selection.end <= sourceEndUtf16;

  bool _isStrictlyInsideHiddenPiece(int sourceOffsetUtf16) {
    return _sourceProjection.isStrictlyInsideHiddenPiece(sourceOffsetUtf16);
  }

  void _validateSourceSelection(TextSelection selection) {
    if (!_isSourceSelectionInLease(selection)) {
      throw RangeError('Source selection escapes the projected input lease.');
    }
  }

  void _validateDisplaySelection(TextSelection selection) {
    if (!selection.isValid ||
        selection.start < 0 ||
        selection.end > displayLengthUtf16) {
      throw RangeError('Display selection escapes the projected input lease.');
    }
  }
}

/// Parser-certified inline styling and projected input mapping for one island.
final class FlarkV3FlutterInlinePresentation {
  FlarkV3FlutterInlinePresentation._(this.inputLease);

  factory FlarkV3FlutterInlinePresentation.fromAuthoritative(
    FlarkV3AuthoritativeInlineIslandPresentation authoritative,
  ) {
    final sourceProjection = authoritative.sourceProjection;
    final inlineProjection = authoritative.projection.sourceProjection;
    return FlarkV3FlutterInlinePresentation._(
      identical(sourceProjection, inlineProjection)
          ? FlarkV3ProjectedInputLease.fromAuthoritative(authoritative)
          : FlarkV3ProjectedInputLease.fromSourceProjectionWithAuthoritativeInline(
              sourceProjection,
              authoritative,
            ),
    );
  }

  final FlarkV3ProjectedInputLease inputLease;

  FlarkV3SourceVersion get sourceVersion => inputLease.certifiedSourceVersion!;
  int get islandStartUtf16 => inputLease.sourceStartUtf16;
  int get islandEndUtf16 => inputLease.sourceEndUtf16;
  String get text => inputLease.displayText;
}

final class FlarkV3InlineTextEditingController extends TextEditingController {
  FlarkV3InlineTextEditingController.fromValue(super.value) : super.fromValue();

  FlarkV3ProjectedInputLease? _inputLease;

  FlarkV3ProjectedInputLease? get projectedInputLease => _inputLease;
  bool get hasProjectedPresentation => _inputLease != null;
  bool get hasCertifiedPresentation => _inputLease?.isCertified ?? false;

  void adoptProjectedInputLease(FlarkV3ProjectedInputLease inputLease) {
    if (inputLease.displayText != text) {
      throw StateError(
        'Projected input lease text must equal the active display value.',
      );
    }
    _inputLease = inputLease;
  }

  /// Replaces the platform value and its source/display map as one observable
  /// controller transition.
  void adoptProjectedEditingValue(
    TextEditingValue value,
    FlarkV3ProjectedInputLease inputLease,
  ) {
    if (inputLease.displayText != value.text) {
      throw StateError(
        'Projected input lease text must equal the adopted display value.',
      );
    }
    _inputLease = inputLease;
    this.value = value;
  }

  void markProjectedInputLeaseProvisional() {
    final current = _inputLease;
    if (current != null) _inputLease = current.asProvisional();
  }

  void clearProjectedInputLease() {
    _inputLease = null;
  }

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final lease = _inputLease;
    if (lease == null || lease.displayText != text) {
      return super.buildTextSpan(
        context: context,
        style: style,
        withComposing: withComposing,
      );
    }
    final composing = withComposing && value.isComposingRangeValid
        ? value.composing
        : TextRange.empty;
    return lease.buildTextSpan(
      baseStyle: style ?? const TextStyle(),
      composing: composing,
    );
  }
}

final class _ProjectedPiece {
  const _ProjectedPiece._({
    required this.sourcePiece,
    required this.semanticStyles,
    required this.linkKind,
  });

  factory _ProjectedPiece.fromSourcePiece(
    FlarkV3SourceProjectionPiece sourcePiece,
    List<FlarkV3InlineFactKind> styles,
    FlarkV3InlineLinkKind? linkKind,
  ) => _ProjectedPiece._(
    sourcePiece: sourcePiece,
    semanticStyles: sourcePiece.isHidden
        ? const <FlarkV3InlineFactKind>[]
        : List<FlarkV3InlineFactKind>.unmodifiable(styles),
    linkKind: sourcePiece.isHidden ? null : linkKind,
  );

  final FlarkV3SourceProjectionPiece sourcePiece;
  final List<FlarkV3InlineFactKind> semanticStyles;
  final FlarkV3InlineLinkKind? linkKind;

  int get sourceStartUtf16 => sourcePiece.sourceStartUtf16;
  int get sourceEndUtf16 => sourcePiece.sourceEndUtf16;
  int get sourceLengthUtf16 => sourcePiece.sourceLengthUtf16;
  int get displayLengthUtf16 => sourcePiece.displayLengthUtf16;
  bool get hidden => sourcePiece.isHidden;

  _ProjectedPiece slice(int start, int end) => _ProjectedPiece._(
    sourcePiece: sourcePiece.slice(start, end),
    semanticStyles: semanticStyles,
    linkKind: linkKind,
  );

  _ProjectedPiece shift(int delta) => _ProjectedPiece._(
    sourcePiece: sourcePiece.shift(delta),
    semanticStyles: semanticStyles,
    linkKind: linkKind,
  );

  bool samePresentation(_ProjectedPiece other) =>
      sourcePiece.kind == other.sourcePiece.kind &&
      !sourcePiece.isReplaced &&
      _sameSemanticStyles(semanticStyles, other.semanticStyles) &&
      linkKind == other.linkKind;
}

final class _ProjectedContinuationAnchor {
  _ProjectedContinuationAnchor({
    required this.sourceOffsetUtf16,
    required this.displayOffsetUtf16,
    required List<FlarkV3InlineFactKind> semanticStyles,
    required this.linkKind,
  }) : semanticStyles = List<FlarkV3InlineFactKind>.unmodifiable(
         semanticStyles,
       );

  final int sourceOffsetUtf16;
  final int displayOffsetUtf16;
  final List<FlarkV3InlineFactKind> semanticStyles;
  final FlarkV3InlineLinkKind? linkKind;
}

List<_ProjectedPiece> _projectedPiecesFromInlineProjection(
  FlarkV3InlineProjection projection,
) {
  final sourcePieces = projection.sourceProjection.pieces;
  final output = <_ProjectedPiece>[];
  var sourcePieceIndex = 0;

  void appendRange(
    int startUtf16,
    int endUtf16, {
    required bool hidden,
    required List<FlarkV3InlineFactKind> semanticStyles,
    required FlarkV3InlineLinkKind? linkKind,
  }) {
    var cursor = startUtf16;
    while (cursor < endUtf16) {
      while (sourcePieceIndex < sourcePieces.length &&
          sourcePieces[sourcePieceIndex].sourceEndUtf16 <= cursor) {
        sourcePieceIndex += 1;
      }
      if (sourcePieceIndex >= sourcePieces.length) {
        throw StateError(
          'Inline presentation escaped its authoritative source projection.',
        );
      }
      final sourcePiece = sourcePieces[sourcePieceIndex];
      if (sourcePiece.sourceStartUtf16 > cursor ||
          sourcePiece.isHidden != hidden) {
        throw StateError(
          'Inline runs disagree with their authoritative projection pieces.',
        );
      }
      final partEnd = sourcePiece.sourceEndUtf16 < endUtf16
          ? sourcePiece.sourceEndUtf16
          : endUtf16;
      final part = sourcePiece.slice(cursor, partEnd);
      output.add(
        _ProjectedPiece.fromSourcePiece(part, semanticStyles, linkKind),
      );
      cursor = partEnd;
    }
  }

  var sourceCursor = projection.sourceStartUtf16;
  for (final run in projection.runs) {
    if (sourceCursor < run.sourceStartUtf16) {
      appendRange(
        sourceCursor,
        run.sourceStartUtf16,
        hidden: true,
        semanticStyles: const <FlarkV3InlineFactKind>[],
        linkKind: null,
      );
    }
    appendRange(
      run.sourceStartUtf16,
      run.sourceEndUtf16,
      hidden: false,
      semanticStyles: run.semanticStyles,
      linkKind: run.linkAnnotation?.kind,
    );
    sourceCursor = run.sourceEndUtf16;
  }
  if (sourceCursor < projection.sourceEndUtf16) {
    appendRange(
      sourceCursor,
      projection.sourceEndUtf16,
      hidden: true,
      semanticStyles: const <FlarkV3InlineFactKind>[],
      linkKind: null,
    );
    sourceCursor = projection.sourceEndUtf16;
  }
  if (sourceCursor != projection.sourceEndUtf16 ||
      projection.sourceLengthUtf16 != 0 && output.isEmpty) {
    throw StateError(
      'Inline presentation does not exhaust its authoritative projection.',
    );
  }
  return _normalizePieces(output);
}

List<_ProjectedPiece> _composeProjectedInlinePieces(
  FlarkV3SourceProjection outer,
  FlarkV3ProjectedInlineProjection inner,
) {
  final output = <_ProjectedPiece>[];
  var projectedCursor = 0;
  var innerPieceIndex = 0;
  var runIndex = 0;

  List<FlarkV3InlineFactKind> stylesFor(int start, int end) {
    while (runIndex < inner.runs.length &&
        inner.runs[runIndex].projectedEndUtf16 <= start) {
      runIndex += 1;
    }
    if (runIndex >= inner.runs.length) {
      throw StateError(
        'Visible projected-inline piece lacks a certified display run.',
      );
    }
    final run = inner.runs[runIndex];
    if (run.projectedStartUtf16 > start || run.projectedEndUtf16 < end) {
      throw StateError(
        'Projected-inline runs disagree with exhaustive projection pieces.',
      );
    }
    return run.semanticStyles;
  }

  for (final outerPiece in outer.pieces) {
    if (outerPiece.isHidden) {
      output.add(
        _ProjectedPiece.fromSourcePiece(
          outerPiece,
          const <FlarkV3InlineFactKind>[],
          null,
        ),
      );
      continue;
    }
    if (!outerPiece.isCopied) {
      throw StateError(
        'Projected-inline composition requires source-backed container text.',
      );
    }

    final outerProjectedStart = projectedCursor;
    final outerProjectedEnd =
        outerProjectedStart + outerPiece.displayLengthUtf16;
    while (projectedCursor < outerProjectedEnd) {
      while (innerPieceIndex < inner.pieces.length &&
          inner.pieces[innerPieceIndex].projectedEndUtf16 <= projectedCursor) {
        innerPieceIndex += 1;
      }
      if (innerPieceIndex >= inner.pieces.length) {
        throw StateError(
          'Projected-inline pieces do not exhaust their container projection.',
        );
      }
      final innerPiece = inner.pieces[innerPieceIndex];
      if (innerPiece.projectedStartUtf16 > projectedCursor) {
        throw StateError(
          'Projected-inline pieces leave a gap in container coordinates.',
        );
      }
      final projectedEnd = innerPiece.projectedEndUtf16 < outerProjectedEnd
          ? innerPiece.projectedEndUtf16
          : outerProjectedEnd;
      final physicalStart =
          outerPiece.sourceStartUtf16 + (projectedCursor - outerProjectedStart);
      final physicalEnd =
          outerPiece.sourceStartUtf16 + (projectedEnd - outerProjectedStart);
      late final FlarkV3SourceProjectionPiece composedSourcePiece;
      switch (innerPiece.kind) {
        case FlarkV3ProjectedInlineProjectionPieceKind.copy:
          composedSourcePiece = FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: physicalStart,
            sourceEndUtf16: physicalEnd,
          );
        case FlarkV3ProjectedInlineProjectionPieceKind.hide:
          composedSourcePiece = FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: physicalStart,
            sourceEndUtf16: physicalEnd,
          );
        case FlarkV3ProjectedInlineProjectionPieceKind.replace:
          if (projectedCursor != innerPiece.projectedStartUtf16 ||
              projectedEnd != innerPiece.projectedEndUtf16) {
            throw StateError(
              'A projected-inline replacement crosses a hidden container gap.',
            );
          }
          composedSourcePiece = FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: physicalStart,
            sourceEndUtf16: physicalEnd,
            displayText: innerPiece.displayText,
          );
      }
      final styles =
          innerPiece.kind == FlarkV3ProjectedInlineProjectionPieceKind.hide
          ? const <FlarkV3InlineFactKind>[]
          : stylesFor(projectedCursor, projectedEnd);
      output.add(
        _ProjectedPiece.fromSourcePiece(composedSourcePiece, styles, null),
      );
      projectedCursor = projectedEnd;
    }
  }

  if (projectedCursor != inner.projectedLengthUtf16) {
    throw StateError(
      'Container projection does not exhaust projected-inline coordinates.',
    );
  }
  return _normalizePieces(output);
}

List<_ProjectedPiece> _normalizePieces(List<_ProjectedPiece> pieces) {
  final output = <_ProjectedPiece>[];
  for (final piece in pieces) {
    if (piece.sourceLengthUtf16 == 0) continue;
    if (output.isNotEmpty &&
        output.last.sourceEndUtf16 == piece.sourceStartUtf16 &&
        output.last.samePresentation(piece)) {
      final previous = output.removeLast();
      output.add(
        _ProjectedPiece._(
          sourcePiece: previous.sourcePiece.slice(
            previous.sourceStartUtf16,
            piece.sourceEndUtf16,
          ),
          semanticStyles: previous.semanticStyles,
          linkKind: previous.linkKind,
        ),
      );
    } else {
      output.add(piece);
    }
  }
  return output;
}

void _validatePresentationPieces({
  required FlarkV3SourceProjection sourceProjection,
  required List<_ProjectedPiece> pieces,
}) {
  var sourceCursor = sourceProjection.sourceStartUtf16;
  final display = StringBuffer();
  for (final piece in pieces) {
    if (piece.sourceStartUtf16 != sourceCursor ||
        piece.sourceEndUtf16 <= piece.sourceStartUtf16 ||
        piece.sourceEndUtf16 > sourceProjection.sourceEndUtf16) {
      throw StateError(
        'Projected presentation pieces must exhaustively cover source.',
      );
    }
    switch (piece.sourcePiece.kind) {
      case FlarkV3SourceProjectionPieceKind.copy:
        display.write(
          sourceProjection.sourceText.substring(
            piece.sourceStartUtf16 - sourceProjection.sourceStartUtf16,
            piece.sourceEndUtf16 - sourceProjection.sourceStartUtf16,
          ),
        );
      case FlarkV3SourceProjectionPieceKind.hide:
        break;
      case FlarkV3SourceProjectionPieceKind.replace:
        display.write(piece.sourcePiece.displayText);
    }
    sourceCursor = piece.sourceEndUtf16;
  }
  if (sourceCursor != sourceProjection.sourceEndUtf16 ||
      (sourceProjection.sourceLengthUtf16 != 0 && pieces.isEmpty) ||
      display.toString() != sourceProjection.displayText) {
    throw StateError(
      'Projected presentation does not match its exact source projection.',
    );
  }
}

void _appendStyledRange(
  List<InlineSpan> output, {
  required String text,
  required int start,
  required int end,
  required TextStyle baseStyle,
  required TextRange composing,
}) {
  if (start == end) return;
  final composingStart = composing.isValid
      ? composing.start.clamp(start, end)
      : start;
  final composingEnd = composing.isValid
      ? composing.end.clamp(start, end)
      : start;
  if (start < composingStart) {
    output.add(
      TextSpan(text: text.substring(start, composingStart), style: baseStyle),
    );
  }
  if (composingStart < composingEnd) {
    output.add(
      TextSpan(
        text: text.substring(composingStart, composingEnd),
        style: _withDecoration(baseStyle, TextDecoration.underline),
      ),
    );
  }
  final trailingStart = composingEnd > start ? composingEnd : start;
  if (trailingStart < end) {
    output.add(
      TextSpan(text: text.substring(trailingStart, end), style: baseStyle),
    );
  }
}

TextStyle _semanticStyle(
  TextStyle base,
  List<FlarkV3InlineFactKind> semanticStyles,
  FlarkV3InlineLinkKind? linkKind,
) {
  var style = base;
  for (final semantic in semanticStyles) {
    style = switch (semantic) {
      FlarkV3InlineFactKind.emphasis => style.copyWith(
        fontStyle: FontStyle.italic,
      ),
      FlarkV3InlineFactKind.strong => style.copyWith(
        fontWeight: FontWeight.w700,
      ),
      FlarkV3InlineFactKind.code => style.copyWith(
        fontFamily: 'monospace',
        backgroundColor: const Color(0x12000000),
      ),
      FlarkV3InlineFactKind.strikethrough => _withDecoration(
        style,
        TextDecoration.lineThrough,
      ),
      FlarkV3InlineFactKind.escapedPunctuation ||
      FlarkV3InlineFactKind.hardLineBreak ||
      FlarkV3InlineFactKind.characterReference ||
      FlarkV3InlineFactKind.directImage ||
      FlarkV3InlineFactKind.referenceImage => style,
      FlarkV3InlineFactKind.autolinkUri ||
      FlarkV3InlineFactKind.autolinkEmail ||
      FlarkV3InlineFactKind.directLink ||
      FlarkV3InlineFactKind.referenceLink => style,
    };
  }
  if (linkKind != null) {
    style = _withLinkStyle(style);
  }
  return style;
}

TextStyle _withLinkStyle(TextStyle style) =>
    _withDecoration(style, TextDecoration.underline);

TextStyle _withDecoration(TextStyle style, TextDecoration decoration) {
  final existing = style.decoration;
  return style.copyWith(
    decoration: existing == null
        ? decoration
        : TextDecoration.combine([existing, decoration]),
  );
}

bool _sameSemanticStyles(
  List<FlarkV3InlineFactKind> left,
  List<FlarkV3InlineFactKind> right,
) {
  if (left.length != right.length) return false;
  for (var index = 0; index < left.length; index += 1) {
    if (left[index] != right[index]) return false;
  }
  return true;
}

TextSelection _mapSelectionThroughSourceDeletion(
  TextSelection selection,
  FlarkV3SourceEdit edit,
) => TextSelection(
  baseOffset: _mapOffsetThroughSourceDeletion(selection.baseOffset, edit),
  extentOffset: _mapOffsetThroughSourceDeletion(selection.extentOffset, edit),
  affinity: selection.affinity,
  isDirectional: selection.isDirectional,
);

TextRange _mapRangeThroughSourceDeletion(
  TextRange range,
  FlarkV3SourceEdit edit,
) {
  if (!range.isValid) return TextRange.empty;
  return TextRange(
    start: _mapOffsetThroughSourceDeletion(range.start, edit),
    end: _mapOffsetThroughSourceDeletion(range.end, edit),
  );
}

int _mapOffsetThroughSourceDeletion(int offset, FlarkV3SourceEdit edit) {
  if (edit.replacement.isNotEmpty) {
    throw StateError('Delimiter cleanup must be a source-only deletion.');
  }
  if (offset <= edit.startUtf16) return offset;
  if (offset >= edit.endUtf16) {
    return offset - (edit.endUtf16 - edit.startUtf16);
  }
  return edit.startUtf16;
}
