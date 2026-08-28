/// Layer attribution for the most recent platform-observed semantic edit.
/// The profile harness joins this receipt to Flutter's proving frame.
final class FlarkSemanticEditPerformance {
  const FlarkSemanticEditPerformance({
    required this.sourceGeneration,
    this.acceptedAtEpochMicros = 0,
    required this.platformCallbackMicros,
    required this.coreQueueMicros,
    required this.workerRoundTripMicros,
    required this.workerQueueMicros,
    required this.nativeFfiMicros,
    required this.coreAdoptionMicros,
    required this.flutterReceiptAdoptionMicros,
    required this.callbackToReceiptMicros,
  });

  final int sourceGeneration;
  final int acceptedAtEpochMicros;
  final int platformCallbackMicros;
  final int coreQueueMicros;
  final int workerRoundTripMicros;
  final int workerQueueMicros;
  final int nativeFfiMicros;
  final int coreAdoptionMicros;
  final int flutterReceiptAdoptionMicros;
  final int callbackToReceiptMicros;
}

/// Layer attribution for a committed source or history transaction.
///
/// This bounded diagnostic stream is consumed by the D0 profile harness. It
/// observes the existing source-authoritative command path and does not
/// participate in editing or presentation authority.
enum FlarkSourceEditPerformanceKind { source, undo, redo }

final class FlarkSourceEditPerformance {
  const FlarkSourceEditPerformance({
    required this.kind,
    required this.sourceGeneration,
    this.acceptedAtEpochMicros = 0,
    this.editorSyncMicros = 0,
    required this.coreQueueMicros,
    required this.workerRoundTripMicros,
    required this.workerQueueMicros,
    required this.nativeFfiMicros,
    required this.coreAdoptionMicros,
    required this.flutterReceiptAdoptionMicros,
    required this.acceptanceToReceiptMicros,
  });

  final FlarkSourceEditPerformanceKind kind;
  final int sourceGeneration;
  final int acceptedAtEpochMicros;
  final int editorSyncMicros;
  final int coreQueueMicros;
  final int workerRoundTripMicros;
  final int workerQueueMicros;
  final int nativeFfiMicros;
  final int coreAdoptionMicros;
  final int flutterReceiptAdoptionMicros;
  final int acceptanceToReceiptMicros;
}
