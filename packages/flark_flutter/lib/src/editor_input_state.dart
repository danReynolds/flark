import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

/// Sole mutable owner of the bounded input window exposed by a Flutter editor.
///
/// Source, history, and Markdown stay in Core. This state owns only the
/// platform value paired with its global UTF-16 origin, the active row, and
/// the canonical-selection mirrors needed for synchronous snapshot/input
/// publication.
final class FlarkEditorInputState {
  TextEditingValue _value = const TextEditingValue(
    selection: TextSelection.collapsed(offset: 0),
  );
  int _globalUtf16Start = 0;
  int? _activeOrdinal;
  int _selectionBaseUtf16 = 0;
  int _selectionExtentUtf16 = 0;
  bool _semanticEditActive = false;
  bool _crossRowSelection = false;
  bool _oversizedSelection = false;
  FlarkCoreInlineContinuationV1? _inlineContinuation;

  TextEditingValue get value => _value;
  int get globalUtf16Start => _globalUtf16Start;
  int? get activeOrdinal => _activeOrdinal;
  int get selectionBaseUtf16 => _selectionBaseUtf16;
  int get selectionExtentUtf16 => _selectionExtentUtf16;
  bool get semanticEditActive => _semanticEditActive;
  bool get crossRowSelection => _crossRowSelection;
  bool get oversizedSelection => _oversizedSelection;
  FlarkCoreInlineContinuationV1? get inlineContinuation => _inlineContinuation;

  /// Replaces the platform-facing value without changing its global origin.
  void replaceValue(TextEditingValue value) {
    _value = value;
  }

  /// Installs a value and its global origin as one bounded-window transition.
  void replaceWindow({
    required int globalUtf16Start,
    required TextEditingValue value,
  }) {
    _checkNonnegative(globalUtf16Start, 'globalUtf16Start');
    _globalUtf16Start = globalUtf16Start;
    _value = value;
  }

  void retargetActiveOrdinal(int? ordinal) {
    _activeOrdinal = ordinal;
  }

  void setSemanticEditActive(bool value) {
    _semanticEditActive = value;
  }

  void abandonInlineContinuation() {
    _inlineContinuation = null;
  }

  void restoreInlineContinuation(FlarkCoreInlineContinuationV1? continuation) {
    _inlineContinuation = continuation;
  }

  void extendCanonicalSelection(int extent) {
    _checkNonnegative(extent, 'extent');
    _selectionExtentUtf16 = extent;
  }

  void setCanonicalSelection(int base, int extent) {
    _checkNonnegative(base, 'base');
    _checkNonnegative(extent, 'extent');
    _selectionBaseUtf16 = base;
    _selectionExtentUtf16 = extent;
  }

  void setCrossRowSelection(bool value) {
    _crossRowSelection = value;
  }

  /// Records an exact selection that cannot fit in the platform window.
  void markOversizedSelection({
    required int base,
    required int extent,
    int? activeOrdinal,
  }) {
    setCanonicalSelection(base, extent);
    _activeOrdinal = activeOrdinal;
    _crossRowSelection = base != extent;
    _oversizedSelection = true;
  }

  void clearOversizedSelection() {
    _oversizedSelection = false;
  }

  /// Collapses canonical selection and retires cross-window selection state.
  void collapseCanonicalSelection({
    required int caret,
    int? activeOrdinal,
    bool clearOversized = false,
  }) {
    setCanonicalSelection(caret, caret);
    _activeOrdinal = activeOrdinal;
    _crossRowSelection = false;
    if (clearOversized) _oversizedSelection = false;
  }

