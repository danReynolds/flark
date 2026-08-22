// SuperEditor M0 Mac profile runner.
//
// This is intentionally a real profile application, not a widget benchmark.
// Native macOS NSEvents enter the focused Flutter view, traverse Flutter's
// platform text-input handling, mutate SuperEditor's default model, and are
// correlated with engine FrameTiming raster completion.

import 'dart:async';
import 'dart:convert';
import 'dart:developer';
import 'dart:io';
import 'dart:ui' show FramePhase, FrameTiming;

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:super_editor/super_editor.dart';

import 'src/competitor_fixture.dart';

const _requiredProtocolId = 'm0-mac-competitor-profile-v1';
const _inputChannel = MethodChannel('dev.flark/competitor_input');
const _typingStream = 'abcdefghijklmnopqrstuvwxyz0123456789';
const _pasteBytes = 32768;

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final config = HarnessConfig.fromEnvironment();
  runApp(CompetitorProfileApp(config: config));
}

enum ProfileWorkload {
  coldOpen('cold-open'),
  typing('sustained-typing'),
  localEdit('local-insert-delete'),
  paste('paste-32kib');

  const ProfileWorkload(this.wireName);

  final String wireName;

  static ProfileWorkload parse(String value) => values.firstWhere(
    (candidate) => candidate.wireName == value,
    orElse: () => throw ArgumentError.value(value, 'COMPETITOR_WORKLOAD'),
  );
}

enum EditLocation {
  start,
  middle,
  end;

  static EditLocation parse(String value) => values.firstWhere(
    (candidate) => candidate.name == value,
    orElse: () => throw ArgumentError.value(value, 'COMPETITOR_LOCATION'),
  );
}

final class HarnessConfig {
  const HarnessConfig({
    required this.protocolId,
    required this.workload,
    required this.location,
    required this.targetBytes,
    required this.warmupCount,
    required this.sampleCount,
    required this.cadenceMillis,
    required this.pasteBytes,
    required this.timeout,
    required this.runId,
    required this.requestedOutputDirectory,
    required this.hostProvenance,
    required this.buildProvenance,
    required this.invocation,
  });

  factory HarnessConfig.fromEnvironment() {
    final environment = Platform.environment;
    final workload = ProfileWorkload.parse(
      environment['COMPETITOR_WORKLOAD'] ?? 'cold-open',
    );
    final defaults = switch (workload) {
      ProfileWorkload.coldOpen => (warmups: 0, samples: 1, cadence: 0),
      ProfileWorkload.typing => (warmups: 20, samples: 200, cadence: 100),
      ProfileWorkload.localEdit => (warmups: 10, samples: 100, cadence: 0),
      ProfileWorkload.paste => (warmups: 2, samples: 20, cadence: 0),
    };

    Map<String, Object?> decodeMap(String key) {
      final encoded = environment[key];
      if (encoded == null || encoded.isEmpty) return const {};
      return (jsonDecode(encoded) as Map).cast<String, Object?>();
    }

    int parseCount(String key, int defaultValue) {
      return int.parse(environment[key] ?? '$defaultValue');
    }

    return HarnessConfig(
      protocolId: const String.fromEnvironment(
        'COMPETITOR_PROTOCOL_ID',
        defaultValue: 'missing-protocol-id',
      ),
      workload: workload,
      location: EditLocation.parse(
        environment['COMPETITOR_LOCATION'] ??
            (workload == ProfileWorkload.typing ? 'end' : 'middle'),
      ),
      targetBytes: parseCount('COMPETITOR_SIZE_BYTES', 1048576),
      warmupCount: parseCount('COMPETITOR_WARMUP_COUNT', defaults.warmups),
      sampleCount: parseCount('COMPETITOR_SAMPLE_COUNT', defaults.samples),
      cadenceMillis: parseCount('COMPETITOR_CADENCE_MILLIS', defaults.cadence),
      pasteBytes: parseCount('COMPETITOR_PASTE_BYTES', _pasteBytes),
      timeout: Duration(seconds: parseCount('COMPETITOR_TIMEOUT_SECONDS', 60)),
      runId:
          environment['COMPETITOR_RUN_ID'] ??
          '${workload.wireName}-${DateTime.now().toUtc().microsecondsSinceEpoch}',
      requestedOutputDirectory: environment['COMPETITOR_OUTPUT_DIRECTORY'],
      hostProvenance: decodeMap('COMPETITOR_HOST_PROVENANCE_JSON'),
      buildProvenance: decodeMap('COMPETITOR_BUILD_PROVENANCE_JSON'),
      invocation: decodeMap('COMPETITOR_INVOCATION_JSON'),
    );
  }

  final String protocolId;
  final ProfileWorkload workload;
  final EditLocation location;
  final int targetBytes;
  final int warmupCount;
  final int sampleCount;
  final int cadenceMillis;
  final int pasteBytes;
  final Duration timeout;
  final String runId;
  final String? requestedOutputDirectory;
  final Map<String, Object?> hostProvenance;
  final Map<String, Object?> buildProvenance;
  final Map<String, Object?> invocation;

  bool get hasProtocolCounts => switch (workload) {
    ProfileWorkload.coldOpen => warmupCount == 0 && sampleCount == 1,
    ProfileWorkload.typing =>
      warmupCount == 20 && sampleCount == 200 && cadenceMillis == 100,
    ProfileWorkload.localEdit => warmupCount == 10 && sampleCount == 100,
    ProfileWorkload.paste =>
      warmupCount == 2 && sampleCount == 20 && pasteBytes == _pasteBytes,
  };

  bool get hasProtocolSize =>
      const {1048576, 5242880, 10485760}.contains(targetBytes);

  bool get hasProtocolTimeout => timeout == const Duration(seconds: 60);

  Map<String, Object?> toJson() => {
    'protocolId': protocolId,
    'workload': workload.wireName,
    'location': location.name,
    'targetBytes': targetBytes,
    'warmupCount': warmupCount,
    'sampleCount': sampleCount,
    'cadenceMillis': cadenceMillis,
    'pasteBytes': pasteBytes,
    'timeoutMicros': timeout.inMicroseconds,
    'runId': runId,
    'requestedOutputDirectory': requestedOutputDirectory,
  };
}

class CompetitorProfileApp extends StatelessWidget {
  const CompetitorProfileApp({super.key, required this.config});

