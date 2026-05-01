import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';
import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import 'document_state.dart';
import 'editor_session_state.dart';
import 'history_state.dart';
import 'projection_state.dart';
import 'telemetry_state.dart';

class EditorSessionStateBuilder {
  const EditorSessionStateBuilder._();

  static EditorSessionState build({
    required TextEditingValue value,
    required int revision,
    required LineIndex lineIndex,
    required GeometryModel geometry,
    required List<TextRange> projectedHiddenRanges,
    required List<TextRange> projectedExclusionRanges,
    required List<TextRange> authoritativeHiddenRanges,
    required List<TextRange> authoritativeExclusionRanges,
    required CursorValidationMask projectedCursorMask,
    required CursorValidationMask authoritativeCursorMask,
    required CursorValidationMask activeCursorMask,
    required SyntaxSnapshot? latestAuthoritativeSnapshot,
    required EditOp? lastOp,
    required DateTime? lastOpTime,
    required int currentUndoGroup,
    required int undoBoundaryDepth,
    required int commandTransactionDepth,
    required int? commandTransactionUndoGroupId,
    required bool forceUndoBoundaryForNextTextOp,
    required TextEditingValue? compositionStartValue,
    required bool canUndo,
    required bool canRedo,
    required int predictiveBudgetExhaustionCount,
    required int predictiveLocalFallbackCount,
    required int predictiveLocalFallbackLastScannedChars,
    required int parsePendingReplaceCount,
    required int parseStaleDropCount,
    required int renderCallCount,
    required int renderLastMicros,
    required int renderMaxMicros,
  }) {
    return EditorSessionState(
      document: DocumentState(
        value: value,
        revision: revision,
        lineIndex: lineIndex,
        geometry: geometry,
      ),
      projection: ProjectionState(
        projectedHiddenRanges: List<TextRange>.unmodifiable(
          projectedHiddenRanges,
        ),
        projectedExclusionRanges: List<TextRange>.unmodifiable(
          projectedExclusionRanges,
        ),
        authoritativeHiddenRanges: List<TextRange>.unmodifiable(
          authoritativeHiddenRanges,
        ),
        authoritativeExclusionRanges: List<TextRange>.unmodifiable(
          authoritativeExclusionRanges,
        ),
        projectedCursorMask: projectedCursorMask,
        authoritativeCursorMask: authoritativeCursorMask,
        activeCursorMask: activeCursorMask,
        latestAuthoritativeSnapshot: latestAuthoritativeSnapshot,
      ),
      history: HistoryState(
        lastOp: lastOp,
        lastOpTime: lastOpTime,
        currentUndoGroup: currentUndoGroup,
        undoBoundaryDepth: undoBoundaryDepth,
        commandTransactionDepth: commandTransactionDepth,
        commandTransactionUndoGroupId: commandTransactionUndoGroupId,
        forceUndoBoundaryForNextTextOp: forceUndoBoundaryForNextTextOp,
        compositionStartValue: compositionStartValue,
        canUndo: canUndo,
        canRedo: canRedo,
      ),
      telemetry: TelemetryState(
        predictiveBudgetExhaustionCount: predictiveBudgetExhaustionCount,
        predictiveLocalFallbackCount: predictiveLocalFallbackCount,
        predictiveLocalFallbackLastScannedChars:
            predictiveLocalFallbackLastScannedChars,
        parsePendingReplaceCount: parsePendingReplaceCount,
        parseStaleDropCount: parseStaleDropCount,
        renderCallCount: renderCallCount,
        renderLastMicros: renderLastMicros,
        renderMaxMicros: renderMaxMicros,
      ),
    );
  }
}
