import 'package:flutter/services.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/syntax_parse_scheduler.dart';

import '../../engine/syntax_engine.dart';
import '../../engine/syntax_snapshot.dart';
import '../../engine/syntax_types.dart';
import '../../logic/fenced_code_scanner.dart';
import '../../logic/sovereign_style_scanner.dart';
import '../../models/block_node.dart';
import '../../models/block_tree.dart';
import '../../models/decoration_model.dart';
import '../../models/edit_op.dart';
import '../../models/line_index.dart';
import 'predictive_decoration_reconciler.dart';

abstract class SovereignSyntaxSyncHost {
  TextEditingValue get value;
  String get text;

  SyntaxParseScheduler? get syntaxParseScheduler;
  MarkdownSyntaxProfile get markdownProfile;
  bool get isDecorationClosed;
  int get revision;
  SyntaxEngine get syntaxEngine;
  int get predictiveScanTimeBudgetMicros;
  int get predictiveScanSpanBudget;
  int? get predictiveScanCharLimitOverride;
  int get predictiveBudgetExhaustionCount;
  set predictiveBudgetExhaustionCount(int value);
  int get predictiveLocalFallbackCount;
  set predictiveLocalFallbackCount(int value);

  SyntaxSnapshot? get latestAuthoritativeSnapshot;
  set latestAuthoritativeSnapshot(SyntaxSnapshot? value);
  List<StyleRun>? get authoritativeInlineRuns;
  set authoritativeInlineRuns(List<StyleRun>? value);
  int get authoritativeInlineRunsRevision;
  set authoritativeInlineRunsRevision(int value);
  List<TextRange> get authoritativeHiddenRanges;
  set authoritativeHiddenRanges(List<TextRange> value);
  List<TextRange> get authoritativeExclusionRanges;
  set authoritativeExclusionRanges(List<TextRange> value);
  CursorValidationMask get authoritativeCursorMask;
  set authoritativeCursorMask(CursorValidationMask value);

  List<TextRange> get projectedHiddenRanges;
  set projectedHiddenRanges(List<TextRange> value);
  List<TextRange> get projectedExclusionRanges;
  set projectedExclusionRanges(List<TextRange> value);
  CursorValidationMask get projectedCursorMask;
  set projectedCursorMask(CursorValidationMask value);
  CursorValidationMask get activeCursorMask;
  set activeCursorMask(CursorValidationMask value);

  DecorationModel get latestDecoration;
  LineIndex get lineIndex;
  void publishDecoration(DecorationModel decoration);

  CursorValidationMask normalizeCursorMaskToText(
    CursorValidationMask mask, {
    required int textLength,
    List<TextRange> fallbackHiddenRanges,
  });
  List<TextRange> shiftAndVerifyRanges(
    List<TextRange> oldRanges,
    EditOp op,
    String newText,
  );
  List<TextRange> scanInlineMarkersNearEdit({
    required String targetText,
    required int editStart,
    required int editEnd,
    required List<TextRange> excludedRanges,
  });

  List<StyleRun> styleRunsFromInlineTokens(
    List<InlineSpanToken> tokens,
    int textLength,
  );
  List<TextRange> normalizeHiddenRanges(List<TextRange> ranges, int textLength);
  int rangeKey(TextRange range);
  List<TextRange> overlayCanonicalBlockMarkerRanges(
    String text,
    List<TextRange> ranges,
  );
  BlockTree blockTreeFromSnapshot(SyntaxSnapshot snapshot);
  List<TextRange> stabilizeRangesInAmbiguity({
    required List<TextRange> predictedRanges,
    required List<TextRange> authoritativeRanges,
    required List<TextRange> ambiguityZones,
    required int textLength,
  });
  int lineStartForOffset(String text, int offset);
  List<TextRange> fencedCodeFenceMarkers(
    String text,
    List<FencedCodeBlock> blocks,
  );
  List<TextRange> markdownBlockMarkerRanges(
    String text, {
    required List<FencedCodeBlock> fencedBlocks,
  });
  int headerMarkerLength(String text, int lineStart, int lineEnd);
  int blockquoteMarkerLength(String text, int lineStart, int lineEnd);
  int unorderedListMarkerLength(String text, int lineStart, int lineEnd);
  int orderedListMarkerLength(String text, int lineStart, int lineEnd);
  bool isFenceLineTrailingRange(String text, TextRange range);
  bool isFenceMarkerRange(String text, TextRange range);
  String? inlineMarkerToken(String text, TextRange range);
  List<TextRange> selectionCenteredEmptyInlineRanges(TextEditingValue value);
}

