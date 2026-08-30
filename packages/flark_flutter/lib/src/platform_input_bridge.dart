import 'dart:convert';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'editor_transactions.dart';
import 'input_window.dart';

const _maximumSmallEditBytes = 4 * 1024;
const _smallEditDescriptorBytes = 32;

/// One normalized Flutter text-service observation.
///
/// Delta-model and full-value clients describe the same user operation with
/// different callback shapes. This immutable value reduces both shapes to one
/// before/after transaction so editor policy cannot diverge by platform API.
final class FlarkPlatformInputObservation {
  const FlarkPlatformInputObservation({
    required this.before,
    required this.after,
    required this.rejection,
    required this.observedMutation,
    required this.effectiveMutation,
    required this.mutatingChanges,
    required this.typingInput,
    required this.fromDeltaBatch,
    required this.newlineCommand,
    required this.deleteBackwardCommand,
    required this.selectedDeletion,
    required this.selectionSupersededByProjection,
  });

  final TextEditingValue before;
  final TextEditingValue after;
  final FlarkInputResyncReason rejection;

  /// The mutation expressed by the platform's selection/delta geometry.
  final FlarkTextMutation? observedMutation;

  /// The minimal net text difference against the controller's current value.
  final FlarkTextMutation? effectiveMutation;
  final int mutatingChanges;
  final bool typingInput;
  final bool fromDeltaBatch;
  final bool newlineCommand;
  final bool deleteBackwardCommand;
  final bool selectedDeletion;
  final bool selectionSupersededByProjection;

  bool get accepted => rejection == FlarkInputResyncReason.none;
  bool get selectionOnly => mutatingChanges == 0;
}

/// Owns the serialized state of Flutter's active text-input connection.
///
/// This boundary knows platform text values, hashes, ranges, and connection
/// epochs. It deliberately knows nothing about Markdown, parser rows,
/// viewports, history, or render publications.
final class FlarkPlatformInputBridge {
  static int _connectionEpochCounter = 0;

  FlarkInputWindowState _state = FlarkInputWindowState.detached;
  FlarkInputResyncReason _lastResyncReason = FlarkInputResyncReason.none;
  int _connectionEpoch = 0;
  int _windowEpoch = 0;
  int _resyncCount = 0;
  String _windowTextSha256 = '';
  String? _shadowText;
  int _shadowWindowStart = 0;
  TextSelection? _shadowSelection;

  FlarkInputWindowState get state => _state;
  FlarkInputResyncReason get lastResyncReason => _lastResyncReason;
  int get connectionEpoch => _connectionEpoch;
  int get windowEpoch => _windowEpoch;
  int get resyncCount => _resyncCount;
  String get windowTextSha256 => _windowTextSha256;
  String? get shadowText => _shadowText;
  int get shadowWindowStart => _shadowWindowStart;
  TextSelection? get shadowSelection => _shadowSelection;
  TextEditingValue? get shadowValue {
    final text = _shadowText;
    final selection = _shadowSelection;
    if (text == null || selection == null || !selection.isValid) return null;
    return TextEditingValue(
      text: text,
      selection: selection,
      composing: TextRange.empty,
    );
  }

  bool matches({
    required String text,
    required int globalStart,
    required TextSelection selection,
  }) =>
      _shadowWindowStart == globalStart &&
      _shadowText == text &&
      _shadowSelection == selection;

  FlarkInputWindowShadow snapshot({
    required int representedRevision,
    required int selectionGeneration,
    required TextEditingValue fallbackValue,
  }) => FlarkInputWindowShadow(
    connectionEpoch: _connectionEpoch,
    windowEpoch: _windowEpoch,
    representedRevision: representedRevision,
    globalUtf16Start: _shadowWindowStart,
    windowUtf16Length: (_shadowText ?? fallbackValue.text).length,
    windowTextSha256: _windowTextSha256,
    selectionGeneration: selectionGeneration,
  );

