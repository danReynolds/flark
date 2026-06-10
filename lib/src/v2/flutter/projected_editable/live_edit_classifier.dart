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
// The classifiers are PURE: they read only their context arguments and the
// static policy helpers — no widget or controller state. The widgets resolve
// the context, classify, and then execute the intent's side effects.
//
// This file is a part (not its own library) only because it shares the
// library-private text helpers; promoting it to a standalone library is the
// remaining step of the pipeline unification, after the host and block
// recognizer sets are merged.

part of '../flark_projected_editable_text.dart';

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
  var value = _textValueWithPureInsertionSelection(
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
      final range = _clampedDisplayRange(context.block, context.displayText);
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
  final range = _clampedDisplayRange(context.block, context.displayText);
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
  final value = _textValueWithPureInsertionSelection(
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
