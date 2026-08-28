import 'dart:math' as math;

import 'package:flark_core/flark_core.dart';
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
}
