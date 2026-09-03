import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

final class FlarkTextMutation {
  const FlarkTextMutation(this.start, this.end, this.replacement);

  final int start;
  final int end;
  final String replacement;
}

/// Returns the smallest scalar-aligned splice that transforms [before] into
/// [after], or `null` when the values are identical.
FlarkTextMutation? flarkDifferenceMutation(String before, String after) {
  if (before == after) return null;
  var prefix = 0;
  while (prefix < before.length &&
      prefix < after.length &&
      before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
    prefix += 1;
  }
  if (_splitsUtf16Scalar(before, prefix) || _splitsUtf16Scalar(after, prefix)) {
    prefix -= 1;
  }
  var oldSuffix = before.length;
  var newSuffix = after.length;
  while (oldSuffix > prefix &&
      newSuffix > prefix &&
      before.codeUnitAt(oldSuffix - 1) == after.codeUnitAt(newSuffix - 1)) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }
  if (_splitsUtf16Scalar(before, oldSuffix) ||
      _splitsUtf16Scalar(after, newSuffix)) {
    oldSuffix += 1;
    newSuffix += 1;
  }
  return FlarkTextMutation(
    prefix,
    oldSuffix,
    after.substring(prefix, newSuffix),
  );
}

bool _splitsUtf16Scalar(String source, int offset) =>
    offset > 0 &&
    offset < source.length &&
    _isHighSurrogate(source.codeUnitAt(offset - 1)) &&
    _isLowSurrogate(source.codeUnitAt(offset));

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xD800 && codeUnit <= 0xDBFF;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xDC00 && codeUnit <= 0xDFFF;

sealed class FlarkMutationAcceptance {
  const FlarkMutationAcceptance();

  bool get accepted;
  bool get publishOptimistically;
}

final class FlarkRejectedMutation extends FlarkMutationAcceptance {
  const FlarkRejectedMutation();

  @override
  bool get accepted => false;

  @override
  bool get publishOptimistically => false;
}

final class FlarkQueuedMutation extends FlarkMutationAcceptance {
  const FlarkQueuedMutation(this.publication);

  final FlarkQueuedEditPublication publication;

  @override
  bool get accepted => true;

  @override
  bool get publishOptimistically => publication.publishesOptimistically;
}

final class FlarkEditorSelectionSnapshot {
  const FlarkEditorSelectionSnapshot(
    this.selection,
    this.activeOrdinal, {
    this.inlineContinuation,
  });

  final TextSelection selection;
  final int? activeOrdinal;
  final FlarkCoreInlineContinuationV1? inlineContinuation;
}

final class FlarkCompositionInputBase {
  const FlarkCompositionInputBase({
    required this.windowStart,
    required this.value,
  });

  final int windowStart;
  final TextEditingValue value;
}

sealed class FlarkSemanticInputSuccessor {
  const FlarkSemanticInputSuccessor();
}

final class FlarkProvisionalInputBatch extends FlarkSemanticInputSuccessor {
  const FlarkProvisionalInputBatch({
    required this.before,
    required this.after,
    required this.typingInput,
    this.platformTiming,
  }) : super();

  final TextEditingValue before;
  final TextEditingValue after;
  final bool typingInput;
  final FlarkPlatformInputTiming? platformTiming;
}

enum FlarkDeferredInputCommand { deleteBackward, deleteForward, insertNewline }

final class FlarkDeferredInputSuccessor extends FlarkSemanticInputSuccessor {
  const FlarkDeferredInputSuccessor(
    this.command, {
    this.replacement,
    this.typingInput = false,
    this.semanticAlreadyAttempted = false,
    this.reclassifyAfterCertification = false,
    this.platformTiming,
  }) : super();

  final FlarkDeferredInputCommand? command;
  final String? replacement;
  final bool typingInput;
  final bool semanticAlreadyAttempted;
  final bool reclassifyAfterCertification;
  final FlarkPlatformInputTiming? platformTiming;
}

final class FlarkDeferredHistorySuccessor extends FlarkSemanticInputSuccessor {
  FlarkDeferredHistorySuccessor({
    required this.undoDirection,
    required this.completion,
  });

  final bool undoDirection;
  final Completer<bool> completion;
}

/// Bounded timing carried with one platform callback until its accepted
/// mutation receives a source generation.
final class FlarkPlatformInputTiming {
  FlarkPlatformInputTiming()
    : acceptedAtEpochMicros = DateTime.now().microsecondsSinceEpoch,
      _watch = (Stopwatch()..start());

  final int acceptedAtEpochMicros;
  final Stopwatch _watch;
  int? _completedMicros;

  void complete() {
    if (_completedMicros != null) return;
    _watch.stop();
    _completedMicros = _watch.elapsedMicroseconds;
  }

  int get editorSyncMicros => _completedMicros ?? _watch.elapsedMicroseconds;
}

abstract base class FlarkPendingPlatformLineage {
  const FlarkPendingPlatformLineage();
}

final class FlarkPendingSemanticInput extends FlarkPendingPlatformLineage {
  FlarkPendingSemanticInput({
    required this.base,
    required this.inputGlobalUtf16Start,
    required this.initialCallbackStartedEpochMicros,
    this.platformTiming,
    this.provisionalMutation,
    required TextEditingValue provisionalAfter,
  }) : provisionalTail = provisionalAfter,
       super();

  final TextEditingValue base;
  final int inputGlobalUtf16Start;
  final int initialCallbackStartedEpochMicros;
  final FlarkPlatformInputTiming? platformTiming;
  final FlarkTextMutation? provisionalMutation;
  FlarkDeferredInputCommand? fallbackWhenNotApplied;
  int initialCallbackMicros = 0;
  TextEditingValue provisionalTail;
  final List<FlarkSemanticInputSuccessor> successors = [];
  Completer<void>? certificationPromotion;
}
