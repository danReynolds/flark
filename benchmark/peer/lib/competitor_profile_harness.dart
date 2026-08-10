import 'dart:async';
import 'dart:convert';
import 'dart:developer' as developer;
import 'dart:io';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_quill/flutter_quill.dart';

import 'profile_config.dart';
import 'profile_evidence.dart';
import 'profile_fixture.dart';

const _resultPrefix = 'FLARK_PEER_RESULT ';
const _typingAlphabet = 'abcdefghijklmnopqrstuvwxyz0123456789';
const _inputTimeout = Duration(seconds: 60);

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final entryEpochMicros = DateTime.now().microsecondsSinceEpoch;
  final entryTraceMicros = developer.Timeline.now;
  Map<String, Object?>? result;
  try {
    final config = loadProfileConfig();
    final native = _NativeHarness();
    final system = await native.systemInfo();
    await native.activateWindow();

    final fixtureStartTraceMicros = developer.Timeline.now;
    final fixture = generateOrdinaryProseExact(config.targetBytes);
    final fixtureReadyTraceMicros = developer.Timeline.now;
    final fixtureSha256 = sha256Text(fixture);
    final documentLoadStartTraceMicros = developer.Timeline.now;
    final document = Document()..insert(0, fixture);
    final controller = QuillController(
      document: document,
      selection: const TextSelection.collapsed(offset: 0),
    );
    final focusNode = FocusNode(debugLabel: 'competitor-profile-editor');
    final frameProbe = _FrameProbe()..start();
    final editorReady = Completer<_EditorReady>();

    runApp(
      _ProfileEditorApp(
        controller: controller,
        focusNode: focusNode,
        onInteractiveFrameSubmitted: editorReady.complete,
      ),
    );

    final ready = await editorReady.future.timeout(_inputTimeout);
    final firstInteractiveFrame = await frameProbe
        .firstRasterFromBuildAfter(documentLoadStartTraceMicros)
        .timeout(
          _inputTimeout,
          onTimeout: () => throw TimeoutException(
            'No interactive FrameTiming began building after document load; '
            'cold-open raster correlation is unproven.',
            _inputTimeout,
          ),
        );
    final initialExport = controller.document.toPlainText();
    final initialFidelity = compareSource(
      expected: fixture,
      actual: initialExport,
    );
    final initialMemory = await native.processMemory();

    final runner = _ScenarioRunner(
      config: config,
      fixture: fixture,
      controller: controller,
      focusNode: focusNode,
      native: native,
      frameProbe: frameProbe,
    );
    final scenario = await runner.run();
    final finalExport = controller.document.toPlainText();
    final expectedFinalSource = runner.expectedFinalSource;
    final finalFidelity = compareSource(
      expected: expectedFinalSource,
      actual: finalExport,
    );
    final finalMemory = await native.processMemory();

    final exportArtifact = await _writeExport(
      path: config.exportPath,
      source: finalExport,
    );
    final completionEnvelope = evaluateProcessCompletionEnvelope(
      protocolConfiguration: config.completionEnvelopeConfigurationEligible,
      allAcceptedInputsHaveProvenFrames: true,
      finalExportWritten: exportArtifact['written'] == true,
      sourceFidelityClassified:
          _fidelityAllowsCompletion(initialFidelity) &&
          _fidelityAllowsCompletion(finalFidelity),
      inputBacklogDrained: runner.inputBacklog == 0,
    );
    final performanceClaim = localPerformanceClaimEligibility(scope: 'process');

    result = <String, Object?>{
      'schemaVersion': 1,
      'resultKind': 'flutter-peer-profile-process',
      'peer': 'flutter_quill',
      'peerPackageVersion': '11.5.1',
      'completionEnvelopeEligible': completionEnvelope.eligible,
      'completionEnvelopeBlockers': completionEnvelope.blockers,
      'performanceClaimEligible': performanceClaim.eligible,
      'performanceClaimBlockers': performanceClaim.blockers,
      'cohortPerformanceEligibility': const <String, Object?>{
        'assessed': false,
        'eligible': null,
        'reason': 'A local Quill process cannot assess cohort eligibility.',
      },
      // Backward-compatible alias. Its meaning is performance-claim
      // eligibility, never completion-envelope eligibility.
      'claimEligible': false,
      'claimBlockers': performanceClaim.blockers,
      'config': config.toJson(),
      'fixture': <String, Object?>{
        'generatorId': ordinaryProseGeneratorId,
        'shapeId': ordinaryProseShapeId,
        'targetBytes': config.targetBytes,
        'actualBytes': utf8.encode(fixture).length,
        'sha256': fixtureSha256,
        'encoding': 'UTF-8',
        'normalization': 'none',
        'lineEndings': 'recipe-owned-LF',
      },
      'coldOpen': <String, Object?>{
        'dartEntrypointEpochMicros': entryEpochMicros,
        'nativeProcessStartEpochMicros': system['processStartEpochMicros'],
        'processStartToInteractiveFrameTimingCallbackMicros': _durationBetween(
          system['processStartEpochMicros'],
          firstInteractiveFrame.callbackEpochMicros,
        ),
        'processStartToInteractiveRasterFinishMicros': _durationBetween(
          system['processStartEpochMicros'],
          firstInteractiveFrame.rasterFinishEpochMicros,
        ),
        'dartEntrypointToInteractiveFrameTimingCallbackMicros':
            firstInteractiveFrame.callbackEpochMicros - entryEpochMicros,
        'fixtureGenerationMicros':
            fixtureReadyTraceMicros - fixtureStartTraceMicros,
        'documentLoadStartToRasterFinishMicros': _clockDelta(
          documentLoadStartTraceMicros,
          firstInteractiveFrame.rasterFinishMicros,
        ),
        'dartEntrypointToRasterFinishMicros': _clockDelta(
          entryTraceMicros,
          firstInteractiveFrame.rasterFinishMicros,
        ),
        'interactiveVerification': <String, Object?>{
          'focusNodeHasFocus': ready.focused,
          'editorStateMounted': ready.editorStateMounted,
          'viewportLogicalWidth': ready.viewportWidth,
          'viewportLogicalHeight': ready.viewportHeight,
          'sourcePrefixMatchesFixture': initialExport.startsWith(
            fixture.substring(0, fixture.length.clamp(0, 80)),
          ),
        },
        'frame': firstInteractiveFrame.toJson(),
      },
      'initialFidelity': initialFidelity,
      'scenarioResult': scenario,
      if (runner.pasteStateContract != null)
        'pasteStateContract': runner.pasteStateContract,
      'finalFidelity': finalFidelity,
      'finalExportArtifact': exportArtifact,
      'memory': <String, Object?>{
        'beforeWorkload': initialMemory,
        'afterWorkload': finalMemory,
      },
      'system': system,
      'measurementSemantics': <String, Object?>{
        'inputPath':
            'native NSEvent keyDown/keyUp into the focused Flutter macOS view',
        'pastePath':
            'NSPasteboard.general plus AppKit Paste action; because '
            'FlutterTextInputPlugin does not advertise paste:, the runner '
            'falls back to insertText on that active platform '
            'NSTextInputClient; clipboard restored after acceptance',
        'acceptedInput':
            'first QuillController notification at the expected document length',
        'rasterCompletion':
            'first Flutter FrameTiming whose buildStart is strictly after '
            'accepted input and whose raster/callback ordering is valid',
        'frameCorrelationFailure':
            'typed process failure; no raster latency sample is retained',
        'acceptedToRasterFinishClock':
            'Timeline.now and FrameTiming rasterFinish, retained only when clocks align',
        'viewport': '600x600 logical pixels',
      },
      'completedUtc': DateTime.now().toUtc().toIso8601String(),
    };
    await _emitResult(config.outputPath, result);
    frameProbe.dispose();
    focusNode.dispose();
    controller.dispose();
    await stdout.flush();
    exit(0);
  } catch (error, stackTrace) {
    final failure = <String, Object?>{
      'schemaVersion': 1,
      'resultKind': 'flutter-peer-profile-process',
      'peer': 'flutter_quill',
      'completionEnvelopeEligible': false,
      'completionEnvelopeBlockers': const <String>[
        'The process failed before completing its evidence envelope.',
      ],
      'performanceClaimEligible': false,
      'performanceClaimBlockers': const <String>[
        'A failed process cannot support a performance claim.',
      ],
      'cohortPerformanceEligibility': const <String, Object?>{
        'assessed': false,
        'eligible': null,
        'reason': 'A local Quill process cannot assess cohort eligibility.',
      },
      'claimEligible': false,
      'status': 'runner-failure',
      'error': '$error',
      'stackTrace': '$stackTrace',
      'completedUtc': DateTime.now().toUtc().toIso8601String(),
    };
    await _emitResult(Platform.environment['COMPETITOR_OUTPUT_PATH'], failure);
    await stdout.flush();
    exit(2);
  }
}