  final HarnessConfig config;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'SuperEditor competitor profile',
      theme: ThemeData(useMaterial3: true),
      home: CompetitorProfileSurface(config: config),
    );
  }
}

class CompetitorProfileSurface extends StatefulWidget {
  const CompetitorProfileSurface({super.key, required this.config});

  final HarnessConfig config;

  @override
  State<CompetitorProfileSurface> createState() =>
      _CompetitorProfileSurfaceState();
}

class _CompetitorProfileSurfaceState extends State<CompetitorProfileSurface> {
  final _viewportKey = GlobalKey();
  final _documentLayoutKey = GlobalKey();
  final _focusNode = FocusNode(debugLabel: 'competitor-profile-editor');
  final _imeConnected = ValueNotifier(false);
  final _frames = <FrameSample>[];
  final _inputs = <InputSample>[];
  final _rssSamples = <Map<String, Object?>>[];
  final _hardwareKeyEvents = <Map<String, Object?>>[];
  final _pendingInputs = <InputSample>[];
  final _resetInputs = <InputSample>[];
  final _pasteTransitions = <Map<String, Object?>>[];

  late final String _initialSource;
  late final String _fixtureSha256;
  late final MutableDocument _document;
  late final MutableDocumentComposer _composer;
  late final Editor _editor;
  late final FunctionalEditListener _editListener;
  late final int _initialCaretOffset;
  late final int _fixtureGenerationStartMicros;
  late final int _fixtureGenerationFinishMicros;
  late final int _documentLoadStartMicros;
  late final int _documentModelReadyMicros;
  late final DateTime _startedAtUtc;

  final _interactiveFrameCompleter = Completer<FrameSample>();
  int? _interactiveEligibleMicros;
  int? _firstDocumentFrameMicros;
  int? _processBootstrapTimelineMicros;
  int? _timelineMinusNativeUptimeMicros;
  Map<String, Object?> _nativeBootstrap = const {};
  int _maxInputBacklog = 0;
  int _peakRss = 0;
  int _nextInputSequence = 0;
  bool _finished = false;
  bool _pasteActionRequiresFallback = false;
  String _phase = 'constructing';

  HarnessConfig get config => widget.config;

  List<InputSample> get _allInputs => <InputSample>[
    ..._inputs,
    ..._resetInputs,
  ];

  Map<String, Object?>? get _pasteStateContract {
    if (config.workload != ProfileWorkload.paste) return null;
    final payload = generateOrdinaryProseFixture(config.pasteBytes);
    final pasted = insertExactSource(
      source: _initialSource,
      payload: payload,
      offset: _initialCaretOffset,
    );
    return <String, Object?>{
      'schemaVersion': 1,
      'mode': 'reset-after-each-paste',
      'pasteViaPlatformInput': true,
      'resetViaPlatformBackspace': true,
      'selectionForReset': 'programmatic-exact-pasted-source-range',
      'warmupTransitions': config.warmupCount,
      'measuredTransitions': config.sampleCount,
      'baseState': canonicalStateDenominator(_initialSource),
      'singlePasteState': canonicalStateDenominator(pasted),
      'expectedFinalState': canonicalStateDenominator(_initialSource),
      'transitions': _pasteTransitions,
    };
  }

  @override
  void initState() {
    super.initState();
    _startedAtUtc = DateTime.now().toUtc();
    _fixtureGenerationStartMicros = Timeline.now;
    _initialSource = generateOrdinaryProseFixture(config.targetBytes);
    _fixtureSha256 = sha256Text(_initialSource);
    _fixtureGenerationFinishMicros = Timeline.now;

    _documentLoadStartMicros = Timeline.now;
    _document = documentFromExactSource(_initialSource);
    _initialCaretOffset = switch (config.location) {
      EditLocation.start => 0,
      EditLocation.middle => _initialSource.length ~/ 2,
      EditLocation.end => _initialSource.length,
    };
    final caret = sourceCaretAt(_document, _initialCaretOffset);
    _composer = MutableDocumentComposer(
      initialSelection: DocumentSelection.collapsed(position: caret.position),
    );
    _editListener = FunctionalEditListener(_onEditorChanges);
    _editor = createDefaultDocumentEditor(
      document: _document,
      composer: _composer,
    )..addListener(_editListener);
    _documentModelReadyMicros = Timeline.now;

    _sampleRss('model-ready');
    if (config.workload == ProfileWorkload.paste) {
      HardwareKeyboard.instance.addHandler(_recordHardwareKeyEvent);
    }
    WidgetsBinding.instance.addTimingsCallback(_onFrameTimings);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      unawaited(_run());
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeTimingsCallback(_onFrameTimings);
    if (config.workload == ProfileWorkload.paste) {
      HardwareKeyboard.instance.removeHandler(_recordHardwareKeyEvent);
    }
    _editor
      ..removeListener(_editListener)
      ..dispose();
    _composer.dispose();
    _focusNode.dispose();
    _imeConnected.dispose();
    super.dispose();
  }

  bool _recordHardwareKeyEvent(KeyEvent event) {
    _hardwareKeyEvents.add({
      'timelineMicros': Timeline.now,
      'eventType': event.runtimeType.toString(),
      'logicalKeyId': event.logicalKey.keyId,
      'logicalKeyLabel': event.logicalKey.keyLabel,
      'physicalKeyUsbHidUsage': event.physicalKey.usbHidUsage,
      'character': event.character,
      'synthesized': event.synthesized,
      'metaPressed': HardwareKeyboard.instance.isMetaPressed,
    });
    return false;
  }

  void _sampleRss(String phase) {
    final rss = ProcessInfo.currentRss;
    if (rss > _peakRss) _peakRss = rss;
    _rssSamples.add({
      'phase': phase,
      'timelineMicros': Timeline.now,
      'rssBytes': rss,
    });
  }

  void _onEditorChanges(List<EditEvent> changes) {
    if (!changes.any((change) => change is DocumentEdit)) return;
    final now = Timeline.now;
    final pending = _pendingInputs.cast<InputSample?>().firstWhere(
      (sample) =>
          sample != null &&
          sample.requestedTimelineMicros <= now &&
          sample.acceptedTimelineMicros == null,
      orElse: () => null,
    );
    if (pending == null) return;
    pending.acceptedTimelineMicros = Timeline.now;
    if (!pending.accepted.isCompleted) pending.accepted.complete();
    pending.backlogAtAcceptance = _unpaintedInputCount;
    // External launch drivers use these progress markers for a watchdog that
    // still fires if Flutter's merged UI/platform thread is blocked.
    // ignore: avoid_print
    print(
      'COMPETITOR_INPUT_ACCEPTED sequence=${pending.sequence} '
      'timelineMicros=${pending.acceptedTimelineMicros}',
    );
    _sampleRss('accepted-${pending.sequence}');
  }

