import '../state/flark_editor_state.dart';
import 'flark_command.dart';
import 'flark_command_result.dart';

final class FlarkCommandRegistry {
  const FlarkCommandRegistry() : _handlers = const {};

  const FlarkCommandRegistry._(this._handlers);

  final Map<String, List<_FlarkCommandHandlerEntryBase>> _handlers;

  FlarkCommandRegistry register<TPayload>(
    FlarkCommand<TPayload> command,
    FlarkCommandHandler<TPayload> handler, {
    int priority = FlarkCommandPriority.normal,
  }) {
    final nextHandlers = <String, List<_FlarkCommandHandlerEntryBase>>{
      for (final entry in _handlers.entries) entry.key: [...entry.value],
    };
    final commandHandlers = nextHandlers.putIfAbsent(command.id, () => []);
    commandHandlers.add(
      _FlarkCommandHandlerEntry<TPayload>(priority: priority, handler: handler),
    );
    commandHandlers.sort((a, b) => b.priority.compareTo(a.priority));

    return FlarkCommandRegistry._(nextHandlers);
  }

  FlarkCommandResult dispatch<TPayload>({
    required FlarkEditorState state,
    required FlarkCommand<TPayload> command,
    required TPayload payload,
  }) {
    final commandHandlers = _handlers[command.id];
    if (commandHandlers == null || commandHandlers.isEmpty) {
      return const FlarkCommandResult.notHandled();
    }

    for (final entry in commandHandlers) {
      final result = entry.invoke(
        FlarkCommandContext<TPayload>(
          state: state,
          command: command,
          payload: payload,
        ),
      );
      if (!result.isNotHandled) return result;
    }

    return const FlarkCommandResult.notHandled();
  }
}

abstract interface class _FlarkCommandHandlerEntryBase {
  int get priority;

  FlarkCommandResult invoke<TPayload>(FlarkCommandContext<TPayload> context);
}

final class _FlarkCommandHandlerEntry<TPayload>
    implements _FlarkCommandHandlerEntryBase {
  const _FlarkCommandHandlerEntry({
    required this.priority,
    required this.handler,
  });

  @override
  final int priority;

  final FlarkCommandHandler<TPayload> handler;

  @override
  FlarkCommandResult invoke<TContextPayload>(
    FlarkCommandContext<TContextPayload> context,
  ) {
    if (context.payload is! TPayload) {
      return FlarkCommandResult.rejected(
        'Command payload for ${context.command.id} is not $TPayload.',
      );
    }

    return handler(
      FlarkCommandContext<TPayload>(
        state: context.state,
        command: FlarkCommand<TPayload>(context.command.id),
        payload: context.payload as TPayload,
      ),
    );
  }
}
