import 'document.dart';
import 'editor_coordinator.dart';
import 'editor_session.dart';
import 'pending_presentation.dart';
import 'presentation.dart';

/// Immutable source transaction admitted to the portable editor command lane.
final class FlarkSourceEditCommand {
  const FlarkSourceEditCommand({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
    required this.beforeSelection,
    required this.afterSelection,
    required this.coalesceTyping,
    this.compositionGroup,
    this.compositionFinal = false,
  });

  final int startUtf16;
  final int endUtf16;
  final String replacement;
  final FlarkCoreSelectionSnapshot beforeSelection;
  final FlarkCoreSelectionSnapshot afterSelection;
  final bool coalesceTyping;
  final int? compositionGroup;
  final bool compositionFinal;
}

enum FlarkHistoryDirection { undo, redo }

/// One native command result whose host adoption has not yet settled.
///
/// The coordinator ticket stays private so a frontend cannot complete a
/// different command, reuse a settled lifetime, or bypass generation checks.
final class FlarkEditorCommandExecution<T> {
  FlarkEditorCommandExecution._({
    required this.generation,
    required this.result,
    required FlarkEditorCommandTicket ticket,
  }) : _ticket = ticket;

  final int generation;
  final Future<T> result;
  final FlarkEditorCommandTicket _ticket;
}

/// Executes native editor effects through one typed coordinator lifetime.
///
/// A host still decides how to adapt a receipt into its bounded input and
/// visual state. It cannot decide command ordering, ticket identity, native
/// invocation, or history serialization.
final class FlarkEditorCommandExecutor {
  FlarkEditorCommandExecutor({
    required FlarkEditorCoordinator coordinator,
    required FlarkCoreEditorSession session,
  }) : _coordinator = coordinator,
       _session = session;

  final FlarkEditorCoordinator _coordinator;
  final FlarkCoreEditorSession _session;

  FlarkEditorCommandExecution<FlarkCoreEditReceipt> executeSourceEdit(
    FlarkSourceEditCommand command,
  ) {
    final ticket = _coordinator.admitCommand(
      FlarkEditorCommandKind.sourceEdit,
      publishSourceImmediately: true,
    );
    final result = _coordinator.queueEdit(
      () => _session.applyEditUtf16(
        command.startUtf16,
        command.endUtf16,
        command.replacement,
        beforeSelection: command.beforeSelection,
        afterSelection: command.afterSelection,
        coalesceTyping: command.coalesceTyping,
        compositionGroup: command.compositionGroup,
        compositionFinal: command.compositionFinal,
      ),
    );
    return _execution(ticket, result);
  }

  FlarkEditorCommandExecution<FlarkCoreEditIntentOutcomeV1> executeSemanticEdit(
    FlarkCoreEditIntentV1 intent,
  ) {
    final ticket = _coordinator.admitCommand(
      FlarkEditorCommandKind.semanticEdit,
    );
    final result = _coordinator.afterEdits(
      () => _session.applyEditIntentOutcomeV1(
        intent,
        compositionActive: _session.compositionActive,
      ),
    );
    return _execution(ticket, result);
  }

  FlarkEditorCommandExecution<FlarkCoreEditIntentReceiptV1>
  executeSemanticAction(
    FlarkCoreSemanticActionV1 action, {
    required int targetUtf16,
  }) {
    final ticket = _coordinator.admitCommand(
      FlarkEditorCommandKind.semanticAction,
    );
    final result = _coordinator.afterEdits(
      () => _session.applySemanticActionV1(action, targetUtf16: targetUtf16),
    );
    return _execution(ticket, result);
  }

  FlarkEditorCommandExecution<FlarkCoreHistoryOutcome?> executeHistory(
    FlarkHistoryDirection direction,
  ) {
    final ticket = _coordinator.admitCommand(
      FlarkEditorCommandKind.historyReplay,
    );
    final result = _coordinator.afterEdits(() async {
      // The history boundary belongs after every edit already admitted ahead
      // of it, never at host callback time.
      _session.breakTypingGroup();
      _session.endCompositionGroup();
      return direction == FlarkHistoryDirection.undo
          ? _session.undo()
          : _session.redo();
    });
    return _execution(ticket, result);
  }

  FlarkEditorCommandExecution<FlarkCoreHistoryOutcome?>
  executeCompositionCancel() {
    final ticket = _coordinator.admitCommand(
      FlarkEditorCommandKind.compositionCancel,
    );
    return _execution(
      ticket,
      _coordinator.afterEdits(_session.cancelComposition),
    );
  }

  bool publishSource<T>(FlarkEditorCommandExecution<T> execution) =>
      _coordinator.publishCommandSource(execution._ticket);

  FlarkPendingPresentationAdoption? adoptCommittedPresentation(
    FlarkEditorCommandExecution<FlarkCoreEditIntentOutcomeV1> execution, {
    required FlarkCoreEditIntentReceiptV1 receipt,
    required FlarkCoreCommittedPresentationTransitionV1? transition,
  }) => _coordinator.adoptCommittedPresentation(
    command: execution._ticket,
    receipt: receipt,
    transition: transition,
  );

  void complete<T>(FlarkEditorCommandExecution<T> execution) {
    _coordinator.completeCommand(execution._ticket);
  }

  void fail<T>(FlarkEditorCommandExecution<T> execution, Object error) {
    _coordinator.failCommand(execution._ticket, error);
  }

  void trackAdoption<T>(Future<T> completion) {
    _coordinator.trackEdit(completion);
  }

  void trackSourceAdoption(Future<void> completion) {
    _coordinator.trackSourceAdoption(completion);
  }

  FlarkEditorCommandExecution<T> _execution<T>(
    FlarkEditorCommandTicket ticket,
    Future<T> result,
  ) => FlarkEditorCommandExecution._(
    generation: ticket.generation,
    result: result,
    ticket: ticket,
  );
}