  int get _unpaintedInputCount {
    final now = Timeline.now;
    return _pendingInputs
        .where(
          (sample) =>
              sample.requestedTimelineMicros <= now &&
              sample.rasterFinishTimelineMicros == null,
        )
        .length;
  }

  void _onFrameTimings(List<FrameTiming> timings) {
    final callbackTimelineMicros = Timeline.now;
    for (final timing in timings) {
      final sample = FrameSample.fromTiming(
        timing,
        callbackTimelineMicros: callbackTimelineMicros,
      );
      _frames.add(sample);
      _firstDocumentFrameMicros ??=
          sample.buildStartTimelineMicros >= _documentModelReadyMicros
          ? sample.rasterFinishTimelineMicros
          : null;

      final eligible = _interactiveEligibleMicros;
      if (eligible != null &&
          !_interactiveFrameCompleter.isCompleted &&
          sample.buildStartTimelineMicros >= eligible) {
        _interactiveFrameCompleter.complete(sample);
      }

      for (final input in _pendingInputs) {
        final accepted = input.acceptedTimelineMicros;
        if (accepted == null || input.rasterFinishTimelineMicros != null) {
          continue;
        }
        // A raster that merely finishes after acceptance might belong to a
        // frame whose build began before the edit. Require a post-acceptance
        // build so this frame can actually contain the changed document.
        if (sample.buildStartTimelineMicros <= accepted) continue;
        input.completeWith(sample, backlogAtRaster: _unpaintedInputCount);
        // ignore: avoid_print
        print(
          'COMPETITOR_INPUT_RASTER sequence=${input.sequence} '
          'frame=${sample.frameNumber} '
          'rasterFinishMicros=${sample.rasterFinishTimelineMicros}',
        );
      }
    }
  }

  Future<void> _run() async {
    try {
      _phase = 'native-bootstrap';
      final dartBefore = Timeline.now;
      _nativeBootstrap =
          (await _inputChannel.invokeMapMethod<String, Object?>('bootstrap')) ??
          const {};
      final dartAfter = Timeline.now;
      final nativeStart = _nativeBootstrap['processBootstrapUptimeMicros'];
      final nativeNow = _nativeBootstrap['nativeNowUptimeMicros'];
      if (nativeStart is num && nativeNow is num) {
        _timelineMinusNativeUptimeMicros =
            ((dartBefore + dartAfter) ~/ 2) - nativeNow.toInt();
        _processBootstrapTimelineMicros =
            nativeStart.toInt() + _timelineMinusNativeUptimeMicros!;
      }

      await _inputChannel.invokeMethod<void>('activate');
      _focusNode.requestFocus();
      await _awaitInteractiveSurface();
      final interactiveFrame = await _interactiveFrameCompleter.future.timeout(
        config.timeout,
      );
      _sampleRss('interactive-raster');

      final initialExport = exportExactSource(_document);
      final initialFidelity = initialExport == _initialSource;
      if (!initialFidelity) {
        throw StateError(
          'Initial SuperEditor source mapping changed the fixture',
        );
      }

      switch (config.workload) {
        case ProfileWorkload.coldOpen:
          break;
        case ProfileWorkload.typing:
          await _runTyping();
        case ProfileWorkload.localEdit:
          await _runLocalInsertDelete();
        case ProfileWorkload.paste:
          await _runPaste();
      }

      await _finish(interactiveFrame: interactiveFrame);
    } catch (error, stackTrace) {
      await _finish(
        interactiveFrame: _interactiveFrameCompleter.isCompleted
            ? await _interactiveFrameCompleter.future
            : null,
        fatalError: '$error',
        fatalStack: '$stackTrace',
      );
    }
  }

