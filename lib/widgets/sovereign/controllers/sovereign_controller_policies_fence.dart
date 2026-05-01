part of 'sovereign_controller.dart';

abstract final class _FencePolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'exit-arrow-up',
      priority: 10,
      apply: (context, value) =>
          context.helpers.maybeExitFencedCodeOnArrowUp(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'exit-arrow-down',
      priority: 20,
      apply: (context, value) => context.helpers.maybeExitFencedCodeOnArrowDown(
        context.oldValue,
        value,
      ),
    ),
    _EditTransformRule(
      name: 'exit-enter',
      priority: 30,
      apply: (context, value) =>
          context.helpers.maybeExitFencedCodeOnEnter(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'continue-outside-closing-fence-eof',
      priority: 35,
      apply: (context, value) => context.helpers
          .maybeContinueOutsideClosingFenceEof(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'normalize-multiline-paste',
      priority: 40,
      apply: (context, value) => context.helpers
          .maybeNormalizeFencedMultilinePaste(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'insert-before-closing-fence-keeps-line',
      priority: 45,
      apply: (context, value) => context.helpers.maybeKeepClosingFenceOnOwnLine(
        context.oldValue,
        value,
      ),
    ),
    _EditTransformRule(
      name: 'expand-pair-on-enter',
      priority: 50,
      apply: (context, value) =>
          context.helpers.maybeExpandFencedPairOnEnter(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'auto-indent-enter',
      priority: 60,
      apply: (context, value) => context.helpers
          .maybeAutoIndentFencedCodeOnEnter(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'wrap-selection-on-opener',
      priority: 70,
      apply: (context, value) => context.helpers
          .maybeWrapFencedSelectionOnOpenerInsert(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'auto-pair-opener',
      priority: 80,
      apply: (context, value) => context.helpers
          .maybeAutoPairFencedOpenerInsert(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'skip-closer',
      priority: 90,
      apply: (context, value) =>
          context.helpers.maybeSkipFencedCloserInsert(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'outdent-on-closer',
      priority: 100,
      apply: (context, value) => context.helpers
          .maybeOutdentFencedCodeOnCloserInsert(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'delete-empty-pair-on-backspace',
      priority: 110,
      apply: (context, value) => context.helpers
          .maybeDeleteFencedPairOnBackspace(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'outdent-on-backspace',
      priority: 120,
      apply: (context, value) => context.helpers
          .maybeOutdentFencedCodeOnBackspace(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'collapse-empty-fence-on-backspace',
      priority: 130,
      apply: (context, value) => context.helpers
          .maybeCollapseEmptyFenceOnBackspace(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'protect-empty-fence-entry-backspace',
      priority: 140,
      apply: (context, value) => context.helpers
          .maybeProtectEmptyFenceEntryBackspace(context.oldValue, value),
    ),
    _EditTransformRule(
      name: 'protect-hidden-fence-backspace',
      priority: 150,
      apply: (context, value) => context.helpers
          .maybeProtectHiddenFenceBackspace(context.oldValue, value),
    ),
  ];
}