  void install({
    required String text,
    required int globalStart,
    required TextSelection selection,
    required bool platformOriginated,
    required bool closed,
    required bool faulted,
  }) {
    if (closed) {
      _state = FlarkInputWindowState.closed;
      return;
    }
    if (faulted) {
      _state = FlarkInputWindowState.faulted;
      return;
    }
    final textChanged = !identical(text, _shadowText) && text != _shadowText;
    final startChanged = globalStart != _shadowWindowStart;
    final selectionChanged = selection != _shadowSelection;
    if (_connectionEpoch != 0 &&
        !textChanged &&
        !startChanged &&
        !selectionChanged) {
      return;
    }
    if (platformOriginated && _connectionEpoch != 0 && !startChanged) {
      _windowEpoch += 1;
      if (textChanged) _windowTextSha256 = flarkWindowTextSha256(text);
    } else {
      _connectionEpoch = ++_connectionEpochCounter;
      _windowEpoch = 1;
      if (textChanged || _windowTextSha256.isEmpty) {
        _windowTextSha256 = flarkWindowTextSha256(text);
      }
      _state = FlarkInputWindowState.synchronized;
    }
    _shadowText = text;
    _shadowWindowStart = globalStart;
    _shadowSelection = selection;
  }

  void resynchronize({
    required FlarkInputResyncReason reason,
    required TextEditingValue authoritativeValue,
    required int globalStart,
  }) {
    _lastResyncReason = reason;
    _resyncCount += 1;
    _state = FlarkInputWindowState.resyncRequired;
    _connectionEpoch = ++_connectionEpochCounter;
    _windowEpoch = 1;
    _windowTextSha256 = flarkWindowTextSha256(authoritativeValue.text);
    _shadowText = authoritativeValue.text;
    _shadowWindowStart = globalStart;
    _shadowSelection = authoritativeValue.selection;
    _state = FlarkInputWindowState.synchronized;
  }

  /// Validates and normalizes one complete delta callback atomically.
  FlarkPlatformInputObservation observeDeltaBatch(
    List<TextEditingDelta> deltas, {
    required TextEditingValue currentValue,
    TextEditingValue? against,
    String? expectedTextSha256,
  }) {
    final before = against ?? currentValue;
    final rejection = validateDeltaBatch(
      deltas,
      against: against,
      expectedTextSha256: expectedTextSha256,
      fallbackValue: currentValue,
    );
    if (rejection != FlarkInputResyncReason.none) {
      return FlarkPlatformInputObservation(
        before: before,
        after: before,
        rejection: rejection,
        observedMutation: null,
        effectiveMutation: null,
        mutatingChanges: 0,
        typingInput: false,
        fromDeltaBatch: true,
        newlineCommand: false,
        deleteBackwardCommand: false,
        selectedDeletion: false,
        selectionSupersededByProjection: false,
      );
    }
    var after = before;
    var mutatingChanges = 0;
    var typingInput = true;
    for (final delta in deltas) {
      after = delta.apply(after);
      if (mutationFor(delta) != null) {
        mutatingChanges += 1;
        typingInput = typingInput && delta is TextEditingDeltaInsertion;
      }
    }
    final observedMutation = deltas.length == 1
        ? mutationFor(deltas.single)
        : null;
    final deleteBackward = isDeleteBackwardDeltaBatch(
      deltas,
      currentValue: before,
    );
    return FlarkPlatformInputObservation(
      before: before,
      after: after,
      rejection: FlarkInputResyncReason.none,
      observedMutation: observedMutation,
      effectiveMutation: differenceMutation(before.text, after.text),
      mutatingChanges: mutatingChanges,
      typingInput: mutatingChanges > 0 && typingInput,
      fromDeltaBatch: true,
      newlineCommand: isNewlineDeltaBatch(deltas, currentValue: before),
      deleteBackwardCommand: deleteBackward,
      selectedDeletion:
          observedMutation != null &&
          isSelectedDeletion(
            observedMutation,
            currentSelection: before.selection,
          ),
      selectionSupersededByProjection:
          against == null &&
          deleteBackward &&
          _shadowText == before.text &&
          _shadowSelection != null &&
          _shadowSelection != before.selection,
    );
  }

