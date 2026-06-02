import '../state/sovereign_editor_state.dart';
import 'sovereign_command.dart';
import 'sovereign_command_result.dart';

final class SovereignCommandRegistry {
  const SovereignCommandRegistry() : _handlers = const {};

  const SovereignCommandRegistry._(this._handlers);

  final Map<String, List<_SovereignCommandHandlerEntryBase>> _handlers;

  SovereignCommandRegistry register<TPayload>(
    SovereignCommand<TPayload> command,
    SovereignCommandHandler<TPayload> handler, {
    int priority = SovereignCommandPriority.normal,
  }) {
    final nextHandlers = <String, List<_SovereignCommandHandlerEntryBase>>{
      for (final entry in _handlers.entries) entry.key: [...entry.value],
    };
    final commandHandlers = nextHandlers.putIfAbsent(command.id, () => []);
    commandHandlers.add(
      _SovereignCommandHandlerEntry<TPayload>(
        priority: priority,
        handler: handler,
      ),
    );
    commandHandlers.sort((a, b) => b.priority.compareTo(a.priority));

    return SovereignCommandRegistry._(nextHandlers);
  }

  SovereignCommandResult dispatch<TPayload>({
    required SovereignEditorState state,
    required SovereignCommand<TPayload> command,
    required TPayload payload,
  }) {
    final commandHandlers = _handlers[command.id];
    if (commandHandlers == null || commandHandlers.isEmpty) {
      return const SovereignCommandResult.notHandled();
    }

    for (final entry in commandHandlers) {
      final result = entry.invoke(
        SovereignCommandContext<TPayload>(
          state: state,
          command: command,
          payload: payload,
        ),
      );
      if (!result.isNotHandled) return result;
    }

    return const SovereignCommandResult.notHandled();
  }
}

abstract interface class _SovereignCommandHandlerEntryBase {
  int get priority;

  SovereignCommandResult invoke<TPayload>(
    SovereignCommandContext<TPayload> context,
  );
}

final class _SovereignCommandHandlerEntry<TPayload>
    implements _SovereignCommandHandlerEntryBase {
  const _SovereignCommandHandlerEntry({
    required this.priority,
    required this.handler,
  });

  @override
  final int priority;

  final SovereignCommandHandler<TPayload> handler;

  @override
  SovereignCommandResult invoke<TContextPayload>(
    SovereignCommandContext<TContextPayload> context,
  ) {
    if (context.payload is! TPayload) {
      return SovereignCommandResult.rejected(
        'Command payload for ${context.command.id} is not $TPayload.',
      );
    }

    return handler(
      SovereignCommandContext<TPayload>(
        state: context.state,
        command: SovereignCommand<TPayload>(context.command.id),
        payload: context.payload as TPayload,
      ),
    );
  }
}
