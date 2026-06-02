import '../transaction/sovereign_transaction.dart';

enum SovereignCommandResultKind {
  handled,
  notHandled,
  rejected,
}

final class SovereignCommandResult {
  const SovereignCommandResult._({
    required this.kind,
    this.transaction,
    this.reason,
  });

  factory SovereignCommandResult.handled({
    SovereignTransaction? transaction,
  }) {
    return SovereignCommandResult._(
      kind: SovereignCommandResultKind.handled,
      transaction: transaction,
    );
  }

  const SovereignCommandResult.notHandled()
      : this._(kind: SovereignCommandResultKind.notHandled);

  factory SovereignCommandResult.rejected(String reason) {
    return SovereignCommandResult._(
      kind: SovereignCommandResultKind.rejected,
      reason: reason,
    );
  }

  final SovereignCommandResultKind kind;
  final SovereignTransaction? transaction;
  final String? reason;

  bool get isHandled => kind == SovereignCommandResultKind.handled;

  bool get isNotHandled => kind == SovereignCommandResultKind.notHandled;

  bool get isRejected => kind == SovereignCommandResultKind.rejected;
}