  Future<void> _awaitInteractiveSurface() async {
    final deadline = DateTime.now().add(config.timeout);
    while (DateTime.now().isBefore(deadline)) {
      final renderObject = _viewportKey.currentContext?.findRenderObject();
      final viewportSize = renderObject is RenderBox ? renderObject.size : null;
      final firstNode = _document.first;
      final hasExpectedText =
          firstNode is TextNode &&
          firstNode.text.toPlainText().startsWith('Ordinary prose opens');
      if (_focusNode.hasFocus &&
          _imeConnected.value &&
          viewportSize == const Size(600, 600) &&
          hasExpectedText) {
        _interactiveEligibleMicros = Timeline.now;
        if (mounted) {
          setState(() => _phase = 'interactive-raster');
        }
        return;
      }
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    throw TimeoutException(
      'Editor did not become focused, IME-connected, exact-viewport, and '
      'text-bearing',
      config.timeout,
    );
  }

  Future<void> _runTyping() async {
    _phase = 'sustained-typing';
    final total = config.warmupCount + config.sampleCount;
    final text = List.generate(
      total,
      (index) => _typingStream[index % _typingStream.length],
    ).join();
    final schedule =
        (await _inputChannel.invokeMapMethod<String, Object?>('scheduleText', {
          'text': text,
          'cadenceMicros': config.cadenceMillis * 1000,
        })) ??
        const {};
    final nativeStart = schedule['scheduledStartUptimeMicros'];
    final clockOffset = _timelineMinusNativeUptimeMicros;
    if (nativeStart is! num || clockOffset == null) {
      throw StateError('Native typing schedule did not provide a clock anchor');
    }
    final startTimelineMicros = nativeStart.toInt() + clockOffset;
    for (var index = 0; index < total; index += 1) {
      final sample = InputSample(
        sequence: _nextInputSequence++,
        operation: 'insert-text',
        evidenceRole: 'workload',
        measured: index >= config.warmupCount,
        pair: null,
        payloadBytes: 1,
        requestedTimelineMicros:
            startTimelineMicros + index * config.cadenceMillis * 1000,
        backlogAtRequest: 0,
      );
      sample.nativeEvent = {
        ...schedule,
        'scheduledIndex': index,
        'scheduledCharacter': text[index],
      };
      _inputs.add(sample);
      _pendingInputs.add(sample);
    }
    await _awaitAllInputs();
  }

  Future<void> _runLocalInsertDelete() async {
    _phase = 'local-insert-delete';
    final totalPairs = config.warmupCount + config.sampleCount;
    for (var pair = 0; pair < totalPairs; pair += 1) {
      final measured = pair >= config.warmupCount;
      final insertion = await _postInput(
        operation: 'insert-text',
        payload: 'x',
        payloadBytes: 1,
        measured: measured,
        pair: pair,
      );
      if (!await _awaitInput(insertion)) break;
      final deletion = await _postInput(
        operation: 'backspace',
        payloadBytes: 0,
        measured: measured,
        pair: pair,
      );
      if (!await _awaitInput(deletion)) break;
    }
  }

  Future<void> _runPaste() async {
    _phase = 'paste-32kib';
    final payload = generateOrdinaryProseFixture(config.pasteBytes);
    final expectedPasted = insertExactSource(
      source: _initialSource,
      payload: payload,
      offset: _initialCaretOffset,
    );
    final total = config.warmupCount + config.sampleCount;
    for (var index = 0; index < total; index += 1) {
      final measured = index >= config.warmupCount;
      final preState = exactCanonicalStateProof(
        expectedCanonical: _initialSource,
        actualPeerSource: exportExactSource(_document),
      );
      late final InputSample sample;
      try {
        sample = await _postInput(
          operation: 'paste',
          payload: payload,
          payloadBytes: config.pasteBytes,
          measured: measured,
          evidenceRole: 'paste-workload',
        );
        if (!await _awaitInput(sample)) {
          throw StateError('Paste input ${sample.sequence} failed');
        }
      } finally {
        await _inputChannel.invokeMethod<void>('restorePasteboard');
      }
      final postState = exactCanonicalStateProof(
        expectedCanonical: expectedPasted,
        actualPeerSource: exportExactSource(_document),
      );
      final resetInput = await _resetPaste(index);
      final resetState = exactCanonicalStateProof(
        expectedCanonical: _initialSource,
        actualPeerSource: exportExactSource(_document),
      );
      sample.stateTransitionIndex = index;
      _pasteTransitions.add({
        'transitionIndex': index,
        'measured': measured,
        'pasteInput': {'evidenceSequence': sample.sequence},
        'preState': preState,
        'postState': postState,
        'resetState': resetState,
        'resetInput': {
          'operation': 'platform-backspace-over-exact-pasted-range',
          'measured': false,
          'accepted': resetInput.acceptedTimelineMicros is int,
          'rastered': resetInput.rasterFinishTimelineMicros is int,
          'platformInputDispatched':
              resetInput.nativeEvent['eventPath'] is String,
          'selectionStart': _initialCaretOffset,
          'selectionEnd': _initialCaretOffset + payload.length,
          'evidenceSequence': resetInput.sequence,
        },
      });
    }
  }

  Future<InputSample> _resetPaste(int transitionIndex) async {
    final start = sourceCaretAt(_document, _initialCaretOffset);
    final end = sourceCaretAt(
      _document,
      _initialCaretOffset + config.pasteBytes,
    );
    _composer.setSelectionWithReason(
      DocumentSelection(base: start.position, extent: end.position),
      SelectionReason.userInteraction,
    );
    _focusNode.requestFocus();
    await WidgetsBinding.instance.endOfFrame;

    final reset = await _postInput(
      operation: 'backspace',
      payloadBytes: 0,
      measured: false,
      pair: transitionIndex,
      evidenceRole: 'paste-reset',
    );
    if (!await _awaitInput(reset)) {
      throw StateError('Paste reset input ${reset.sequence} failed');
    }
    reset.stateTransitionIndex = transitionIndex;
    final caret = sourceCaretAt(_document, _initialCaretOffset);
    _composer.setSelectionWithReason(
      DocumentSelection.collapsed(position: caret.position),
      SelectionReason.userInteraction,
    );
    await WidgetsBinding.instance.endOfFrame;
    return reset;
  }

  Future<InputSample> _postInput({
    required String operation,
    required int payloadBytes,
    required bool measured,
    String? payload,
    int? pair,
    String evidenceRole = 'workload',
  }) async {
    final sample = InputSample(
      sequence: _nextInputSequence++,
      operation: operation,
      evidenceRole: evidenceRole,
      measured: measured,
      pair: pair,
      payloadBytes: payloadBytes,
      requestedTimelineMicros: Timeline.now,
      backlogAtRequest: _unpaintedInputCount,
    );
    if (evidenceRole == 'paste-reset') {
      _resetInputs.add(sample);
    } else {
      _inputs.add(sample);
    }
    _pendingInputs.add(sample);
    // ignore: avoid_print
    print(
      'COMPETITOR_INPUT_REQUEST sequence=${sample.sequence} '
      'operation=$operation timelineMicros=${sample.requestedTimelineMicros}',
    );
    _maxInputBacklog = _unpaintedInputCount > _maxInputBacklog
        ? _unpaintedInputCount
        : _maxInputBacklog;

    try {
      final response = switch (operation) {
        'insert-text' => await _inputChannel.invokeMapMethod<String, Object?>(
          'text',
          {'text': payload},
        ),
        'backspace' => await _inputChannel.invokeMapMethod<String, Object?>(
          'backspace',
        ),
        'paste' => await _inputChannel.invokeMapMethod<String, Object?>(
          'paste',
          {
            'text': payload,
            'preferTextInputFallback': _pasteActionRequiresFallback,
          },
        ),
        _ => throw ArgumentError.value(operation, 'operation'),
      };
      var effectiveResponse = response ?? const <String, Object?>{};
      if (operation == 'paste' &&
          response?['pasteActionSent'] == true &&
          response?['textInputFallback'] != true) {
        try {
          await sample.accepted.future.timeout(
            const Duration(milliseconds: 50),
          );
          effectiveResponse = {
            ...effectiveResponse,
            'pasteActionAcceptedByModel': true,
          };
        } on TimeoutException {
          _pasteActionRequiresFallback = true;
          final fallback =
              (await _inputChannel.invokeMapMethod<String, Object?>(
                'pasteFallback',
                {'text': payload},
              )) ??
              const <String, Object?>{};
          effectiveResponse = {
            ...effectiveResponse,
            'pasteActionAcceptedByModel': false,
            'fallbackAfterUnobservedAction': fallback,
            'effectiveEventPath': fallback['eventPath'],
            'effectivePostedUptimeMicros': fallback['postedUptimeMicros'],
            'platformRouteInvoked': fallback['platformRouteInvoked'],
          };
        }
      }
      sample.nativeEvent = effectiveResponse;
      if (operation == 'paste' &&
          effectiveResponse['platformRouteInvoked'] != true) {
        sample.completeError('native-paste-not-delivered');
      }
      final posted =
          effectiveResponse['effectivePostedUptimeMicros'] ??
          effectiveResponse['postedUptimeMicros'];
      final clockOffset = _timelineMinusNativeUptimeMicros;
      if (posted is num && clockOffset != null) {
        sample.platformIngressTimelineMicros = posted.toInt() + clockOffset;
      }
    } catch (error) {
      sample
        ..nativeError = '$error'
        ..completeError('native-input-error');
    }
    return sample;
  }

  Future<bool> _awaitInput(InputSample sample) async {
    try {
      await sample.completed.future.timeout(config.timeout);
      return sample.failure == null;
    } on TimeoutException {
      sample.completeError('input-to-raster-timeout');
      return false;
    }
  }

  Future<void> _awaitAllInputs() async {
    for (final sample in _inputs) {
      if (!await _awaitInput(sample)) return;
    }
  }

  String? _expectedFinalSource() {
    if (_allInputs.any(
      (sample) =>
          sample.failure != null ||
          sample.acceptedTimelineMicros == null ||
          sample.rasterFinishTimelineMicros == null,
    )) {
      return null;
    }
    return switch (config.workload) {
      ProfileWorkload.coldOpen => _initialSource,
      ProfileWorkload.localEdit => _initialSource,
      ProfileWorkload.typing =>
        _initialSource.substring(0, _initialCaretOffset) +
            List.generate(
              _inputs.length,
              (index) => _typingStream[index % _typingStream.length],
            ).join() +
            _initialSource.substring(_initialCaretOffset),
      ProfileWorkload.paste => _initialSource,
    };
  }

  void _recalculateInputBacklogs() {
    _maxInputBacklog = 0;
    final allInputs = _allInputs;
    for (final sample in allInputs) {
      sample.backlogAtRequest = allInputs
          .where(
            (candidate) =>
                candidate.effectiveIngressTimelineMicros <=
                    sample.effectiveIngressTimelineMicros &&
                (candidate.rasterFinishTimelineMicros == null ||
                    candidate.rasterFinishTimelineMicros! >
                        sample.effectiveIngressTimelineMicros),
          )
          .length;
      final accepted = sample.acceptedTimelineMicros;
      if (accepted != null) {
        sample.backlogAtAcceptance = allInputs
            .where(
              (candidate) =>
                  candidate.effectiveIngressTimelineMicros <= accepted &&
                  (candidate.rasterFinishTimelineMicros == null ||
                      candidate.rasterFinishTimelineMicros! > accepted),
            )
            .length;
      }
      final raster = sample.rasterFinishTimelineMicros;
      if (raster != null) {
        sample.backlogAtRaster = allInputs
            .where(
              (candidate) =>
                  candidate.effectiveIngressTimelineMicros <= raster &&
                  (candidate.rasterFinishTimelineMicros == null ||
                      candidate.rasterFinishTimelineMicros! > raster),
            )
            .length;
      }
      for (final backlog in [
        sample.backlogAtRequest,
        sample.backlogAtAcceptance,
      ]) {
        if (backlog != null && backlog > _maxInputBacklog) {
          _maxInputBacklog = backlog;
        }
      }
    }
  }

  Future<void> _finish({
    required FrameSample? interactiveFrame,
    String? fatalError,
    String? fatalStack,
  }) async {
    if (_finished) return;
    _finished = true;
    _phase = fatalError == null ? 'exporting' : 'failed';
    _sampleRss('before-export');

    String? finalSource;
    String? exportError;
    try {
      finalSource = exportExactSource(_document);
    } catch (error) {
      exportError = '$error';
    }
    final expected = _expectedFinalSource();
    _recalculateInputBacklogs();
    final fidelityPass =
        finalSource != null && expected != null && finalSource == expected;

    final artifacts = await _writeArtifacts(
      finalSource: finalSource,
      expectedSource: expected,
      fidelityPass: fidelityPass,
      interactiveFrame: interactiveFrame,
      fatalError: fatalError,
      fatalStack: fatalStack,
      exportError: exportError,
    );

    // One parseable marker is sufficient for a launch driver to retain the
    // sandbox-temporary artifacts and hash captured stdout.
    // ignore: avoid_print
    print('COMPETITOR_RESULT_JSON=${artifacts.result.path}');
    // ignore: avoid_print
    print(
      'competitor_profile peer=super_editor workload=${config.workload.wireName} '
      'bytes=${config.targetBytes} completion=${artifacts.completion} '
      'fidelity=$fidelityPass result=${artifacts.result.path}',
    );
    if (mounted) setState(() => _phase = artifacts.completion);
    await Future<void>.delayed(const Duration(milliseconds: 50));
    exitCode = artifacts.completion == 'complete' ? 0 : 2;
    exit(exitCode);
  }

  Future<({File result, String completion})> _writeArtifacts({
    required String? finalSource,
    required String? expectedSource,
    required bool fidelityPass,
    required FrameSample? interactiveFrame,
    required String? fatalError,
    required String? fatalStack,
    required String? exportError,
  }) async {
    Directory artifactDirectory;
    String? outputFallbackReason;
    final requested = config.requestedOutputDirectory;
    if (requested != null) {
      try {
        artifactDirectory = Directory(requested)..createSync(recursive: true);
        final probe = File('${artifactDirectory.path}/.write-probe')
          ..writeAsStringSync('probe')
          ..deleteSync();
        if (probe.existsSync()) throw StateError('write probe survived delete');
      } catch (error) {
        outputFallbackReason = '$error';
        artifactDirectory = Directory.systemTemp.createTempSync(
          'flark-peer-supereditor-${config.runId}-',
        );
      }
    } else {
      artifactDirectory = Directory.systemTemp.createTempSync(
        'flark-peer-supereditor-${config.runId}-',
      );
    }

    final basename = _safeBasename(config.runId);
    File? exportFile;
    String? exportHash;
    if (finalSource != null) {
      exportFile = File('${artifactDirectory.path}/$basename.final-source.md');
      await exportFile.writeAsString(finalSource, flush: true);
      exportHash = await _sha256File(exportFile);
    }

    File? diffFile;
    Map<String, Object?>? difference;
    if (finalSource != null && expectedSource != null && !fidelityPass) {
      difference = _exactDifference(expectedSource, finalSource);
      diffFile = File('${artifactDirectory.path}/$basename.fidelity-diff.json');
      await diffFile.writeAsString(
        const JsonEncoder.withIndent('  ').convert(difference),
        flush: true,
      );
    }

    final timelineFile = File(
      '${artifactDirectory.path}/$basename.raw-timeline.json',
    );
    await timelineFile.writeAsString(
      const JsonEncoder.withIndent('  ').convert({
        'clock': 'dart-timeline-microseconds',
        'frames': _frames
            .map(
              (frame) => frame.toJson(
                includeCallback: config.workload == ProfileWorkload.paste,
              ),
            )
            .toList(),
        'hardwareKeyEvents': _hardwareKeyEvents,
        'inputs': _inputs.map((input) => input.toJson()).toList(),
        if (config.workload == ProfileWorkload.paste)
          'resetInputs': _resetInputs.map((input) => input.toJson()).toList(),
        if (_pasteStateContract != null)
          'pasteStateContract': _pasteStateContract,
        'rssSamples': _rssSamples,
      }),
      flush: true,
    );
    final timelineHash = await _sha256File(timelineFile);

    final measured = _inputs
        .where(
          (sample) => sample.measured && sample.inputToRasterMicros != null,
        )
        .toList();
    final measuredFramesByNumber = <int, FrameSample>{
      for (final frame
          in measured.map((sample) => sample.frame).whereType<FrameSample>())
        frame.frameNumber: frame,
    };
    final measuredFrames = measuredFramesByNumber.values.toList();
    final refreshRate = View.of(context).display.refreshRate;
    final frameBudgetMicros = refreshRate > 0
        ? (1000000 / refreshRate).round()
        : 16667;
    final allInputComplete = _allInputs.every(
      (sample) =>
          sample.failure == null &&
          sample.acceptedTimelineMicros != null &&
          sample.rasterFinishTimelineMicros != null,
    );
    final completion =
        fatalError == null &&
            exportError == null &&
            fidelityPass &&
            allInputComplete
        ? 'complete'
        : 'failed';

    final renderObject = _viewportKey.currentContext?.findRenderObject();
    final viewportSize = renderObject is RenderBox ? renderObject.size : null;
    final display = View.of(context).display;
    final result = <String, Object?>{
      'schemaVersion': 1,
      'receiptKind': 'competitor-profile-run',
      'peer': 'super_editor',
      'peerVersion': '0.3.0-dev.51',
      'peerSourceRevision': '22853bcc89def2b234017202a3f3cac36d3c088f',
      'compatibilityPatch': null,
      'compatibilityStatement':
          'Pinned tree was clean and compiled without a cache patch on the '
          'recorded toolchain.',
      'config': config.toJson(),
      'profileMode': kProfileMode,
      'protocolConformant':
          kProfileMode &&
          config.protocolId == _requiredProtocolId &&
          config.hasProtocolCounts &&
          config.hasProtocolSize &&
          config.hasProtocolTimeout,
      'completion': completion,
      'failure': {
        'fatalError': fatalError,
        'fatalStack': fatalStack,
        'exportError': exportError,
        'inputFailures': [
          for (final input in _allInputs)
            if (input.failure != null) input.toJson(),
        ],
      },
      'fixture': {
        'generatorId': 'flark-v4-deterministic-markdown-v1',
        'shapeId': 'ordinary-prose',
        'encoding': 'UTF-8',
        'normalization': 'none',
        'lineEndings': 'recipe-owned-LF',
        'targetBytes': config.targetBytes,
        'actualBytes': utf8.encode(_initialSource).length,
        'sha256': _fixtureSha256,
        'nodeMapping': 'split-LF-preserve-empty-join-LF',
      },
      if (_pasteStateContract != null)
        'pasteStateContract': _pasteStateContract,
      'coldOpen': {
        'nativeBootstrap': _nativeBootstrap,
        'timelineMinusNativeUptimeMicros': _timelineMinusNativeUptimeMicros,
        'estimatedProcessBootstrapTimelineMicros':
            _processBootstrapTimelineMicros,
        'fixtureGenerationStartMicros': _fixtureGenerationStartMicros,
        'fixtureGenerationFinishMicros': _fixtureGenerationFinishMicros,
        'documentLoadStartMicros': _documentLoadStartMicros,
        'documentModelReadyMicros': _documentModelReadyMicros,
        'firstDocumentRasterFinishMicros': _firstDocumentFrameMicros,
        'interactiveFrame': interactiveFrame?.toJson(),
        'nativeBootstrapToInteractiveRasterMicros':
            _processBootstrapTimelineMicros == null || interactiveFrame == null
            ? null
            : interactiveFrame.rasterFinishTimelineMicros -
                  _processBootstrapTimelineMicros!,
        'documentLoadToInteractiveRasterMicros': interactiveFrame == null
            ? null
            : interactiveFrame.rasterFinishTimelineMicros -
                  _documentLoadStartMicros,
        'driverProcessLaunchRequestToInteractiveRasterMicros':
            interactiveFrame == null || _driverLaunchWallMicros == null
            ? null
            : interactiveFrame.rasterFinishWallTimeMicros -
                  _driverLaunchWallMicros!,
        'endpointEvidence': {
          'focus': _focusNode.hasFocus,
          'imeConnected': _imeConnected.value,
          'viewportLogicalWidth': viewportSize?.width,
          'viewportLogicalHeight': viewportSize?.height,
          'expectedLeadingTextInRenderedModel':
              _document.first is TextNode &&
              (_document.first as TextNode).text.toPlainText().startsWith(
                'Ordinary prose opens',
              ),
          'rasterTimingReceived': interactiveFrame != null,
        },
      },
      'measurements': {
        'rawSampleCount': _inputs.length,
        if (config.workload == ProfileWorkload.paste)
          'resetInputCount': _resetInputs.length,
        'measuredSampleCount': measured.length,
        'inputToAcceptMicros': _distribution(
          measured.map((sample) => sample.inputToAcceptMicros),
        ),
        'requestToPlatformIngressMicros': _distribution(
          measured.map((sample) => sample.requestToPlatformIngressMicros),
        ),
        'acceptedToRasterMicros': _distribution(
          measured.map((sample) => sample.acceptedToRasterMicros),
        ),
        'inputToRasterMicros': _distribution(
          measured.map((sample) => sample.inputToRasterMicros),
        ),
        'buildMicros': _distribution(
          measuredFrames.map((frame) => frame.buildMicros),
        ),
        'rasterMicros': _distribution(
          measuredFrames.map((frame) => frame.rasterMicros),
        ),
        'totalSpanMicros': _distribution(
          measuredFrames.map((frame) => frame.totalSpanMicros),
        ),
        'maxInputBacklog': _maxInputBacklog,
        'frameBudgetMicros': frameBudgetMicros,
        'missedMeasuredFrames': measuredFrames
            .where((frame) => frame.totalSpanMicros > frameBudgetMicros)
            .length,
        'longestSynchronousSpan': {
          'supported': false,
          'reason':
              'FrameTiming exposes build/raster spans, not arbitrary '
              'synchronous Dart/native spans.',
        },
        'peakSampledRssBytes': _peakRss,
        'retainedRssBytes': ProcessInfo.currentRss,
      },
      'fidelity': {
        'initialSourceSha256': _fixtureSha256,
        'expectedFinalSourceSha256': expectedSource == null
            ? null
            : sha256Text(expectedSource),
        'exportedFinalSourceBytes': finalSource == null
            ? null
            : utf8.encode(finalSource).length,
        'exportedFinalSourceSha256': exportHash,
        'pass': fidelityPass,
        'exactDifference': difference == null
            ? null
            : {'path': diffFile?.path, 'sha256': await _sha256File(diffFile!)},
      },
      'artifacts': {
        'artifactDirectory': artifactDirectory.path,
        'requestedOutputDirectory': requested,
        'outputFallbackReason': outputFallbackReason,
        'rawTimeline': {'path': timelineFile.path, 'sha256': timelineHash},
        'finalExport': {'path': exportFile?.path, 'sha256': exportHash},
        'stdout': {
          'path': null,
          'sha256': null,
          'status': 'driver-must-finalize-after-process-exit',
        },
      },
      'provenance': {
        'startedAtUtc': _startedAtUtc.toIso8601String(),
        'finishedAtUtc': DateTime.now().toUtc().toIso8601String(),
        'dartRuntime': Platform.version,
        'host': config.hostProvenance,
        'build': config.buildProvenance,
        'invocation': config.invocation,
        'display': {
          'refreshRateHz': display.refreshRate,
          'devicePixelRatio': display.devicePixelRatio,
          'physicalWidth': display.size.width,
          'physicalHeight': display.size.height,
        },
      },
    };

    final resultFile = File('${artifactDirectory.path}/$basename.result.json');
    await resultFile.writeAsString(
      const JsonEncoder.withIndent('  ').convert(result),
      flush: true,
    );
    return (result: resultFile, completion: completion);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: const Color(0xfff7f7f7),
      body: Stack(
        children: [
          Center(
            child: SizedBox(
              key: _viewportKey,
              width: 600,
              height: 600,
              child: DecoratedBox(
                decoration: const BoxDecoration(color: Colors.white),
                child: SuperEditor(
                  editor: _editor,
                  focusNode: _focusNode,
                  autofocus: true,
                  isImeConnected: _imeConnected,
                  documentLayoutKey: _documentLayoutKey,
                ),
              ),
            ),
          ),
          Positioned(
            top: 2,
            left: 4,
            child: Text(_phase, style: const TextStyle(fontSize: 9)),
          ),
        ],
      ),
    );
  }