final class _ProfileEditorApp extends StatelessWidget {
  const _ProfileEditorApp({
    required this.controller,
    required this.focusNode,
    required this.onInteractiveFrameSubmitted,
  });

  final QuillController controller;
  final FocusNode focusNode;
  final ValueChanged<_EditorReady> onInteractiveFrameSubmitted;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      localizationsDelegates: FlutterQuillLocalizations.localizationsDelegates,
      supportedLocales: FlutterQuillLocalizations.supportedLocales,
      home: _EditorViewport(
        controller: controller,
        focusNode: focusNode,
        onInteractiveFrameSubmitted: onInteractiveFrameSubmitted,
      ),
    );
  }
}

final class _EditorViewport extends StatefulWidget {
  const _EditorViewport({
    required this.controller,
    required this.focusNode,
    required this.onInteractiveFrameSubmitted,
  });

  final QuillController controller;
  final FocusNode focusNode;
  final ValueChanged<_EditorReady> onInteractiveFrameSubmitted;

  @override
  State<_EditorViewport> createState() => _EditorViewportState();
}

final class _EditorViewportState extends State<_EditorViewport> {
  final _editorKey = GlobalKey<QuillEditorState>();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      widget.focusNode.requestFocus();
      WidgetsBinding.instance.addPostFrameCallback((_) {
        final size = context.size ?? Size.zero;
        widget.onInteractiveFrameSubmitted(
          _EditorReady(
            traceMicros: developer.Timeline.now,
            focused: widget.focusNode.hasFocus,
            editorStateMounted: _editorKey.currentState?.mounted ?? false,
            viewportWidth: size.width,
            viewportHeight: size.height,
          ),
        );
      });
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SizedBox(
        width: 600,
        height: 600,
        child: QuillEditor.basic(
          key: _editorKey,
          controller: widget.controller,
          focusNode: widget.focusNode,
          config: const QuillEditorConfig(padding: EdgeInsets.all(12)),
        ),
      ),
    );
  }
}

