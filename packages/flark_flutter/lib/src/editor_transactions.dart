import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

final class FlarkTextMutation {
  const FlarkTextMutation(this.start, this.end, this.replacement);

  final int start;
  final int end;
  final String replacement;
}

/// The only two publication outcomes of an accepted source edit.
enum FlarkQueuedEditPublication {
  publishOptimistically,
  retainPublishedUntilCertified;

  bool get publishesOptimistically => this == publishOptimistically;
  bool get requiresParserCertification => this == retainPublishedUntilCertified;
}

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
