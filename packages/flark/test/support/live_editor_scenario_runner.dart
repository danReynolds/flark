import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'live_editor_scenario.dart';
import 'live_editor_scenario_executor.dart';

base class NoWindowLiveEditorScenarioDriver
    implements LiveEditorScenarioDriver {
  NoWindowLiveEditorScenarioDriver({required this.libraryPath});

  final String libraryPath;
  FlarkEditorController? _controller;
  TextEditingValue? _platformValue;
  String? _clipboardText;

  FlarkEditorController get _activeController =>
      _controller ?? (throw StateError('scenario driver is not started'));

  FlarkEditorController get activeController => _activeController;

  @override
  String get name => 'no-window';

  @override
  bool get observesPaint => false;

  @override
  bool get observesScroll => false;

  @override
  Future<void> start(LiveEditorScenarioPlan plan) async {
    if (_controller != null) {
      throw StateError('scenario driver already started');
    }
    final controller = await FlarkEditorController.open(
      plan.initialSource,
      libraryPath: libraryPath,
    );
    _controller = controller;
    await controller.continueParsing();
    _platformValue = controller.inputValue;
  }

  @override
  Future<void> activateAtUtf16(int offset) async {
    final controller = _activeController;
    final row = controller.rows.firstWhere(
      (candidate) =>
          candidate.editableUtf16 != null &&
          candidate.editableUtf16!.start <= offset &&
          offset <= candidate.editableUtf16!.end,
      orElse: () => throw StateError(
        'activation offset $offset is not in an editable viewport row',
      ),
    );
    controller.activateRow(row, offset);
    _platformValue = controller.inputValue;
  }

  @override
  Future<void> insertText(String text, {required Duration cadence}) async {
    final controller = _activeController;
    var platformValue = _platformValue ?? controller.inputValue;
    for (final rune in text.runes) {
      final character = String.fromCharCode(rune);
      // A platform burst is a lineage of observations against the text
      // service's preceding local value. The controller may still expose the
      // pre-barrier or post-barrier window while a semantic Return is in
      // flight; sampling it between synchronous callbacks creates a sequence
      // no real text service emitted.
      final before = platformValue;
      final selection = before.selection;
      final start = selection.start;
      final end = selection.end;
      final delta = start == end
          ? TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: character,
              insertionOffset: start,
              selection: TextSelection.collapsed(
                offset: start + character.length,
              ),
              composing: TextRange.empty,
            )
          : TextEditingDeltaReplacement(
              oldText: before.text,
              replacementText: character,
              replacedRange: TextRange(start: start, end: end),
              selection: TextSelection.collapsed(
                offset: start + character.length,
              ),
              composing: TextRange.empty,
            );
      controller.applyDeltas([delta]);
      platformValue = delta.apply(before);
      if (cadence > Duration.zero) {
        await Future<void>.delayed(cadence);
        // Once the authoritative edit has settled, model the platform
        // adopting the remote editing state before its next paced callback.
        if (controller.pendingEdits == 0) {
          platformValue = controller.inputValue;
        }
      }
    }
    _platformValue = platformValue;
  }

  @override
  Future<void> pressKey(LiveEditorScenarioKey key) async {
    final controller = _activeController;
    switch (key) {
      case LiveEditorScenarioKey.enter:
        final before = _platformValue ?? controller.inputValue;
        final selection = before.selection;
        final start = selection.start;
        final end = selection.end;
        final delta = start == end
            ? TextEditingDeltaInsertion(
                oldText: before.text,
                textInserted: '\n',
                insertionOffset: start,
                selection: TextSelection.collapsed(offset: start + 1),
                composing: TextRange.empty,
              )
            : TextEditingDeltaReplacement(
                oldText: before.text,
                replacementText: '\n',
                replacedRange: TextRange(start: start, end: end),
                selection: TextSelection.collapsed(offset: start + 1),
                composing: TextRange.empty,
              );
        controller.applyDeltas([delta]);
        controller.observePlatformNewlineAction();
        _platformValue = delta.apply(before);
      case LiveEditorScenarioKey.backspace:
        controller.deleteBackward();
        _platformValue = controller.inputValue;
      case LiveEditorScenarioKey.delete:
        controller.deleteForward();
        _platformValue = controller.inputValue;
      case LiveEditorScenarioKey.selectAll:
        await controller.selectOversizedRangeUtf16(
          0,
          controller.sourceUtf16Length,
        );
        _platformValue = controller.inputValue;
      case LiveEditorScenarioKey.copy:
        final selected = await controller.readSelectedText();
        if (selected != null) _clipboardText = selected;
      case LiveEditorScenarioKey.cut:
        final selected = await controller.readSelectedText();
        if (selected != null) {
          _clipboardText = selected;
          controller.replaceSelection('');
          _platformValue = controller.inputValue;
        }
      case LiveEditorScenarioKey.paste:
        final text = _clipboardText;
        if (text != null && text.isNotEmpty) {
          controller.replaceSelection(text);
          _platformValue = controller.inputValue;
        }
      case LiveEditorScenarioKey.undo:
        await controller.undo();
        _platformValue = controller.inputValue;
      case LiveEditorScenarioKey.redo:
        await controller.redo();
        _platformValue = controller.inputValue;
    }
  }

  @override
  Future<void> selectSourceRange({
    required int base,
    required int extent,
  }) async {
    await activateAtUtf16(base);
    _activeController.extendSelectionTo(extent);
    _platformValue = _activeController.inputValue;
  }

  @override
  Future<void> pasteText(String text) async {
    _activeController.replaceSelection(text);
    _platformValue = _activeController.inputValue;
  }

  @override
  Future<void> toggleTaskAtUtf16(int targetUtf16) async {
    final controller = _activeController;
    final row = controller.rows.firstWhere(
      (candidate) =>
          candidate.listItem?.taskChecked != null &&
          candidate.editableUtf16 != null &&
          candidate.editableUtf16!.start <= targetUtf16 &&
          targetUtf16 <= candidate.editableUtf16!.end,
      orElse: () => throw StateError(
        'task target $targetUtf16 is not in a certified task row',
      ),
    );
    if (!await controller.toggleTaskChecked(row)) {
      throw StateError('task target $targetUtf16 was not applicable');
    }
    _platformValue = controller.inputValue;
  }

  @override
  Future<void> scrollBy(int deltaY) async {
    // The no-window runner still proves that viewport-only input cannot alter
    // source, selection, history, or fault state. Higher runners additionally
    // prove that a mounted/native viewport actually moved.
  }

  @override
  Future<void> pause(Duration duration) async {
    if (duration > Duration.zero) await Future<void>.delayed(duration);
    final controller = _activeController;
    if (controller.pendingEdits == 0) {
      _platformValue = controller.inputValue;
    }
  }

  @override
  Future<void> awaitBarrier(LiveEditorScenarioBarrier barrier) async {
    await _settle(_activeController);
    _platformValue = _activeController.inputValue;
  }

  @override
  Future<LiveEditorScenarioSnapshot> snapshot() async {
    final controller = _activeController;
    final selection = await controller.resolveCanonicalSelection();
    if (selection == null) {
      throw StateError('canonical selection is unavailable');
    }
    final presentation = controller.rows.isEmpty
        ? '<empty>'
        : controller.rows
              .map(controller.surfaceRow)
              .map((row) => '${row.leadingText}${row.text}')
              .join('\n');
    return LiveEditorScenarioSnapshot(
      source: await controller.readSource(),
      selectionBaseUtf16: selection.base,
      selectionExtentUtf16: selection.extent,
      resyncCount: controller.resyncCount,
      faulted: controller.status == FlarkEditorStatus.faulted,
      lastError: controller.lastError,
      settledPresentation: presentation,
      paintedPresentations: const [],
      paintedRenderPlanHashes: const [],
      paintedVisualStateHashes: const [],
      revision: controller.revision,
      scrollOffset: null,
    );
  }

  @override
  Future<void> stop() async {
    final controller = _controller;
    _controller = null;
    _platformValue = null;
    _clipboardText = null;
    if (controller != null) await controller.close();
  }
}

Future<void> _settle(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  if (controller.pendingEdits == 0) await controller.continueParsing();
  if (controller.pendingEdits != 0) {
    throw StateError('scenario did not settle before its 5 second deadline');
  }
  if (controller.lastError case final error?) throw error;
}
