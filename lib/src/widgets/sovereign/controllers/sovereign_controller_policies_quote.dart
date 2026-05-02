part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

abstract final class _QuotePolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'exit-arrow-up-blockquote',
      priority: 15,
      apply: _onArrowUp,
    ),
    _EditTransformRule(
      name: 'exit-arrow-down-blockquote',
      priority: 25,
      apply: _onArrowDown,
    ),
    _EditTransformRule(name: 'enter-blockquote', priority: 35, apply: _onEnter),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeContinueOrExitBlockquoteOnEnter(
        context.oldValue,
        newValue,
        enterCaret: context.intent.enterCaret,
      );

  static TextEditingValue _onArrowDown(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeExitBlockquoteOnArrowDown(
        context.oldValue,
        newValue,
      );

  static TextEditingValue _onArrowUp(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeExitBlockquoteOnArrowUp(
        context.oldValue,
        newValue,
      );
}