final class _ScenarioRunner {
  _ScenarioRunner({
    required this.config,
    required this.fixture,
    required this.controller,
    required this.focusNode,
    required this.native,
    required this.frameProbe,
  }) : _lengthProbe = _DocumentLengthProbe(controller)..start();

  final ProfileConfig config;
  final String fixture;
  final QuillController controller;
  final FocusNode focusNode;
  final _NativeHarness native;
  final _FrameProbe frameProbe;
  final _DocumentLengthProbe _lengthProbe;
  final List<String> _typedCharacters = <String>[];
  final List<Map<String, Object?>> _pasteTransitions = <Map<String, Object?>>[];
  var _maxInputBacklog = 0;
  var _inputBacklog = 0;
  var _nextInputSequence = 0;

  int get inputBacklog => _inputBacklog;

  Map<String, Object?>? get pasteStateContract {
    if (config.scenario != CompetitorScenario.paste32Kib) return null;
    final payload = generateOrdinaryProseExact(pastePayloadBytes);
    final pasted = insertExactSource(
      source: fixture,
      payload: payload,
      offset: _locationOffset(config.location, fixture.length),
    );
    return <String, Object?>{
      'schemaVersion': 1,
      'mode': 'reset-after-each-paste',
      'pasteViaPlatformInput': true,
      'resetViaPlatformBackspace': true,
      'selectionForReset': 'programmatic-exact-pasted-source-range',
      'warmupTransitions': config.pasteWarmups,
      'measuredTransitions': config.pasteSamples,
      'baseState': canonicalStateDenominator(fixture),
      'singlePasteState': canonicalStateDenominator(pasted),
      'expectedFinalState': canonicalStateDenominator(fixture),
      'transitions': _pasteTransitions,
    };
  }

  String get expectedFinalSource {
    if (config.scenario != CompetitorScenario.sustainedTyping) return fixture;
    final offset = _locationOffset(config.location, fixture.length);
    final inserted = _typedCharacters.join();
    return '${fixture.substring(0, offset)}$inserted${fixture.substring(offset)}';
  }

  Future<Map<String, Object?>> run() async {
    await _setCaret(config.location);
    switch (config.scenario) {
      case CompetitorScenario.coldOpen:
        return <String, Object?>{
          'operation': config.scenario.wireName,
          'rawSamples': const <Object?>[],
          'note':
              'Cold-open timing is recorded in the top-level coldOpen object.',
        };
      case CompetitorScenario.sustainedTyping:
        return _runTyping();
      case CompetitorScenario.localInsertDelete:
        return _runLocalInsertDelete();
      case CompetitorScenario.paste32Kib:
        return _runPaste();
    }
  }

  Future<Map<String, Object?>> _runTyping() async {
    var expectedLength = controller.document.length;
    for (var index = 0; index < config.typingWarmups; index += 1) {
      final character = _typingAlphabet[index % _typingAlphabet.length];
      expectedLength += 1;
      await _measureInput(
        action: 'type-character',
        sampleIndex: index,
        expectedDocumentLength: expectedLength,
        nativeAction: () => native.typeCharacter(character),
        measured: false,
      );
      _typedCharacters.add(character);
    }

    final raw = <Future<Map<String, Object?>>>[];
    final cadenceStart = developer.Timeline.now;
    final cadenceMicros =
        Duration.microsecondsPerSecond ~/ config.typingCadenceHz;
    for (var index = 0; index < config.typingSamples; index += 1) {
      final scheduledMicros = cadenceStart + index * cadenceMicros;
      final delayMicros = scheduledMicros - developer.Timeline.now;
      if (delayMicros > 0) {
        await Future<void>.delayed(Duration(microseconds: delayMicros));
      }
      final character =
          _typingAlphabet[(config.typingWarmups + index) %
              _typingAlphabet.length];
      expectedLength += 1;
      _typedCharacters.add(character);
      raw.add(
        _measureInput(
          action: 'type-character',
          sampleIndex: index,
          expectedDocumentLength: expectedLength,
          nativeAction: () => native.typeCharacter(character),
          measured: true,
          scheduledTraceMicros: scheduledMicros,
          payload: character,
        ),
      );
    }
    final samples = await Future.wait(raw).timeout(_inputTimeout);
    return <String, Object?>{
      'operation': config.scenario.wireName,
      'cadenceHz': config.typingCadenceHz,
      'warmupEdits': config.typingWarmups,
      'sampleEdits': config.typingSamples,
      'maxInputBacklogUntilRaster': _maxInputBacklog,
      'rawSamples': samples,
      'distributions': _distributions(samples),
    };
  }

