// The edit-intent classifiers for live-rendered editing.
//
// Flutter text input is a round trip with the platform IME: when Flark
// intercepts an edit (Enter on a fence opening line, a language shortcut, an
// auto-closed fence) and rewrites the document itself, the platform may still
// deliver its own version of the change afterwards — an echo that must be
// recognized and swallowed or it corrupts the document. These classifiers
// resolve one incoming TextEditingValue into a single typed intent, so the
// recognizer ordering lives in exactly one place per surface and is
// table-testable without pumping widgets.
//
// This is a standalone library on purpose: it cannot import the editor
// widgets (that would be a circular import), so "pure function of its
// inputs" is a structural guarantee, not a review convention. The widgets
// resolve the context, classify, and execute the intent's side effects.
//
// The two recognizer sets intentionally remain separate functions: almost
// every recognizer is specific to one surface granularity, and each chain's
// ordering is its safety-critical property. The full recognizer matrix —
// every host/block asymmetry named intentional or convergence candidate —
// and the device protocol gating behavioral convergence live in
// doc/architecture/live_edit_intent_pipeline.md.

import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../render_plan/render_plan.dart';
import 'flark_live_block_source_edit.dart';
import 'flark_live_code_fence_input_policy.dart';
import 'flark_markdown_input_policy.dart';

// ---------------------------------------------------------------------------
// Block surface
// ---------------------------------------------------------------------------

/// Everything the block classifier is allowed to know.
final class FlarkLiveBlockEditContext {
  const FlarkLiveBlockEditContext({
    required this.markdown,
    required this.block,
    required this.displayText,
    required this.sourceRange,
    required this.oldValue,
    required this.newValue,
    required this.markdownInputPolicyEnabled,
    required this.pendingCodeBodyEchoText,
  });

  final String markdown;
  final FlarkRenderBlock block;
  final String displayText;

  /// The block's direct source-edit range, if it edits raw source.
  final FlarkSourceRange? sourceRange;

  /// The last value the widget knew about (snapshot-resolved).
  final TextEditingValue oldValue;

  /// The value the platform just delivered.
  final TextEditingValue newValue;

  final bool markdownInputPolicyEnabled;

  /// Pending code-body echo recorded after a policy-handled structural edit.
  final String? pendingCodeBodyEchoText;
}

/// Why a [FlarkLiveBlockResyncIntent] decided the change was an echo.
enum FlarkLiveBlockResyncReason {
  pendingCodeBodyEcho,
  openingEmptyUnclosedNewlineEcho,
  languageShortcutEcho,
  trailingLineBreakEcho,
  normalizedPlatformLineBreakEcho,
}

sealed class FlarkLiveBlockEditIntent {
  const FlarkLiveBlockEditIntent();
}

/// The change is a platform echo of state Flark already applied — discard it
/// and re-sync the editable from the controller.
final class FlarkLiveBlockResyncIntent extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockResyncIntent(this.reason);

  final FlarkLiveBlockResyncReason reason;
}

/// Move the source caret (Enter on a fence opening line whose body exists).
final class FlarkLiveBlockCaretMoveIntent extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockCaretMoveIntent({
    required this.selection,
    required this.userEvent,
  });

  final FlarkSelection selection;
  final String userEvent;
}

/// Dispatch a markdown Enter (fence opening line without a body).
final class FlarkLiveBlockEnterDispatchIntent extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockEnterDispatchIntent({required this.currentSelection});

  /// The local selection the Enter applies at.
  final FlarkSelection currentSelection;
}

/// Apply a fully computed source edit (code-fence language shortcut).
final class FlarkLiveBlockLanguageShortcutIntent
    extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockLanguageShortcutIntent({required this.edit});

  final FlarkLiveBlockSourceEdit edit;
}

/// Replace the block's direct source range with the new local text. The
/// widget resolves its per-block-type edit hook (tables, code bodies) or the
/// default replacement.
final class FlarkLiveBlockDirectReplacementIntent
    extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockDirectReplacementIntent({required this.sourceRange});

  final FlarkSourceRange sourceRange;
}

