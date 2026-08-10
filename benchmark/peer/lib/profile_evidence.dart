import 'dart:io';

/// Whether one engine frame can be proven to contain a model edit accepted at
/// [acceptedTraceMicros].
///
/// Raster completion alone is insufficient: a frame that began building
/// before acceptance cannot prove that it painted the accepted model state.
/// Equality is also rejected because microsecond timestamps cannot establish
/// which event happened first.
bool provesAcceptedEditFrame({
  required int acceptedTraceMicros,
  required int buildStartTraceMicros,
  required int rasterFinishTraceMicros,
  required int timingCallbackTraceMicros,
}) =>
    buildStartTraceMicros > acceptedTraceMicros &&
    rasterFinishTraceMicros >= buildStartTraceMicros &&
    timingCallbackTraceMicros >= rasterFinishTraceMicros;

/// Builds a collision-resistant export path for one fresh profile process.
///
/// [processRunId] is unique across orchestrator invocations as well as within
/// one run plan. Restricting it to a filename-safe alphabet also prevents two
/// distinct identifiers from collapsing to the same sanitized name.
String exportArtifactPath({
  required String exportDirectory,
  required String processRunId,
}) {
  if (processRunId.isEmpty ||
      !RegExp(r'^[A-Za-z0-9][A-Za-z0-9._-]*$').hasMatch(processRunId)) {
    throw ArgumentError.value(
      processRunId,
      'processRunId',
      'must be a nonempty filename-safe identifier',
    );
  }
  return File(
    '${Directory(exportDirectory).path}/$processRunId.final-source.txt',
  ).path;
}

final class EvidenceEligibility {
  const EvidenceEligibility({required this.eligible, required this.blockers});

  final bool eligible;
  final List<String> blockers;
}

EvidenceEligibility evaluateProcessCompletionEnvelope({
  required bool protocolConfiguration,
  required bool allAcceptedInputsHaveProvenFrames,
  required bool finalExportWritten,
  required bool sourceFidelityClassified,
  required bool inputBacklogDrained,
}) {
  final blockers = <String>[
    if (!protocolConfiguration)
      'The process used non-protocol size, sample counts, or timeout.',
    if (!allAcceptedInputsHaveProvenFrames)
      'At least one accepted input lacks a frame whose build began after acceptance.',
    if (!finalExportWritten)
      'The process did not write its final source export artifact.',
    if (!sourceFidelityClassified)
      'The process has an unclassified or lossy source-fidelity difference.',
    if (!inputBacklogDrained)
      'The process completed with accepted input still awaiting raster evidence.',
  ];
  return EvidenceEligibility(
    eligible: blockers.isEmpty,
    blockers: List<String>.unmodifiable(blockers),
  );
}

EvidenceEligibility evaluateAggregateCompletionEnvelope({
  required bool protocolInvocation,
  required int plannedProcessCount,
  required int completedProcessCount,
  required int eligibleProcessCount,
  required int failedProcessCount,
}) {
  if (plannedProcessCount < 0 ||
      completedProcessCount < 0 ||
      eligibleProcessCount < 0 ||
      failedProcessCount < 0 ||
      completedProcessCount > plannedProcessCount ||
      eligibleProcessCount > completedProcessCount) {
    throw ArgumentError('Aggregate process counts are inconsistent.');
  }
  final blockers = <String>[
    if (!protocolInvocation) 'This aggregate used the non-claim smoke plan.',
    if (completedProcessCount != plannedProcessCount)
      'Only $completedProcessCount of $plannedProcessCount planned processes produced results.',
    if (eligibleProcessCount != plannedProcessCount)
      'Only $eligibleProcessCount of $plannedProcessCount planned processes satisfy the completion envelope.',
    if (failedProcessCount != 0)
      '$failedProcessCount process run(s) failed or timed out.',
  ];
  return EvidenceEligibility(
    eligible: blockers.isEmpty,
    blockers: List<String>.unmodifiable(blockers),
  );
}

/// A single-peer local runner never establishes a publishable cohort claim.
EvidenceEligibility localPerformanceClaimEligibility({required String scope}) {
  return EvidenceEligibility(
    eligible: false,
    blockers: List<String>.unmodifiable(<String>[
      if (scope == 'process')
        'An individual process result is not an aggregate performance receipt.',
      'This Quill-only runner does not establish two-peer cohort eligibility.',
      'The minimal runner does not capture a VM timeline or longest synchronous span.',
      'Five-minute idle and exclusive-host controls require external operator evidence.',
    ]),
  );
}