/// Owns parse scheduling, authoritative snapshot adoption, and projection
/// emission. The controller remains the public API / text mutation surface.
class SovereignSyntaxSyncCoordinator {
  SovereignSyntaxSyncCoordinator(this._host);

  final SovereignSyntaxSyncHost _host;
  late final SovereignPredictiveDecorationReconciler _predictiveReconciler =
      SovereignPredictiveDecorationReconciler(_host);

  void scheduleParse(
    String text,
    int revision, {
    TextEditingValue? currentValue,
  }) {
    final composingValue = currentValue ?? _host.value;
    if (composingValue.composing.isValid) {
      return;
    }

    _host.syntaxParseScheduler?.schedule(
      SyntaxParseRequest(
        revision: revision,
        text: text,
        profile: _host.markdownProfile,
      ),
    );
  }

  void handleSyntaxSnapshot(SyntaxSnapshot snapshot) {
    if (_host.isDecorationClosed) return;
    if (snapshot.revision != _host.revision) return;

    _host.latestAuthoritativeSnapshot = snapshot;
    _host.authoritativeInlineRuns = _host.styleRunsFromInlineTokens(
      snapshot.inlineTokens,
      _host.value.text.length,
    );
    _host.authoritativeInlineRunsRevision = snapshot.revision;
    _host.authoritativeHiddenRanges = _host.overlayCanonicalBlockMarkerRanges(
      _host.value.text,
      snapshot.markerRanges,
    );
    _host.authoritativeExclusionRanges = _host.normalizeHiddenRanges(
      snapshot.exclusionRanges,
      _host.value.text.length,
    );
    _host.authoritativeCursorMask = _host.normalizeCursorMaskToText(
      snapshot.cursorMask,
      textLength: _host.value.text.length,
      fallbackHiddenRanges: _host.authoritativeHiddenRanges,
    );
    _host.projectedHiddenRanges = _host.authoritativeHiddenRanges;
    _host.projectedExclusionRanges = _host.authoritativeExclusionRanges;
    _host.projectedCursorMask = _host.authoritativeCursorMask;
    updateProjection(
      newTree: _host.blockTreeFromSnapshot(snapshot),
      treeIsAuthoritative: true,
    );
  }

