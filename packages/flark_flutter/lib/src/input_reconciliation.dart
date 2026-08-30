import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'editor_transactions.dart';

/// Briefly retains the platform-provisional lineage after a semantic receipt
/// wins the race.
final class FlarkLateSemanticInput extends FlarkPendingPlatformLineage {
  FlarkLateSemanticInput({
    required this.provisionalTail,
    required this.reconciliation,
    required this.successorCount,
  }) : super();

  TextEditingValue provisionalTail;
  final FlarkInputReconciliationMap reconciliation;
  int successorCount;
}

/// Immutable facts needed to translate one provisional successor into an
/// effect against the current committed input window.
final class FlarkInputSuccessorPlanningRequest {
  const FlarkInputSuccessorPlanningRequest({
    required this.successor,
    required this.reconciliation,
    required this.currentInput,
    required this.currentInputGlobalUtf16Start,
    required this.inlineContinuation,
    required this.publicationCertificationBarrierActive,
  });

  final FlarkSemanticInputSuccessor successor;
  final FlarkInputReconciliationMap reconciliation;
  final TextEditingValue currentInput;
  final int currentInputGlobalUtf16Start;
  final FlarkCoreInlineContinuationV1? inlineContinuation;
  final bool publicationCertificationBarrierActive;
}

/// Closed set of host effects produced by successor reconciliation.
sealed class FlarkInputSuccessorPlan {
  const FlarkInputSuccessorPlan();
}

final class FlarkInputHistorySuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputHistorySuccessorPlan(this.successor);

  final FlarkDeferredHistorySuccessor successor;
}

final class FlarkInputReplacementSuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputReplacementSuccessorPlan({
    required this.replacement,
    required this.typingInput,
    required this.platformTiming,
  });

  final String replacement;
  final bool typingInput;
  final FlarkPlatformInputTiming? platformTiming;
}

final class FlarkInputCommandSuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputCommandSuccessorPlan({
    required this.command,
    required this.semanticAlreadyAttempted,
    required this.reclassifyAfterCertification,
    required this.platformTiming,
  });

  final FlarkDeferredInputCommand command;
  final bool semanticAlreadyAttempted;
  final bool reclassifyAfterCertification;
  final FlarkPlatformInputTiming? platformTiming;
}

final class FlarkInputSelectionSuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputSelectionSuccessorPlan({
    required this.selection,
    required this.composing,
  });

  final TextSelection selection;
  final TextRange composing;
}

final class FlarkInputMutationSuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputMutationSuccessorPlan({
    required this.mutation,
    required this.selection,
    required this.composing,
    required this.typingInput,
    required this.platformTiming,
  });

  final FlarkTextMutation mutation;
  final TextSelection selection;
  final TextRange composing;
  final bool typingInput;
  final FlarkPlatformInputTiming? platformTiming;
}

final class FlarkInputRejectedSuccessorPlan extends FlarkInputSuccessorPlan {
  const FlarkInputRejectedSuccessorPlan();
}

/// Plans one successor without executing controller callbacks or source work.
///
/// Platform callback lineage is provisional. This planner owns its monotone
/// mapping onto the committed input window, including canonical continuation
/// placement. The controller remains the executor of the resulting fixed
/// host effects.
final class FlarkInputSuccessorPlanner {
  const FlarkInputSuccessorPlanner();