  /// Normalizes a full-value callback into the same transaction shape used by
  /// [observeDeltaBatch].
  FlarkPlatformInputObservation observeValue(
    TextEditingValue value, {
    required TextEditingValue currentValue,
    TextEditingValue? against,
  }) {
    final before = against ?? currentValue;
    final platformBefore = against ?? shadowValue ?? currentValue;
    final observedMutation = selectionObservedMutation(platformBefore, value);
    final effectiveMutation = differenceMutation(before.text, value.text);
    final selection = before.selection;
    final typingInput =
        effectiveMutation != null &&
        selection.isCollapsed &&
        effectiveMutation.start == selection.extentOffset &&
        effectiveMutation.end == selection.extentOffset &&
        effectiveMutation.replacement.isNotEmpty;
    return FlarkPlatformInputObservation(
      before: before,
      after: value,
      rejection: FlarkInputResyncReason.none,
      observedMutation: observedMutation,
      effectiveMutation: effectiveMutation,
      mutatingChanges: before.text == value.text ? 0 : 1,
      typingInput: typingInput,
      fromDeltaBatch: false,
      newlineCommand: isNewlineValue(
        currentValue: before,
        observedValue: value,
      ),
      deleteBackwardCommand: isDeleteBackwardValue(
        currentValue: before,
        observedValue: value,
      ),
      selectedDeletion:
          effectiveMutation != null &&
          isSelectedDeletion(
            effectiveMutation,
            currentSelection: before.selection,
          ),
      selectionSupersededByProjection: false,
    );
  }