  int? get _driverLaunchWallMicros {
    final encoded = config.invocation['processLaunchRequestedAtUtc'];
    if (encoded is! String) return null;
    return DateTime.tryParse(encoded)?.microsecondsSinceEpoch;
  }
}

final class FrameSample {
  const FrameSample({
    required this.frameNumber,
    required this.vsyncStartTimelineMicros,
    required this.buildStartTimelineMicros,
    required this.buildFinishTimelineMicros,
    required this.rasterStartTimelineMicros,
    required this.rasterFinishTimelineMicros,
    required this.callbackTimelineMicros,
    required this.rasterFinishWallTimeMicros,
    required this.buildMicros,
    required this.rasterMicros,
    required this.totalSpanMicros,
  });

  factory FrameSample.fromTiming(
    FrameTiming timing, {
    required int callbackTimelineMicros,
  }) => FrameSample(
    frameNumber: timing.frameNumber,
    vsyncStartTimelineMicros: timing.timestampInMicroseconds(
      FramePhase.vsyncStart,
    ),
    buildStartTimelineMicros: timing.timestampInMicroseconds(
      FramePhase.buildStart,
    ),
    buildFinishTimelineMicros: timing.timestampInMicroseconds(
      FramePhase.buildFinish,
    ),
    rasterStartTimelineMicros: timing.timestampInMicroseconds(
      FramePhase.rasterStart,
    ),
    rasterFinishTimelineMicros: timing.timestampInMicroseconds(
      FramePhase.rasterFinish,
    ),
    callbackTimelineMicros: callbackTimelineMicros,
    rasterFinishWallTimeMicros: timing.timestampInMicroseconds(
      FramePhase.rasterFinishWallTime,
    ),
    buildMicros: timing.buildDuration.inMicroseconds,
    rasterMicros: timing.rasterDuration.inMicroseconds,
    totalSpanMicros: timing.totalSpan.inMicroseconds,
  );

