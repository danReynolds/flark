import '../command/flark_command.dart';
import '../command/flark_command_registry.dart';
import '../command/flark_command_result.dart';
import '../extension/flark_extension.dart';
import '../history/flark_history_stack.dart';
import '../state/flark_editor_state.dart';
import '../transaction/flark_transaction.dart';

final class FlarkEditorRuntime {
  FlarkEditorRuntime({
    required this.state,
    FlarkHistoryStack? history,
    FlarkExtensionSet? extensions,
    FlarkCommandRegistry? commandRegistry,
  }) : history = history ?? const FlarkHistoryStack(),
       extensions = extensions ?? const FlarkExtensionSet.empty(),
       commandRegistry =
           commandRegistry ??
           (extensions ?? const FlarkExtensionSet.empty()).commandRegistry();

  factory FlarkEditorRuntime.fromMarkdown(
    String markdown, {
    FlarkExtensionSet? extensions,
  }) {
    return FlarkEditorRuntime(
      state: FlarkEditorState.fromMarkdown(markdown),
      extensions: extensions,
    );
  }

  final FlarkEditorState state;
  final FlarkHistoryStack history;
  final FlarkExtensionSet extensions;
  final FlarkCommandRegistry commandRegistry;

  bool get canUndo => history.canUndo;

  bool get canRedo => history.canRedo;

  FlarkEditorRuntimeResult dispatch<TPayload>({
    required FlarkCommand<TPayload> command,
    required TPayload payload,
  }) {
    final commandResult = commandRegistry.dispatch(
      state: state,
      command: command,
      payload: payload,
    );

    final transaction = commandResult.transaction;
    if (!commandResult.isHandled || transaction == null) {
      return FlarkEditorRuntimeResult(
        runtime: this,
        commandResult: commandResult,
      );
    }

    return applyTransaction(transaction, commandResult: commandResult);
  }

  FlarkEditorRuntimeResult applyTransaction(
    FlarkTransaction transaction, {
    FlarkCommandResult? commandResult,
  }) {
    final nextState = state.applyTransaction(transaction);
    final nextHistory = history.record(
      transaction: transaction,
      documentBefore: state.document,
    );

    return FlarkEditorRuntimeResult(
      runtime: copyWith(state: nextState, history: nextHistory),
      commandResult:
          commandResult ?? FlarkCommandResult.handled(transaction: transaction),
    );
  }

  FlarkEditorRuntimeResult undo() {
    final result = history.undo(state);
    return FlarkEditorRuntimeResult(
      runtime: copyWith(state: result.state, history: result.history),
      commandResult: FlarkCommandResult.handled(),
    );
  }

  FlarkEditorRuntimeResult redo() {
    final result = history.redo(state);
    return FlarkEditorRuntimeResult(
      runtime: copyWith(state: result.state, history: result.history),
      commandResult: FlarkCommandResult.handled(),
    );
  }

  FlarkEditorRuntime copyWith({
    FlarkEditorState? state,
    FlarkHistoryStack? history,
    FlarkExtensionSet? extensions,
    FlarkCommandRegistry? commandRegistry,
  }) {
    final nextExtensions = extensions ?? this.extensions;
    return FlarkEditorRuntime(
      state: state ?? this.state,
      history: history ?? this.history,
      extensions: nextExtensions,
      commandRegistry:
          commandRegistry ??
          (identical(nextExtensions, this.extensions)
              ? this.commandRegistry
              : nextExtensions.commandRegistry()),
    );
  }
}

final class FlarkEditorRuntimeResult {
  const FlarkEditorRuntimeResult({
    required this.runtime,
    required this.commandResult,
  });

  final FlarkEditorRuntime runtime;
  final FlarkCommandResult commandResult;
}
