import '../extension/sovereign_extension.dart';
import '../selection/sovereign_selection.dart';
import '../transaction/sovereign_source_operation.dart';
import '../transaction/sovereign_source_range.dart';
import '../transaction/sovereign_transaction.dart';
import '../transaction/sovereign_transaction_metadata.dart';
import 'sovereign_command.dart';
import 'sovereign_command_registry.dart';
import 'sovereign_command_result.dart';

abstract final class SovereignCoreEditingCommands {
  static const insertText =
      SovereignCommand<SovereignInsertTextPayload>('core.insertText');
}

final class SovereignInsertTextPayload {
  const SovereignInsertTextPayload(
    this.text, {
    this.userEvent = 'input.insertText',
  });

  final String text;
  final String userEvent;
}

final class SovereignCoreEditingExtension extends SovereignExtension {
  const SovereignCoreEditingExtension();

  @override
  String get id => 'core.editing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry.register<SovereignInsertTextPayload>(
      SovereignCoreEditingCommands.insertText,
      _insertText,
    );
  }

  SovereignCommandResult _insertText(
    SovereignCommandContext<SovereignInsertTextPayload> context,
  ) {
    final selection = context.state.selection;
    final range = SovereignSourceRange(selection.start, selection.end);
    final nextOffset = selection.start + context.payload.text.length;

    return SovereignCommandResult.handled(
      transaction: SovereignTransaction.single(
        SovereignSourceOperation.replace(
          replacedRange: range,
          replacementText: context.payload.text,
        ),
        selectionBefore: selection,
        selectionAfter: SovereignSelection.collapsed(nextOffset),
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.input,
          userEvent: context.payload.userEvent,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
  }
}
