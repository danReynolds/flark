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
    this.forwardDeleteUserEvent = 'input.forwardDelete',
    this.onHandled,
  });

  final FlarkFlutterController controller;
  final String enterUserEvent;
  final String backspaceUserEvent;
  final String forwardDeleteUserEvent;
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
          onForwardDelete: () {
            return dispatchForwardDelete(
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
      _stepOutOfInlineRunBeforeLineBreak();
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
    _stepOutOfInlineRunBeforeLineBreak();
    final result = controller.dispatch(
      command: FlarkMarkdownInputCommands.handleEnter,
      payload: FlarkHandleEnterPayload(userEvent: enterUserEvent),
    );
    return _finish(result);
  }

  /// A line break with the caret inside a styled run's trailing edge would
  /// split the run's source and orphan its markers as literal text (a code
  /// span cannot contain a blank line). Inline runs are line-scoped, so
  /// Enter first steps the caret past the closing marker and then splits.
  void _stepOutOfInlineRunBeforeLineBreak() {
    final selection = controller.selection;
    if (!selection.isCollapsed) return;
    final offset = selection.extentOffset;
    final projection = controller.projection;
    if (offset < 0 || offset > projection.textLength) return;
    final marker = projection.inlineRunClosingMarkerAt(offset);
    if (marker == null) return;
    controller.applySelection(
      FlarkSelection.collapsed(marker.end),
      userEvent: 'selection.inlineRunLineBreakExit',
    );
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

  /// Routes a Backspace through the projection's boundary-aware deletion
  /// Canonicalizes a resolved non-collapsed deletion [range] through the
  /// inline placement repairs, so a keyboard deletion never leaves invalid
  /// markdown (stranded edge whitespace, fused adjacent runs, orphaned
  /// crossing markers). Returns true when a repair was applied and consumed
  /// the key; false when the plain deletion of [range] is already valid and
  /// the caller should perform it.
  bool _canonicalizeResolvedDeletion(FlarkSourceRange range, String userEvent) {
    if (!controller.applyResolvedInlineDeletion(range, userEvent: userEvent)) {
      return false;
    }
    onHandled?.call();
    return true;
  }

  /// The source range a Backspace would delete when it is an inline deletion
  /// worth canonicalizing — never a block-structural Backspace:
  ///
  ///  * the resolver's non-collapsed range directly (`**foo x**|` → delete the
  ///    `x`, canonicalized to `**foo** `); or
  ///  * when the resolver stepped the caret before a run's opening marker
  ///    (`**a** **|b**` → caret before `**b**`), the single grapheme before
  ///    the step (the gap that a join would consume), unless that grapheme is
  ///    a line break — a line merge stays with the engine.
  ///
  /// Null when the Backspace is not an inline deletion (unchanged resolution,
  /// document start, or a line merge).
  FlarkSourceRange? _backspaceRepairRange(
    FlarkSelection before,
    FlarkSelection resolved,
  ) {
    if (!resolved.isCollapsed) {
      return FlarkSourceRange(resolved.start, resolved.end);
    }
    final at = resolved.extentOffset;
    if (resolved == before || at <= 0) return null;
    if (controller.markdown.codeUnitAt(at - 1) == 0x0A) return null;
    return FlarkSourceRange(at - 1, at);
  }

  static bool _isHighSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;
  static bool _isLowSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;

  /// Expands a resolved deletion so it never starts or ends inside a surrogate
  /// pair. The boundary-aware resolvers build single-UTF-16-code-unit ranges
  /// when stepping past a run's hidden markers; without this, a forward Delete
  /// before an emoji-led run (`|**😀x**`) or a Backspace past a run closing on
  /// an emoji (`**x😀**|`) would delete half the surrogate pair and corrupt the
  /// text. The platform default handles the non-stepped path, so this only
  /// covers the resolver-adjusted ranges. Aligned and collapsed selections are
  /// returned unchanged.
  FlarkSelection _graphemeSafeDeletion(FlarkSelection resolved) {
    if (resolved.isCollapsed) return resolved;
    final source = controller.markdown;
    var start = resolved.start;
    var end = resolved.end;
    if (start > 0 &&
        start < source.length &&
        _isLowSurrogate(source.codeUnitAt(start)) &&
        _isHighSurrogate(source.codeUnitAt(start - 1))) {
      start -= 1;
    }
    if (end > 0 &&
        end < source.length &&
        _isHighSurrogate(source.codeUnitAt(end - 1)) &&
        _isLowSurrogate(source.codeUnitAt(end))) {
      end += 1;
    }
    if (start == resolved.start && end == resolved.end) return resolved;
    final inverted = resolved.baseOffset > resolved.extentOffset;
    return inverted
        ? FlarkSelection(baseOffset: end, extentOffset: start)
        : FlarkSelection(baseOffset: start, extentOffset: end);
  }

  bool dispatchBackspace({
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final sourceSelection = controller.selection;
    final resolvedRaw = controller.projection.resolveBackspaceSelection(
      sourceSelection,
    );
    // A marker-adjacent step must not leave the deletion splitting a surrogate
    // pair; the unchanged (defer) case is passed through untouched.
    final resolved = resolvedRaw == sourceSelection
        ? resolvedRaw
        : _graphemeSafeDeletion(resolvedRaw);
    // Canonicalize the effective inline deletion (edge whitespace, joins,
    // crossings) before it reaches the engine so the source stays valid.
    final repairRange = _backspaceRepairRange(sourceSelection, resolved);
    if (repairRange != null &&
        _canonicalizeResolvedDeletion(repairRange, backspaceUserEvent)) {
      return true;
    }
    // Marker-aware but not repaired (a plain range delete, or a collapsed
    // step past an opening marker): apply the resolved selection and let the
    // engine's block-aware Backspace (lists, headings, quotes) run.
    if (resolved != sourceSelection) {
      controller.applySelection(
        resolved,
        userEvent: 'selection.inlineRunDeletion',
      );
    }
    final result = controller.dispatch(
      command: FlarkMarkdownInputCommands.handleBackspace,
      payload: FlarkHandleBackspacePayload(userEvent: backspaceUserEvent),
    );
    return _finish(result);
  }

  /// Routes a forward Delete through the projection's boundary-aware deletion
  /// resolver — the mirror of [dispatchBackspace], built on
  /// `FlarkProjection.resolveForwardDeleteSelection`.
  ///
  /// When the resolver adjusts the selection (the deletion would otherwise
  /// split or orphan a styled run's hidden markers), the resolved range is
  /// canonicalized through the placement repairs (or plain-deleted) and the
  /// intent is consumed. When no inline-run marker is adjacent the resolver
  /// returns the selection unchanged and this returns false, so the caller
  /// falls through to its default forward delete (grapheme-aware character
  /// removal, display-space line merges).
  bool dispatchForwardDelete({
    required FlarkTextSelectionReader currentSelection,
    required FlarkTextSelectionApplier applySelection,
  }) {
    if (!isEnabled) return false;
    final selection = currentSelection();
    if (selection != null) applySelection(selection);
    final sourceSelection = controller.selection;
    final resolvedRaw = controller.projection.resolveForwardDeleteSelection(
      sourceSelection,
    );
    if (resolvedRaw == sourceSelection) return false;
    if (resolvedRaw.isCollapsed) {
      // Only hidden markers separated the caret from the document end; the
      // caret stepped past them and there is nothing left to delete, so let
      // the caller's default forward delete no-op at the true position.
      controller.applySelection(
        resolvedRaw,
        userEvent: 'selection.inlineRunDeletion',
      );
      return false;
    }
    // A marker-adjacent step must not split a surrogate pair (an emoji at the
    // run's content edge) — clamp the resolved range to grapheme boundaries.
    final resolved = _graphemeSafeDeletion(resolvedRaw);
    // Canonicalize the resolved deletion (edge whitespace, joins, crossings);
    // fall back to the plain range delete when no repair applies.
    if (_canonicalizeResolvedDeletion(
      FlarkSourceRange(resolved.start, resolved.end),
      forwardDeleteUserEvent,
    )) {
      return true;
    }
    controller.applySelection(
      resolved,
      userEvent: 'selection.inlineRunDeletion',
    );
    // The engine deletes a non-collapsed selection exactly, so the resolved
    // range never re-enters block-aware Backspace handling.
    final result = controller.dispatch(
      command: FlarkMarkdownInputCommands.handleBackspace,
      payload: FlarkHandleBackspacePayload(userEvent: forwardDeleteUserEvent),
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
  _FlarkMarkdownDeleteCharacterAction({
    required this.onBackspace,
    required this.onForwardDelete,
  });

  final bool Function() onBackspace;
  final bool Function() onForwardDelete;

  @override
  Object? invoke(DeleteCharacterIntent intent) {
    if (intent.forward) {
      if (onForwardDelete()) return null;
    } else if (onBackspace()) {
      return null;
    }
    return callingAction?.invoke(intent);
  }
}
