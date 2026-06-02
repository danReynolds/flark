import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/sovereign_markdown_fenced_code_policy.dart';
import 'sovereign_flutter_controller.dart';

typedef SovereignTextSelectionReader = SovereignSelection? Function();
typedef SovereignTextSelectionApplier = void Function(
  SovereignSelection selection,
);

final class SovereignMarkdownInputPolicy {
  const SovereignMarkdownInputPolicy({
    required this.controller,
    required this.enterUserEvent,
    required this.backspaceUserEvent,
    this.onHandled,
  });

  final SovereignFlutterController controller;
  final String enterUserEvent;
  final String backspaceUserEvent;
  final VoidCallback? onHandled;

  bool get isEnabled {
    return controller.runtime.extensions
        .whereType<SovereignMarkdownInputEditingExtension>()
        .isNotEmpty;
  }

  Widget wrapKeyboardShortcuts({
    required Widget child,
    required SovereignTextSelectionReader currentSelection,
    required SovereignTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return child;
    return Actions(
      actions: {
        _SovereignMarkdownEnterIntent:
            CallbackAction<_SovereignMarkdownEnterIntent>(
          onInvoke: (intent) {
            dispatchEnter(
              currentSelection: currentSelection,
              applySelection: applySelection,
            );
            return null;
          },
        ),
        _SovereignMarkdownSoftLineBreakIntent:
            CallbackAction<_SovereignMarkdownSoftLineBreakIntent>(
          onInvoke: (intent) {
            dispatchSoftLineBreak(
              currentSelection: currentSelection,
              applySelection: applySelection,
            );
            return null;
          },
        ),
        _SovereignMarkdownBackspaceIntent:
            CallbackAction<_SovereignMarkdownBackspaceIntent>(
          onInvoke: (intent) {
            dispatchBackspace(
              currentSelection: currentSelection,
              applySelection: applySelection,
            );
            return null;
          },
        ),
        DeleteCharacterIntent: _SovereignMarkdownDeleteCharacterAction(
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
              _SovereignMarkdownEnterIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter):
              _SovereignMarkdownEnterIntent(),
          SingleActivator(LogicalKeyboardKey.enter, shift: true):
              _SovereignMarkdownSoftLineBreakIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter, shift: true):
              _SovereignMarkdownSoftLineBreakIntent(),
          SingleActivator(LogicalKeyboardKey.backspace):
              _SovereignMarkdownBackspaceIntent(),
        },
        child: child,
      ),
    );
  }

  bool handlePlatformTextChange({
    required String oldText,
    required TextEditingValue newValue,
    required SovereignSelection? oldTextSelection,
    required SovereignTextSelectionApplier applyOldTextSelection,
  }) {
    if (!isEnabled) return false;
    final diff = _SovereignTextEditDiff.between(oldText, newValue.text);
    if (diff == null) return false;

    final oldSelection = oldTextSelection;
    if (diff.isNewlineInsertion) {
      final fallbackSelection = diff.isInsertion
          ? SovereignSelection.collapsed(diff.oldStart)
          : SovereignSelection(
              baseOffset: diff.oldStart,
              extentOffset: diff.oldEnd,
            );
      final selectionBefore = oldSelection ?? fallbackSelection;
      if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
      return dispatchEnter(
        currentSelection: () => selectionBefore,
        applySelection: applyOldTextSelection,
      );
    }

    if (diff.isInsertion) {
      final selectionBefore =
          oldSelection ?? SovereignSelection.collapsed(diff.oldStart);
      if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
      applyOldTextSelection(selectionBefore);
      final sourceInsertionOffset = controller.selection.start;
      final closerEdit =
          SovereignMarkdownFencedCodePolicy.autoOutdentCloserInsertion(
        markdown: controller.markdown,
        insertionOffset: sourceInsertionOffset,
        insertedText: diff.replacementText,
      );
      final pasteEdit = closerEdit == null
          ? SovereignMarkdownFencedCodePolicy.multilinePasteIndentation(
              markdown: controller.markdown,
              insertionOffset: sourceInsertionOffset,
              insertedText: diff.replacementText,
            )
          : null;
      final edit = closerEdit ?? pasteEdit;
      if (edit == null) return false;
      final isPaste = pasteEdit != null;
      controller.applyTransaction(
        SovereignTransaction.single(
          SovereignSourceOperation.replace(
            replacedRange: edit.range,
            replacementText: edit.replacementText,
          ),
          selectionBefore: controller.selection,
          selectionAfter: edit.selectionAfter,
          metadata: SovereignTransactionMetadata(
            intent: isPaste
                ? SovereignTransactionIntent.paste
                : SovereignTransactionIntent.input,
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
        oldSelection ?? SovereignSelection.collapsed(diff.oldEnd);
    if (!_selectionMatchesDiff(selectionBefore, diff)) return false;
    return dispatchBackspace(
      currentSelection: () => selectionBefore,
      applySelection: applyOldTextSelection,
    );
  }

  bool dispatchEnter({
    required SovereignTextSelectionReader currentSelection,
    required SovereignTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final result = controller.dispatch(
      command: SovereignMarkdownInputCommands.handleEnter,
      payload: SovereignHandleEnterPayload(userEvent: enterUserEvent),
    );
    return _finish(result);
  }

  bool dispatchSoftLineBreak({
    required SovereignTextSelectionReader currentSelection,
    required SovereignTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);

    final sourceSelection = controller.selection;
    final range = SovereignSourceRange(
      sourceSelection.start,
      sourceSelection.end,
    );
    controller.applyTransaction(
      SovereignTransaction.single(
        SovereignSourceOperation.replace(
          replacedRange: range,
          replacementText: '\n',
        ),
        selectionBefore: sourceSelection,
        selectionAfter: SovereignSelection.collapsed(range.start + 1),
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.input,
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
    required SovereignTextSelectionReader currentSelection,
    required SovereignTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final result = controller.dispatch(
      command: SovereignMarkdownInputCommands.handleBackspace,
      payload: SovereignHandleBackspacePayload(userEvent: backspaceUserEvent),
    );
    return _finish(result);
  }

  bool _selectionMatchesDiff(
    SovereignSelection selection,
    _SovereignTextEditDiff diff,
  ) {
    if (selection.isCollapsed) {
      return diff.isInsertion
          ? selection.extentOffset == diff.oldStart
          : selection.extentOffset == diff.oldEnd;
    }
    return selection.start == diff.oldStart && selection.end == diff.oldEnd;
  }

  bool _finish(SovereignEditorRuntimeResult result) {
    final handled = result.commandResult.isHandled &&
        result.commandResult.transaction != null;
    if (handled) onHandled?.call();
    return handled;
  }

  static SovereignSelection? selectionFromTextSelection(
    TextSelection selection,
  ) {
    if (!selection.isValid) return null;
    return SovereignSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }
}

final class _SovereignTextEditDiff {
  const _SovereignTextEditDiff({
    required this.oldStart,
    required this.oldEnd,
    required this.replacementText,
  });

  final int oldStart;
  final int oldEnd;
  final String replacementText;

  bool get isInsertion => oldStart == oldEnd && replacementText.isNotEmpty;

  bool get isDeletion => replacementText.isEmpty && oldEnd > oldStart;

  bool get isNewlineInsertion => replacementText == '\n';

  static _SovereignTextEditDiff? between(String oldText, String newText) {
    if (oldText == newText) return null;

    var prefixLength = 0;
    final sharedPrefixLimit =
        oldText.length < newText.length ? oldText.length : newText.length;
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

    return _SovereignTextEditDiff(
      oldStart: prefixLength,
      oldEnd: oldSuffix,
      replacementText: newText.substring(prefixLength, newSuffix),
    );
  }
}

final class _SovereignMarkdownEnterIntent extends Intent {
  const _SovereignMarkdownEnterIntent();
}

final class _SovereignMarkdownSoftLineBreakIntent extends Intent {
  const _SovereignMarkdownSoftLineBreakIntent();
}

final class _SovereignMarkdownBackspaceIntent extends Intent {
  const _SovereignMarkdownBackspaceIntent();
}

final class _SovereignMarkdownDeleteCharacterAction
    extends Action<DeleteCharacterIntent> {
  _SovereignMarkdownDeleteCharacterAction({required this.onBackspace});

  final bool Function() onBackspace;

  @override
  Object? invoke(DeleteCharacterIntent intent) {
    if (!intent.forward && onBackspace()) return null;
    return callingAction?.invoke(intent);
  }
}
