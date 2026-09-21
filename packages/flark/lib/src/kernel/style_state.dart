/// Formatting of the caret's next input or the visible selected text.
enum FlarkStyleValue { off, on, mixed }

/// A snapshot of an inline formatting control, independent of any UI toolkit.
///
/// [value] describes the selection; availability describes supported edits.
/// Hosts additionally apply their own read-only/permission policy. Commands
/// still validate Markdown and admission limits before publishing a change.
final class FlarkStyleState {
  const FlarkStyleState({
    required this.value,
    required this.canEnable,
    required this.canDisable,
  });

  final FlarkStyleValue value;
  final bool canEnable;
  final bool canDisable;
  bool get isOn => value == FlarkStyleValue.on;
  bool get isMixed => value == FlarkStyleValue.mixed;

  /// Toggle removes a uniform style; off/mixed selections become uniform.
  bool get canToggle => isOn ? canDisable : canEnable;
}
