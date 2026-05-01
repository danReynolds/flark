part of 'sovereign_controller.dart';

extension SovereignControllerDiagnostics on SovereignController {
  @visibleForTesting
  void setPredictiveScanOverridesForTesting({
    int? timeBudgetMicros,
    int? spanBudget,
    int? charLimit,
  }) {
    if (timeBudgetMicros != null) {
      _predictiveScanTimeBudgetMicros = timeBudgetMicros;
    }
    if (spanBudget != null) {
      _predictiveScanSpanBudget = spanBudget;
    }
    _predictiveScanCharLimitOverride = charLimit;
  }

  @visibleForTesting
  void clearPredictiveScanOverridesForTesting() {
    _predictiveScanTimeBudgetMicros = SovereignStyleScanner.kTimeBudgetMicros;
    _predictiveScanSpanBudget = SovereignStyleScanner.kSpanBudget;
    _predictiveScanCharLimitOverride = null;
  }

  @visibleForTesting
  int get predictiveBudgetExhaustionCount => _predictiveBudgetExhaustionCount;

  @visibleForTesting
  int get predictiveLocalFallbackCount => _predictiveLocalFallbackCount;

  @visibleForTesting
  int get predictiveLocalFallbackLastScannedChars =>
      _predictiveLocalFallbackLastScannedChars;

  @visibleForTesting
  int get predictiveLocalInlineScanCharCap =>
      PredictiveEditRangeUtils.defaultLocalInlineScanCharCap;

  @visibleForTesting
  void resetPredictiveTelemetryForTesting() {
    _predictiveBudgetExhaustionCount = 0;
    _predictiveLocalFallbackCount = 0;
    _predictiveLocalFallbackLastScannedChars = 0;
  }

  @visibleForTesting
  int get parsePendingReplaceCount =>
      _syntaxParseScheduler?.pendingReplaceCount ?? 0;

  @visibleForTesting
  int get parseStaleDropCount => _syntaxParseScheduler?.staleDropCount ?? 0;

  @visibleForTesting
  MarkdownSyntaxProfile get markdownProfile => _markdownProfile;

  @visibleForTesting
  void resetParseTelemetryForTesting() =>
      _syntaxParseScheduler?.resetCounters();

  @visibleForTesting
  int get renderCallCount => _renderer.renderCallCount;

  @visibleForTesting
  int get renderLastMicros => _renderer.renderLastMicros;

  @visibleForTesting
  int get renderMaxMicros => _renderer.renderMaxMicros;

  @visibleForTesting
  void resetRenderTelemetryForTesting() => _renderer.resetRenderTelemetry();

  @visibleForTesting
  EditorSessionState get sessionState => EditorSessionStateBuilder.build(
        value: value,
        revision: _revision,
        lineIndex: _lineIndex,
        geometry: _geometry,
        projectedHiddenRanges: _projectedHiddenRanges,
        projectedExclusionRanges: _projectedExclusionRanges,
        authoritativeHiddenRanges: _authoritativeHiddenRanges,
        authoritativeExclusionRanges: _authoritativeExclusionRanges,
        projectedCursorMask: _projectedCursorMask,
        authoritativeCursorMask: _authoritativeCursorMask,
        activeCursorMask: _activeCursorMask,
        latestAuthoritativeSnapshot: _latestAuthoritativeSnapshot,
        lastOp: _lastOp,
        lastOpTime: _lastOpTime,
        currentUndoGroup: _currentUndoGroup,
        undoBoundaryDepth: _undoBoundaryDepth,
        commandTransactionDepth: _commandTransactionDepth,
        commandTransactionUndoGroupId: _commandTransactionUndoGroupId,
        forceUndoBoundaryForNextTextOp: _forceUndoBoundaryForNextTextOp,
        compositionStartValue: _compositionStartValue,
        canUndo: _undoStack.canUndo,
        canRedo: _undoStack.canRedo,
        predictiveBudgetExhaustionCount: _predictiveBudgetExhaustionCount,
        predictiveLocalFallbackCount: _predictiveLocalFallbackCount,
        predictiveLocalFallbackLastScannedChars:
            _predictiveLocalFallbackLastScannedChars,
        parsePendingReplaceCount: parsePendingReplaceCount,
        parseStaleDropCount: parseStaleDropCount,
        renderCallCount: renderCallCount,
        renderLastMicros: renderLastMicros,
        renderMaxMicros: renderMaxMicros,
      );
}