/// Apply the change as a projected display-text edit of the whole document.
final class FlarkLiveBlockProjectedEditIntent extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockProjectedEditIntent({
    required this.blockValue,
    required this.adoptBlockValue,
    required this.oldDisplayText,
    required this.newDisplayText,
    required this.immediateParseAfterApply,
  });

  /// The local value to edit with (a completed standalone fence opener may
  /// rewrite it).
  final TextEditingValue blockValue;

  /// Whether [blockValue] differs from the normalized value and must be
  /// adopted into the text controller before applying.
  final bool adoptBlockValue;

  final String oldDisplayText;
  final String newDisplayText;
  final bool immediateParseAfterApply;
}

/// Offer the change to the markdown input policy (Enter/Backspace
/// structural handling) first; execute [fallback] when it declines.
final class FlarkLiveBlockPlatformTextChangeIntent
    extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockPlatformTextChangeIntent({
    required this.policyValue,
    required this.oldText,
    required this.oldTextSelection,
    required this.resyncWhenHandled,
    required this.fallback,
  });

  final TextEditingValue policyValue;
  final String oldText;
  final FlarkSelection? oldTextSelection;

  /// True when the policy value was line-break-normalized, so a handled
  /// change must re-sync the editable afterwards.
  final bool resyncWhenHandled;

  final FlarkLiveBlockEditIntent fallback;
}

/// Selection-only change inside a direct source range.
final class FlarkLiveBlockSourceSelectionIntent
    extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockSourceSelectionIntent({
    required this.selection,
    required this.snapshotRange,
  });

  final FlarkSelection selection;
  final FlarkSourceRange snapshotRange;
}

/// Selection-only change mapped through the projection.
final class FlarkLiveBlockProjectedSelectionIntent
    extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockProjectedSelectionIntent({required this.selection});

  final FlarkSelection selection;
}

/// Nothing actionable (invalid selection).
final class FlarkLiveBlockIgnoreIntent extends FlarkLiveBlockEditIntent {
  const FlarkLiveBlockIgnoreIntent();
}

/// The classifier's complete answer for one incoming value.
final class FlarkLiveBlockEditClassification {
  const FlarkLiveBlockEditClassification({
    required this.normalizedValue,
    required this.nextPendingCodeBodyEchoText,
    required this.intent,
  });

  /// The platform value after pure-insertion and line-break normalization;
  /// the widget adopts this into its text controller before executing.
  final TextEditingValue normalizedValue;

  /// The pending code-body echo slot after classification (unchanged unless
  /// the pending-echo recognizer ran).
  final String? nextPendingCodeBodyEchoText;

  final FlarkLiveBlockEditIntent intent;
}