  final int frameNumber;
  final int vsyncStartTimelineMicros;
  final int buildStartTimelineMicros;
  final int buildFinishTimelineMicros;
  final int rasterStartTimelineMicros;
  final int rasterFinishTimelineMicros;
  final int callbackTimelineMicros;
  final int rasterFinishWallTimeMicros;
  final int buildMicros;
  final int rasterMicros;
  final int totalSpanMicros;

  Map<String, Object?> toJson({bool includeCallback = false}) => {
    'frameNumber': frameNumber,
    'vsyncStartTimelineMicros': vsyncStartTimelineMicros,
    'buildStartTimelineMicros': buildStartTimelineMicros,
    'buildFinishTimelineMicros': buildFinishTimelineMicros,
    'rasterStartTimelineMicros': rasterStartTimelineMicros,
    'rasterFinishTimelineMicros': rasterFinishTimelineMicros,
    if (includeCallback) 'callbackTimelineMicros': callbackTimelineMicros,
    'rasterFinishWallTimeMicros': rasterFinishWallTimeMicros,
    'buildMicros': buildMicros,
    'rasterMicros': rasterMicros,
    'totalSpanMicros': totalSpanMicros,
  };
}

final class InputSample {
  InputSample({
    required this.sequence,
    required this.operation,
    required this.evidenceRole,
    required this.measured,
    required this.pair,
    required this.payloadBytes,
    required this.requestedTimelineMicros,
    required this.backlogAtRequest,
  });

