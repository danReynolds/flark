import 'dart:convert';

import 'package:crypto/crypto.dart' as crypto;

/// Adapter states from `test/fixtures/v4/input_window_matrix_v1.json`.
enum FlarkInputWindowState {
  detached,
  synchronized,
  compositionPinned,
  bulkStaging,
  resyncRequired,
  closed,
  faulted,
}

/// Typed reasons an active-callback mismatch retired the connection. The
/// contract requires that the rejected callback mutates nothing.
enum FlarkInputResyncReason {
  none,
  staleRevision,
  oldTextMismatch,
  deltaChainMismatch,
  rangeOutOfWindow,
  batchOverEnvelope,
  staleSelectionGeneration,
  successorQueueOverflow,
  successorReconciliationFailed,
  unsupportedSuccessorObservation,
}

/// The host-attached serialized shadow of the active platform client: the
/// authority every platform callback is validated against before any source
/// or selection mutation.
final class FlarkInputWindowShadow {
  const FlarkInputWindowShadow({
    required this.connectionEpoch,
    required this.windowEpoch,
    required this.representedRevision,
    required this.globalUtf16Start,
    required this.windowUtf16Length,
    required this.windowTextSha256,
    required this.selectionGeneration,
  });

  /// Nonzero while attached; changes on every reconnect, resynchronization,
  /// or host-originated exposed-state change.
  final int connectionEpoch;

  /// Nonzero; increases for platform-originated updates within one
  /// connection and resets to one on a new connection.
  final int windowEpoch;

  final int representedRevision;
  final int globalUtf16Start;
  final int windowUtf16Length;

  /// SHA-256 of the complete exposed window text (UTF-8 bytes), hex encoded.
  final String windowTextSha256;

  final int selectionGeneration;
}

String flarkWindowTextSha256(String text) =>
    crypto.sha256.convert(utf8.encode(text)).toString();
