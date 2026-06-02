import '../../core/command/sovereign_command.dart';
import '../../core/command/sovereign_command_registry.dart';
import '../../core/command/sovereign_command_result.dart';
import '../../core/extension/sovereign_extension.dart';
import '../../core/selection/sovereign_selection.dart';
import '../../core/state/sovereign_editor_state.dart';
import '../../core/transaction/sovereign_source_operation.dart';
import '../../core/transaction/sovereign_source_range.dart';
import '../../core/transaction/sovereign_transaction.dart';
import '../../core/transaction/sovereign_transaction_metadata.dart';
import '../source/sovereign_markdown_editing_result.dart';
import '../source/sovereign_markdown_input_engine.dart';

abstract final class SovereignMarkdownInputCommands {
  static const handleEnter = SovereignCommand<SovereignHandleEnterPayload>(
    'markdown.handleEnter',
  );

  static const handleBackspace =
      SovereignCommand<SovereignHandleBackspacePayload>(
    'markdown.handleBackspace',
  );
}

final class SovereignHandleEnterPayload {
  const SovereignHandleEnterPayload({
    this.userEvent = 'input.enter',
  });

  final String userEvent;
}

final class SovereignHandleBackspacePayload {
  const SovereignHandleBackspacePayload({
    this.userEvent = 'input.backspace',
  });

  final String userEvent;
}

final class SovereignMarkdownInputEditingExtension extends SovereignExtension {
  const SovereignMarkdownInputEditingExtension();

  @override
  String get id => 'markdown.inputEditing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry
        .register<SovereignHandleEnterPayload>(
          SovereignMarkdownInputCommands.handleEnter,
          _handleEnter,
        )
        .register<SovereignHandleBackspacePayload>(
          SovereignMarkdownInputCommands.handleBackspace,
          _handleBackspace,
        );
  }

  SovereignCommandResult _handleEnter(
    SovereignCommandContext<SovereignHandleEnterPayload> context,
  ) {
    final state = context.state;
    final result = SovereignMarkdownInputEngine.enter(
      markdown: state.markdown,
      selection: state.selection,
    );
    return _resultForInputResult(state, result, context.payload.userEvent);
  }

  SovereignCommandResult _handleBackspace(
    SovereignCommandContext<SovereignHandleBackspacePayload> context,
  ) {
    final state = context.state;
    final result = SovereignMarkdownInputEngine.backspace(
      markdown: state.markdown,
      selection: state.selection,
    );
    if (result == null) return const SovereignCommandResult.notHandled();
    return _resultForInputResult(state, result, context.payload.userEvent);
  }

  SovereignCommandResult _resultForInputResult(
    SovereignEditorState state,
    SovereignMarkdownInputResult result,
    String userEvent,
  ) {
    if (result is SovereignMarkdownSourceEdit) {
      return _singleEdit(
        state,
        result.range,
        result.replacementText,
        userEvent,
        selectionAfter: result.selectionAfter,
      );
    }
    if (result is SovereignMarkdownSelectionMove) {
      return _selectionOnly(state, result.selectionAfter, userEvent);
    }
    throw StateError('Unhandled markdown input result: $result');
  }

  SovereignCommandResult _singleEdit(
    SovereignEditorState state,
    SovereignSourceRange range,
    String replacement,
    String userEvent, {
    SovereignSelection? selectionAfter,
  }) {
    final nextSelection = selectionAfter ??
        SovereignSelection.collapsed(
          range.start + replacement.length,
        );
    return SovereignCommandResult.handled(
      transaction: SovereignTransaction.single(
        SovereignSourceOperation.replace(
          replacedRange: range,
          replacementText: replacement,
        ),
        selectionBefore: state.selection,
        selectionAfter: nextSelection,
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.input,
          userEvent: userEvent,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
  }

  SovereignCommandResult _selectionOnly(
    SovereignEditorState state,
    SovereignSelection selection,
    String userEvent,
  ) {
    return SovereignCommandResult.handled(
      transaction: SovereignTransaction(
        operations: const [],
        selectionBefore: state.selection,
        selectionAfter: selection,
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
  }
}