  /// Installs a bounded window around a requested selection and reports
  /// whether both endpoints fit without clamping.
  bool activateWindow({
    required String text,
    required int sourceStart,
    required int caret,
    int? selectionExtent,
    required int ordinal,
    required TextAffinity affinity,
    required int maximumCodeUnits,
  }) {
    if (maximumCodeUnits <= 0) {
      throw ArgumentError.value(
        maximumCodeUnits,
        'maximumCodeUnits',
        'must be positive',
      );
    }
    final requestedLocalCaret = caret - sourceStart;
    final requestedLocalExtent = selectionExtent == null
        ? requestedLocalCaret
        : selectionExtent - sourceStart;
    final localCaret = requestedLocalCaret.clamp(0, text.length);
    final localExtent = selectionExtent == null
        ? localCaret
        : requestedLocalExtent.clamp(0, text.length);
    var windowStart = 0;
    var windowEnd = text.length;
    if (text.length > maximumCodeUnits) {
      windowStart = (localCaret - maximumCodeUnits ~/ 2).clamp(
        0,
        text.length - maximumCodeUnits,
      );
      final selectionStart = math.min(localCaret, localExtent);
      final selectionEnd = math.max(localCaret, localExtent);
      if (selectionEnd - selectionStart <= maximumCodeUnits) {
        if (selectionStart < windowStart) windowStart = selectionStart;
        if (selectionEnd > windowStart + maximumCodeUnits) {
          windowStart = selectionEnd - maximumCodeUnits;
        }
      }
      windowEnd = windowStart + maximumCodeUnits;
    }
    final alignedWindow = scalarAlignedUtf16Window(
      text,
      windowStart,
      windowEnd,
    );
    windowStart = alignedWindow.start;
    windowEnd = alignedWindow.end;
    final window = text.substring(windowStart, windowEnd);
    final windowCaret = localCaret - windowStart;
    final windowExtent = localExtent - windowStart;
    final selectionRepresented =
        requestedLocalCaret == localCaret &&
        requestedLocalExtent == localExtent &&
        windowCaret >= 0 &&
        windowCaret <= window.length &&
        windowExtent >= 0 &&
        windowExtent <= window.length;
    _globalUtf16Start = sourceStart + windowStart;
    _value = TextEditingValue(
      text: window,
      selection: selectionExtent != null && selectionRepresented
          ? TextSelection(
              baseOffset: windowCaret,
              extentOffset: windowExtent,
              affinity: affinity,
              isDirectional: true,
            )
          : TextSelection.collapsed(
              offset: windowCaret.clamp(0, window.length),
              affinity: affinity,
            ),
    );
    _activeOrdinal = ordinal;
    _crossRowSelection =
        selectionExtent != null &&
        selectionRepresented &&
        caret != selectionExtent;
    _selectionBaseUtf16 = selectionExtent != null && !selectionRepresented
        ? caret
        : _globalUtf16Start + _value.selection.baseOffset;
    _selectionExtentUtf16 = selectionExtent != null && !selectionRepresented
        ? selectionExtent
        : _globalUtf16Start + _value.selection.extentOffset;
    return selectionRepresented;
  }

  void activateCollapsedWindow({
    required String text,
    required int sourceStart,
    required int caret,
    required int ordinal,
    required int maximumCodeUnits,
  }) {
    if (maximumCodeUnits <= 0) {
      throw ArgumentError.value(
        maximumCodeUnits,
        'maximumCodeUnits',
        'must be positive',
      );
    }
    final localCaret = (caret - sourceStart).clamp(0, text.length);
    final candidateStart = text.length <= maximumCodeUnits
        ? 0
        : (localCaret - maximumCodeUnits ~/ 2).clamp(
            0,
            text.length - maximumCodeUnits,
          );
    final candidateEnd = math.min(
      text.length,
      candidateStart + maximumCodeUnits,
    );
    final window = scalarAlignedUtf16Window(text, candidateStart, candidateEnd);
    _globalUtf16Start = sourceStart + window.start;
    _value = TextEditingValue(
      text: text.substring(window.start, window.end),
      selection: TextSelection.collapsed(offset: localCaret - window.start),
    );
    _activeOrdinal = ordinal;
    _crossRowSelection = false;
    updateCanonicalFromLocal();
  }

  void updateCanonicalFromLocal() {
    _selectionBaseUtf16 = _globalUtf16Start + _value.selection.baseOffset;
    _selectionExtentUtf16 = _globalUtf16Start + _value.selection.extentOffset;
    if (_selectionBaseUtf16 != _selectionExtentUtf16 ||
        _inlineContinuation?.caretUtf16 != _selectionExtentUtf16) {
      _inlineContinuation = null;
    }
  }

  static void _checkNonnegative(int value, String name) {
    if (value < 0) {
      throw ArgumentError.value(value, name, 'must be nonnegative');
    }
  }
}
