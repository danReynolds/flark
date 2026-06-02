import '../state/flark_editor_state.dart';
import 'flark_command_result.dart';

final class FlarkCommand<TPayload> {
  const FlarkCommand(this.id);

  final String id;

  @override
  bool operator ==(Object other) {
    return other is FlarkCommand<TPayload> && other.id == id;
  }

  @override
  int get hashCode => Object.hash(TPayload, id);

  @override
  String toString() => 'FlarkCommand<$TPayload>($id)';
}

final class FlarkCommandContext<TPayload> {
  const FlarkCommandContext({
    required this.state,
    required this.command,
    required this.payload,
  });

  final FlarkEditorState state;
  final FlarkCommand<TPayload> command;
  final TPayload payload;
}

typedef FlarkCommandHandler<TPayload> =
    FlarkCommandResult Function(FlarkCommandContext<TPayload> context);

abstract final class FlarkCommandPriority {
  static const fallback = -100;
  static const normal = 0;
  static const high = 100;
}