  Future<Map<String, Object?>> _runLocalInsertDelete() async {
    final baseLength = controller.document.length;
    for (var index = 0; index < config.localWarmupPairs; index += 1) {
      await _measureInput(
        action: 'insert-x',
        sampleIndex: index,
        expectedDocumentLength: baseLength + 1,
        nativeAction: () => native.typeCharacter('x'),
        measured: false,
      );
      await _measureInput(
        action: 'delete-x',
        sampleIndex: index,
        expectedDocumentLength: baseLength,
        nativeAction: native.backspace,
        measured: false,
      );
    }

    final samples = <Map<String, Object?>>[];
    for (var index = 0; index < config.localSamplePairs; index += 1) {
      samples.add(
        await _measureInput(
          action: 'insert-x',
          sampleIndex: index,
          expectedDocumentLength: baseLength + 1,
          nativeAction: () => native.typeCharacter('x'),
          measured: true,
        ),
      );
      samples.add(
        await _measureInput(
          action: 'delete-x',
          sampleIndex: index,
          expectedDocumentLength: baseLength,
          nativeAction: native.backspace,
          measured: true,
        ),
      );
    }
    return <String, Object?>{
      'operation': config.scenario.wireName,
      'location': config.location.name,
      'warmupPairs': config.localWarmupPairs,
      'samplePairs': config.localSamplePairs,
      'maxInputBacklogUntilRaster': _maxInputBacklog,
      'rawSamples': samples,
      'distributions': _distributions(samples),
    };
  }

  Future<Map<String, Object?>> _runPaste() async {
    final payload = generateOrdinaryProseExact(pastePayloadBytes);
    final baseLength = controller.document.length;
    final baseOffset = _locationOffset(config.location, fixture.length);
    final expectedPasted = insertExactSource(
      source: fixture,
      payload: payload,
      offset: baseOffset,
    );
    for (var index = 0; index < config.pasteWarmups; index += 1) {
      final preState = quillCanonicalStateProof(
        expectedCanonical: fixture,
        actualPeerSource: controller.document.toPlainText(),
      );
      late final Map<String, Object?> pasteSample;
      try {
        pasteSample = await _measureInput(
          action: 'paste-32kib',
          sampleIndex: index,
          expectedDocumentLength: baseLength + pastePayloadBytes,
          nativeAction: () => native.pasteText(payload),
          measured: false,
          evidenceRole: 'paste-workload',
          stateTransitionIndex: index,
        );
      } finally {
        await native.restoreClipboard();
      }
      final postState = quillCanonicalStateProof(
        expectedCanonical: expectedPasted,
        actualPeerSource: controller.document.toPlainText(),
      );
      final resetInput = await _removePaste(baseOffset, baseLength, index);
      final resetState = quillCanonicalStateProof(
        expectedCanonical: fixture,
        actualPeerSource: controller.document.toPlainText(),
      );
      _pasteTransitions.add(
        _pasteTransition(
          transitionIndex: index,
          measured: false,
          pasteInput: <String, Object?>{
            'evidenceSequence': pasteSample['inputSequence'],
            'evidence': pasteSample,
          },
          preState: preState,
          postState: postState,
          resetState: resetState,
          resetInput: resetInput,
        ),
      );
      pasteSample['stateTransitionIndex'] = index;
    }

    final samples = <Map<String, Object?>>[];
    for (var index = 0; index < config.pasteSamples; index += 1) {
      final transitionIndex = config.pasteWarmups + index;
      final preState = quillCanonicalStateProof(
        expectedCanonical: fixture,
        actualPeerSource: controller.document.toPlainText(),
      );
      late final Map<String, Object?> pasteSample;
      try {
        pasteSample = await _measureInput(
          action: 'paste-32kib',
          sampleIndex: index,
          expectedDocumentLength: baseLength + pastePayloadBytes,
          nativeAction: () => native.pasteText(payload),
          measured: true,
          payloadSha256: sha256Text(payload),
          evidenceRole: 'paste-workload',
          stateTransitionIndex: transitionIndex,
        );
      } finally {
        await native.restoreClipboard();
      }
      final postState = quillCanonicalStateProof(
        expectedCanonical: expectedPasted,
        actualPeerSource: controller.document.toPlainText(),
      );
      final resetInput = await _removePaste(
        baseOffset,
        baseLength,
        transitionIndex,
      );
      final resetState = quillCanonicalStateProof(
        expectedCanonical: fixture,
        actualPeerSource: controller.document.toPlainText(),
      );
      _pasteTransitions.add(
        _pasteTransition(
          transitionIndex: transitionIndex,
          measured: true,
          pasteInput: <String, Object?>{
            'evidenceSequence': pasteSample['inputSequence'],
            'evidence': pasteSample,
          },
          preState: preState,
          postState: postState,
          resetState: resetState,
          resetInput: resetInput,
        ),
      );
      pasteSample['stateTransitionIndex'] = transitionIndex;
      samples.add(pasteSample);
    }
    return <String, Object?>{
      'operation': config.scenario.wireName,
      'location': config.location.name,
      'payloadBytes': pastePayloadBytes,
      'payloadSha256': sha256Text(payload),
      'warmupEdits': config.pasteWarmups,
      'sampleEdits': config.pasteSamples,
      'pasteStateContract': pasteStateContract,
      'maxInputBacklogUntilRaster': _maxInputBacklog,
      'rawSamples': samples,
      'distributions': _distributions(samples),
    };
  }

