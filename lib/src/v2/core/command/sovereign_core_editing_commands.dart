import '../extension/sovereign_extension.dart';
import '../selection/sovereign_selection.dart';
import '../transaction/sovereign_source_operation.dart';
import '../transaction/sovereign_source_range.dart';
import '../transaction/sovereign_transaction.dart';
import '../transaction/sovereign_transaction_metadata.dart';
import 'sovereign_command.dart';
import 'sovereign_command_registry.dart';
import 'sovereign_command_result.dart';

abstract final class FlarkCoreEditingCommands {
  static const insertText = FlarkCommand<FlarkInsertTextPayload>(
    'core.insertText',
  );
}

final class FlarkInsertTextPayload {
  const FlarkInsertTextPayload(
    this.text, {
    this.userEvent = 'input.insertText',
  });

  final String text;
  final String userEvent;
}

final class FlarkCoreEditingExtension extends FlarkExtension {
  const FlarkCoreEditingExtension();

  @override
  String get id => 'core.editing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry.register<FlarkInsertTextPayload>(
      FlarkCoreEditingCommands.insertText,
      _insertText,
    );
  }

  FlarkCommandResult _insertText(
    FlarkCommandContext<FlarkInsertTextPayload> context,
  ) {
    final selection = context.state.selection;
    final range = FlarkSourceRange(selection.start, selection.end);
    final nextOffset = selection.start + context.payload.text.length;

    return FlarkCommandResult.handled(
      transaction: FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: range,
          replacementText: context.payload.text,
        ),
        selectionBefore: selection,
        selectionAfter: FlarkSelection.collapsed(nextOffset),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: context.payload.userEvent,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
  }
}
