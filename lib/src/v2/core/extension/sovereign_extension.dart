import '../command/sovereign_command_registry.dart';

abstract base class SovereignExtension {
  const SovereignExtension();

  String get id;

  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry;
  }
}

final class SovereignExtensionSet {
  const SovereignExtensionSet.empty() : extensions = const [];

  SovereignExtensionSet(Iterable<SovereignExtension> extensions)
      : extensions = List<SovereignExtension>.unmodifiable(extensions) {
    final ids = <String>{};
    for (final extension in this.extensions) {
      if (!ids.add(extension.id)) {
        throw StateError('Duplicate Sovereign extension id: ${extension.id}');
      }
    }
  }

  final List<SovereignExtension> extensions;

  Iterable<T> whereType<T extends SovereignExtension>() {
    return extensions.whereType<T>();
  }

  SovereignCommandRegistry commandRegistry({
    SovereignCommandRegistry base = const SovereignCommandRegistry(),
  }) {
    var registry = base;
    for (final extension in extensions) {
      registry = extension.registerCommands(registry);
    }
    return registry;
  }
}
