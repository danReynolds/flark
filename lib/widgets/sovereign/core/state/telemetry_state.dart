class TelemetryState {
  const TelemetryState({
    required this.predictiveBudgetExhaustionCount,
    required this.predictiveLocalFallbackCount,
    required this.predictiveLocalFallbackLastScannedChars,
    required this.parsePendingReplaceCount,
    required this.parseStaleDropCount,
    required this.renderCallCount,
    required this.renderLastMicros,
    required this.renderMaxMicros,
  });

  final int predictiveBudgetExhaustionCount;
  final int predictiveLocalFallbackCount;
  final int predictiveLocalFallbackLastScannedChars;
  final int parsePendingReplaceCount;
  final int parseStaleDropCount;
  final int renderCallCount;
  final int renderLastMicros;
  final int renderMaxMicros;

  TelemetryState copyWith({
    int? predictiveBudgetExhaustionCount,
    int? predictiveLocalFallbackCount,
    int? predictiveLocalFallbackLastScannedChars,
    int? parsePendingReplaceCount,
    int? parseStaleDropCount,
    int? renderCallCount,
    int? renderLastMicros,
    int? renderMaxMicros,
  }) {
    return TelemetryState(
      predictiveBudgetExhaustionCount: predictiveBudgetExhaustionCount ??
          this.predictiveBudgetExhaustionCount,
      predictiveLocalFallbackCount:
          predictiveLocalFallbackCount ?? this.predictiveLocalFallbackCount,
      predictiveLocalFallbackLastScannedChars:
          predictiveLocalFallbackLastScannedChars ??
              this.predictiveLocalFallbackLastScannedChars,
      parsePendingReplaceCount:
          parsePendingReplaceCount ?? this.parsePendingReplaceCount,
      parseStaleDropCount: parseStaleDropCount ?? this.parseStaleDropCount,
      renderCallCount: renderCallCount ?? this.renderCallCount,
      renderLastMicros: renderLastMicros ?? this.renderLastMicros,
      renderMaxMicros: renderMaxMicros ?? this.renderMaxMicros,
    );
  }
}
