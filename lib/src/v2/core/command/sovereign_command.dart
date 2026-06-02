import '../state/sovereign_editor_state.dart';
import 'sovereign_command_result.dart';

final class SovereignCommand<TPayload> {
  const SovereignCommand(this.id);

  final String id;

  @override
  bool operator ==(Object other) {
    return other is SovereignCommand<TPayload> && other.id == id;
  }

  @override
  int get hashCode => Object.hash(TPayload, id);

  @override
  String toString() => 'SovereignCommand<$TPayload>($id)';
}

final class SovereignCommandContext<TPayload> {
  const SovereignCommandContext({
    required this.state,
    required this.command,
    required this.payload,
  });

  final SovereignEditorState state;
  final SovereignCommand<TPayload> command;
  final TPayload payload;
}

typedef SovereignCommandHandler<TPayload> = SovereignCommandResult Function(
  SovereignCommandContext<TPayload> context,
);

abstract final class SovereignCommandPriority {
  static const fallback = -100;
  static const normal = 0;
  static const high = 100;
}
