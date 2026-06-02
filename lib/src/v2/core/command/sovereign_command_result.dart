import '../transaction/sovereign_transaction.dart';

enum FlarkCommandResultKind { handled, notHandled, rejected }

final class FlarkCommandResult {
  const FlarkCommandResult._({
    required this.kind,
    this.transaction,
    this.reason,
  });

  factory FlarkCommandResult.handled({FlarkTransaction? transaction}) {
    return FlarkCommandResult._(
      kind: FlarkCommandResultKind.handled,
      transaction: transaction,
    );
  }

  const FlarkCommandResult.notHandled()
    : this._(kind: FlarkCommandResultKind.notHandled);

  factory FlarkCommandResult.rejected(String reason) {
    return FlarkCommandResult._(
      kind: FlarkCommandResultKind.rejected,
      reason: reason,
    );
  }

  final FlarkCommandResultKind kind;
  final FlarkTransaction? transaction;
  final String? reason;

  bool get isHandled => kind == FlarkCommandResultKind.handled;

  bool get isNotHandled => kind == FlarkCommandResultKind.notHandled;

  bool get isRejected => kind == FlarkCommandResultKind.rejected;
}
