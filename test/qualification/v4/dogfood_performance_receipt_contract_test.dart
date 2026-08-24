// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

import '../../../packages/flark/example/lib/dogfood_documents.dart';
import '../../../scripts/dogfood_fixture_identity.dart';
import '../../../scripts/verify_v4_dogfood_receipt.dart';
import '../../../scripts/dogfood_performance_receipt.dart';

String _visibleHash(String text) =>
    sha256.convert(utf8.encode(text)).toString();

void main() {
  test('schema and replay freeze the complete D0 denominator', () async {
    final schema =
        jsonDecode(
              File(
                'docs/testing/dogfood_performance_v1.schema.json',
              ).readAsStringSync(),
            )
            as Map<String, Object?>;
    expect(schema[r'$schema'], 'https://json-schema.org/draft/2020-12/schema');
    expect(schema['additionalProperties'], isFalse);

    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final result = await verifyDogfoodPerformanceReceipt(
      sealed,
      verifyArtifactFiles: false,
    );
    expect(result.blockers, isEmpty);
    expect((sealed['assessment']! as Map)['result'], 'PASS');
    expect(
      (result.metrics['sourceToPaintMicros']! as Map)['sampleCount'],
      3760,
    );
    expect((result.metrics['flutterFrameMicros']! as Map)['sampleCount'], 5335);
    expect((result.metrics['engineMicros']! as Map)['sampleCount'], 5310);
    expect((result.metrics['openToEditableMicros']! as Map)['sampleCount'], 25);
  });

  test('replay fails closed on timing and denominator tampering', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final timingTampered = _copy(sealed);
    final sample = _firstSample(timingTampered);
    sample['sourcePaintMicros'] = (sample['acceptedMicros']! as int) + 20000;
    final timingResult = await verifyDogfoodPerformanceReceipt(
      timingTampered,
      verifyArtifactFiles: false,
    );
    expect(
      timingResult.blockers.join('\n'),
      contains('did not paint source/caret/selection by the next frame'),
    );
    expect(
      timingResult.blockers.join('\n'),
      contains('assessment.metrics does not match replayed metrics'),
    );

    final denominatorTampered = _copy(sealed);
    _cells(denominatorTampered).first['samplesPerRun'] = 119;
    final denominatorResult = await verifyDogfoodPerformanceReceipt(
      denominatorTampered,
      verifyArtifactFiles: false,
    );
    expect(
      denominatorResult.blockers.join('\n'),
      contains('denominator does not match the frozen D0 matrix'),
    );
  });

  test('replay rejects a forged assessment and missing open proof', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final forged = _copy(sealed);
    final metrics = (forged['assessment']! as Map)['metrics']! as Map;
    (metrics['engineMicros']! as Map)['p99'] = 0;
    final forgedResult = await verifyDogfoodPerformanceReceipt(
      forged,
      verifyArtifactFiles: false,
    );
    expect(
      forgedResult.blockers,
      contains('assessment.metrics does not match replayed metrics'),
    );

    final missingOpen = _copy(sealed);
    final cold = _cells(
      missingOpen,
    ).firstWhere((cell) => cell['id'] == 'product-tour-cold-launch');
    final coldRun = (cold['runs']! as List).first as Map;
    coldRun['openObservation'] = null;
    final openResult = await verifyDogfoodPerformanceReceipt(
      missingOpen,
      verifyArtifactFiles: false,
    );
    expect(
      openResult.blockers.join('\n'),
      contains('openObservation is required'),
    );
  });

  test('replay rejects raw paint, input, and engine disagreement', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );

    final paintTampered = _copy(sealed);
    final paintRun = _firstRun(paintTampered);
    final paint = (paintRun['paintObservations']! as List).first as Map;
    paint['expectedVisibleSourceSha256'] = _hash('d');
    final paintResult = await verifyDogfoodPerformanceReceipt(
      paintTampered,
      verifyArtifactFiles: false,
    );
    expect(paintResult.blockers.join('\n'), contains('raw paint 0 is torn'));

    final inputTampered = _copy(sealed);
    final inputRun = _firstRun(inputTampered);
    final input = (inputRun['inputObservations']! as List).first as Map;
    input['sourceSha256'] = _hash('d');
    final inputResult = await verifyDogfoodPerformanceReceipt(
      inputTampered,
      verifyArtifactFiles: false,
    );
    expect(
      inputResult.blockers.join('\n'),
      contains('does not match its raw input observations'),
    );

    final engineTampered = _copy(sealed);
    final engineRun = _firstRun(engineTampered);
    final engine = (engineRun['engineObservations']! as List).first as Map;
    engine['nativeFfiMicros'] = 101;
    final engineResult = await verifyDogfoodPerformanceReceipt(
      engineTampered,
      verifyArtifactFiles: false,
    );
    expect(
      engineResult.blockers.join('\n'),
      contains('engine timing does not replay'),
    );
  });

  test('replay rejects forged clock mapping and paint frame joins', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );

    final clockTampered = _copy(sealed);
    final clockRun = _firstRun(clockTampered);
    final clockFrame = (clockRun['frames']! as List).first as Map;
    clockFrame['vsyncMicros'] = (clockFrame['vsyncMicros']! as int) + 1;
    final clockResult = await verifyDogfoodPerformanceReceipt(
      clockTampered,
      verifyArtifactFiles: false,
    );
    expect(
      clockResult.blockers.join('\n'),
      contains('epoch timestamp does not replay its clock'),
    );

    final joinTampered = _copy(sealed);
    final joinRun = _firstRun(joinTampered);
    final paint = (joinRun['paintObservations']! as List).first as Map;
    paint['frameStampMicros'] = 0;
    final joinResult = await verifyDogfoodPerformanceReceipt(
      joinTampered,
      verifyArtifactFiles: false,
    );
    expect(
      joinResult.blockers.join('\n'),
      contains('does not replay its FrameTiming join'),
    );

    final paintClockTampered = _copy(sealed);
    final paintClockRun = _firstRun(paintClockTampered);
    final clockPaint =
        (paintClockRun['paintObservations']! as List).first as Map;
    for (final name in const [
      'timestampMicros',
      'paintEpochBeforeMicros',
      'paintEpochAfterMicros',
    ]) {
      clockPaint[name] = (clockPaint[name]! as int) + 1000;
    }
    final paintClockResult = await verifyDogfoodPerformanceReceipt(
      paintClockTampered,
      verifyArtifactFiles: false,
    );
    expect(
      paintClockResult.blockers.join('\n'),
      contains('does not replay its paint clock'),
    );

    final coordinatedShift = _copy(sealed);
    final coordinatedRun = _firstRun(coordinatedShift);
    final coordinatedPaint =
        (coordinatedRun['paintObservations']! as List).first as Map;
    for (final name in const [
      'timestampMicros',
      'paintMonotonicMicros',
      'paintEpochBeforeMicros',
      'paintEpochAfterMicros',
    ]) {
      coordinatedPaint[name] = (coordinatedPaint[name]! as int) + 100000;
    }
    final coordinatedResult = await verifyDogfoodPerformanceReceipt(
      coordinatedShift,
      verifyArtifactFiles: false,
    );
    expect(
      coordinatedResult.blockers.join('\n'),
      contains('does not replay its paint clock'),
    );

    final buildIntervalMismatch = _copy(sealed);
    final buildIntervalRun = _firstRun(buildIntervalMismatch);
    final buildIntervalFrame =
        (buildIntervalRun['frames']! as List).first as Map;
    buildIntervalFrame['buildFinishMonotonicMicros'] =
        (buildIntervalFrame['buildFinishMonotonicMicros']! as int) + 1;
    final buildIntervalResult = await verifyDogfoodPerformanceReceipt(
      buildIntervalMismatch,
      verifyArtifactFiles: false,
    );
    expect(
      buildIntervalResult.blockers.join('\n'),
      contains('has an invalid build interval'),
    );
  });

  test(
    'budgets are enforced per workload rather than diluted globally',
    () async {
      final sealed = await sealDogfoodPerformanceReceipt(
        validRawDogfoodPerformanceReceiptForTest(),
        verifyArtifactFiles: false,
      );
      final tampered = _copy(sealed);
      final cell = _cells(
        tampered,
      ).firstWhere((value) => value['id'] == 'product-tour-typing');
      final run = ((cell['runs']! as List).first as Map)
          .cast<String, Object?>();
      final engines = (run['engineObservations']! as List).cast<Map>();
      final samples = (run['samples']! as List).cast<Map>();
      for (var index = 0; index < 4; index += 1) {
        engines[20 + index]['nativeFfiMicros'] = 5000;
        samples[index]['engineMicros'] = 5000;
      }
      final result = await verifyDogfoodPerformanceReceipt(
        tampered,
        verifyArtifactFiles: false,
      );
      expect(
        result.blockers,
        contains('cell[product-tour-typing] Rust engine p99 exceeded 4 ms'),
      );
      expect(
        result.blockers,
        isNot(contains('Rust engine aggregate p99 exceeded 4 ms')),
      );
    },
  );

  test(
    'structural coalescing and complete frame intervals are explicit',
    () async {
      Future<String> replayBlockers(Map<String, Object?> raw) async {
        final sealed = await sealDogfoodPerformanceReceipt(
          raw,
          verifyArtifactFiles: false,
        );
        final result = await verifyDogfoodPerformanceReceipt(
          sealed,
          verifyArtifactFiles: false,
        );
        return result.blockers.join('\n');
      }

      final coalesced = validRawDogfoodPerformanceReceiptForTest();
      final coalescedStructural = _cells(
        coalesced,
      ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
      final coalescedRun = ((coalescedStructural['runs']! as List).first as Map)
          .cast<String, Object?>();
      for (final phase in [
        coalescedRun,
        (coalescedRun['structuralBurst']! as Map).cast<String, Object?>(),
      ]) {
        _coalesceFirstStructuralReturn(phase, attributeSync: true);
      }
      final coalescedSealed = await sealDogfoodPerformanceReceipt(
        coalesced,
        verifyArtifactFiles: false,
      );
      final coalescedResult = await verifyDogfoodPerformanceReceipt(
        coalescedSealed,
        verifyArtifactFiles: false,
      );
      expect(coalescedResult.blockers, isEmpty);

      final omittedCoalescedSync = validRawDogfoodPerformanceReceiptForTest();
      final omittedStructural = _cells(
        omittedCoalescedSync,
      ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
      final omittedRun = ((omittedStructural['runs']! as List).first as Map)
          .cast<String, Object?>();
      for (final phase in [
        omittedRun,
        (omittedRun['structuralBurst']! as Map).cast<String, Object?>(),
      ]) {
        _coalesceFirstStructuralReturn(phase, attributeSync: false);
      }
      final omittedCoalescedSyncSealed = await sealDogfoodPerformanceReceipt(
        omittedCoalescedSync,
        verifyArtifactFiles: false,
      );
      final omittedCoalescedSyncResult = await verifyDogfoodPerformanceReceipt(
        omittedCoalescedSyncSealed,
        verifyArtifactFiles: false,
      );
      expect(
        omittedCoalescedSyncResult.blockers.join('\n'),
        contains(
          'does not exactly attribute coalesced synchronous editor work',
        ),
      );

      final framedCoalescing = validRawDogfoodPerformanceReceiptForTest();
      final framedRun =
          ((_cells(framedCoalescing).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      (framedRun['paintObservations']! as List).removeWhere(
        (value) => (value as Map)['sourceGeneration'] == 1,
      );
      expect(
        await replayBlockers(framedCoalescing),
        contains('coalesced Return despite a frame opportunity'),
      );

      final substitutedControl = validRawDogfoodPerformanceReceiptForTest();
      final substitutedRun =
          ((_cells(substitutedControl).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final substituted =
          _copy(
              (substitutedRun['structuralBurst']! as Map)
                  .cast<String, Object?>(),
            )
            ..['structuralPhase'] = 'perEditControl'
            ..['structuralSessionIdentity'] = 'substituted-control'
            ..['structuralCommandTranscript'] = _structuralTranscript(
              'perEditControl',
              140,
            );
      substitutedRun['structuralPerEditControl'] = substituted;
      expect(
        await replayBlockers(substitutedControl),
        anyOf(
          contains('observations escaped their app-echoed session'),
          contains('does not preserve the frozen pair denominator'),
          contains('paint observations do not exactly cover'),
        ),
      );

      final staleReturn = validRawDogfoodPerformanceReceiptForTest();
      final staleReturnRun =
          ((_cells(staleReturn).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final staleReturnBurst = (staleReturnRun['structuralBurst']! as Map)
          .cast<String, Object?>();
      final staleInputs = (staleReturnBurst['inputObservations']! as List)
          .cast<Map>();
      final stalePaint = (staleReturnBurst['paintObservations']! as List)
          .cast<Map>()
          .firstWhere((paint) => paint['sourceGeneration'] == 1);
      stalePaint['timestampMicros'] = staleInputs[1]['acceptedMicros'];
      expect(
        await replayBlockers(staleReturn),
        contains('Return did not paint on its first opportunity'),
      );

      final staleSuccessor = validRawDogfoodPerformanceReceiptForTest();
      final staleSuccessorRun =
          ((_cells(staleSuccessor).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final staleSuccessorInputs =
          (staleSuccessorRun['inputObservations']! as List).cast<Map>();
      final staleSuccessorPaint =
          (staleSuccessorRun['paintObservations']! as List)
              .cast<Map>()
              .firstWhere((paint) => paint['sourceGeneration'] == 2);
      final thirdAcceptance = staleSuccessorInputs[2]['acceptedMicros']! as int;
      staleSuccessorPaint
        ..['timestampMicros'] = thirdAcceptance
        ..['paintMonotonicMicros'] = thirdAcceptance
        ..['paintEpochBeforeMicros'] = thirdAcceptance
        ..['paintEpochAfterMicros'] = thirdAcceptance;
      expect(
        await replayBlockers(staleSuccessor),
        contains('has a stale or raw paint'),
      );

      final unhealthyBurst = validRawDogfoodPerformanceReceiptForTest();
      final unhealthyRun =
          ((_cells(unhealthyBurst).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      (unhealthyRun['structuralBurst']! as Map).cast<String, Object?>()
        ..['faulted'] = true
        ..['resyncCount'] = 1;
      expect(
        await replayBlockers(unhealthyBurst),
        contains('structuralBurst faulted or resynchronized'),
      );

      final unhealthySample = validRawDogfoodPerformanceReceiptForTest();
      final unhealthySampleRun =
          ((_cells(unhealthySample).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final unhealthySampleBurst =
          (unhealthySampleRun['structuralBurst']! as Map)
              .cast<String, Object?>();
      ((unhealthySampleBurst['samples']! as List).single as Map)
        ..['faulted'] = true
        ..['resyncCount'] = 1;
      expect(
        await replayBlockers(unhealthySample),
        contains('burst sample faulted or resynchronized'),
      );

      final forgedAcknowledgement = validRawDogfoodPerformanceReceiptForTest();
      final forgedAcknowledgementRun =
          ((_cells(forgedAcknowledgement).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      ((forgedAcknowledgementRun['structuralSetupAcknowledgements']! as List)
                  .first
              as Map)['canaryId'] =
          'relabelled-session';
      expect(
        await replayBlockers(forgedAcknowledgement),
        contains('setup app acknowledgement 0 is invalid'),
      );

      final skippedAppCommand = validRawDogfoodPerformanceReceiptForTest();
      final skippedAppRun =
          ((_cells(skippedAppCommand).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final firstMeasurementAcknowledgement =
          ((skippedAppRun['structuralAppAcknowledgements']! as List).first
              as Map);
      firstMeasurementAcknowledgement['appCommandSequence'] =
          (firstMeasurementAcknowledgement['appCommandSequence']! as int) + 1;
      expect(
        await replayBlockers(skippedAppCommand),
        contains('measurement app acknowledgement 0 is invalid'),
      );

      final omittedPreparation = validRawDogfoodPerformanceReceiptForTest();
      final omittedPreparationRun =
          ((_cells(omittedPreparation).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      omittedPreparationRun['structuralActuatorSequenceStart'] = 4;
      expect(
        await replayBlockers(omittedPreparation),
        contains('structural phases do not have ordered actuator ranges'),
      );

      final shiftedAppOrigin = validRawDogfoodPerformanceReceiptForTest();
      final shiftedAppRun =
          ((_cells(shiftedAppOrigin).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      for (final acknowledgement
          in (shiftedAppRun['structuralSetupAcknowledgements']! as List)
              .cast<Map>()) {
        acknowledgement['appCommandSequence'] =
            (acknowledgement['appCommandSequence']! as int) + 1;
      }
      expect(
        await replayBlockers(shiftedAppOrigin),
        contains('structural phases do not have contiguous app commands'),
      );

      final foreignNestedFrame = validRawDogfoodPerformanceReceiptForTest();
      final foreignNestedRun =
          ((_cells(foreignNestedFrame).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final foreignBurst = (foreignNestedRun['structuralBurst']! as Map)
          .cast<String, Object?>();
      ((foreignBurst['frames']! as List).first as Map)['sessionOrdinal'] = 1;
      expect(
        await replayBlockers(foreignNestedFrame),
        contains('has a stale or raw paint'),
      );

      final hiddenActiveRow = validRawDogfoodPerformanceReceiptForTest();
      final hiddenActiveRun =
          ((_cells(hiddenActiveRow).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final hiddenActivePaint = (hiddenActiveRun['paintObservations']! as List)
          .cast<Map>()
          .first;
      hiddenActivePaint
        ..['activeRowVisible'] = false
        ..['caretSourceUtf16'] = null
        ..['caretDisplayUtf16'] = null;
      expect(
        await replayBlockers(hiddenActiveRow),
        contains('has a stale or raw paint'),
      );

      final forgedVisible = validRawDogfoodPerformanceReceiptForTest();
      final forgedVisibleRun =
          ((_cells(forgedVisible).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final forgedPaint = (forgedVisibleRun['paintObservations']! as List)
          .cast<Map>()
          .first;
      forgedPaint
        ..['visibleSourceSha256'] = _hash('f')
        ..['expectedVisibleSourceSha256'] = _hash('f');
      expect(
        await replayBlockers(forgedVisible),
        contains('has a stale or raw paint'),
      );

      final delayedSuccessor = validRawDogfoodPerformanceReceiptForTest();
      final delayedRun =
          ((_cells(delayedSuccessor).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final delayedInputs = (delayedRun['inputObservations']! as List)
          .cast<Map>();
      final firstAccepted = delayedInputs.first['acceptedMicros']! as int;
      delayedInputs[1]['acceptedMicros'] = firstAccepted + 30001;
      expect(
        await replayBlockers(delayedSuccessor),
        contains('successor was not immediate'),
      );

      final forgedStructuralJoin = validRawDogfoodPerformanceReceiptForTest();
      final forgedStructuralRun =
          ((_cells(forgedStructuralJoin).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final forgedStructuralBurst =
          (forgedStructuralRun['structuralBurst']! as Map)
              .cast<String, Object?>();
      final forgedStructuralPaint =
          (forgedStructuralBurst['paintObservations']! as List)
              .cast<Map>()
              .firstWhere((paint) => paint['sourceGeneration'] == 2);
      forgedStructuralPaint['frameOrdinal'] =
          (forgedStructuralPaint['frameOrdinal']! as int) + 1;
      expect(
        await replayBlockers(forgedStructuralJoin),
        contains('has a stale or raw paint'),
      );

      final delayedStructuralPaint = validRawDogfoodPerformanceReceiptForTest();
      final delayedStructuralRun =
          ((_cells(delayedStructuralPaint).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final delayedBurst = (delayedStructuralRun['structuralBurst']! as Map)
          .cast<String, Object?>();
      final delayedPaint = (delayedBurst['paintObservations']! as List)
          .cast<Map>()
          .firstWhere((paint) => paint['sourceGeneration'] == 2);
      delayedPaint['timestampMicros'] =
          ((delayedBurst['inputObservations']! as List)
                  .cast<Map>()[1]['acceptedMicros']!
              as int) +
          16001;
      expect(
        await replayBlockers(delayedStructuralPaint),
        contains('successor exceeded the visibility budget'),
      );

      final delayedTerminalCertification =
          validRawDogfoodPerformanceReceiptForTest();
      final delayedTerminalRun =
          ((_cells(delayedTerminalCertification).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      final delayedTerminalBurst =
          (delayedTerminalRun['structuralBurst']! as Map)
              .cast<String, Object?>();
      final terminalInput = (delayedTerminalBurst['inputObservations']! as List)
          .cast<Map>()
          .last;
      final terminalPaint = (delayedTerminalBurst['paintObservations']! as List)
          .cast<Map>()
          .last;
      terminalPaint['timestampMicros'] =
          (terminalInput['acceptedMicros']! as int) + 500000;
      expect(
        await replayBlockers(delayedTerminalCertification),
        contains('terminal certification exceeded 500 ms'),
      );

      final missingWarmupSuccessor = validRawDogfoodPerformanceReceiptForTest();
      final missingWarmupRun =
          ((_cells(missingWarmupSuccessor).firstWhere(
                            (value) =>
                                value['id'] == 'product-tour-structural-burst',
                          )['runs']!
                          as List)
                      .first
                  as Map)
              .cast<String, Object?>();
      (missingWarmupRun['paintObservations']! as List).removeWhere(
        (value) => (value as Map)['sourceGeneration'] == 2,
      );
      expect(
        await replayBlockers(missingWarmupSuccessor),
        contains('pair 0 successor never painted'),
      );

      final missingPaint = validRawDogfoodPerformanceReceiptForTest();
      final structural = _cells(
        missingPaint,
      ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
      final run = ((structural['runs']! as List).first as Map)
          .cast<String, Object?>();
      final sample = ((run['samples']! as List).first as Map)
          .cast<String, Object?>();
      final finalGeneration = sample['sourceGeneration']! as int;
      (run['paintObservations']! as List).removeWhere(
        (value) => (value as Map)['sourceGeneration'] == finalGeneration,
      );
      final missingPaintSealed = await sealDogfoodPerformanceReceipt(
        missingPaint,
        verifyArtifactFiles: false,
      );
      final missingResult = await verifyDogfoodPerformanceReceipt(
        missingPaintSealed,
        verifyArtifactFiles: false,
      );
      expect(
        missingResult.blockers.join('\n'),
        contains('sample[0] has no final-generation paint'),
      );

      final missingControlPaint = validRawDogfoodPerformanceReceiptForTest();
      final missingControlCell = _cells(
        missingControlPaint,
      ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
      final missingControlRun =
          ((missingControlCell['runs']! as List).first as Map)
              .cast<String, Object?>();
      final control = (missingControlRun['structuralPerEditControl']! as Map)
          .cast<String, Object?>();
      (control['paintObservations']! as List).removeWhere(
        (value) => (value as Map)['sourceGeneration'] == 1,
      );
      final missingControlSealed = await sealDogfoodPerformanceReceipt(
        missingControlPaint,
        verifyArtifactFiles: false,
      );
      final missingControlResult = await verifyDogfoodPerformanceReceipt(
        missingControlSealed,
        verifyArtifactFiles: false,
      );
      expect(
        missingControlResult.blockers.join('\n'),
        contains(
          'structuralPerEditControl paint observations do not exactly cover',
        ),
      );

      final wrongTransition = validRawDogfoodPerformanceReceiptForTest();
      final wrongTransitionCell = _cells(
        wrongTransition,
      ).firstWhere((value) => value['id'] == 'ordinary-1m-structural-burst');
      final wrongTransitionRun =
          ((wrongTransitionCell['runs']! as List).first as Map)
              .cast<String, Object?>();
      final wrongBurst = (wrongTransitionRun['structuralBurst']! as Map)
          .cast<String, Object?>();
      ((wrongBurst['inputObservations']! as List).first
          as Map)['sourceSha256'] = _hash(
        'f',
      );
      final wrongTransitionSealed = await sealDogfoodPerformanceReceipt(
        wrongTransition,
        verifyArtifactFiles: false,
      );
      final wrongTransitionResult = await verifyDogfoodPerformanceReceipt(
        wrongTransitionSealed,
        verifyArtifactFiles: false,
      );
      expect(
        wrongTransitionResult.blockers.join('\n'),
        contains('does not match the exact parser-authored transition'),
      );

      final sealed = await sealDogfoodPerformanceReceipt(
        validRawDogfoodPerformanceReceiptForTest(),
        verifyArtifactFiles: false,
      );
      final missedFrame = _copy(sealed);
      final missedStructural = _cells(
        missedFrame,
      ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
      final missedRun = ((missedStructural['runs']! as List).first as Map)
          .cast<String, Object?>();
      final missedSample = ((missedRun['samples']! as List).first as Map)
          .cast<String, Object?>();
      final start = missedSample['startFrameOrdinal']! as int;
      ((missedRun['frames']! as List)[start] as Map)['missed'] = true;
      final missedResult = await verifyDogfoodPerformanceReceipt(
        missedFrame,
        verifyArtifactFiles: false,
      );
      expect(missedResult.blockers.join('\n'), contains('frame $start missed'));

      final hiddenBeforePaint = _copy(sealed);
      final hiddenCell = _cells(
        hiddenBeforePaint,
      ).firstWhere((cell) => cell['id'] == 'product-tour-typing');
      final hiddenRun = ((hiddenCell['runs']! as List).first as Map)
          .cast<String, Object?>();
      final hiddenSample = ((hiddenRun['samples']! as List)[1] as Map)
          .cast<String, Object?>();
      final hiddenStart = hiddenSample['startFrameOrdinal']! as int;
      final accepted = hiddenSample['acceptedMicros']! as int;
      final escaped = ((hiddenRun['frames']! as List)[hiddenStart - 1] as Map);
      escaped['vsyncMicros'] = accepted + 1;
      escaped['missed'] = true;
      final hiddenResult = await verifyDogfoodPerformanceReceipt(
        hiddenBeforePaint,
        verifyArtifactFiles: false,
      );
      expect(
        hiddenResult.blockers.join('\n'),
        contains('frame interval does not begin at acceptance'),
      );
    },
  );

  test('lifecycle replay keys restarted generations by session', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final lifecycle = _cells(
      sealed,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final run = (lifecycle['runs']! as List).single as Map;
    final samples = (run['samples']! as List).cast<Map>();
    expect(samples[0]['acceptedSourceGenerations'], [1, 2]);
    expect(samples[1]['acceptedSourceGenerations'], [1, 2]);
    expect(samples[0]['sessionOrdinal'], 0);
    expect(samples[1]['sessionOrdinal'], 1);

    final conflated = _copy(sealed);
    final conflatedLifecycle = _cells(
      conflated,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final conflatedRun = (conflatedLifecycle['runs']! as List).single as Map;
    for (final key in const [
      'samples',
      'inputObservations',
      'paintObservations',
      'engineObservations',
    ]) {
      for (final value in (conflatedRun[key]! as List).cast<Map>()) {
        if (value['sessionOrdinal'] == 1) value['sessionOrdinal'] = 0;
      }
    }
    final result = await verifyDogfoodPerformanceReceipt(
      conflated,
      verifyArtifactFiles: false,
    );
    expect(
      result.blockers.join('\n'),
      contains('declared source generations must be unique'),
    );

    final foreignFrame = _copy(sealed);
    final foreignLifecycle = _cells(
      foreignFrame,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final foreignRun = (foreignLifecycle['runs']! as List).single as Map;
    final foreignFrames = (foreignRun['frames']! as List).cast<Map>();
    final firstIdentity = foreignFrames.firstWhere(
      (frame) => frame['sessionOrdinal'] == 0,
    )['measurementSessionIdentity'];
    foreignFrames.firstWhere(
      (frame) => frame['sessionOrdinal'] == 1,
    )['measurementSessionIdentity'] = firstIdentity;
    final foreignResult = await verifyDogfoodPerformanceReceipt(
      foreignFrame,
      verifyArtifactFiles: false,
    );
    expect(
      foreignResult.blockers.join('\n'),
      contains('raw observations do not preserve app-authored sessions'),
    );

    final foreignPaintJoin = _copy(sealed);
    final foreignPaintLifecycle = _cells(
      foreignPaintJoin,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final foreignPaintRun =
        (foreignPaintLifecycle['runs']! as List).single as Map;
    final foreignPaint = (foreignPaintRun['paintObservations']! as List)
        .cast<Map>()
        .firstWhere((paint) => paint['sessionOrdinal'] == 0);
    final foreignPaintFrame = (foreignPaintRun['frames']! as List)
        .cast<Map>()
        .firstWhere((frame) => frame['sessionOrdinal'] == 1);
    foreignPaint['frameOrdinal'] = foreignPaintFrame['ordinal'];
    final foreignPaintResult = await verifyDogfoodPerformanceReceipt(
      foreignPaintJoin,
      verifyArtifactFiles: false,
    );
    expect(
      foreignPaintResult.blockers.join('\n'),
      contains('joined a foreign-session frame'),
    );
  });

  test('open replay rejects hidden work and a torn first paint', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final hiddenWork = _copy(sealed);
    final run =
        (_cells(hiddenWork).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final open = run['openObservation']! as Map;
    open['openToEditableMicros'] = 1;
    final hiddenResult = await verifyDogfoodPerformanceReceipt(
      hiddenWork,
      verifyArtifactFiles: false,
    );
    expect(
      hiddenResult.blockers.join('\n'),
      contains('timing does not replay'),
    );

    final torn = _copy(sealed);
    final tornRun =
        (_cells(torn).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final tornOpen = tornRun['openObservation']! as Map;
    tornOpen['expectedVisibleSourceSha256'] = _hash('d');
    final tornResult = await verifyDogfoodPerformanceReceipt(
      torn,
      verifyArtifactFiles: false,
    );
    expect(
      tornResult.blockers.join('\n'),
      contains('source identity does not match its frozen preset'),
    );

    final shifted = _copy(sealed);
    final shiftedRun =
        (_cells(shifted).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final shiftedOpen = shiftedRun['openObservation']! as Map;
    shiftedOpen['acceptedMicros'] =
        (shiftedOpen['acceptedMicros']! as int) + 1000;
    shiftedOpen['paintMicros'] = (shiftedOpen['paintMicros']! as int) + 1000;
    final shiftedResult = await verifyDogfoodPerformanceReceipt(
      shifted,
      verifyArtifactFiles: false,
    );
    expect(
      shiftedResult.blockers.join('\n'),
      contains('does not match its raw generation-zero acceptance'),
    );

    final relabelledOpen = _copy(sealed);
    final relabelledRun =
        (_cells(relabelledOpen).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    (relabelledRun['openObservation']! as Map)['measurementSessionIdentity'] =
        'relabelled-open';
    final relabelledResult = await verifyDogfoodPerformanceReceipt(
      relabelledOpen,
      verifyArtifactFiles: false,
    );
    expect(
      relabelledResult.blockers.join('\n'),
      contains('escaped its app-authored measurement session'),
    );

    final emptyVisible = _copy(sealed);
    final emptyRun =
        (_cells(emptyVisible).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final emptyOpen = emptyRun['openObservation']! as Map;
    final emptyPaint = (emptyRun['paintObservations']! as List)
        .cast<Map>()
        .firstWhere((paint) => paint['sourceGeneration'] == 0);
    for (final value in [emptyOpen, emptyPaint]) {
      value['visibleUtf16Length'] = 0;
      value['visibleSourceSha256'] = sha256.convert(const <int>[]).toString();
      value['expectedVisibleSourceSha256'] = sha256
          .convert(const <int>[])
          .toString();
    }
    final emptyResult = await verifyDogfoodPerformanceReceipt(
      emptyVisible,
      verifyArtifactFiles: false,
    );
    expect(
      emptyResult.blockers.join('\n'),
      contains('source identity does not match its frozen preset'),
    );

    final partialSurface = _copy(sealed);
    final partialRun =
        (_cells(partialSurface).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final partialOpen = partialRun['openObservation']! as Map;
    final partialPaint = (partialRun['paintObservations']! as List)
        .cast<Map>()
        .firstWhere((paint) => paint['sourceGeneration'] == 0);
    for (final value in [partialOpen, partialPaint]) {
      value['paintedSourceUtf16End'] = value['paintedSourceUtf16Start'];
    }
    final partialResult = await verifyDogfoodPerformanceReceipt(
      partialSurface,
      verifyArtifactFiles: false,
    );
    expect(
      partialResult.blockers.join('\n'),
      contains('source identity does not match its frozen preset'),
    );

    final omittedVisibleFragment = _copy(sealed);
    final omittedVisibleRun =
        (_cells(omittedVisibleFragment).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final omittedVisibleOpen = omittedVisibleRun['openObservation']! as Map;
    final omittedVisiblePaint =
        (omittedVisibleRun['paintObservations']! as List)
            .cast<Map>()
            .firstWhere((paint) => paint['sourceGeneration'] == 0);
    for (final value in [omittedVisibleOpen, omittedVisiblePaint]) {
      value['requiredVisibleFragmentCount'] =
          (value['paintedRowCount']! as int) + 1;
    }
    final omittedVisibleResult = await verifyDogfoodPerformanceReceipt(
      omittedVisibleFragment,
      verifyArtifactFiles: false,
    );
    expect(
      omittedVisibleResult.blockers.join('\n'),
      contains('source identity does not match its frozen preset'),
    );
  });

  test('fragment assembly is complete, ordered, and display-bound', () {
    final raw = validRawDogfoodPerformanceReceiptForTest();
    final display = (raw['display']! as Map).cast<String, Object?>();
    final fragments = <Map<String, Object?>>[];
    final binding = {
      'candidateCommit': _hash40('a'),
      'candidateTree': _hash40('b'),
      'bundleManifestSha256': _hash('c'),
      'mainExecutable': _identity('app'),
      'embeddedAbi': _identity('abi'),
      'measurementHost': {
        'hostname': 'benchmark-mac',
        'operatingSystem': 'macOS',
        'architecture': 'arm64',
        'logicalCores': 8,
        'physicalMemoryBytes': 16000000000,
      },
    };
    for (final cell in _cells(raw).reversed) {
      for (final run in ((cell['runs']! as List).reversed)) {
        final fixture = dogfoodFixtureIdentity(cell['id']! as String);
        fragments.add({
          'id': cell['id'],
          'sourceBytes': fixture['sourceBytes'],
          'warmupsPerRun': cell['warmupsPerRun'],
          'samplesPerRun': cell['samplesPerRun'],
          'runCount': cell['runCount'],
          'cadenceHz': cell['cadenceHz'],
          'binding': binding,
          'fixture': fixture,
          'display': display,
          'run': run,
        });
      }
    }
    final assembly = assembleDogfoodProfileFragments(fragments);
    expect(assembly.cells.map((cell) => cell['id']), requiredDogfoodCells.keys);
    for (final cell in assembly.cells) {
      final runs = (cell['runs']! as List).cast<Map>();
      expect(
        runs.map((run) => run['run']),
        List<int>.generate(runs.length, (index) => index),
      );
    }

    final mismatched = _copy(fragments.first);
    mismatched['display'] = {...display, 'refreshHz': 120};
    expect(
      () => assembleDogfoodProfileFragments([mismatched, ...fragments.skip(1)]),
      throwsStateError,
    );

    final wrongFixture = _copy(fragments.first);
    (wrongFixture['fixture']! as Map)['sourceBytes'] = 1;
    expect(
      () =>
          assembleDogfoodProfileFragments([wrongFixture, ...fragments.skip(1)]),
      throwsStateError,
    );

    final wrongCandidate = _copy(fragments.first);
    (wrongCandidate['binding']! as Map)['candidateCommit'] = _hash40('d');
    expect(
      () => assembleDogfoodProfileFragments([
        wrongCandidate,
        ...fragments.skip(1),
      ]),
      throwsStateError,
    );

    final wrongHost = _copy(fragments.first);
    ((wrongHost['binding']! as Map)['measurementHost']! as Map)['hostname'] =
        'other-mac';
    expect(
      () => assembleDogfoodProfileFragments([wrongHost, ...fragments.skip(1)]),
      throwsStateError,
    );
  });
}

Map<String, Object?> validRawDogfoodPerformanceReceiptForTest() {
  final cells = <Map<String, Object?>>[];
  for (final entry in requiredDogfoodCells.entries) {
    final denominator = entry.value;
    final fixture = dogfoodFixtureIdentity(entry.key);
    final sourceBytes = fixture['sourceBytes']! as int;
    final runs = <Map<String, Object?>>[];
    for (var run = 0; run < denominator.runs; run += 1) {
      final warmups = <Map<String, Object?>>[];
      final samples = <Map<String, Object?>>[];
      final frames = <Map<String, Object?>>[];
      final inputObservations = <Map<String, Object?>>[];
      final paintObservations = <Map<String, Object?>>[];
      final engineObservations = <Map<String, Object?>>[];
      var frameOrdinal = 0;
      var sourceGeneration = denominator.requiresInput ? 1 : 0;
      final structural = entry.key.endsWith('structural-burst');
      final latencySessionIdentity = structural
          ? '${entry.key}:$run:latency'
          : '${entry.key}:$run';
      final structuralExpected = structural
          ? _structuralExpectedGenerations(
              entry.key,
              denominator.warmups + denominator.samples,
            )
          : const <({String sourceSha256, int caret})>[];
      final lifecycle = entry.key.startsWith('lifecycle-');
      if (denominator.requiresOpen && denominator.requiresInput) {
        _addRawObservations(
          inputObservations: inputObservations,
          paintObservations: paintObservations,
          engineObservations: engineObservations,
          accepted: 400000,
          paintTimestamp: 400500,
          sourceGeneration: 0,
          frameOrdinal: frameOrdinal,
          sourceSha256: fixture['sourceSha256']! as String,
          caret: 0,
        );
        frames.add(_frame(frameOrdinal, 400000));
        frameOrdinal += 1;
      }
      for (var warmup = 0; warmup < denominator.warmups; warmup += 1) {
        final accepted = structural
            ? 500000 + warmup * 33333
            : 500000 + warmup * 20000;
        final finalGeneration = sourceGeneration + (structural ? 1 : 0);
        final finalFrame = frameOrdinal + (structural ? 1 : 0);
        warmups.add(
          _warmupFromSample(
            _sample(
              index: warmup,
              accepted: accepted + (structural ? 100 : 0),
              scheduleAccepted: accepted,
              acceptedSourceGenerations: [
                sourceGeneration,
                if (structural) finalGeneration,
              ],
              sourceGeneration: finalGeneration,
              frameOrdinal: finalFrame,
              startFrameOrdinal: structural ? finalFrame : null,
              sourceSha256: structural
                  ? structuralExpected[finalGeneration - 1].sourceSha256
                  : finalGeneration == 0
                  ? fixture['sourceSha256']! as String
                  : null,
              caret: structural
                  ? structuralExpected[finalGeneration - 1].caret
                  : null,
              paintDelayMicros: structural ? 400 : 500,
            ),
          ),
        );
        if (structural) {
          _addRawObservations(
            inputObservations: inputObservations,
            paintObservations: paintObservations,
            engineObservations: engineObservations,
            accepted: accepted,
            paintTimestamp: accepted + 50,
            sourceGeneration: sourceGeneration,
            frameOrdinal: frameOrdinal,
            semanticsCurrent: false,
            activeNeutralRowCount: 0,
            sourceSha256: structuralExpected[sourceGeneration - 1].sourceSha256,
            caret: structuralExpected[sourceGeneration - 1].caret,
          );
          frames.add(
            _frame(
              frameOrdinal,
              accepted,
              paintDelayMicros: 50,
              sessionOrdinal: 0,
            ),
          );
          frameOrdinal += 1;
          sourceGeneration += 1;
        }
        _addRawObservations(
          inputObservations: inputObservations,
          paintObservations: paintObservations,
          engineObservations: engineObservations,
          accepted: accepted + (structural ? 100 : 0),
          paintTimestamp: accepted + 500,
          sourceGeneration: sourceGeneration,
          frameOrdinal: frameOrdinal,
          sourceSha256: structural
              ? structuralExpected[sourceGeneration - 1].sourceSha256
              : sourceGeneration == 0
              ? fixture['sourceSha256']! as String
              : null,
          caret: structural
              ? structuralExpected[sourceGeneration - 1].caret
              : null,
        );
        frames.add(_frame(frameOrdinal, accepted));
        frameOrdinal += 1;
        sourceGeneration += 1;
      }
      for (var sample = 0; sample < denominator.samples; sample += 1) {
        final sessionOrdinal = lifecycle && denominator.samples > 1
            ? sample
            : 0;
        if (lifecycle) sourceGeneration = 1;
        final scheduled = denominator.cadenceHz == 0
            ? null
            : (sample * 1000000 / denominator.cadenceHz).round();
        final accepted = structural
            ? 500000 + (denominator.warmups + sample) * 33333
            : 1000000 + (scheduled ?? sample * 20000);
        final multiGeneration = structural || lifecycle;
        final finalGeneration = sourceGeneration + (multiGeneration ? 1 : 0);
        final finalFrame = frameOrdinal + (multiGeneration ? 1 : 0);
        samples.add(
          _sample(
            index: sample,
            sessionOrdinal: sessionOrdinal,
            scheduled: scheduled,
            accepted: accepted + (structural ? 100 : 0),
            scheduleAccepted: accepted,
            acceptedSourceGenerations: [
              sourceGeneration,
              if (multiGeneration) finalGeneration,
            ],
            sourceGeneration: finalGeneration,
            frameOrdinal: finalFrame,
            startFrameOrdinal: structural ? finalFrame : null,
            requiresLiveStateZero: denominator.requiresLiveStateZero,
            sourceSha256: structural
                ? structuralExpected[finalGeneration - 1].sourceSha256
                : finalGeneration == 0
                ? fixture['sourceSha256']! as String
                : null,
            caret: structural
                ? structuralExpected[finalGeneration - 1].caret
                : null,
            paintDelayMicros: structural ? 400 : 500,
          ),
        );
        if (multiGeneration) {
          _addRawObservations(
            inputObservations: inputObservations,
            paintObservations: paintObservations,
            engineObservations: engineObservations,
            accepted: accepted,
            paintTimestamp: accepted + 50,
            sourceGeneration: sourceGeneration,
            sessionOrdinal: sessionOrdinal,
            frameOrdinal: frameOrdinal,
            semanticsCurrent: false,
            activeNeutralRowCount: structural ? 0 : 1,
            sourceSha256: structural
                ? structuralExpected[sourceGeneration - 1].sourceSha256
                : null,
            caret: structural
                ? structuralExpected[sourceGeneration - 1].caret
                : null,
          );
          frames.add(
            _frame(
              frameOrdinal,
              accepted,
              paintDelayMicros: 50,
              sessionOrdinal: sessionOrdinal,
            ),
          );
          frameOrdinal += 1;
          sourceGeneration += 1;
        }
        _addRawObservations(
          inputObservations: inputObservations,
          paintObservations: paintObservations,
          engineObservations: engineObservations,
          accepted: accepted + (structural ? 100 : 0),
          paintTimestamp: accepted + 500,
          sourceGeneration: sourceGeneration,
          sessionOrdinal: sessionOrdinal,
          frameOrdinal: frameOrdinal,
          sourceSha256: structural
              ? structuralExpected[sourceGeneration - 1].sourceSha256
              : sourceGeneration == 0
              ? fixture['sourceSha256']! as String
              : null,
          caret: structural
              ? structuralExpected[sourceGeneration - 1].caret
              : null,
        );
        frames.add(
          _frame(frameOrdinal, accepted, sessionOrdinal: sessionOrdinal),
        );
        frameOrdinal += 1;
        sourceGeneration += 1;
      }
      final runValue = <String, Object?>{
        'run': run,
        'processId':
            denominator.processRule == DogfoodProcessRule.oneSharedProcess
            ? 'shared'
            : '${entry.key}-$run',
        'freshProcess':
            denominator.processRule == DogfoodProcessRule.freshEveryRun,
        'openObservation': denominator.requiresOpen
            ? {
                'kind': entry.key == 'product-tour-cold-launch'
                    ? 'processLaunch'
                    : 'presetSelection',
                'measurementSessionIdentity': latencySessionIdentity,
                'acceptedMicros': denominator.requiresInput ? 400000 : 1000000,
                'paintMicros': denominator.requiresInput ? 400500 : 1000500,
                'paintMonotonicMicros': denominator.requiresInput
                    ? 400500
                    : 1000500,
                'paintEpochBeforeMicros': denominator.requiresInput
                    ? 400500
                    : 1000500,
                'paintEpochAfterMicros': denominator.requiresInput
                    ? 400500
                    : 1000500,
                'openToEditableMicros': 500,
                'sourceGeneration': 0,
                'sourceSha256': fixture['sourceSha256'],
                'frameOrdinal': 0,
                'visibleUtf16Start': 0,
                'visibleUtf16Length': 1,
                'completeVisibleSurface': true,
                'completeVisiblePlusOverscanSurface': true,
                'requiredVisibleFragmentCount': 1,
                'laidOutVisiblePlusOverscanFragmentCount': 1,
                'requiredVisibleFragments': const [
                  {
                    'ordinal': 0,
                    'fragmentStart': 0,
                    'fragmentEnd': 1,
                    'sourceUtf16Start': 0,
                    'sourceUtf16End': 1,
                  },
                ],
                'laidOutVisiblePlusOverscanFragments': const [
                  {
                    'ordinal': 0,
                    'fragmentStart': 0,
                    'fragmentEnd': 1,
                    'sourceUtf16Start': 0,
                    'sourceUtf16End': 1,
                  },
                ],
                'paintedFragments': const [
                  {
                    'ordinal': 0,
                    'fragmentStart': 0,
                    'fragmentEnd': 1,
                    'sourceUtf16Start': 0,
                    'sourceUtf16End': 1,
                  },
                ],
                'paintedRowCount': 1,
                'paintedSourceUtf16Start': 0,
                'paintedSourceUtf16End': 1,
                'visiblePlusOverscanUtf16Start': 0,
                'visiblePlusOverscanUtf16End': 1,
                'visiblePlusOverscanSourceSha256': _visibleHash('#'),
                'expectedVisiblePlusOverscanSourceSha256': _visibleHash('#'),
                'visibleSourceSha256': _visibleHash('#'),
                'expectedVisibleSourceSha256': _visibleHash('#'),
                'canonicalSelectionBaseUtf16': 0,
                'canonicalSelectionExtentUtf16': 0,
                'expectedSelectionBaseUtf16': 0,
                'expectedSelectionExtentUtf16': 0,
                'caretSourceUtf16': 0,
                'caretDisplayUtf16': 1,
                'semanticsCurrent': true,
                'activeNeutralRowCount': 0,
              }
            : null,
        'warmups': warmups,
        'samples': samples,
        'frames': frames,
        'inputObservations': inputObservations,
        'paintObservations': paintObservations,
        'engineObservations': engineObservations,
        'faulted': false,
        'resyncCount': 0,
        'memory': const [
          {'stage': 'baseline', 'timestampMicros': 1, 'rssBytes': 100000000},
          {'stage': 'peak', 'timestampMicros': 2, 'rssBytes': 110000000},
          {'stage': 'close', 'timestampMicros': 3, 'rssBytes': 105000000},
          {'stage': 'postClose', 'timestampMicros': 4, 'rssBytes': 102000000},
        ],
      };
      _bindRawSessionIdentity(runValue, latencySessionIdentity);
      if (structural) {
        runValue['structuralPhase'] = 'latency';
        runValue['structuralSessionIdentity'] = latencySessionIdentity;
        runValue['structuralCommandTranscript'] = _structuralTranscript(
          'latency',
          denominator.warmups + denominator.samples,
        );
        runValue['structuralActuatorSequenceStart'] = 3;
        runValue['structuralActuatorSequenceEnd'] =
            (runValue['structuralActuatorSequenceStart']! as int) +
            (runValue['structuralCommandTranscript']! as List).length;
        var structuralAppSequence = _bindStructuralAcknowledgements(
          runValue,
          latencySessionIdentity,
        );
        runValue['structuralEvidenceVersion'] = 1;
        final latencySequenceEnd =
            runValue['structuralActuatorSequenceEnd']! as int;
        final burstIdentity = '${entry.key}:$run:burst';
        final burst = _copy(runValue)
          ..['structuralPhase'] = 'burst'
          ..['structuralSessionIdentity'] = burstIdentity
          ..['structuralCommandTranscript'] = _structuralTranscript(
            'burst',
            denominator.warmups + denominator.samples,
          )
          ..['warmups'] = <Object?>[];
        burst['structuralActuatorSequenceStart'] = latencySequenceEnd + 2;
        burst['structuralActuatorSequenceEnd'] =
            (burst['structuralActuatorSequenceStart']! as int) +
            (burst['structuralCommandTranscript']! as List).length;
        structuralAppSequence = _bindStructuralAcknowledgements(
          burst,
          burstIdentity,
          previousAppSequence: structuralAppSequence,
        );
        _bindRawSessionIdentity(burst, burstIdentity);
        final finalSample =
            _copy(
                ((runValue['samples']! as List).last as Map)
                    .cast<String, Object?>(),
              )
              ..['index'] = 0
              ..['scheduleAcceptedMicros'] =
                  ((runValue['inputObservations']! as List).first
                      as Map)['acceptedMicros']
              ..['acceptedSourceGenerations'] = [
                for (
                  var generation = 1;
                  generation <= (denominator.warmups + denominator.samples) * 2;
                  generation += 1
                )
                  generation,
              ];
        burst['samples'] = [finalSample];
        runValue['structuralBurst'] = burst;
        if (run == 0) {
          final controlIdentity = '${entry.key}:$run:control';
          final control = _copy(runValue)
            ..['structuralPhase'] = 'perEditControl'
            ..['structuralSessionIdentity'] = controlIdentity
            ..['structuralCommandTranscript'] = _structuralTranscript(
              'perEditControl',
              denominator.warmups + denominator.samples,
            );
          control['structuralActuatorSequenceStart'] =
              (burst['structuralActuatorSequenceEnd']! as int) + 2;
          control['structuralActuatorSequenceEnd'] =
              (control['structuralActuatorSequenceStart']! as int) +
              (control['structuralCommandTranscript']! as List).length;
          structuralAppSequence = _bindStructuralAcknowledgements(
            control,
            controlIdentity,
            previousAppSequence: structuralAppSequence,
          );
          _bindRawSessionIdentity(control, controlIdentity);
          control
            ..remove('structuralEvidenceVersion')
            ..remove('structuralBurst');
          runValue['structuralPerEditControl'] = control;
        }
      }
      runs.add(runValue);
    }
    cells.add({
      'id': entry.key,
      'sourceBytes': sourceBytes,
      'warmupsPerRun': denominator.warmups,
      'samplesPerRun': denominator.samples,
      'runCount': denominator.runs,
      'cadenceHz': denominator.cadenceHz,
      'fixture': fixture,
      'runs': runs,
    });
  }
  return {
    'schema': 'dogfood_performance_v1',
    'schemaVersion': 1,
    'candidate': {
      'commit': List.filled(40, 'a').join(),
      'tree': List.filled(40, 'b').join(),
      'clean': true,
    },
    'configuration': {
      'ledger': _identity('ledger'),
      'streamedOpeningEnabled': false,
      'enabledPresetIds': const [
        'productTour',
        'prose1MiB',
        'prose5MiB',
        'prose10MiB',
        'giantLine5MiB',
        'denseBlocks1MiB',
      ],
    },
    'artifacts': {
      'appBundleManifest': _identity('manifest'),
      'mainExecutable': _identity('app'),
      'embeddedAbi': _identity('abi'),
      'profileHarness': _identity('harness'),
      'profileFragments': [_identity('fragment')],
    },
    'host': {
      'hostname': 'benchmark-mac',
      'operatingSystem': 'macOS',
      'architecture': 'arm64',
      'cpu': 'Apple',
      'logicalCores': 8,
      'physicalMemoryBytes': 16000000000,
      'flutterVersion': 'test',
      'dartVersion': 'test',
      'rustcVersion': 'test',
      'cargoVersion': 'test',
      'xcodeVersion': 'test',
    },
    'display': {
      'refreshHz': 60,
      'framePeriodMicros': 1000000 / 60,
      'widthLogical': 1569,
      'heightLogical': 906,
      'devicePixelRatio': 2,
    },
    'cells': cells,
  };
}

final _structuralExpectedCache =
    <String, List<({String sourceSha256, int caret})>>{};

List<({String sourceSha256, int caret})> _structuralExpectedGenerations(
  String cellId,
  int pairCount,
) => _structuralExpectedCache.putIfAbsent('$cellId/$pairCount', () {
  final preset = cellId.startsWith('product-tour')
      ? DogfoodDocumentPreset.productTour
      : DogfoodDocumentPreset.prose1MiB;
  var source = buildDogfoodDocument(preset);
  final marker = cellId.startsWith('product-tour')
      ? 'locally.'
      : 'parser catches up.';
  var caret = source.indexOf(marker) + marker.length;
  final result = <({String sourceSha256, int caret})>[];
  for (var pair = 0; pair < pairCount; pair += 1) {
    source = source.replaceRange(caret, caret, '\n\n');
    caret += 2;
    result.add((
      sourceSha256: sha256.convert(utf8.encode(source)).toString(),
      caret: caret,
    ));
    source = source.replaceRange(caret, caret, 'x');
    caret += 1;
    result.add((
      sourceSha256: sha256.convert(utf8.encode(source)).toString(),
      caret: caret,
    ));
  }
  return result;
});

Map<String, Object?> _sample({
  required int index,
  int sessionOrdinal = 0,
  required int accepted,
  int? scheduleAccepted,
  required List<int> acceptedSourceGenerations,
  required int sourceGeneration,
  required int frameOrdinal,
  int? startFrameOrdinal,
  int? scheduled,
  bool requiresLiveStateZero = false,
  String? sourceSha256,
  int? caret,
  int paintDelayMicros = 500,
}) => {
  'index': index,
  'sessionOrdinal': sessionOrdinal,
  'scheduledMicros': scheduled,
  'acceptedMicros': accepted,
  'scheduleAcceptedMicros': scheduleAccepted ?? accepted,
  'sourcePaintMicros': accepted + paintDelayMicros,
  'caretPaintMicros': accepted + paintDelayMicros,
  'selectionPaintMicros': accepted + paintDelayMicros,
  'acceptedSourceGenerations': acceptedSourceGenerations,
  'sourceGeneration': sourceGeneration,
  'paintedSourceGeneration': sourceGeneration,
  'sourceSha256': sourceSha256 ?? _hash('a'),
  'visibleSourceSha256': _visibleHash(sourceGeneration == 0 ? '#' : 'x'),
  'canonicalSelectionBaseUtf16': caret ?? sourceGeneration,
  'canonicalSelectionExtentUtf16': caret ?? sourceGeneration,
  'paintedCaretSourceUtf16': caret ?? sourceGeneration,
  'startFrameOrdinal':
      startFrameOrdinal ??
      frameOrdinal - (acceptedSourceGenerations.length - 1),
  'endFrameOrdinal': frameOrdinal,
  'provingFrameOrdinal': frameOrdinal,
  'engineMicros': 100,
  'visibleCertificationMicros': paintDelayMicros,
  'rawProjectionFrames': 0,
  'sourceIdentityMatched': true,
  'caretIdentityMatched': true,
  'selectionIdentityMatched': true,
  'faulted': false,
  'resyncCount': 0,
  if (requiresLiveStateZero)
    'globalLiveState': {
      'sessions': 0,
      'transactions': 0,
      'continuations': 0,
      'anchors': 0,
      'historyTokens': 0,
    },
};

Map<String, Object?> _warmupFromSample(Map<String, Object?> sample) => {
  for (final name in const [
    'index',
    'sessionOrdinal',
    'acceptedMicros',
    'scheduleAcceptedMicros',
    'acceptedSourceGenerations',
    'sourceGeneration',
    'sourceSha256',
    'canonicalSelectionBaseUtf16',
    'canonicalSelectionExtentUtf16',
    'engineMicros',
  ])
    name: sample[name],
};

Map<String, Object?> _frame(
  int ordinal,
  int accepted, {
  int paintDelayMicros = 500,
  int sessionOrdinal = 0,
}) {
  final vsync = accepted + paintDelayMicros;
  return {
    'ordinal': ordinal,
    'sessionOrdinal': sessionOrdinal,
    'vsyncMicros': vsync,
    'monotonicVsyncMicros': vsync,
    'buildStartMonotonicMicros': vsync,
    'buildFinishMonotonicMicros': vsync + 1000,
    'clockAnchorEpochBeforeMicros': 0,
    'clockAnchorEpochAfterMicros': 0,
    'clockAnchorMonotonicMicros': 0,
    'buildMicros': 1000,
    'rasterMicros': 1000,
    'editorSyncMicros': 100,
    'editorAttributed': true,
    'missed': false,
  };
}

void _addRawObservations({
  required List<Map<String, Object?>> inputObservations,
  required List<Map<String, Object?>> paintObservations,
  required List<Map<String, Object?>> engineObservations,
  required int accepted,
  required int paintTimestamp,
  required int sourceGeneration,
  int sessionOrdinal = 0,
  required int frameOrdinal,
  bool semanticsCurrent = true,
  int activeNeutralRowCount = 0,
  String? sourceSha256,
  int? caret,
}) {
  final expectedCaret = caret ?? sourceGeneration;
  final visibleStart = expectedCaret == 0 ? 0 : expectedCaret - 1;
  final visibleText = sourceGeneration == 0
      ? '#'
      : sourceSha256 != null && sourceGeneration.isOdd
      ? '\n'
      : 'x';
  final visibleHash = _visibleHash(visibleText);
  inputObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'sourceGeneration': sourceGeneration,
    'acceptedMicros': accepted,
    'editorSyncMicros': 100,
    'sourceSha256': sourceSha256 ?? _hash('a'),
    'canonicalSelectionBaseUtf16': expectedCaret,
    'canonicalSelectionExtentUtf16': expectedCaret,
  });
  paintObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'timestampMicros': paintTimestamp,
    'paintMonotonicMicros': paintTimestamp,
    'paintEpochBeforeMicros': paintTimestamp,
    'paintEpochAfterMicros': paintTimestamp,
    'frameStampMicros': paintTimestamp + 16667,
    'frameOrdinal': frameOrdinal,
    'sourceGeneration': sourceGeneration,
    'visibleUtf16Start': visibleStart,
    'visibleUtf16Length': 1,
    'completeVisibleSurface': true,
    'completeVisiblePlusOverscanSurface': true,
    'requiredVisibleFragmentCount': 1,
    'laidOutVisiblePlusOverscanFragmentCount': 1,
    'requiredVisibleFragments': [
      {
        'ordinal': 0,
        'fragmentStart': 0,
        'fragmentEnd': 1,
        'sourceUtf16Start': visibleStart,
        'sourceUtf16End': visibleStart + 1,
      },
    ],
    'laidOutVisiblePlusOverscanFragments': [
      {
        'ordinal': 0,
        'fragmentStart': 0,
        'fragmentEnd': 1,
        'sourceUtf16Start': visibleStart,
        'sourceUtf16End': visibleStart + 1,
      },
    ],
    'paintedFragments': [
      {
        'ordinal': 0,
        'fragmentStart': 0,
        'fragmentEnd': 1,
        'sourceUtf16Start': visibleStart,
        'sourceUtf16End': visibleStart + 1,
      },
    ],
    'paintedRowCount': 1,
    'paintedSourceUtf16Start': visibleStart,
    'paintedSourceUtf16End': visibleStart + 1,
    'visiblePlusOverscanUtf16Start': visibleStart,
    'visiblePlusOverscanUtf16End': visibleStart + 1,
    'visiblePlusOverscanSourceSha256': visibleHash,
    'expectedVisiblePlusOverscanSourceSha256': visibleHash,
    'visibleSourceSha256': visibleHash,
    'expectedVisibleSourceSha256': visibleHash,
    'canonicalSelectionBaseUtf16': expectedCaret,
    'canonicalSelectionExtentUtf16': expectedCaret,
    'expectedSelectionBaseUtf16': expectedCaret,
    'expectedSelectionExtentUtf16': expectedCaret,
    'caretSourceUtf16': expectedCaret,
    'caretDisplayUtf16': 1,
    'semanticsCurrent': semanticsCurrent,
    'activeNeutralRowCount': activeNeutralRowCount,
    'activeRowVisible': true,
  });
  engineObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'sourceGeneration': sourceGeneration,
    'nativeFfiMicros': 100,
  });
}

void _bindRawSessionIdentity(Map<String, Object?> run, String sessionIdentity) {
  final sessionOrdinals = <int>{
    for (final key in const [
      'frames',
      'inputObservations',
      'paintObservations',
      'engineObservations',
    ])
      for (final observation in (run[key]! as List).cast<Map>())
        observation['sessionOrdinal']! as int,
  };
  String identityFor(int sessionOrdinal) => sessionOrdinals.length == 1
      ? sessionIdentity
      : '$sessionIdentity:session-$sessionOrdinal';
  for (final key in const [
    'frames',
    'inputObservations',
    'paintObservations',
    'engineObservations',
  ]) {
    for (final observation in (run[key]! as List).cast<Map>()) {
      observation['measurementSessionIdentity'] = identityFor(
        observation['sessionOrdinal']! as int,
      );
    }
  }
}

int _bindStructuralAcknowledgements(
  Map<String, Object?> phase,
  String sessionIdentity, {
  int previousAppSequence = 1,
}) {
  final start = phase['structuralActuatorSequenceStart']! as int;
  final transcript = (phase['structuralCommandTranscript']! as List)
      .cast<String>();
  var appSequence = previousAppSequence;
  Map<String, Object?> acknowledgement(int sequence, String operation) => {
    'actuatorSequence': sequence,
    'operation': operation,
    'appCommandSequence': ++appSequence,
    'canaryId': sessionIdentity,
  };
  final resetAcknowledgement = acknowledgement(start - 1, 'reset');
  // Activation first asks the app for source geometry, then returns the
  // acknowledgement from its selection-settle request.
  appSequence += 1;
  final activationAcknowledgement = acknowledgement(start, 'activateAtUtf16');
  phase['structuralSetupAcknowledgements'] = [
    resetAcknowledgement,
    activationAcknowledgement,
  ];
  final appAcknowledgements = <Map<String, Object?>>[];
  for (var index = 0; index < transcript.length; index += 1) {
    final operation = transcript[index].split(':').first;
    if (operation == 'typeStructuralBursts' ||
        operation == 'pressKey' ||
        operation == 'typeText') {
      appSequence += 1;
    }
    if (operation == 'settle' || operation == 'closeSession') {
      appAcknowledgements.add(acknowledgement(start + index + 1, operation));
    }
  }
  phase['structuralAppAcknowledgements'] = appAcknowledgements;
  return appSequence;
}

Map<String, Object> _identity(String path) => {
  'path': path,
  'bytes': 1,
  'sha256': _hash('c'),
};

String _hash(String character) => List.filled(64, character).join();

String _hash40(String character) => List.filled(40, character).join();

Map<String, Object?> _copy(Map<String, Object?> value) =>
    jsonDecode(jsonEncode(value)) as Map<String, Object?>;

List<String> _structuralTranscript(String phase, int pairCount) {
  final result = <String>[];
  if (phase == 'latency') {
    for (var index = 0; index < pairCount; index += 1) {
      result
        ..add('typeStructuralBursts:1:0')
        ..add('settle');
    }
  } else if (phase == 'burst') {
    result.add('typeStructuralBursts:$pairCount:33333');
  } else {
    for (var index = 0; index < pairCount; index += 1) {
      result
        ..add('pressKey:enter')
        ..add('settle')
        ..add('typeText:x:0')
        ..add('settle');
    }
  }
  return result
    ..add('settle')
    ..add('closeSession');
}

void _coalesceFirstStructuralReturn(
  Map<String, Object?> phase, {
  required bool attributeSync,
}) {
  final inputs = (phase['inputObservations']! as List).cast<Map>();
  final successorAccepted =
      inputs.firstWhere(
            (input) => input['sourceGeneration'] == 2,
          )['acceptedMicros']!
          as int;
  final frames = (phase['frames']! as List).cast<Map>();
  final firstFrame = frames[0];
  final secondFrame = frames[1];
  final firstVsync = successorAccepted + 10;
  firstFrame
    ..['vsyncMicros'] = firstVsync
    ..['monotonicVsyncMicros'] = firstVsync
    ..['buildStartMonotonicMicros'] = firstVsync
    ..['buildFinishMonotonicMicros'] = firstVsync + 1000
    ..['editorSyncMicros'] = attributeSync ? 200 : 100;
  secondFrame['editorSyncMicros'] = 0;
  final paints = (phase['paintObservations']! as List).cast<Map>();
  paints.removeWhere((paint) => paint['sourceGeneration'] == 1);
  final successorPaint = paints.firstWhere(
    (paint) => paint['sourceGeneration'] == 2,
  );
  successorPaint
    ..['timestampMicros'] = firstVsync
    ..['paintMonotonicMicros'] = firstVsync
    ..['paintEpochBeforeMicros'] = firstVsync
    ..['paintEpochAfterMicros'] = firstVsync
    ..['frameStampMicros'] = firstVsync + 16667
    ..['frameOrdinal'] = 0;
}

List<Map<String, Object?>> _cells(Map<String, Object?> receipt) =>
    (receipt['cells']! as List).cast<Map<String, Object?>>();

Map<String, Object?> _firstSample(Map<String, Object?> receipt) {
  final cell = _cells(
    receipt,
  ).firstWhere((candidate) => candidate['id'] == 'product-tour-typing');
  final run = ((cell['runs']! as List).first as Map).cast<String, Object?>();
  return (run['samples']! as List).first as Map<String, Object?>;
}

Map<String, Object?> _firstRun(Map<String, Object?> receipt) =>
    ((_cells(receipt).first['runs']! as List).first as Map)
        .cast<String, Object?>();