  Future<Map<String, Object?>> _removePaste(
    int offset,
    int baseLength,
    int sampleIndex,
  ) async {
    controller.updateSelection(
      TextSelection(
        baseOffset: offset,
        extentOffset: offset + pastePayloadBytes,
      ),
      ChangeSource.local,
    );
    await Future<void>.delayed(const Duration(milliseconds: 1));
    final reset = await _measureInput(
      action: 'paste-cleanup-delete',
      sampleIndex: sampleIndex,
      expectedDocumentLength: baseLength,
      nativeAction: native.backspace,
      measured: false,
      evidenceRole: 'paste-reset',
      stateTransitionIndex: sampleIndex,
    );
    controller.updateSelection(
      TextSelection.collapsed(offset: offset),
      ChangeSource.local,
    );
    await WidgetsBinding.instance.endOfFrame;
    return <String, Object?>{
      'operation': 'platform-backspace-over-exact-pasted-range',
      'measured': false,
      'accepted': reset['acceptedTraceMicros'] is int,
      'rastered': reset['frame'] is Map,
      'platformInputDispatched': reset['nativeInput'] is Map,
      'selectionStart': offset,
      'selectionEnd': offset + pastePayloadBytes,
      'evidenceSequence': reset['inputSequence'],
      'evidence': reset,
    };
  }

  Future<Map<String, Object?>> _measureInput({
    required String action,
    required int sampleIndex,
    required int expectedDocumentLength,
    required Future<Map<String, Object?>> Function() nativeAction,
    required bool measured,
    int? scheduledTraceMicros,
    String? payload,
    String? payloadSha256,
    String evidenceRole = 'workload',
    int? stateTransitionIndex,
  }) async {
    final inputSequence = _nextInputSequence++;
    final actionStart = developer.Timeline.now;
    final acceptedFuture = _lengthProbe.waitForLength(expectedDocumentLength);
    final backlogAtDispatch = _inputBacklog;
    _inputBacklog += 1;
    if (_inputBacklog > _maxInputBacklog) _maxInputBacklog = _inputBacklog;
    try {
      final nativeFuture = nativeAction();
      final nativeResult = await nativeFuture.timeout(config.inputTimeout);
      late final _AcceptedInput accepted;
      try {
        accepted = await acceptedFuture.timeout(config.inputTimeout);
      } on TimeoutException {
        throw TimeoutException(
          'Input was dispatched but did not reach the expected document '
          'length. nativeInput=$nativeResult',
          config.inputTimeout,
        );
      }
      final frame = await frameProbe
          .firstRasterFromBuildAfter(accepted.traceMicros)
          .timeout(
            config.inputTimeout,
            onTimeout: () => throw TimeoutException(
              'No FrameTiming began building after the model accepted $action '
              'sample $sampleIndex; input-to-raster correlation is unproven.',
              config.inputTimeout,
            ),
          );
      final nativeIngressTraceMicros = _projectEpochToTrace(
        eventEpochMicros: nativeResult['dispatchEpochMicros'],
        acceptedEpochMicros: accepted.epochMicros,
        acceptedTraceMicros: accepted.traceMicros,
      );
      return <String, Object?>{
        if (stateTransitionIndex != null) 'inputSequence': inputSequence,
        if (stateTransitionIndex != null) 'evidenceRole': evidenceRole,
        'stateTransitionIndex': ?stateTransitionIndex,
        'action': action,
        'sampleIndex': sampleIndex,
        'measured': measured,
        'payload': payload,
        'payloadSha256': payloadSha256,
        'expectedDocumentLength': expectedDocumentLength,
        'scheduledTraceMicros': scheduledTraceMicros,
        'actionStartTraceMicros': actionStart,
        'acceptedTraceMicros': accepted.traceMicros,
        'acceptedEpochMicros': accepted.epochMicros,
        'nativeDispatchUptimeMicros': nativeResult['dispatchUptimeMicros'],
        'nativeDispatchEpochMicros': nativeResult['dispatchEpochMicros'],
        if (stateTransitionIndex != null)
          'nativeIngressTraceMicros': nativeIngressTraceMicros,
        'nativeInput': nativeResult,
        'cadenceLatenessMicros': scheduledTraceMicros == null
            ? null
            : actionStart - scheduledTraceMicros,
        'actionCallStartToAcceptedMicros': accepted.traceMicros - actionStart,
        'nativeDispatchToAcceptedMicros': _clockDelta(
          nativeResult['dispatchEpochMicros'],
          accepted.epochMicros,
        ),
        'acceptedInputToRasterFinishMicros': _clockDelta(
          accepted.traceMicros,
          frame.rasterFinishMicros,
        ),
        'nativeDispatchToRasterFinishMicros': _sumDurations(
          _clockDelta(
            nativeResult['dispatchEpochMicros'],
            accepted.epochMicros,
          ),
          _clockDelta(accepted.traceMicros, frame.rasterFinishMicros),
        ),
        'acceptedInputToFrameTimingCallbackMicros':
            frame.callbackTraceMicros - accepted.traceMicros,
        'frameCorrelation': <String, Object?>{
          'proven': true,
          'criterion':
              'buildStartTraceMicros > acceptedTraceMicros with ordered raster and callback timestamps',
          'acceptedToBuildStartMicros':
              frame.buildStartMicros - accepted.traceMicros,
        },
        'inputBacklogAtDispatch': backlogAtDispatch,
        'frame': frame.toJson(),
      };
    } finally {
      _inputBacklog -= 1;
    }
  }

