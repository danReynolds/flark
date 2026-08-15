import 'package:flutter/widgets.dart';

/// Ambient read-only flag for Flark editing surfaces.
///
/// The live-rendered editor is a tree of block editables built across
/// several widgets; threading a `readOnly` constructor parameter through
/// every layer would also require the block widget-instance cache to key on
/// it. An inherited scope avoids both: every [EditableText] site reads the
/// flag from context, and a change dirties dependents even when their
/// widget instances are reference-reused by the rebuild-isolation cache.
final class FlarkEditorReadOnlyScope extends InheritedWidget {
  const FlarkEditorReadOnlyScope({
    super.key,
    required this.readOnly,
    required super.child,
  });

  final bool readOnly;

  /// Whether the nearest enclosing editor is read-only (false when none).
  static bool of(BuildContext context) {
    return context
            .dependOnInheritedWidgetOfExactType<FlarkEditorReadOnlyScope>()
            ?.readOnly ??
        false;
  }

  @override
  bool updateShouldNotify(FlarkEditorReadOnlyScope oldWidget) {
    return oldWidget.readOnly != readOnly;
  }
}