  FlarkInputSuccessorPlan plan(FlarkInputSuccessorPlanningRequest request) {
    final successor = request.successor;
    if (successor is FlarkDeferredHistorySuccessor) {
      return FlarkInputHistorySuccessorPlan(successor);
    }
    if (successor is FlarkDeferredInputSuccessor) {
      final replacement = successor.replacement;
      if (replacement != null) {
        return FlarkInputReplacementSuccessorPlan(
          replacement: replacement,
          typingInput: successor.typingInput,
          platformTiming: successor.platformTiming,
        );
      }
      final command = successor.command;
      if (command == null) return const FlarkInputRejectedSuccessorPlan();
      return FlarkInputCommandSuccessorPlan(
        command: command,
        semanticAlreadyAttempted: successor.semanticAlreadyAttempted,
        reclassifyAfterCertification:
            successor.reclassifyAfterCertification ||
            request.publicationCertificationBarrierActive,
        platformTiming: successor.platformTiming,
      );
    }

    final batch = successor as FlarkProvisionalInputBatch;
    final selection = request.reconciliation.mapSelection(
      batch.after.selection,
    );
    final composing = request.reconciliation.mapRange(batch.after.composing);
    if (selection == null || composing == null) {
      return const FlarkInputRejectedSuccessorPlan();
    }
    final mutation = flarkDifferenceMutation(
      batch.before.text,
      batch.after.text,
    );
    if (mutation == null) {
      return FlarkInputSelectionSuccessorPlan(
        selection: selection,
        composing: composing,
      );
    }
    final mappedStart = request.reconciliation.mapOffset(
      mutation.start,
      downstream: true,
    );
    final mappedEnd = request.reconciliation.mapOffset(
      mutation.end,
      downstream: true,
    );
    final continuation = request.inlineContinuation;
    final continuationLocalCaret = continuation == null
        ? null
        : continuation.caretUtf16 - request.currentInputGlobalUtf16Start;
    final continuesAtCanonicalCaret =
        continuationLocalCaret != null &&
        batch.typingInput &&
        mutation.start == mutation.end &&
        batch.after.selection.isCollapsed &&
        !batch.after.composing.isValid &&
        0 <= continuationLocalCaret &&
        continuationLocalCaret <= request.currentInput.text.length;
    final promotedStart = continuesAtCanonicalCaret
        ? continuationLocalCaret
        : mappedStart;
    final promotedEnd = continuesAtCanonicalCaret
        ? continuationLocalCaret
        : mappedEnd;
    if (promotedStart == null || promotedEnd == null) {
      return const FlarkInputRejectedSuccessorPlan();
    }
    return FlarkInputMutationSuccessorPlan(
      mutation: FlarkTextMutation(
        promotedStart,
        promotedEnd,
        mutation.replacement,
      ),
      selection: continuesAtCanonicalCaret
          ? TextSelection.collapsed(
              offset: continuationLocalCaret + mutation.replacement.length,
              affinity: selection.affinity,
            )
          : selection,
      composing: continuesAtCanonicalCaret ? TextRange.empty : composing,
      typingInput: batch.typingInput,
      platformTiming: batch.platformTiming,
    );
  }
}

/// One bounded monotone map between a platform-provisional input window and
/// the Rust-committed window. Offsets inside differing interiors are
/// intentionally unmappable; callers resynchronize instead of guessing.
final class FlarkInputReconciliationMap {
  const FlarkInputReconciliationMap({
    required this.fromStart,
    required this.fromEnd,
    required this.toStart,
    required this.toEnd,
    this.canonicalCaretFrom,
    this.canonicalCaretTo,
  });

  final int fromStart;
  final int fromEnd;
  final int toStart;
  final int toEnd;
  final int? canonicalCaretFrom;
  final int? canonicalCaretTo;