/// Classifies one platform text change on a live block editable.
///
/// This is the single ordered recognizer chain for the block surface. The
/// order is load-bearing: each recognizer assumes the earlier ones did not
/// match. Behavior is identical to the previous open-coded chain in
/// `_EditableProjectedBlockTextState._handleTextChanged`.
FlarkLiveBlockEditClassification classifyFlarkLiveBlockEdit(
  FlarkLiveBlockEditContext context,
) {
  final oldLocalText = context.oldValue.text;
  final oldLocalSelection = context.oldValue.selection;
  var value = flarkTextValueWithPureInsertionSelection(
    oldText: oldLocalText,
    oldSelection: oldLocalSelection,
    newValue: context.newValue,
    normalizeAutoClosedFenceEcho: context.markdownInputPolicyEnabled,
  );
  value =
      FlarkLiveCodeFenceInputPolicy.normalizeLineBreakInsertionValue(
        block: context.block,
        oldText: oldLocalText,
        value: value,
      ) ??
      value;

  var nextPendingEchoText = context.pendingCodeBodyEchoText;

  FlarkLiveBlockEditClassification finish(FlarkLiveBlockEditIntent intent) {
    return FlarkLiveBlockEditClassification(
      normalizedValue: value,
      nextPendingCodeBodyEchoText: nextPendingEchoText,
      intent: intent,
    );
  }

  if (value.text != oldLocalText) {
    var offerToPolicy = false;
    var policyValue = value;
    var resyncWhenHandled = false;

    if (context.markdownInputPolicyEnabled) {
      final pendingEcho = FlarkLiveCodeFenceInputPolicy.consumePendingEcho(
        pendingText: context.pendingCodeBodyEchoText,
        markdown: context.markdown,
        block: context.block,
        value: value,
      );
      nextPendingEchoText = pendingEcho.nextPendingText;
      if (pendingEcho.consumed) {
        return finish(
          const FlarkLiveBlockResyncIntent(
            FlarkLiveBlockResyncReason.pendingCodeBodyEcho,
          ),
        );
      }
      if (FlarkLiveCodeFenceInputPolicy.isOpeningLinePlatformEnter(
        markdown: context.markdown,
        block: context.block,
        range: context.sourceRange,
        oldText: oldLocalText,
        newValue: value,
      )) {
        final existingBodyStart =
            FlarkLiveCodeFenceInputPolicy.existingBodyStartAfterOpeningLine(
              markdown: context.markdown,
              block: context.block,
              range: context.sourceRange,
            );
        if (existingBodyStart != null) {
          return finish(
            FlarkLiveBlockCaretMoveIntent(
              selection: FlarkSelection.collapsed(existingBodyStart),
              userEvent: 'selection.liveBlock.codeFenceOpeningEnter',
            ),
          );
        }
        return finish(
          FlarkLiveBlockEnterDispatchIntent(
            currentSelection:
                FlarkMarkdownInputPolicy.selectionFromTextSelection(
                  oldLocalSelection,
                ) ??
                FlarkSelection.collapsed(oldLocalText.length),
          ),
        );
      }
      if (FlarkLiveCodeFenceInputPolicy.isOpeningEmptyUnclosedNewlineEcho(
        markdown: context.markdown,
        block: context.block,
        range: context.sourceRange,
        oldText: oldLocalText,
        newValue: value,
      )) {
        return finish(
          const FlarkLiveBlockResyncIntent(
            FlarkLiveBlockResyncReason.openingEmptyUnclosedNewlineEcho,
          ),
        );
      }
      if (FlarkLiveCodeFenceInputPolicy.isLanguageShortcutPlatformEcho(
        markdown: context.markdown,
        block: context.block,
        range: context.sourceRange,
        oldText: oldLocalText,
        value: value,
      )) {
        return finish(
          const FlarkLiveBlockResyncIntent(
            FlarkLiveBlockResyncReason.languageShortcutEcho,
          ),
        );
      }
      final languageShortcutEdit =
          FlarkLiveCodeFenceInputPolicy.languageShortcutEdit(
            markdown: context.markdown,
            block: context.block,
            range: context.sourceRange,
            oldText: oldLocalText,
            value: value,
          );
      if (languageShortcutEdit != null) {
        return finish(
          FlarkLiveBlockLanguageShortcutIntent(edit: languageShortcutEdit),
        );
      }
      if (FlarkLiveCodeFenceInputPolicy.isTrailingLineBreakPlatformEcho(
        markdown: context.markdown,
        block: context.block,
        range: context.sourceRange,
        oldText: oldLocalText,
        value: value,
      )) {
        return finish(
          const FlarkLiveBlockResyncIntent(
            FlarkLiveBlockResyncReason.trailingLineBreakEcho,
          ),
        );
      }
      final platformTextValue =
          FlarkLiveCodeFenceInputPolicy.normalizePlatformLineBreakValue(
            markdown: context.markdown,
            block: context.block,
            range: context.sourceRange,
            oldText: oldLocalText,
            value: value,
          );
      if (platformTextValue != null &&
          FlarkLiveCodeFenceInputPolicy.sourceTextEquals(
            markdown: context.markdown,
            block: context.block,
            text: platformTextValue.text,
          )) {
        return finish(
          const FlarkLiveBlockResyncIntent(
            FlarkLiveBlockResyncReason.normalizedPlatformLineBreakEcho,
          ),
        );
      }
      policyValue = platformTextValue ?? value;
      resyncWhenHandled = platformTextValue != null;
      offerToPolicy =
          !FlarkLiveCodeFenceInputPolicy.shouldHandleTypedClosingFence(
            markdown: context.markdown,
            block: context.block,
            value: policyValue,
          );
    }

    final FlarkLiveBlockEditIntent fallback;
    final sourceRange = context.sourceRange;
    if (sourceRange != null) {
      fallback = FlarkLiveBlockDirectReplacementIntent(
        sourceRange: sourceRange,
      );
    } else {
      final completedStandaloneFenceValue = context.markdownInputPolicyEnabled
          ? FlarkLiveCodeFenceInputPolicy.valueAfterCompletingStandaloneOpener(
              oldDisplayText: oldLocalText,
              oldSelection: oldLocalSelection,
              newValue: value,
            )
          : null;
      final blockValue = completedStandaloneFenceValue ?? value;
      final range = flarkClampedDisplayRange(
        context.block,
        context.displayText,
      );
      fallback = FlarkLiveBlockProjectedEditIntent(
        blockValue: blockValue,
        adoptBlockValue: completedStandaloneFenceValue != null,
        oldDisplayText: context.displayText,
        newDisplayText: context.displayText.replaceRange(
          range.start,
          range.end,
          blockValue.text,
        ),
        immediateParseAfterApply: completedStandaloneFenceValue != null,
      );
    }

    if (!offerToPolicy) return finish(fallback);
    return finish(
      FlarkLiveBlockPlatformTextChangeIntent(
        policyValue: policyValue,
        oldText: oldLocalText,
        oldTextSelection: FlarkMarkdownInputPolicy.selectionFromTextSelection(
          oldLocalSelection,
        ),
        resyncWhenHandled: resyncWhenHandled,
        fallback: fallback,
      ),
    );
  }

  final selection = value.selection;
  if (!selection.isValid) return finish(const FlarkLiveBlockIgnoreIntent());
  final sourceRange = context.sourceRange;
  if (sourceRange != null) {
    return finish(
      FlarkLiveBlockSourceSelectionIntent(
        selection: FlarkSelection(
          baseOffset: sourceRange.start + selection.baseOffset,
          extentOffset: sourceRange.start + selection.extentOffset,
        ),
        snapshotRange: sourceRange,
      ),
    );
  }
  final range = flarkClampedDisplayRange(context.block, context.displayText);
  return finish(
    FlarkLiveBlockProjectedSelectionIntent(
      selection: FlarkSelection(
        baseOffset: range.start + selection.baseOffset,
        extentOffset: range.start + selection.extentOffset,
      ),
    ),
  );
}