  void emitDecoration({
    required BlockTree tree,
    TextEditingValue? overrideValue,
    EditOp? op,
  }) {
    if (_host.isDecorationClosed) return;

    final targetText = overrideValue?.text ?? _host.text;
    if (targetText.isEmpty) {
      final emptyMask = PassthroughCursorValidationMask(textLength: 0);
      _host.latestAuthoritativeSnapshot = null;
      _host.authoritativeInlineRuns = null;
      _host.authoritativeInlineRunsRevision = -1;
      _host.authoritativeHiddenRanges = const <TextRange>[];
      _host.authoritativeExclusionRanges = const <TextRange>[];
      _host.authoritativeCursorMask = emptyMask;
      _host.projectedHiddenRanges = const <TextRange>[];
      _host.projectedExclusionRanges = const <TextRange>[];
      _host.projectedCursorMask = emptyMask;
      updateProjection(
        newTree: BlockTree.empty(),
        treeIsAuthoritative: true,
        overrideValue: overrideValue,
      );
      return;
    }

    if (op != null && overrideValue != null) {
      final projection = _predictiveReconciler.reconcile(
        targetText: targetText,
        op: op,
      );
      _host.projectedHiddenRanges = projection.hiddenRanges;
      _host.projectedExclusionRanges = projection.exclusionRanges;
      _host.projectedCursorMask = projection.cursorMask;

      final suppressPop =
          op.replacedRange.start == 0 || op.insertedText.contains('\n');
      updateProjection(overrideValue: overrideValue, suppressPop: suppressPop);
      return;
    }

    if (overrideValue != null) {
      final prediction = _host.syntaxEngine.predict(
        SyntaxPredictRequest(
          revision: _host.revision,
          text: targetText,
          profile: _host.markdownProfile,
          previousSnapshot: _host.latestAuthoritativeSnapshot,
          timeBudgetMicros: _host.predictiveScanTimeBudgetMicros,
          spanBudget: _host.predictiveScanSpanBudget,
          charLimit: _host.predictiveScanCharLimitOverride,
        ),
      );
      final ambiguityZones = _host.normalizeHiddenRanges(
        prediction.ambiguityZones,
        targetText.length,
      );
      final predictedExclusions = _host.normalizeHiddenRanges(
        prediction.exclusionRanges,
        targetText.length,
      );
      _host.projectedExclusionRanges = _host.stabilizeRangesInAmbiguity(
        predictedRanges: predictedExclusions,
        authoritativeRanges: _host.authoritativeExclusionRanges,
        ambiguityZones: ambiguityZones,
        textLength: targetText.length,
      );
      _host.projectedHiddenRanges = _host.overlayCanonicalBlockMarkerRanges(
        targetText,
        _host.stabilizeRangesInAmbiguity(
          predictedRanges: _host.normalizeHiddenRanges(
            prediction.markerRanges,
            targetText.length,
          ),
          authoritativeRanges: _host.authoritativeHiddenRanges,
          ambiguityZones: ambiguityZones,
          textLength: targetText.length,
        ),
      );
      _host.projectedCursorMask = _host.normalizeCursorMaskToText(
        prediction.cursorMask,
        textLength: targetText.length,
        fallbackHiddenRanges: _host.projectedHiddenRanges,
      );
      updateProjection(overrideValue: overrideValue);
      return;
    }

    final excludedRanges = <TextRange>[];
    final blockHiddenRanges = <TextRange>[];

    if (overrideValue == null) {
      final fencedBlocks = <FencedCodeBlock>[];
      for (final block in tree.blocks) {
        if (block.type == BlockType.fencedCode) {
          excludedRanges.add(TextRange(start: block.start, end: block.end));
          fencedBlocks.add(FencedCodeBlock(block.start, block.end));
        }
      }
      blockHiddenRanges.addAll(
        _host.fencedCodeFenceMarkers(targetText, fencedBlocks),
      );
      blockHiddenRanges.addAll(
        _host.markdownBlockMarkerRanges(targetText, fencedBlocks: fencedBlocks),
      );
    } else {
      final fencedBlocks = FencedCodeScanner.scan(targetText);
      excludedRanges.addAll(
        fencedBlocks.map(
          (block) => TextRange(start: block.start, end: block.end),
        ),
      );
      blockHiddenRanges.addAll(
        _host.fencedCodeFenceMarkers(targetText, fencedBlocks),
      );
      blockHiddenRanges.addAll(
        _host.markdownBlockMarkerRanges(targetText, fencedBlocks: fencedBlocks),
      );
    }

    final scanResult = SovereignStyleScanner.scan(
      targetText,
      excludedRanges: excludedRanges,
    );
    final inlineHiddenRanges = SovereignStyleScanner.extractHiddenRanges(
      targetText,
      scanResult.runs,
    );

    _host.projectedHiddenRanges = _host.overlayCanonicalBlockMarkerRanges(
      targetText,
      [...blockHiddenRanges, ...inlineHiddenRanges],
    );
    _host.projectedExclusionRanges = _host.normalizeHiddenRanges(
      excludedRanges,
      targetText.length,
    );
    _host.projectedCursorMask = HiddenRangeCursorValidationMask(
      textLength: targetText.length,
      hiddenRanges: _host.projectedHiddenRanges,
    );
    if (overrideValue == null) {
      _host.authoritativeHiddenRanges = _host.projectedHiddenRanges;
      _host.authoritativeExclusionRanges = _host.projectedExclusionRanges;
      _host.authoritativeCursorMask = _host.projectedCursorMask;
    }

    if (overrideValue == null) {
      updateProjection(newTree: tree, treeIsAuthoritative: true);
    } else {
      updateProjection(overrideValue: overrideValue);
    }
  }

