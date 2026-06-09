import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_policy.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import 'flark_flutter_controller.dart';

typedef FlarkTextSelectionReader = FlarkSelection? Function();
typedef FlarkTextSelectionApplier = void Function(FlarkSelection selection);

final class FlarkMarkdownInputPolicy {
  const FlarkMarkdownInputPolicy({
    required this.controller,
    required this.enterUserEvent,
    required this.backspaceUserEvent,
    this.onHandled,
  });

  final FlarkFlutterController controller;
  final String enterUserEvent;
  final String backspaceUserEvent;
  final VoidCallback? onHandled;

  bool get isEnabled {
    return controller.runtime.extensions
        .whereType<FlarkMarkdownInputEditingExtension>()
        .isNotEmpty;
  }

  Widget wrapKeyboardShortcuts({
    required Widget child,
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return child;
    return Actions(
      actions: {
        _FlarkMarkdownEnterIntent: CallbackAction<_FlarkMarkdownEnterIntent>(
          onInvoke: (intent) {
            dispatchEnter(
              currentSelection: currentSelection,
              applySelection: applySelection,
            );
            return null;
          },
        ),
        _FlarkMarkdownSoftLineBreakIntent:
            CallbackAction<_FlarkMarkdownSoftLineBreakIntent>(
              onInvoke: (intent) {
                dispatchSoftLineBreak(
                  currentSelection: currentSelection,
                  applySelection: applySelection,
                );
                return null;
              },
            ),
        _FlarkMarkdownBackspaceIntent:
            CallbackAction<_FlarkMarkdownBackspaceIntent>(
              onInvoke: (intent) {
                dispatchBackspace(
                  currentSelection: currentSelection,
                  applySelection: applySelection,
                );
                return null;
              },
            ),
        DeleteCharacterIntent: _FlarkMarkdownDeleteCharacterAction(
          onBackspace: () {
            return dispatchBackspace(
              currentSelection: currentSelection,
              applySelection: applySelection,
            );
          },
        ),
      },
      child: Shortcuts(
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.enter):
              _FlarkMarkdownEnterIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter):
              _FlarkMarkdownEnterIntent(),
          SingleActivator(LogicalKeyboardKey.enter, shift: true):
              _FlarkMarkdownSoftLineBreakIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter, shift: true):
              _FlarkMarkdownSoftLineBreakIntent(),
          SingleActivator(LogicalKeyboardKey.backspace):
              _FlarkMarkdownBackspaceIntent(),
        },
        child: child,
      ),
    );
  }

  bool handlePlatformTextChange({
    required String oldText,
    required TextEditingValue newValue,
    required FlarkSelection? oldTextSelection,
    required FlarkTextSelectionApplier applyOldTextSelection,
  }) {
    if (!isEnabled) return false;
    final diff = _FlarkTextEditDiff.between(oldText, newValue.text);
    if (diff == null) return false;

    final oldSelection = oldTextSelection;
    if (_isAutoClosedStandaloneFenceEcho(
      oldText: oldText,
      newValue: newValue,
    )) {
      final selectionBefore = FlarkSelection.collapsed(oldText.length);
      return dispatchEnter(
        currentSelection: () => selectionBefore,
        applySelection: applyOldTextSelection,
      );
    }

    if (diff.isLineBreakInsertion) {
      final fallbackSelection = diff.isInsertion
          ? FlarkSelection.collapsed(diff.oldStart)
          : FlarkSelection(
              baseOffset: diff.oldStart,
              extentOffset: diff.oldEnd,
            );
      final selectionBefore = oldSelection ?? fallbackSelection;
      if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
      applyOldTextSelection(selectionBefore);
      var handled = false;
      for (var index = 0; index < diff.lineBreakInsertionCount; index++) {
        final result = controller.dispatch(
          command: FlarkMarkdownInputCommands.handleEnter,
          payload: FlarkHandleEnterPayload(userEvent: enterUserEvent),
        );
        final didHandle = _finish(result);
        if (!didHandle) return handled;
        handled = true;
      }
      return handled;
    }

    if (diff.isInsertion) {
      final selectionBefore =
          oldSelection ?? FlarkSelection.collapsed(diff.oldStart);
      if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
      applyOldTextSelection(selectionBefore);
      final sourceInsertionOffset = controller.selection.start;
      final closerEdit =
          FlarkMarkdownFencedCodePolicy.autoOutdentCloserInsertion(
            markdown: controller.markdown,
            insertionOffset: sourceInsertionOffset,
            insertedText: diff.replacementText,
          );
      final pasteEdit = closerEdit == null
          ? FlarkMarkdownFencedCodePolicy.multilinePasteIndentation(
              markdown: controller.markdown,
              insertionOffset: sourceInsertionOffset,
              insertedText: diff.replacementText,
            )
          : null;
      final edit = closerEdit ?? pasteEdit;
      if (edit == null) return false;
      final isPaste = pasteEdit != null;
      controller.applyTransaction(
        FlarkTransaction.single(
          FlarkSourceOperation.replace(
            replacedRange: edit.range,
            replacementText: edit.replacementText,
          ),
          selectionBefore: controller.selection,
          selectionAfter: edit.selectionAfter,
          metadata: FlarkTransactionMetadata(
            intent: isPaste
                ? FlarkTransactionIntent.paste
                : FlarkTransactionIntent.input,
            userEvent: isPaste
                ? '$enterUserEvent.fencedCodePaste'
                : '$enterUserEvent.fencedCodeCloser',
            parseInvalidationRange: edit.range,
            projectionInvalidationRange: edit.range,
          ),
        ),
      );
      onHandled?.call();
      return true;
    }

    if (!diff.isDeletion) return false;
    final selectionBefore =
        oldSelection ?? FlarkSelection.collapsed(diff.oldEnd);
    if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
    return dispatchBackspace(
      currentSelection: () => selectionBefore,
      applySelection: applyOldTextSelection,
    );
  }

  bool dispatchEnter({
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final result = controller.dispatch(
      command: FlarkMarkdownInputCommands.handleEnter,
      payload: FlarkHandleEnterPayload(userEvent: enterUserEvent),
    );
    return _finish(result);
  }

  bool dispatchSoftLineBreak({
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);

    final sourceSelection = controller.selection;
    final range = FlarkSourceRange(sourceSelection.start, sourceSelection.end);
    controller.applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: range,
          replacementText: '\n',
        ),
        selectionBefore: sourceSelection,
        selectionAfter: FlarkSelection.collapsed(range.start + 1),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: '$enterUserEvent.softLineBreak',
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
    onHandled?.call();
    return true;
  }

  bool dispatchBackspace({
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final result = controller.dispatch(
      command: FlarkMarkdownInputCommands.handleBackspace,
      payload: FlarkHandleBackspacePayload(userEvent: backspaceUserEvent),
    );
    return _finish(result);
  }

  bool _selectionMatchesDiff(
    FlarkSelection selection,
    _FlarkTextEditDiff diff,
  ) {
    if (selection.isCollapsed) {
      return diff.isInsertion
          ? selection.extentOffset == diff.oldStart
          : selection.extentOffset == diff.oldEnd;
    }
    return selection.start == diff.oldStart && selection.end == diff.oldEnd;
  }

  bool _finish(FlarkEditorRuntimeResult result) {
    final handled =
        result.commandResult.isHandled &&
        result.commandResult.transaction != null;
    if (handled) onHandled?.call();
    return handled;
  }

  static FlarkSelection? selectionFromTextSelection(TextSelection selection) {
    if (!selection.isValid) return null;
    return FlarkSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }
}

