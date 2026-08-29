import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'text_adaptation.dart';

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
    final window = FlarkEditorInputWindowPlanner.activate(
      text: text,
      sourceStart: sourceStart,
      caret: caret,
      selectionExtent: selectionExtent,
      ordinal: ordinal,
      affinity: portableTextAffinity(affinity),
      maximumCodeUnits: maximumCodeUnits,
    );
    _installPlannedWindow(window);
    return window.selectionRepresented;
  }

  void activateCollapsedWindow({
    required String text,
    required int sourceStart,
    required int caret,
    required int ordinal,
    required int maximumCodeUnits,
  }) {
    final window = FlarkEditorInputWindowPlanner.collapsed(
      text: text,
      sourceStart: sourceStart,
      caret: caret,
      ordinal: ordinal,
      maximumCodeUnits: maximumCodeUnits,
    );
    _installPlannedWindow(window);
  }

  void _installPlannedWindow(FlarkEditorInputWindow window) {
    _globalUtf16Start = window.globalUtf16Start;
    _value = TextEditingValue(
      text: window.text,
      selection: TextSelection(
        baseOffset: window.selection.baseOffset,
        extentOffset: window.selection.extentOffset,
        affinity: flutterTextAffinity(window.selection.affinity),
        isDirectional: window.selection.isDirectional,
      ),
    );
    _activeOrdinal = window.activeOrdinal;
    _selectionBaseUtf16 = window.canonicalSelectionBaseUtf16;
    _selectionExtentUtf16 = window.canonicalSelectionExtentUtf16;
    _crossRowSelection = window.crossRowSelection;
    if (_selectionBaseUtf16 != _selectionExtentUtf16 ||
        _inlineContinuation?.caretUtf16 != _selectionExtentUtf16) {
      _inlineContinuation = null;
    }
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