// ---------------------------------------------------------------------------
// Host surface
// ---------------------------------------------------------------------------

/// Everything the host classifier is allowed to know.
final class FlarkHostEditContext {
  const FlarkHostEditContext({
    required this.markdown,
    required this.oldDisplayText,
    required this.oldDisplaySelection,
    required this.newValue,
    required this.liveRendered,
  });

  final String markdown;
  final String oldDisplayText;
  final FlarkSelection oldDisplaySelection;
  final TextEditingValue newValue;
  final bool liveRendered;
}

sealed class FlarkHostEditIntent {
  const FlarkHostEditIntent();
}

/// Replace the whole source document (an auto-closed fence echo carried the
/// complete document text).
final class FlarkHostWholeDocumentReplaceIntent extends FlarkHostEditIntent {
  const FlarkHostWholeDocumentReplaceIntent({
    required this.replacementMarkdown,
  });

  final String replacementMarkdown;
}

/// Offer the change to the markdown input policy first; execute [fallback]
/// when it declines.
final class FlarkHostPlatformTextChangeIntent extends FlarkHostEditIntent {
  const FlarkHostPlatformTextChangeIntent({
    required this.policyValue,
    required this.oldText,
    required this.oldTextSelection,
    required this.fallback,
  });

  final TextEditingValue policyValue;
  final String oldText;
  final FlarkSelection? oldTextSelection;
  final FlarkHostProjectedEditIntent fallback;
}