  final int sequence;
  final String operation;
  final String evidenceRole;
  final bool measured;
  final int? pair;
  final int payloadBytes;
  final int requestedTimelineMicros;
  int backlogAtRequest;
  final completed = Completer<void>();
  final accepted = Completer<void>();

  Map<String, Object?> nativeEvent = const {};
  String? nativeError;
  int? platformIngressTimelineMicros;
  int? acceptedTimelineMicros;
  int? rasterFinishTimelineMicros;
  int? backlogAtAcceptance;
  int? backlogAtRaster;
  String? failure;
  FrameSample? frame;
  int? stateTransitionIndex;

  int get effectiveIngressTimelineMicros =>
      platformIngressTimelineMicros ?? requestedTimelineMicros;
  int? get requestToPlatformIngressMicros =>
      platformIngressTimelineMicros == null
      ? null
      : platformIngressTimelineMicros! - requestedTimelineMicros;
  int? get inputToAcceptMicros => acceptedTimelineMicros == null
      ? null
      : acceptedTimelineMicros! - effectiveIngressTimelineMicros;
  int? get acceptedToRasterMicros =>
      acceptedTimelineMicros == null || rasterFinishTimelineMicros == null
      ? null
      : rasterFinishTimelineMicros! - acceptedTimelineMicros!;
  int? get inputToRasterMicros => rasterFinishTimelineMicros == null
      ? null
      : rasterFinishTimelineMicros! - effectiveIngressTimelineMicros;