  /// Validates a complete callback batch against the serialized shadow before
  /// the editor applies any member of the batch.
  FlarkInputResyncReason validateDeltaBatch(
    List<TextEditingDelta> deltas, {
    TextEditingValue? against,
    String? expectedTextSha256,
    required TextEditingValue fallbackValue,
  }) {
    if (deltas.isEmpty) return FlarkInputResyncReason.none;
    final initial = against ?? fallbackValue;
    final expectedHash = expectedTextSha256 ?? _windowTextSha256;
    if (flarkWindowTextSha256(deltas.first.oldText) != expectedHash) {
      return FlarkInputResyncReason.oldTextMismatch;
    }
    var value = initial;
    var runningHash = expectedHash;
    var envelopeBytes = 0;
    var mutatingDeltas = 0;
    for (final delta in deltas) {
      if (flarkWindowTextSha256(delta.oldText) != runningHash) {
        return FlarkInputResyncReason.deltaChainMismatch;
      }
      final mutation = mutationFor(delta);
      if (mutation != null) {
        if (mutation.start < 0 ||
            mutation.end < mutation.start ||
            mutation.end > value.text.length) {
          return FlarkInputResyncReason.rangeOutOfWindow;
        }
        mutatingDeltas += 1;
        envelopeBytes += _smallEditDescriptorBytes;
        envelopeBytes += utf8
            .encode(value.text.substring(mutation.start, mutation.end))
            .length;
        envelopeBytes += utf8.encode(mutation.replacement).length;
      }
      try {
        value = delta.apply(value);
      } on Object {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      final selection = delta.selection;
      if (!selection.isValid ||
          selection.start < 0 ||
          selection.end > value.text.length) {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      final composing = delta.composing;
      if (composing != TextRange.empty &&
          (!composing.isValid ||
              composing.start < 0 ||
              composing.end > value.text.length)) {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      runningHash = flarkWindowTextSha256(value.text);
    }
    if (mutatingDeltas > 1 && envelopeBytes > _maximumSmallEditBytes) {
      return FlarkInputResyncReason.batchOverEnvelope;
    }
    return FlarkInputResyncReason.none;
  }

  FlarkTextMutation? mutationFor(TextEditingDelta delta) => switch (delta) {
    TextEditingDeltaInsertion insertion => FlarkTextMutation(
      insertion.insertionOffset,
      insertion.insertionOffset,
      insertion.textInserted,
    ),
    TextEditingDeltaDeletion deletion => FlarkTextMutation(
      deletion.deletedRange.start,
      deletion.deletedRange.end,
      '',
    ),
    TextEditingDeltaReplacement replacement => FlarkTextMutation(
      replacement.replacedRange.start,
      replacement.replacedRange.end,
      replacement.replacementText,
    ),
    TextEditingDeltaNonTextUpdate() => null,
    _ => null,
  };

  bool isNewlineDeltaBatch(
    List<TextEditingDelta> deltas, {
    required TextEditingValue currentValue,
  }) {
    if (deltas.length != 1 ||
        currentValue.composing != TextRange.empty ||
        deltas.single.composing != TextRange.empty) {
      return false;
    }
    final mutation = mutationFor(deltas.single);
    if (mutation == null || mutation.replacement != '\n') return false;
    bool replaces(TextSelection? selection) =>
        selection != null &&
        selection.isValid &&
        mutation.start == selection.start &&
        mutation.end == selection.end;
    return replaces(_shadowSelection) || replaces(currentValue.selection);
  }

  bool isNewlineValue({
    required TextEditingValue currentValue,
    required TextEditingValue observedValue,
  }) {
    if (currentValue.composing != TextRange.empty ||
        observedValue.composing != TextRange.empty) {
      return false;
    }
    final shadowText = _shadowText ?? currentValue.text;
    final selection = _shadowSelection ?? currentValue.selection;
    if (!selection.isValid) return false;
    return shadowText.replaceRange(selection.start, selection.end, '\n') ==
        observedValue.text;
  }

  bool isDeleteBackwardDeltaBatch(
    List<TextEditingDelta> deltas, {
    required TextEditingValue currentValue,
  }) {
    if (deltas.length != 1 ||
        currentValue.composing != TextRange.empty ||
        deltas.single.composing != TextRange.empty) {
      return false;
    }
    return isBackspaceAtRecognizedCaret(
      mutationFor(deltas.single),
      currentSelection: currentValue.selection,
    );
  }

  bool isDeleteBackwardValue({
    required TextEditingValue currentValue,
    required TextEditingValue observedValue,
  }) {
    if (currentValue.composing != TextRange.empty ||
        observedValue.composing != TextRange.empty) {
      return false;
    }
    final before = shadowValue ?? currentValue;
    return isBackspaceAtRecognizedCaret(
      selectionObservedMutation(before, observedValue),
      currentSelection: currentValue.selection,
    );
  }

  bool isSelectedDeletion(
    FlarkTextMutation mutation, {
    required TextSelection currentSelection,
  }) {
    final selection = _shadowSelection ?? currentSelection;
    return !selection.isCollapsed &&
        mutation.replacement.isEmpty &&
        mutation.start == selection.start &&
        mutation.end == selection.end;
  }

  bool isBackspaceAtRecognizedCaret(
    FlarkTextMutation? mutation, {
    required TextSelection currentSelection,
  }) {
    if (mutation == null ||
        mutation.replacement.isNotEmpty ||
        mutation.start >= mutation.end) {
      return false;
    }
    return (_shadowSelection != null &&
            _shadowSelection!.isCollapsed &&
            mutation.end == _shadowSelection!.extentOffset) ||
        (currentSelection.isCollapsed &&
            mutation.end == currentSelection.extentOffset);
  }

  FlarkTextMutation? selectionObservedMutation(
    TextEditingValue before,
    TextEditingValue after,
  ) {
    final start = before.selection.start;
    final end = before.selection.end;
    final resultCaret = after.selection.extentOffset;
    if (resultCaret >= start && resultCaret <= after.text.length) {
      final replacement = after.text.substring(start, resultCaret);
      if (before.text.replaceRange(start, end, replacement) == after.text) {
        return FlarkTextMutation(start, end, replacement);
      }
    }
    if (before.selection.isCollapsed) {
      final previous = FlarkCoreGraphemePolicy.previousClusterRange(
        before.text,
        start,
      );
      if (previous != null &&
          after.selection.extentOffset == previous.$1 &&
          before.text.replaceRange(previous.$1, previous.$2, '') ==
              after.text) {
        return FlarkTextMutation(previous.$1, previous.$2, '');
      }
      final next = FlarkCoreGraphemePolicy.nextClusterRange(before.text, start);
      if (next != null &&
          after.selection.extentOffset == start &&
          before.text.replaceRange(next.$1, next.$2, '') == after.text) {
        return FlarkTextMutation(next.$1, next.$2, '');
      }
    }
    return differenceMutation(before.text, after.text);
  }

  FlarkTextMutation? differenceMutation(String before, String after) {
    return flarkDifferenceMutation(before, after);
  }

  bool validObservedValue(
    TextEditingValue value, {
    required int maximumCodeUnits,
  }) {
    if (value.text.length > maximumCodeUnits ||
        !value.selection.isValid ||
        value.selection.start < 0 ||
        value.selection.end > value.text.length) {
      return false;
    }
    final composing = value.composing;
    return composing == TextRange.empty ||
        (composing.isValid &&
            composing.start >= 0 &&
            composing.end <= value.text.length);
  }
}