/// Apply the change as a projected display-text edit.
final class FlarkHostProjectedEditIntent extends FlarkHostEditIntent {
  const FlarkHostProjectedEditIntent({
    required this.oldDisplayText,
    required this.newDisplayText,
    required this.immediateParseAfterApply,
  });

  final String oldDisplayText;
  final String newDisplayText;
  final bool immediateParseAfterApply;
}

/// Selection-only change mapped through the projection.
final class FlarkHostProjectedSelectionIntent extends FlarkHostEditIntent {
  const FlarkHostProjectedSelectionIntent({required this.selection});

  final FlarkSelection selection;
}

/// Nothing actionable (invalid selection).
final class FlarkHostIgnoreIntent extends FlarkHostEditIntent {
  const FlarkHostIgnoreIntent();
}

/// The host classifier's complete answer for one incoming value.
final class FlarkHostEditClassification {
  const FlarkHostEditClassification({
    required this.normalizedValue,
    required this.intent,
  });

  /// The value the widget adopts into its text controller before executing
  /// (for whole-document replaces this is the normalized echo value).
  final TextEditingValue normalizedValue;

  final FlarkHostEditIntent intent;
}

/// Classifies one platform text change on the projected host editable.
///
/// Behavior is identical to the previous open-coded chain in
/// `_FlarkProjectedEditableHostState._handleTextEditingValueChanged`.
FlarkHostEditClassification classifyFlarkHostEdit(
  FlarkHostEditContext context,
) {
  final autoClosedWholeValueFenceText = context.liveRendered
      ? FlarkLiveCodeFenceInputPolicy.displayTextAfterAutoClosedWholeValueEcho(
          context.newValue,
        )
      : null;
  if (autoClosedWholeValueFenceText != null) {
    return FlarkHostEditClassification(
      normalizedValue: context.newValue.copyWith(
        text: autoClosedWholeValueFenceText,
        selection: TextSelection.collapsed(
          offset: autoClosedWholeValueFenceText.length,
          affinity: context.newValue.selection.affinity,
        ),
        composing: TextRange.empty,
      ),
      intent: FlarkHostWholeDocumentReplaceIntent(
        replacementMarkdown: autoClosedWholeValueFenceText,
      ),
    );
  }

  final oldDisplaySelection = TextSelection(
    baseOffset: context.oldDisplaySelection.baseOffset,
    extentOffset: context.oldDisplaySelection.extentOffset,
  );
  final value = flarkTextValueWithPureInsertionSelection(
    oldText: context.oldDisplayText,
    oldSelection: oldDisplaySelection,
    newValue: context.newValue,
    normalizeAutoClosedFenceEcho: context.liveRendered,
  );

  FlarkHostEditClassification finish(FlarkHostEditIntent intent) {
    return FlarkHostEditClassification(normalizedValue: value, intent: intent);
  }

  final autoClosedStandaloneFenceMarkdown = context.liveRendered
      ? FlarkLiveCodeFenceInputPolicy.markdownAfterAutoClosedStandaloneEcho(
          oldMarkdown: context.markdown,
          newValue: value,
        )
      : null;
  if (autoClosedStandaloneFenceMarkdown != null) {
    return finish(
      FlarkHostWholeDocumentReplaceIntent(
        replacementMarkdown: autoClosedStandaloneFenceMarkdown,
      ),
    );
  }

  if (value.text != context.oldDisplayText) {
    final completedCodeFenceText = context.liveRendered
        ? FlarkLiveCodeFenceInputPolicy.displayTextAfterCompletingStandaloneOpener(
            oldDisplayText: context.oldDisplayText,
            oldSelection: oldDisplaySelection,
            newValue: value,
          )
        : null;
    final newDisplayText = completedCodeFenceText ?? value.text;
    final needsImmediateParse =
        context.liveRendered &&
        (completedCodeFenceText != null ||
            _hasImmediatelyRenderableBlockLine(newDisplayText));
    return finish(
      FlarkHostPlatformTextChangeIntent(
        policyValue: value.copyWith(text: newDisplayText),
        oldText: context.oldDisplayText,
        oldTextSelection: context.oldDisplaySelection,
        fallback: FlarkHostProjectedEditIntent(
          oldDisplayText: context.oldDisplayText,
          newDisplayText: newDisplayText,
          immediateParseAfterApply: needsImmediateParse,
        ),
      ),
    );
  }

  final selection = FlarkMarkdownInputPolicy.selectionFromTextSelection(
    value.selection,
  );
  if (selection == null) return finish(const FlarkHostIgnoreIntent());
  return finish(FlarkHostProjectedSelectionIntent(selection: selection));
}

