import '../command/sovereign_command.dart';
import '../command/sovereign_command_registry.dart';
import '../command/sovereign_command_result.dart';
import '../extension/sovereign_extension.dart';
import '../history/sovereign_history_stack.dart';
import '../state/sovereign_editor_state.dart';
import '../transaction/sovereign_transaction.dart';

final class SovereignEditorRuntime {
  SovereignEditorRuntime({
    required this.state,
    SovereignHistoryStack? history,
    SovereignExtensionSet? extensions,
    SovereignCommandRegistry? commandRegistry,
  })  : history = history ?? const SovereignHistoryStack(),
        extensions = extensions ?? const SovereignExtensionSet.empty(),
        commandRegistry = commandRegistry ??
            (extensions ?? const SovereignExtensionSet.empty())
                .commandRegistry();

  factory SovereignEditorRuntime.fromMarkdown(
    String markdown, {
    SovereignExtensionSet? extensions,
  }) {
    return SovereignEditorRuntime(
      state: SovereignEditorState.fromMarkdown(markdown),
      extensions: extensions,
    );
  }

  final SovereignEditorState state;
  final SovereignHistoryStack history;
  final SovereignExtensionSet extensions;
  final SovereignCommandRegistry commandRegistry;

  bool get canUndo => history.canUndo;

  bool get canRedo => history.canRedo;

  SovereignEditorRuntimeResult dispatch<TPayload>({
    required SovereignCommand<TPayload> command,
    required TPayload payload,
  }) {
    final commandResult = commandRegistry.dispatch(
      state: state,
      command: command,
      payload: payload,
    );

    final transaction = commandResult.transaction;
    if (!commandResult.isHandled || transaction == null) {
      return SovereignEditorRuntimeResult(
        runtime: this,
        commandResult: commandResult,
      );
    }

    return applyTransaction(
      transaction,
      commandResult: commandResult,
    );
  }

  SovereignEditorRuntimeResult applyTransaction(
    SovereignTransaction transaction, {
    SovereignCommandResult? commandResult,
  }) {
    final nextState = state.applyTransaction(transaction);
    final nextHistory = history.record(
      transaction: transaction,
      documentBefore: state.document,
    );

    return SovereignEditorRuntimeResult(
      runtime: copyWith(
        state: nextState,
        history: nextHistory,
      ),
      commandResult: commandResult ??
          SovereignCommandResult.handled(transaction: transaction),
    );
  }

  SovereignEditorRuntimeResult undo() {
    final result = history.undo(state);
    return SovereignEditorRuntimeResult(
      runtime: copyWith(state: result.state, history: result.history),
      commandResult: SovereignCommandResult.handled(),
    );
  }

  SovereignEditorRuntimeResult redo() {
    final result = history.redo(state);
    return SovereignEditorRuntimeResult(
      runtime: copyWith(state: result.state, history: result.history),
      commandResult: SovereignCommandResult.handled(),
    );
  }

  SovereignEditorRuntime copyWith({
    SovereignEditorState? state,
    SovereignHistoryStack? history,
    SovereignExtensionSet? extensions,
    SovereignCommandRegistry? commandRegistry,
  }) {
    final nextExtensions = extensions ?? this.extensions;
    return SovereignEditorRuntime(
      state: state ?? this.state,
      history: history ?? this.history,
      extensions: nextExtensions,
      commandRegistry: commandRegistry ??
          (identical(nextExtensions, this.extensions)
              ? this.commandRegistry
              : nextExtensions.commandRegistry()),
    );
  }
}

final class SovereignEditorRuntimeResult {
  const SovereignEditorRuntimeResult({
    required this.runtime,
    required this.commandResult,
  });

  final SovereignEditorRuntime runtime;
  final SovereignCommandResult commandResult;
}
