part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

abstract final class _HeadingPolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(name: 'enter-heading', priority: 34, apply: _onEnter),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeExitEmptyAtxHeadingOnEnter(
        context.oldValue,
        newValue,
        enterCaret: context.intent.enterCaret,
      );
}
