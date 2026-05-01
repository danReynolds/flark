import 'package:flutter/services.dart';

/// Stable reason codes for command no-op and rejection results.
enum SovereignCommandReasonCode {
  /// Command did not provide a more specific reason.
  unknown('unknown'),

  /// Command would not change the document.
  noChange('no_change'),

  /// Command was blocked while IME composition was active.
  imeComposing('ime_composing'),

  /// Command required a non-empty URL.
  emptyUrl('empty_url'),

  /// Command expected an active inline style but none was present.
  noActiveInlineStyle('no_active_inline_style'),

  /// Command does not support the requested inline style.
  unsupportedInlineStyle('unsupported_inline_style'),

  /// Command could not operate on an empty inline wrapper.
  noEmptyWrapper('no_empty_wrapper');

  /// Stable string used for logging, persistence, or telemetry.
  final String wireName;

  const SovereignCommandReasonCode(this.wireName);
}

/// Result returned from a markdown command.
sealed class SovereignCommandResult {
  const SovereignCommandResult();
}

/// Command applied a document mutation.
class SovereignCommandApplied extends SovereignCommandResult {
  /// Selection after the command completed.
  final TextSelection selection;

  /// Creates an applied result with the resulting [selection].
  const SovereignCommandApplied(this.selection);
}

/// Command was valid but did not need to mutate the document.
class SovereignCommandNoOp extends SovereignCommandResult {
  /// Human-readable reason.
  final String reason;

  /// Stable reason code.
  final SovereignCommandReasonCode reasonCode;

  /// Creates a no-op result.
  const SovereignCommandNoOp(
    this.reason, {
    this.reasonCode = SovereignCommandReasonCode.unknown,
  });

  /// Creates a no-op result from a stable [reasonCode].
  factory SovereignCommandNoOp.code(SovereignCommandReasonCode reasonCode) {
    return SovereignCommandNoOp(reasonCode.wireName, reasonCode: reasonCode);
  }
}

/// Command was rejected because its preconditions were not met.
class SovereignCommandRejected extends SovereignCommandResult {
  /// Human-readable rejection reason.
  final String reason;

  /// Stable rejection reason code.
  final SovereignCommandReasonCode reasonCode;

  /// Creates a rejected result.
  const SovereignCommandRejected(
    this.reason, {
    this.reasonCode = SovereignCommandReasonCode.unknown,
  });

  /// Creates a rejected result from a stable [reasonCode].
  factory SovereignCommandRejected.code(SovereignCommandReasonCode reasonCode) {
    return SovereignCommandRejected(
      reasonCode.wireName,
      reasonCode: reasonCode,
    );
  }
}
