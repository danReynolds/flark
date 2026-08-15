import '../command/flark_command_registry.dart';

abstract base class FlarkExtension {
  const FlarkExtension();

  String get id;

  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry;
  }
}

final class FlarkExtensionSet {
  const FlarkExtensionSet.empty() : extensions = const [];

  FlarkExtensionSet(Iterable<FlarkExtension> extensions)
    : extensions = List<FlarkExtension>.unmodifiable(extensions) {
    final ids = <String>{};
    for (final extension in this.extensions) {
      if (!ids.add(extension.id)) {
        throw StateError('Duplicate Flark extension id: ${extension.id}');
      }
    }
  }

  final List<FlarkExtension> extensions;

  Iterable<T> whereType<T extends FlarkExtension>() {
    return extensions.whereType<T>();
  }

  FlarkCommandRegistry commandRegistry({
    FlarkCommandRegistry base = const FlarkCommandRegistry(),
  }) {
    var registry = base;
    for (final extension in extensions) {
      registry = extension.registerCommands(registry);
    }
    return registry;
  }
}