bool _isAutoClosedStandaloneFenceEcho({
  required String oldText,
  required TextEditingValue newValue,
}) {
  if (!_isCollapsedTextSelectionAt(newValue.selection, newValue.text.length)) {
    return false;
  }
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(oldText);
  if (fence == null || !fence.canClose) return false;
  final markerText =
      fence.indent + List.filled(fence.markerLength, fence.marker).join();
  return newValue.text == '$oldText\n$markerText\n' ||
      newValue.text == '$oldText\n$markerText';
}

bool _isCollapsedTextSelectionAt(TextSelection selection, int offset) {
  return selection.isValid &&
      selection.isCollapsed &&
      selection.extentOffset == offset;
}

final class _FlarkTextEditDiff {
  const _FlarkTextEditDiff({
    required this.oldStart,
    required this.oldEnd,
    required this.replacementText,
  });

  final int oldStart;
  final int oldEnd;
  final String replacementText;

  bool get isInsertion => oldStart == oldEnd && replacementText.isNotEmpty;

  bool get isDeletion => replacementText.isEmpty && oldEnd > oldStart;

  bool get isLineBreakInsertion =>
      isInsertion && _isOnlyLineBreaks(replacementText);

  int get lineBreakInsertionCount => _lineBreakCount(replacementText);

  static _FlarkTextEditDiff? between(String oldText, String newText) {
    if (oldText == newText) return null;

    var prefixLength = 0;
    final sharedPrefixLimit = oldText.length < newText.length
        ? oldText.length
        : newText.length;
    while (prefixLength < sharedPrefixLimit &&
        oldText.codeUnitAt(prefixLength) == newText.codeUnitAt(prefixLength)) {
      prefixLength++;
    }

    var oldSuffix = oldText.length;
    var newSuffix = newText.length;
    while (oldSuffix > prefixLength &&
        newSuffix > prefixLength &&
        oldText.codeUnitAt(oldSuffix - 1) ==
            newText.codeUnitAt(newSuffix - 1)) {
      oldSuffix--;
      newSuffix--;
    }

    return _FlarkTextEditDiff(
      oldStart: prefixLength,
      oldEnd: oldSuffix,
      replacementText: newText.substring(prefixLength, newSuffix),
    );
  }
}

bool _isOnlyLineBreaks(String text) {
  if (text.isEmpty) return false;
  var index = 0;
  while (index < text.length) {
    final codeUnit = text.codeUnitAt(index);
    if (codeUnit == 0x0D) {
      index++;
      if (index < text.length && text.codeUnitAt(index) == 0x0A) index++;
      continue;
    }
    if (codeUnit == 0x0A) {
      index++;
      continue;
    }
    return false;
  }
  return true;
}

int _lineBreakCount(String text) {
  var count = 0;
  var index = 0;
  while (index < text.length) {
    final codeUnit = text.codeUnitAt(index);
    if (codeUnit == 0x0D) {
      count++;
      index++;
      if (index < text.length && text.codeUnitAt(index) == 0x0A) index++;
      continue;
    }
    if (codeUnit == 0x0A) {
      count++;
      index++;
      continue;
    }
    index++;
  }
  return count;
}

final class _FlarkMarkdownEnterIntent extends Intent {
  const _FlarkMarkdownEnterIntent();
}

final class _FlarkMarkdownSoftLineBreakIntent extends Intent {
  const _FlarkMarkdownSoftLineBreakIntent();
}

final class _FlarkMarkdownBackspaceIntent extends Intent {
  const _FlarkMarkdownBackspaceIntent();
}

final class _FlarkMarkdownDeleteCharacterAction
    extends Action<DeleteCharacterIntent> {
  _FlarkMarkdownDeleteCharacterAction({required this.onBackspace});

  final bool Function() onBackspace;

  @override
  Object? invoke(DeleteCharacterIntent intent) {
    if (!intent.forward && onBackspace()) return null;
    return callingAction?.invoke(intent);
  }
}