  void completeWith(FrameSample value, {required int backlogAtRaster}) {
    if (completed.isCompleted) return;
    frame = value;
    rasterFinishTimelineMicros = value.rasterFinishTimelineMicros;
    this.backlogAtRaster = backlogAtRaster;
    completed.complete();
  }

  void completeError(String value) {
    if (completed.isCompleted) return;
    failure = value;
    completed.complete();
  }

  Map<String, Object?> toJson() => {
    'sequence': sequence,
    'operation': operation,
    if (stateTransitionIndex != null) 'evidenceRole': evidenceRole,
    'measured': measured,
    if (stateTransitionIndex != null)
      'stateTransitionIndex': stateTransitionIndex,
    'pair': pair,
    'payloadBytes': payloadBytes,
    'requestedTimelineMicros': requestedTimelineMicros,
    'platformIngressTimelineMicros': platformIngressTimelineMicros,
    'requestToPlatformIngressMicros': requestToPlatformIngressMicros,
    'nativeEvent': nativeEvent,
    'nativeError': nativeError,
    'acceptedTimelineMicros': acceptedTimelineMicros,
    'rasterFinishTimelineMicros': rasterFinishTimelineMicros,
    'inputToAcceptMicros': inputToAcceptMicros,
    'acceptedToRasterMicros': acceptedToRasterMicros,
    'inputToRasterMicros': inputToRasterMicros,
    'backlogAtRequest': backlogAtRequest,
    'backlogAtAcceptance': backlogAtAcceptance,
    'backlogAtRaster': backlogAtRaster,
    'failure': failure,
    'frameNumber': frame?.frameNumber,
  };
}

Map<String, Object?>? _distribution(Iterable<int?> values) {
  final sorted = values.whereType<int>().toList()..sort();
  if (sorted.isEmpty) return null;
  int percentile(double fraction) {
    final rank = (fraction * sorted.length).ceil().clamp(1, sorted.length);
    return sorted[rank - 1];
  }

  return {
    'count': sorted.length,
    'p50': percentile(0.50),
    'p90': percentile(0.90),
    'p99': percentile(0.99),
    'max': sorted.last,
  };
}

/// Inserts [payload] into the frozen peer-neutral source denominator.
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

Map<String, Object?> canonicalStateDenominator(String canonicalSource) => {
  'utf8Bytes': utf8.encode(canonicalSource).length,
  'sha256': sha256Text(canonicalSource),
};

/// Proves SuperEditor's exported source against an exact canonical state.
Map<String, Object?> exactCanonicalStateProof({
  required String expectedCanonical,
  required String actualPeerSource,
}) {
  if (actualPeerSource != expectedCanonical) {
    throw StateError(
      'SuperEditor source does not match the canonical paste-state '
      'denominator: ${_exactDifference(expectedCanonical, actualPeerSource)}',
    );
  }
  return {
    'canonicalUtf8Bytes': utf8.encode(expectedCanonical).length,
    'canonicalSha256': sha256Text(expectedCanonical),
    'rawUtf8Bytes': utf8.encode(actualPeerSource).length,
    'rawSha256': sha256Text(actualPeerSource),
    'classification': 'exact',
    'matchesExpectedCanonical': true,
  };
}

String _safeBasename(String value) =>
    value.replaceAll(RegExp(r'[^A-Za-z0-9_.-]'), '_');

Future<String> _sha256File(File file) async {
  return (await sha256.bind(file.openRead()).first).toString();
}

Map<String, Object?> _exactDifference(String expected, String actual) {
  var prefix = 0;
  final shorterLength = expected.length < actual.length
      ? expected.length
      : actual.length;
  while (prefix < shorterLength &&
      expected.codeUnitAt(prefix) == actual.codeUnitAt(prefix)) {
    prefix += 1;
  }

  var suffix = 0;
  while (suffix < shorterLength - prefix &&
      expected.codeUnitAt(expected.length - suffix - 1) ==
          actual.codeUnitAt(actual.length - suffix - 1)) {
    suffix += 1;
  }
  final expectedEnd = expected.length - suffix;
  final actualEnd = actual.length - suffix;
  return {
    'encoding': 'base64-of-UTF8',
    'commonPrefixCodeUnits': prefix,
    'commonSuffixCodeUnits': suffix,
    'expectedLengthCodeUnits': expected.length,
    'actualLengthCodeUnits': actual.length,
    'expectedMiddleBase64': base64Encode(
      utf8.encode(expected.substring(prefix, expectedEnd)),
    ),
    'actualMiddleBase64': base64Encode(
      utf8.encode(actual.substring(prefix, actualEnd)),
    ),
  };
}