// ---------------------------------------------------------------------------
// Shared text-normalization helpers
// ---------------------------------------------------------------------------

/// Normalizes a platform-delivered value whose selection lags a pure
/// insertion (some platforms deliver the inserted text with the caret still
/// at the insertion point), and unwraps whole-text auto-closed-fence echoes
/// when [normalizeAutoClosedFenceEcho] is set.
TextEditingValue flarkTextValueWithPureInsertionSelection({
  required String oldText,
  required TextSelection oldSelection,
  required TextEditingValue newValue,
  bool normalizeAutoClosedFenceEcho = false,
}) {
  if (normalizeAutoClosedFenceEcho) {
    final normalizedFenceText =
        FlarkLiveCodeFenceInputPolicy.displayTextAfterAutoClosedWholeTextEcho(
          oldDisplayText: oldText,
          newValue: newValue,
        );
    if (normalizedFenceText != null) {
      return newValue.copyWith(
        text: normalizedFenceText,
        selection: TextSelection.collapsed(
          offset: normalizedFenceText.length,
          affinity: newValue.selection.affinity,
        ),
        composing: TextRange.empty,
      );
    }
  }

  if (!oldSelection.isValid || !oldSelection.isCollapsed) return newValue;
  final newSelection = newValue.selection;
  if (!newSelection.isValid || !newSelection.isCollapsed) return newValue;
  final insertion = _pureTextInsertion(
    oldText: oldText,
    newText: newValue.text,
  );
  if (insertion == null) return newValue;
  if (oldSelection.extentOffset != insertion.offset ||
      newSelection.extentOffset != insertion.offset) {
    return newValue;
  }
  return newValue.copyWith(
    selection: TextSelection.collapsed(
      offset: insertion.offset + insertion.length,
      affinity: newSelection.affinity,
    ),
  );
}

_PureTextInsertion? _pureTextInsertion({
  required String oldText,
  required String newText,
}) {
  if (newText.length <= oldText.length) return null;
  var prefixLength = 0;
  while (prefixLength < oldText.length &&
      prefixLength < newText.length &&
      oldText.codeUnitAt(prefixLength) == newText.codeUnitAt(prefixLength)) {
    prefixLength++;
  }

  var oldSuffix = oldText.length;
  var newSuffix = newText.length;
  while (oldSuffix > prefixLength &&
      newSuffix > prefixLength &&
      oldText.codeUnitAt(oldSuffix - 1) == newText.codeUnitAt(newSuffix - 1)) {
    oldSuffix--;
    newSuffix--;
  }

  if (oldSuffix != prefixLength) return null;
  final insertedLength = newSuffix - prefixLength;
  if (insertedLength <= 0) return null;
  return _PureTextInsertion(prefixLength, insertedLength);
}

final class _PureTextInsertion {
  const _PureTextInsertion(this.offset, this.length);

  final int offset;
  final int length;
}