  Future<void> _setCaret(EditLocation location) async {
    final offset = _locationOffset(location, fixture.length);
    controller.updateSelection(
      TextSelection.collapsed(offset: offset),
      ChangeSource.local,
    );
    focusNode.requestFocus();
    await WidgetsBinding.instance.endOfFrame;
  }
}

final class _DocumentLengthProbe {
  _DocumentLengthProbe(this.controller);

  final QuillController controller;
  final List<_LengthWaiter> _waiters = <_LengthWaiter>[];

  void start() => controller.addListener(_onChanged);

  Future<_AcceptedInput> waitForLength(int expectedLength) {
    final waiter = _LengthWaiter(expectedLength);
    _waiters.add(waiter);
    return waiter.completer.future;
  }

  void _onChanged() {
    final length = controller.document.length;
    final match = _waiters.indexWhere(
      (waiter) => waiter.expectedLength == length,
    );
    if (match < 0) return;
    final waiter = _waiters.removeAt(match);
    waiter.completer.complete(
      _AcceptedInput(
        traceMicros: developer.Timeline.now,
        epochMicros: DateTime.now().microsecondsSinceEpoch,
      ),
    );
  }
}

final class _LengthWaiter {
  _LengthWaiter(this.expectedLength);
  final int expectedLength;
  final completer = Completer<_AcceptedInput>();
}

final class _AcceptedInput {
  const _AcceptedInput({required this.traceMicros, required this.epochMicros});
  final int traceMicros;
  final int epochMicros;
}

final class _FrameProbe {
  final List<_FrameRecord> _records = <_FrameRecord>[];
  final List<_FrameWaiter> _waiters = <_FrameWaiter>[];

  void start() => WidgetsBinding.instance.addTimingsCallback(_onTimings);

  void dispose() => WidgetsBinding.instance.removeTimingsCallback(_onTimings);

  Future<_FrameRecord> firstRasterFromBuildAfter(int traceMicros) {
    for (final record in _records) {
      if (record.provesBuildAfter(traceMicros)) {
        return Future.value(record);
      }
    }
    final waiter = _FrameWaiter(traceMicros);
    _waiters.add(waiter);
    return waiter.completer.future;
  }

  void _onTimings(List<FrameTiming> timings) {
    final callbackTrace = developer.Timeline.now;
    final callbackEpoch = DateTime.now().microsecondsSinceEpoch;
    for (final timing in timings) {
      _records.add(
        _FrameRecord(
          timing: timing,
          callbackTraceMicros: callbackTrace,
          callbackEpochMicros: callbackEpoch,
        ),
      );
    }
    for (final waiter in List<_FrameWaiter>.of(_waiters)) {
      final match = _records.where(
        (record) => record.provesBuildAfter(waiter.afterTraceMicros),
      );
      if (match.isEmpty) continue;
      _waiters.remove(waiter);
      waiter.completer.complete(match.first);
    }
    if (_records.length > 4096) {
      _records.removeRange(0, _records.length - 2048);
    }
  }
}

