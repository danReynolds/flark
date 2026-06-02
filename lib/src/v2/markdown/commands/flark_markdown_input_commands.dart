import '../../core/command/flark_command.dart';
import '../../core/command/flark_command_registry.dart';
import '../../core/command/flark_command_result.dart';
import '../../core/extension/flark_extension.dart';
import '../../core/selection/flark_selection.dart';
import '../../core/state/flark_editor_state.dart';
import '../../core/transaction/flark_source_operation.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../core/transaction/flark_transaction.dart';
import '../../core/transaction/flark_transaction_metadata.dart';
import '../source/flark_markdown_editing_result.dart';
import '../source/flark_markdown_input_engine.dart';

abstract final class FlarkMarkdownInputCommands {
  static const handleEnter = FlarkCommand<FlarkHandleEnterPayload>(
    'markdown.handleEnter',
  );

  static const handleBackspace = FlarkCommand<FlarkHandleBackspacePayload>(
    'markdown.handleBackspace',
  );
}

final class FlarkHandleEnterPayload {
  const FlarkHandleEnterPayload({this.userEvent = 'input.enter'});

  final String userEvent;
}

final class FlarkHandleBackspacePayload {
  const FlarkHandleBackspacePayload({this.userEvent = 'input.backspace'});

  final String userEvent;
}

final class FlarkMarkdownInputEditingExtension extends FlarkExtension {
  const FlarkMarkdownInputEditingExtension();

  @override
  String get id => 'markdown.inputEditing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry
        .register<FlarkHandleEnterPayload>(
          FlarkMarkdownInputCommands.handleEnter,
          _handleEnter,
        )
        .register<FlarkHandleBackspacePayload>(
          FlarkMarkdownInputCommands.handleBackspace,
          _handleBackspace,
        );
  }

  FlarkCommandResult _handleEnter(
    FlarkCommandContext<FlarkHandleEnterPayload> context,
  ) {
    final state = context.state;
    final result = FlarkMarkdownInputEngine.enter(
      markdown: state.markdown,
      selection: state.selection,
    );
    return _resultForInputResult(state, result, context.payload.userEvent);
  }

  FlarkCommandResult _handleBackspace(
    FlarkCommandContext<FlarkHandleBackspacePayload> context,
  ) {
    final state = context.state;
    final result = FlarkMarkdownInputEngine.backspace(
      markdown: state.markdown,
      selection: state.selection,
    );
    if (result == null) return const FlarkCommandResult.notHandled();
    return _resultForInputResult(state, result, context.payload.userEvent);
  }

  FlarkCommandResult _resultForInputResult(
    FlarkEditorState state,
    FlarkMarkdownInputResult result,
    String userEvent,
  ) {
    if (result is FlarkMarkdownSourceEdit) {
      return _singleEdit(
        state,
        result.range,
        result.replacementText,
        userEvent,
        selectionAfter: result.selectionAfter,
      );
    }
    if (result is FlarkMarkdownSelectionMove) {
      return _selectionOnly(state, result.selectionAfter, userEvent);
    }
    throw StateError('Unhandled markdown input result: $result');
  }

  FlarkCommandResult _singleEdit(
    FlarkEditorState state,
    FlarkSourceRange range,
    String replacement,
    String userEvent, {
    FlarkSelection? selectionAfter,
  }) {
    final nextSelection =
        selectionAfter ??
        FlarkSelection.collapsed(range.start + replacement.length);
    return FlarkCommandResult.handled(
      transaction: FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: range,
          replacementText: replacement,
        ),
        selectionBefore: state.selection,
        selectionAfter: nextSelection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: userEvent,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
  }

  FlarkCommandResult _selectionOnly(
    FlarkEditorState state,
    FlarkSelection selection,
    String userEvent,
  ) {
    return FlarkCommandResult.handled(
      transaction: FlarkTransaction(
        operations: const [],
        selectionBefore: state.selection,
        selectionAfter: selection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
  }
}
