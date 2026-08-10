import 'package:flark_peer_benchmark/profile_evidence.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('input-to-raster correlation', () {
    test('rejects a frame that only finishes after model acceptance', () {
      expect(
        provesAcceptedEditFrame(
          acceptedTraceMicros: 150,
          buildStartTraceMicros: 100,
          rasterFinishTraceMicros: 200,
          timingCallbackTraceMicros: 220,
        ),
        isFalse,
      );
    });

    test('requires strict post-acceptance build ordering', () {
      expect(
        provesAcceptedEditFrame(
          acceptedTraceMicros: 150,
          buildStartTraceMicros: 150,
          rasterFinishTraceMicros: 200,
          timingCallbackTraceMicros: 220,
        ),
        isFalse,
      );
      expect(
        provesAcceptedEditFrame(
          acceptedTraceMicros: 150,
          buildStartTraceMicros: 151,
          rasterFinishTraceMicros: 200,
          timingCallbackTraceMicros: 220,
        ),
        isTrue,
      );
    });

    test('fails closed when raster or callback clocks are unordered', () {
      expect(
        provesAcceptedEditFrame(
          acceptedTraceMicros: 100,
          buildStartTraceMicros: 120,
          rasterFinishTraceMicros: 119,
          timingCallbackTraceMicros: 140,
        ),
        isFalse,
      );
      expect(
        provesAcceptedEditFrame(
          acceptedTraceMicros: 100,
          buildStartTraceMicros: 120,
          rasterFinishTraceMicros: 130,
          timingCallbackTraceMicros: 129,
        ),
        isFalse,
      );
    });
  });

  test('fresh process identifiers produce distinct export artifacts', () {
    final first = exportArtifactPath(
      exportDirectory: '/tmp/quill-exports',
      processRunId: 'invocation-a-000-cold-open-r0',
    );
    final second = exportArtifactPath(
      exportDirectory: '/tmp/quill-exports',
      processRunId: 'invocation-a-001-cold-open-r1',
    );
    expect(first, isNot(second));
    expect(first, endsWith('invocation-a-000-cold-open-r0.final-source.txt'));
    expect(
      () => exportArtifactPath(
        exportDirectory: '/tmp/quill-exports',
        processRunId: '../collision',
      ),
      throwsArgumentError,
    );
  });

  test('completion eligibility is independent of performance claims', () {
    final processCompletion = evaluateProcessCompletionEnvelope(
      protocolConfiguration: true,
      allAcceptedInputsHaveProvenFrames: true,
      finalExportWritten: true,
      sourceFidelityClassified: true,
      inputBacklogDrained: true,
    );
    final aggregateCompletion = evaluateAggregateCompletionEnvelope(
      protocolInvocation: true,
      plannedProcessCount: 2,
      completedProcessCount: 2,
      eligibleProcessCount: 2,
      failedProcessCount: 0,
    );
    final performance = localPerformanceClaimEligibility(scope: 'aggregate');

    expect(processCompletion.eligible, isTrue);
    expect(aggregateCompletion.eligible, isTrue);
    expect(performance.eligible, isFalse);
    expect(
      performance.blockers,
      contains(contains('two-peer cohort eligibility')),
    );
  });

  test('smoke and incomplete aggregates cannot complete the envelope', () {
    final decision = evaluateAggregateCompletionEnvelope(
      protocolInvocation: false,
      plannedProcessCount: 4,
      completedProcessCount: 3,
      eligibleProcessCount: 2,
      failedProcessCount: 1,
    );
    expect(decision.eligible, isFalse);
    expect(decision.blockers, hasLength(4));
  });
}