final class _FrameWaiter {
  _FrameWaiter(this.afterTraceMicros);
  final int afterTraceMicros;
  final completer = Completer<_FrameRecord>();
}

final class _FrameRecord {
  const _FrameRecord({
    required this.timing,
    required this.callbackTraceMicros,
    required this.callbackEpochMicros,
  });

  final FrameTiming timing;
  final int callbackTraceMicros;
  final int callbackEpochMicros;

  int get buildStartMicros =>
      timing.timestampInMicroseconds(FramePhase.buildStart);

  int get rasterFinishMicros =>
      timing.timestampInMicroseconds(FramePhase.rasterFinish);

  bool provesBuildAfter(int acceptedTraceMicros) => provesAcceptedEditFrame(
    acceptedTraceMicros: acceptedTraceMicros,
    buildStartTraceMicros: buildStartMicros,
    rasterFinishTraceMicros: rasterFinishMicros,
    timingCallbackTraceMicros: callbackTraceMicros,
  );

  int get rasterFinishEpochMicros =>
      callbackEpochMicros - (callbackTraceMicros - rasterFinishMicros);

  Map<String, Object?> toJson() => <String, Object?>{
    'frameNumber': timing.frameNumber,
    'vsyncStartMicros': timing.timestampInMicroseconds(FramePhase.vsyncStart),
    'buildStartMicros': buildStartMicros,
    'buildFinishMicros': timing.timestampInMicroseconds(FramePhase.buildFinish),
    'rasterStartMicros': timing.timestampInMicroseconds(FramePhase.rasterStart),
    'rasterFinishMicros': rasterFinishMicros,
    'rasterFinishEpochMicros': rasterFinishEpochMicros,
    'buildDurationMicros': timing.buildDuration.inMicroseconds,
    'rasterDurationMicros': timing.rasterDuration.inMicroseconds,
    'totalSpanMicros': timing.totalSpan.inMicroseconds,
    'vsyncOverheadMicros': timing.vsyncOverhead.inMicroseconds,
    'frameTimingCallbackTraceMicros': callbackTraceMicros,
    'frameTimingCallbackEpochMicros': callbackEpochMicros,
  };
}

final class _NativeHarness {
  static const _channel = MethodChannel('dev.flark.peer_benchmark/harness');

  Future<Map<String, Object?>> systemInfo() => _invokeMap('systemInfo');

  Future<Map<String, Object?>> processMemory() => _invokeMap('processMemory');

  Future<void> activateWindow() =>
      _channel.invokeMethod<void>('activateWindow');

  Future<Map<String, Object?>> typeCharacter(String character) =>
      _invokeMap('typeCharacter', <String, Object?>{'character': character});

  Future<Map<String, Object?>> backspace() => _invokeMap('backspace');

  Future<Map<String, Object?>> pasteText(String text) =>
      _invokeMap('pasteText', <String, Object?>{'text': text});

  Future<void> restoreClipboard() =>
      _channel.invokeMethod<void>('restoreClipboard');

  Future<Map<String, Object?>> _invokeMap(
    String method, [
    Map<String, Object?>? arguments,
  ]) async {
    final raw = await _channel.invokeMapMethod<Object?, Object?>(
      method,
      arguments,
    );
    if (raw == null) throw StateError('$method returned null');
    return raw.map((key, value) => MapEntry('$key', value));
  }
}

final class _EditorReady {
  const _EditorReady({
    required this.traceMicros,
    required this.focused,
    required this.editorStateMounted,
    required this.viewportWidth,
    required this.viewportHeight,
  });

  final int traceMicros;
  final bool focused;
  final bool editorStateMounted;
  final double viewportWidth;
  final double viewportHeight;
}

int _locationOffset(EditLocation location, int sourceLength) =>
    switch (location) {
      EditLocation.start => 0,
      EditLocation.middle => sourceLength ~/ 2,
      EditLocation.end => sourceLength,
    };

/// Inserts [payload] into the frozen source model used by both peer harnesses.
///
/// The coordinator reconstructs this independently; keeping this operation
/// explicit makes an accidental accumulating-paste workload observable.
String insertExactSource({
  required String source,
  required String payload,
  required int offset,
}) {
  if (offset < 0 || offset > source.length) {
    throw RangeError.range(offset, 0, source.length, 'offset');
  }
  return '${source.substring(0, offset)}$payload${source.substring(offset)}';
}

Map<String, Object?> canonicalStateDenominator(String canonicalSource) =>
    <String, Object?>{
      'utf8Bytes': utf8.encode(canonicalSource).length,
      'sha256': sha256Text(canonicalSource),
    };