  static FlarkInputReconciliationMap? forSemanticBarrier({
    required FlarkPendingSemanticInput pending,
    required FlarkCoreEditIntentReceiptV1 receipt,
    required int canonicalResultSelectionUtf16,
    required int committedInputGlobalUtf16Start,
    required int committedInputLength,
  }) {
    final provisional = pending.provisionalMutation;
    if (provisional != null) {
      final windowStart = pending.inputGlobalUtf16Start;
      final windowEnd = windowStart + pending.base.text.length;
      if (receipt.baseUtf16End <= windowStart ||
          receipt.baseUtf16Start >= windowEnd) {
        return FlarkInputReconciliationMap(
          fromStart: provisional.start,
          fromEnd: provisional.start + provisional.replacement.length,
          toStart: provisional.start,
          toEnd: provisional.end,
        );
      }
      final committedStart =
          receipt.baseUtf16Start - pending.inputGlobalUtf16Start;
      final committedEnd = receipt.baseUtf16End - pending.inputGlobalUtf16Start;
      if (provisional.start < 0 ||
          provisional.end < provisional.start ||
          provisional.end > pending.base.text.length) {
        return null;
      }

      if (committedStart < 0 ||
          committedEnd < committedStart ||
          committedEnd > pending.base.text.length) {
        final receiptCoversInputWindow =
            receipt.baseUtf16Start <= windowStart &&
            receipt.baseUtf16End >= windowEnd;
        final committedCaret =
            canonicalResultSelectionUtf16 - committedInputGlobalUtf16Start;
        final provisionalLength =
            pending.base.text.length -
            (provisional.end - provisional.start) +
            provisional.replacement.length;
        if (!receiptCoversInputWindow ||
            committedCaret < 0 ||
            committedCaret > committedInputLength) {
          return null;
        }
        return FlarkInputReconciliationMap(
          fromStart: 0,
          fromEnd: provisionalLength,
          toStart: committedCaret,
          toEnd: committedCaret,
        );
      }

      final affectedStart = math.min(provisional.start, committedStart);
      final affectedEnd = math.max(provisional.end, committedEnd);
      final fromStart = _mapBaseBoundaryThroughSplice(
        affectedStart,
        start: provisional.start,
        end: provisional.end,
        replacementLength: provisional.replacement.length,
        downstream: false,
      );
      final fromEnd = _mapBaseBoundaryThroughSplice(
        affectedEnd,
        start: provisional.start,
        end: provisional.end,
        replacementLength: provisional.replacement.length,
        downstream: true,
      );
      final toStart = _mapBaseBoundaryThroughSplice(
        affectedStart,
        start: committedStart,
        end: committedEnd,
        replacementLength: receipt.replacement.length,
        downstream: false,
      );
      final toEnd = _mapBaseBoundaryThroughSplice(
        affectedEnd,
        start: committedStart,
        end: committedEnd,
        replacementLength: receipt.replacement.length,
        downstream: true,
      );
      return FlarkInputReconciliationMap(
        fromStart: fromStart,
        fromEnd: fromEnd,
        toStart: toStart,
        toEnd: toEnd,
        canonicalCaretFrom:
            canonicalResultSelectionUtf16 == receipt.resultSelectionUtf16
            ? null
            : provisional.start + provisional.replacement.length,
        canonicalCaretTo:
            canonicalResultSelectionUtf16 == receipt.resultSelectionUtf16
            ? null
            : canonicalResultSelectionUtf16 - windowStart,
      );
    }
    final windowStart = pending.inputGlobalUtf16Start;
    final windowEnd = windowStart + pending.base.text.length;
    if (receipt.baseUtf16End <= windowStart ||
        receipt.baseUtf16Start >= windowEnd) {
      return const FlarkInputReconciliationMap(
        fromStart: 0,
        fromEnd: 0,
        toStart: 0,
        toEnd: 0,
      );
    }
    if (receipt.baseUtf16Start < windowStart ||
        receipt.baseUtf16End > windowEnd) {
      return null;
    }
    final localStart = receipt.baseUtf16Start - windowStart;
    final localEnd = receipt.baseUtf16End - windowStart;
    return FlarkInputReconciliationMap(
      fromStart: localStart,
      fromEnd: localEnd,
      toStart: localStart,
      toEnd: localStart + receipt.replacement.length,
    );
  }

  static int _mapBaseBoundaryThroughSplice(
    int offset, {
    required int start,
    required int end,
    required int replacementLength,
    required bool downstream,
  }) {
    if (offset < start) return offset;
    if (offset > end) return start + replacementLength + offset - end;
    if (start == end && offset == start) {
      return downstream ? start + replacementLength : start;
    }
    if (offset == start) return start;
    if (offset == end) return start + replacementLength;
    throw StateError('union boundary fell inside a source splice');
  }

  int? mapOffset(int offset, {required bool downstream}) {
    if (downstream &&
        canonicalCaretFrom != null &&
        offset == canonicalCaretFrom) {
      return canonicalCaretTo;
    }
    if (offset < fromStart) return offset;
    if (offset > fromEnd) return toEnd + offset - fromEnd;
    if (fromStart == fromEnd && offset == fromStart) {
      return downstream ? toEnd : toStart;
    }
    if (offset == fromStart) return toStart;
    if (offset == fromEnd) return toEnd;
    return null;
  }

  TextSelection? mapSelection(TextSelection selection) {
    final downstream = selection.affinity == TextAffinity.downstream;
    final base = mapOffset(selection.baseOffset, downstream: downstream);
    final extent = mapOffset(selection.extentOffset, downstream: downstream);
    if (base == null || extent == null) return null;
    return TextSelection(
      baseOffset: base,
      extentOffset: extent,
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    );
  }

  TextRange? mapRange(TextRange range) {
    if (range == TextRange.empty) return TextRange.empty;
    final start = mapOffset(range.start, downstream: true);
    final end = mapOffset(range.end, downstream: true);
    if (start == null || end == null) return null;
    return TextRange(start: start, end: end);
  }
}
