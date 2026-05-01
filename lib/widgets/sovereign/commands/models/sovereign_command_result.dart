import 'package:flutter/services.dart';

enum SovereignCommandReasonCode {
  unknown('unknown'),
  noChange('no_change'),
  imeComposing('ime_composing'),
  emptyUrl('empty_url'),
  noActiveInlineStyle('no_active_inline_style'),
  unsupportedInlineStyle('unsupported_inline_style'),
  noEmptyWrapper('no_empty_wrapper');

  final String wireName;

  const SovereignCommandReasonCode(this.wireName);
}

sealed class SovereignCommandResult {
  const SovereignCommandResult();
}

class SovereignCommandApplied extends SovereignCommandResult {
  final TextSelection selection;

  const SovereignCommandApplied(this.selection);
}

class SovereignCommandNoOp extends SovereignCommandResult {
  final String reason;
  final SovereignCommandReasonCode reasonCode;

  const SovereignCommandNoOp(
    this.reason, {
    this.reasonCode = SovereignCommandReasonCode.unknown,
  });

  factory SovereignCommandNoOp.code(SovereignCommandReasonCode reasonCode) {
    return SovereignCommandNoOp(reasonCode.wireName, reasonCode: reasonCode);
  }
}

class SovereignCommandRejected extends SovereignCommandResult {
  final String reason;
  final SovereignCommandReasonCode reasonCode;

  const SovereignCommandRejected(
    this.reason, {
    this.reasonCode = SovereignCommandReasonCode.unknown,
  });

  factory SovereignCommandRejected.code(SovereignCommandReasonCode reasonCode) {
    return SovereignCommandRejected(
      reasonCode.wireName,
      reasonCode: reasonCode,
    );
  }
}