/// Proves Quill's current source against the peer-neutral source denominator.
///
/// Quill owns one required terminal newline. It is removed only when the raw
/// value is exactly the expected canonical source plus that one newline; no
/// general trimming or normalization is permitted.
Map<String, Object?> quillCanonicalStateProof({
  required String expectedCanonical,
  required String actualPeerSource,
}) {
  final classification = actualPeerSource == expectedCanonical
      ? 'exact'
      : actualPeerSource == '$expectedCanonical\n'
      ? 'peer-appended-terminal-newline'
      : null;
  if (classification == null) {
    throw StateError(
      'Quill source does not match the canonical paste-state denominator: '
      '${compareSource(expected: expectedCanonical, actual: actualPeerSource)}',
    );
  }
  return <String, Object?>{
    'canonicalUtf8Bytes': utf8.encode(expectedCanonical).length,
    'canonicalSha256': sha256Text(expectedCanonical),
    'rawUtf8Bytes': utf8.encode(actualPeerSource).length,
    'rawSha256': sha256Text(actualPeerSource),
    'classification': classification,
    'matchesExpectedCanonical': true,
  };
}

Map<String, Object?> _pasteTransition({
  required int transitionIndex,
  required bool measured,
  required Map<String, Object?> pasteInput,
  required Map<String, Object?> preState,
  required Map<String, Object?> postState,
  required Map<String, Object?> resetState,
  required Map<String, Object?> resetInput,
}) => <String, Object?>{
  'transitionIndex': transitionIndex,
  'measured': measured,
  'pasteInput': pasteInput,
  'preState': preState,
  'postState': postState,
  'resetState': resetState,
  'resetInput': resetInput,
};

int? _projectEpochToTrace({
  required Object? eventEpochMicros,
  required int acceptedEpochMicros,
  required int acceptedTraceMicros,
}) {
  final ingressToAcceptance = _clockDelta(
    eventEpochMicros,
    acceptedEpochMicros,
  );
  if (ingressToAcceptance == null) return null;
  return acceptedTraceMicros - ingressToAcceptance;
}

int? _clockDelta(Object? start, Object? end) {
  if (start is! int || end is! int) return null;
  final delta = end - start;
  if (delta < 0 || delta > _inputTimeout.inMicroseconds) return null;
  return delta;
}

int? _durationBetween(Object? start, Object? end) {
  if (start is! int || end is! int) return null;
  final delta = end - start;
  return delta < 0 ? null : delta;
}

int? _sumDurations(int? first, int? second) {
  if (first == null || second == null) return null;
  return first + second;
}

Map<String, Object?> _distributions(List<Map<String, Object?>> samples) {
  const fields = <String>[
    'acceptedInputToRasterFinishMicros',
    'nativeDispatchToRasterFinishMicros',
    'acceptedInputToFrameTimingCallbackMicros',
    'actionCallStartToAcceptedMicros',
    'cadenceLatenessMicros',
  ];
  return <String, Object?>{
    for (final field in fields) field: _percentiles(samples, field),
  };
}

Map<String, Object?> _percentiles(
  List<Map<String, Object?>> samples,
  String field,
) {
  final values =
      samples.map((sample) => sample[field]).whereType<int>().toList()..sort();
  if (values.isEmpty) {
    return const <String, Object?>{
      'count': 0,
      'p50': null,
      'p90': null,
      'p99': null,
      'max': null,
    };
  }
  int percentile(double fraction) =>
      values[((values.length - 1) * fraction).ceil()];
  return <String, Object?>{
    'count': values.length,
    'p50': percentile(0.50),
    'p90': percentile(0.90),
    'p99': percentile(0.99),
    'max': values.last,
  };
}

bool _fidelityAllowsCompletion(Map<String, Object?> fidelity) =>
    fidelity['exact'] == true ||
    fidelity['classification'] == 'peer-appended-terminal-newline';

Future<Map<String, Object?>> _writeExport({
  required String? path,
  required String source,
}) async {
  if (path == null || path.isEmpty) {
    return <String, Object?>{
      'written': false,
      'reason': 'COMPETITOR_EXPORT_PATH was not provided',
      'sha256': sha256Text(source),
      'utf8Bytes': utf8.encode(source).length,
    };
  }
  final file = File(path);
  await file.parent.create(recursive: true);
  try {
    await file.create(exclusive: true);
  } on FileSystemException catch (error) {
    throw StateError(
      'Refusing to overwrite a prior process export at ${file.path}: $error',
    );
  }
  await file.writeAsString(source, flush: true);
  return <String, Object?>{
    'written': true,
    'path': file.absolute.path,
    'sha256': sha256Text(source),
    'utf8Bytes': await file.length(),
  };
}

Future<void> _emitResult(String? path, Map<String, Object?> result) async {
  const encoder = JsonEncoder.withIndent('  ');
  final encoded = encoder.convert(result);
  if (path != null && path.isNotEmpty) {
    final file = File(path);
    await file.parent.create(recursive: true);
    await file.writeAsString('$encoded\n', flush: true);
  }
  stdout.writeln('$_resultPrefix${jsonEncode(result)}');
}
