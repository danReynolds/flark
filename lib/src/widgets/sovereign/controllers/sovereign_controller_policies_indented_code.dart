part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

abstract final class _IndentedCodePolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'outdent-indented-code-on-backspace',
      priority: 121,
      apply: (context, value) => context.helpers
          .maybeOutdentIndentedCodeOnBackspace(context.oldValue, value),
    ),
  ];
}