/// The editable slice of [block] inside [displayText]: its display range
/// clamped to the text, with trailing line breaks excluded for every block
/// type except blockquotes.
FlarkSourceRange flarkClampedDisplayRange(
  FlarkRenderBlock block,
  String displayText,
) {
  final start = block.displayRange.start.clamp(0, displayText.length);
  var end = block.displayRange.end.clamp(start, displayText.length);
  if (block.kind == FlarkMarkdownBlockKind.blockquote) {
    return FlarkSourceRange(start, end);
  }
  while (end > start) {
    final unit = displayText.codeUnitAt(end - 1);
    if (unit != 0x0A && unit != 0x0D) break;
    end--;
  }
  return FlarkSourceRange(start, end);
}

// ---------------------------------------------------------------------------
// Immediately renderable lines (host immediate-parse heuristic)
// ---------------------------------------------------------------------------

bool _hasImmediatelyRenderableBlockLine(String text) {
  var lineStart = 0;
  while (lineStart <= text.length) {
    final lineEndWithBreak = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
      text,
      lineStart,
    );
    final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      text,
      lineStart,
    );
    final line = text.substring(lineStart, lineEnd);
    if (_isImmediatelyRenderableQuoteLine(line) ||
        _isImmediatelyRenderableListLine(line) ||
        _isImmediatelyRenderableCodeFenceLine(
          line,
          hasLineBreak: lineEndWithBreak > lineEnd,
        )) {
      return true;
    }
    if (lineEndWithBreak <= lineStart || lineEndWithBreak >= text.length) {
      break;
    }
    lineStart = lineEndWithBreak;
  }
  return false;
}

bool _isImmediatelyRenderableQuoteLine(String line) {
  var index = _skipHorizontalWhitespace(line, 0);
  if (index >= line.length || line.codeUnitAt(index) != 0x3E) return false;
  index++;
  return index < line.length && _isHorizontalWhitespace(line.codeUnitAt(index));
}

bool _isImmediatelyRenderableListLine(String line) {
  final index = _skipHorizontalWhitespace(line, 0);
  if (index >= line.length) return false;

  final marker = line.codeUnitAt(index);
  if (marker == 0x2D || marker == 0x2A || marker == 0x2B) {
    final afterMarker = index + 1;
    return afterMarker < line.length &&
        _isHorizontalWhitespace(line.codeUnitAt(afterMarker));
  }

  return _orderedListMarkerLabel(line, requireFollowingWhitespace: true) !=
      null;
}

bool _isImmediatelyRenderableCodeFenceLine(
  String line, {
  required bool hasLineBreak,
}) {
  if (!hasLineBreak) return false;
  return FlarkMarkdownFencedCodeScanner.fenceLine(line) != null;
}

String? _orderedListMarkerLabel(
  String line, {
  bool requireFollowingWhitespace = false,
}) {
  var index = _skipHorizontalWhitespace(line, 0);
  final digitStart = index;
  while (index < line.length &&
      index - digitStart < 9 &&
      _isAsciiDigit(line.codeUnitAt(index))) {
    index++;
  }
  if (index == digitStart) return null;
  if (index < line.length && _isAsciiDigit(line.codeUnitAt(index))) {
    return null;
  }
  if (index >= line.length) return null;

  final delimiter = line.codeUnitAt(index);
  if (delimiter != 0x2E && delimiter != 0x29) return null;
  index++;
  if (requireFollowingWhitespace &&
      (index >= line.length ||
          !_isHorizontalWhitespace(line.codeUnitAt(index)))) {
    return null;
  }
  return line.substring(digitStart, index);
}

int _skipHorizontalWhitespace(String text, int start) {
  var index = start;
  while (index < text.length &&
      _isHorizontalWhitespace(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

bool _isHorizontalWhitespace(int codeUnit) {
  return codeUnit == 0x20 || codeUnit == 0x09;
}

bool _isAsciiDigit(int codeUnit) {
  return codeUnit >= 0x30 && codeUnit <= 0x39;
}