  void updateProjection({
    BlockTree? newTree,
    bool treeIsAuthoritative = false,
    TextEditingValue? overrideValue,
    bool suppressPop = false,
  }) {
    if (_host.isDecorationClosed) return;

    final currentVal = overrideValue ?? _host.value;
    final selection = currentVal.selection;
    final activeHiddenRanges = <TextRange>[];
    final textLen = currentVal.text.length;

    final selStart = selection.isValid ? selection.start : 0;
    final selEnd = selection.isValid ? selection.end : 0;

    final candidateRanges = <TextRange>[
      ..._host.projectedHiddenRanges,
      ..._host.selectionCenteredEmptyInlineRanges(currentVal),
    ]..sort((a, b) {
        final byStart = a.start.compareTo(b.start);
        if (byStart != 0) return byStart;
        return a.end.compareTo(b.end);
      });
    final sourceRanges = <TextRange>[];
    for (final range in candidateRanges) {
      if (sourceRanges.isNotEmpty &&
          sourceRanges.last.start == range.start &&
          sourceRanges.last.end == range.end) {
        continue;
      }
      sourceRanges.add(range);
    }

    final inlineTokenParity = <String, bool>{};

    for (final range in sourceRanges) {
      final inlineToken = _host.inlineMarkerToken(currentVal.text, range);
      final isInlineClosingMarker =
          inlineToken != null && (inlineTokenParity[inlineToken] ?? false);
      if (inlineToken != null) {
        inlineTokenParity[inlineToken] = !isInlineClosingMarker;
      }

      if (suppressPop) {
        activeHiddenRanges.add(range);
        continue;
      }

      if (selection.isValid &&
          selection.isCollapsed &&
          selection.start == range.end &&
          _host.isFenceLineTrailingRange(currentVal.text, range)) {
        continue;
      }

      var keepHiddenAtBoundary = false;
      if (selection.isValid) {
        final caret = selection.start;
        if (caret >= range.start && caret < range.end) {
          if (selection.isCollapsed &&
              caret == range.start &&
              (_host.isFenceMarkerRange(currentVal.text, range) ||
                  isInlineClosingMarker)) {
            keepHiddenAtBoundary = true;
          } else {
            continue;
          }
        }
      }
      if (keepHiddenAtBoundary) {
        activeHiddenRanges.add(range);
        continue;
      }

      final zoneStart = range.start;
      final zoneEnd = (range.end - 1).clamp(0, textLen);

      var intersects = false;
      if (selection.isValid) {
        if (selection.isCollapsed) {
          if (isInlineClosingMarker) {
            intersects = (selStart > zoneStart && selStart <= zoneEnd);
          } else {
            intersects = (selStart <= zoneEnd && selEnd >= zoneStart);
          }
        } else {
          intersects = (selStart < range.end && selEnd > range.start);
        }
      }

      if (!intersects) {
        activeHiddenRanges.add(range);
      }
    }

    final hasAuthoritativeTree = treeIsAuthoritative && newTree != null;
    final tree = hasAuthoritativeTree ? newTree : _host.latestDecoration.tree;

    final epoch = _host.latestDecoration.projectionEpoch + 1;
    final originRevision = hasAuthoritativeTree
        ? _host.revision
        : _host.latestDecoration.originRevision;

    final normalizedActiveHiddenRanges = _host.normalizeHiddenRanges(
      activeHiddenRanges,
      textLen,
    );

    final cursorSafetyRanges = originRevision == _host.revision
        ? normalizedActiveHiddenRanges
        : _host.projectedHiddenRanges;
    final sourceMask = originRevision == _host.revision
        ? _host.authoritativeCursorMask
        : _host.projectedCursorMask;
    _host.activeCursorMask = _host.normalizeCursorMaskToText(
      sourceMask,
      textLength: textLen,
      fallbackHiddenRanges: cursorSafetyRanges,
    );

    final nextDecoration = DecorationModel(
      tree: tree,
      lineIndex: _host.lineIndex,
      originRevision: originRevision,
      hiddenRanges: normalizedActiveHiddenRanges,
      projectionEpoch: epoch,
    );
    _host.publishDecoration(nextDecoration);
  }
}
