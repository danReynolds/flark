import 'dart:async';
import 'dart:convert';
import 'dart:io';

final class MacosNativeCanarySnapshot {
  const MacosNativeCanarySnapshot({
    required this.canaryId,
    required this.commandSequence,
    required this.source,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.resyncCount,
    required this.lastResyncReason,
    required this.faulted,
    required this.lastError,
    required this.settledPresentation,
    required this.paintedPresentations,
    required this.revision,
    required this.sourceGeneration,
    required this.scrollOffset,
    required this.paintedCaretIdentities,
    required this.paintedSourceGenerations,
    required this.paintedSelectionBases,
    required this.paintedSelectionExtents,
    required this.paintedCaretSources,
    required this.paintedCaretDisplays,
    required this.paintedVisibleSources,
    required this.paintedStyledTexts,
    required this.paintReceipts,
    required this.frameTimingReceipts,
    required this.sourceEditPerformanceReceipts,
    required this.semanticEditPerformanceReceipts,
    required this.inputEvents,
    required this.processLaunchEpochMicros,
    required this.openAcceptedEpochMicros,
    required this.currentRssBytes,
    required this.maximumRssBytes,
    required this.display,
    required this.appProcessId,
    required this.receiptEpochMicros,
    required this.frameClockAnchor,
  });

  final String canaryId;
  final int commandSequence;
  final String source;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final int resyncCount;
  final String lastResyncReason;
  final bool faulted;
  final Object? lastError;
  final String settledPresentation;
  final List<String> paintedPresentations;
  final int revision;
  final int sourceGeneration;
  final double scrollOffset;
  final List<bool> paintedCaretIdentities;
  final List<int> paintedSourceGenerations;
  final List<int> paintedSelectionBases;
  final List<int> paintedSelectionExtents;
  final List<int?> paintedCaretSources;
  final List<int?> paintedCaretDisplays;
  final List<String> paintedVisibleSources;
  final List<List<String>> paintedStyledTexts;
  final List<Map<String, Object?>> paintReceipts;
  final List<Map<String, Object?>> frameTimingReceipts;
  final List<Map<String, Object?>> sourceEditPerformanceReceipts;
  final List<Map<String, Object?>> semanticEditPerformanceReceipts;
  final List<String> inputEvents;
  final int? processLaunchEpochMicros;
  final int? openAcceptedEpochMicros;
  final int currentRssBytes;
  final int maximumRssBytes;
  final Map<String, Object?> display;
  final int appProcessId;
  final int receiptEpochMicros;
  final Map<String, int> frameClockAnchor;
}

/// Thin actuator for the few macOS facts that headless Flutter cannot prove:
/// real key routing, pointer selection, clipboard shortcuts, and scrolling.
/// Product semantics remain in the ordinary transition and core tests.
final class MacosNativeCanaryDriver {
  MacosNativeCanaryDriver({
    required this.appExecutable,
    required this.libraryPath,
    required this.actuatorScript,
    this.initialPresetName,
    this.initialWindowWidth,
    this.initialWindowHeight,
  });

  final String appExecutable;
  final String libraryPath;
  final String actuatorScript;
  final String? initialPresetName;
  final int? initialWindowWidth;
  final int? initialWindowHeight;
  Process? _process;
  StreamIterator<String>? _responses;
  int _sequence = 0;
  int _paintObservationStart = 0;
  int _paintReceiptStart = 0;
  int _frameTimingStart = 0;
  int _sourcePerformanceStart = 0;
  int _semanticPerformanceStart = 0;
  int _inputEventStart = 0;
  int _windowWidth = 800;
  int _windowHeight = 632;
  Map<String, Object?>? _lastRawSnapshot;
  final List<Map<String, Object?>> _appAcknowledgements = [];
  final List<Map<String, Object?>> _inputDeliveryAcknowledgements = [];

  int get commandSequence => _sequence;
  int get appAcknowledgementCount => _appAcknowledgements.length;

  List<Map<String, Object?>> appAcknowledgementsSince(int index) =>
      List<Map<String, Object?>>.unmodifiable(
        _appAcknowledgements.skip(index).map(Map<String, Object?>.unmodifiable),
      );

  int get inputDeliveryAcknowledgementCount =>
      _inputDeliveryAcknowledgements.length;

  List<Map<String, Object?>> inputDeliveryAcknowledgementsSince(int index) =>
      List<Map<String, Object?>>.unmodifiable(
        _inputDeliveryAcknowledgements
            .skip(index)
            .map(Map<String, Object?>.unmodifiable),
      );

  String get debugLastReceipt =>
      const JsonEncoder.withIndent('  ').convert(_lastRawSnapshot);

  String get debugLastInputEvents {
    final events = _lastRawSnapshot?['inputEvents'];
    if (events is! List) return 'no input events';
    return events.skip(events.length > 24 ? events.length - 24 : 0).join('\n');
  }

  Future<MacosNativeCanarySnapshot> start() async {
    if (_process == null) await _startActuator();
    return _snapshot(await _request('settle'));
  }

  Future<MacosNativeCanarySnapshot> prepareObservationWindow({
    required int windowWidth,
    required int windowHeight,
  }) async {
    if (_process == null) await _startActuator();
    _windowWidth = windowWidth;
    _windowHeight = windowHeight;
    _paintObservationStart = 0;
    _paintReceiptStart = 0;
    _frameTimingStart = 0;
    _sourcePerformanceStart = 0;
    _semanticPerformanceStart = 0;
    _inputEventStart = 0;
    return _snapshot(
      await _request('prepareObservationWindow', {
        'windowWidth': windowWidth,
        'windowHeight': windowHeight,
      }),
    );
  }

  Future<MacosNativeCanarySnapshot> reset({
    required String id,
    required String source,
  }) async {
    if (_process == null) await _startActuator();
    _paintObservationStart = 0;
    _paintReceiptStart = 0;
    _frameTimingStart = 0;
    _sourcePerformanceStart = 0;
    _semanticPerformanceStart = 0;
    _inputEventStart = 0;
    return _snapshot(
      await _request('reset', {'canaryId': id, 'source': source}),
    );
  }

  Future<MacosNativeCanarySnapshot> selectPreset(String presetName) async {
    if (_process == null) await _startActuator();
    _paintObservationStart = 0;
    _paintReceiptStart = 0;
    _frameTimingStart = 0;
    _sourcePerformanceStart = 0;
    _semanticPerformanceStart = 0;
    _inputEventStart = 0;
    return _snapshot(
      await _request('selectPreset', {'presetName': presetName}),
    );
  }

  Future<void> _startActuator() async {
    final process = await Process.start(
      'swift',
      [actuatorScript, appExecutable, libraryPath, ?initialPresetName],
      environment: {
        if (initialWindowWidth != null)
          'FLARK_CANARY_INITIAL_WINDOW_WIDTH': '$initialWindowWidth',
        if (initialWindowHeight != null)
          'FLARK_CANARY_INITIAL_WINDOW_HEIGHT': '$initialWindowHeight',
      },
    );
    _process = process;
    _responses = StreamIterator(
      process.stdout.transform(utf8.decoder).transform(const LineSplitter()),
    );
    unawaited(
      process.stderr
          .transform(utf8.decoder)
          .forEach((chunk) => stderr.write(chunk)),
    );
  }

  Future<MacosNativeCanarySnapshot> activateAtUtf16(
    int offset, {
    int windowWidth = 800,
    int windowHeight = 632,
    bool retainObservations = false,
  }) async {
    _windowWidth = windowWidth;
    _windowHeight = windowHeight;
    final response = await _request('activateAtUtf16', {
      'utf16Offset': offset,
      'windowWidth': windowWidth,
      'windowHeight': windowHeight,
    });
    final snapshot = _snapshot(response);
    final json = _lastRawSnapshot!;
    if (!retainObservations) {
      _paintObservationStart = (json['surfaceFrames']! as List).length;
      _paintReceiptStart = (json['paintReceipts']! as List).length;
      _frameTimingStart = (json['frameTimingReceipts']! as List).length;
      _sourcePerformanceStart =
          (json['sourceEditPerformanceReceipts']! as List).length;
      _semanticPerformanceStart =
          (json['semanticEditPerformanceReceipts']! as List).length;
      _inputEventStart = (json['inputEvents']! as List).length;
    }
    return snapshot;
  }

  void beginPaintObservation() => _paintObservationStart = 0;

  Future<void> typeText(
    String text, {
    Duration cadence = const Duration(milliseconds: 2),
  }) {
    final selection = _expectedSelectionArguments();
    return _request('insertText', {
      'text': text,
      'cadenceMicros': cadence.inMicroseconds,
      ...selection,
    });
  }

  Future<void> pressKey(String key) =>
      _request('key', {'key': key, ..._expectedSelectionArguments()});

  Future<void> repeatKey(
    String key, {
    required int count,
    required Duration cadence,
  }) => _request('repeatKey', {
    'key': key,
    'count': count,
    'cadenceMicros': cadence.inMicroseconds,
    ..._expectedSelectionArguments(),
  });

  Future<void> typeStructuralBursts({
    required int count,
    required Duration cadence,
  }) => _request('structuralBursts', {
    'count': count,
    'cadenceMicros': cadence.inMicroseconds,
    ..._expectedSelectionArguments(),
  });

  Map<String, Object?> _expectedSelectionArguments() {
    final snapshot = _lastRawSnapshot;
    if (snapshot == null) return const {};
    return {
      'expectedBaseUtf16': snapshot['selectionBaseUtf16'],
      'expectedExtentUtf16': snapshot['selectionExtentUtf16'],
      'windowWidth': _windowWidth,
      'windowHeight': _windowHeight,
    };
  }

  Future<void> selectSourceRange({
    required int base,
    required int extent,
  }) async {
    await _request('selectSourceRange', {
      'baseUtf16': base,
      'extentUtf16': extent,
    });
    _snapshot(await _request('settle'));
  }

  Future<void> pasteText(String text) =>
      _request('pasteText', {'text': text, ..._expectedSelectionArguments()});

  Future<void> scrollBy(int deltaY) => _request('scrollBy', {'deltaY': deltaY});

  Future<MacosNativeCanarySnapshot> settle() async =>
      _snapshot(await _request('settle'));

  Future<Map<String, Object?>> closeSession() async {
    final response = await _request('closeSession');
    return Map<String, Object?>.unmodifiable(
      (response['snapshot']! as Map).cast<String, Object?>(),
    );
  }

  Future<void> close() async {
    final process = _process;
    final responses = _responses;
    _process = null;
    _responses = null;
    if (process == null) return;
    try {
      await _requestOn(process, responses!, 'stop');
      await process.stdin.close();
      await process.exitCode.timeout(const Duration(seconds: 5));
    } on Object {
      process.kill();
      await process.exitCode.timeout(
        const Duration(seconds: 2),
        onTimeout: () => -1,
      );
    } finally {
      await responses?.cancel();
    }
  }

  Future<Map<String, Object?>> _request(
    String operation, [
    Map<String, Object?> arguments = const {},
  ]) {
    final process =
        _process ?? (throw StateError('macOS canary driver is not started'));
    return _requestOn(process, _responses!, operation, arguments);
  }

  Future<Map<String, Object?>> _requestOn(
    Process process,
    StreamIterator<String> responses,
    String operation, [
    Map<String, Object?> arguments = const {},
  ]) async {
    final sequence = ++_sequence;
    process.stdin.writeln(
      jsonEncode({
        'sequence': sequence,
        'operation': operation,
        'arguments': arguments,
      }),
    );
    await process.stdin.flush();
    final hasResponse = await responses.moveNext().timeout(
      const Duration(seconds: 70),
    );
    if (!hasResponse) {
      throw StateError('macOS actuator exited during $operation');
    }
    final response = jsonDecode(responses.current) as Map<String, Object?>;
    if (response['sequence'] != sequence) {
      throw StateError(
        'macOS actuator response ${response['sequence']} did not match '
        '$sequence',
      );
    }
    if (response['ok'] != true) {
      throw StateError(
        'macOS actuator $operation failed: ${response['error']}',
      );
    }
    if (response['inputDeliveryAcknowledgement'] case final Map value) {
      final acknowledgement = value.cast<String, Object?>();
      final acknowledgedOperation = acknowledgement['operation'];
      if (acknowledgedOperation is! String ||
          (acknowledgedOperation != operation &&
              !acknowledgedOperation.startsWith('$operation:')) ||
          acknowledgement['terminalInputEventOrdinal'] is! int ||
          acknowledgement['baselineInputEventOrdinal'] is! int ||
          acknowledgement['terminalSourceGeneration'] is! int ||
          acknowledgement['baselineSourceGeneration'] is! int ||
          acknowledgement['expectedGenerationAdvance'] is! int ||
          acknowledgement['terminalEvent'] is! String) {
        throw StateError(
          'macOS actuator $operation returned an invalid input delivery '
          'acknowledgement',
        );
      }
      _inputDeliveryAcknowledgements.add({
        'actuatorSequence': sequence,
        ...acknowledgement,
      });
    }
    final snapshot = response['snapshot'];
    if (snapshot is Map) {
      final json = snapshot.cast<String, Object?>();
      final appCommandSequence = json['commandSequence'];
      final canaryId = json['canaryId'];
      if (appCommandSequence is! int ||
          canaryId is! String ||
          canaryId.isEmpty) {
        throw StateError(
          'macOS actuator $operation returned an invalid app acknowledgement',
        );
      }
      _appAcknowledgements.add({
        'actuatorSequence': sequence,
        'operation': operation,
        'appCommandSequence': appCommandSequence,
        'canaryId': canaryId,
      });
    }
    return response;
  }

  MacosNativeCanarySnapshot _snapshot(Map<String, Object?> response) {
    final json = (response['snapshot']! as Map).cast<String, Object?>();
    _lastRawSnapshot = json;
    return MacosNativeCanarySnapshot(
      canaryId: json['canaryId']! as String,
      commandSequence: json['commandSequence']! as int,
      source: json['source']! as String,
      selectionBaseUtf16: json['selectionBaseUtf16']! as int,
      selectionExtentUtf16: json['selectionExtentUtf16']! as int,
      resyncCount: json['resyncCount']! as int,
      lastResyncReason: json['lastResyncReason']! as String,
      faulted: json['status'] == 'faulted',
      lastError: json['lastError'],
      settledPresentation: json['settledPresentation']! as String,
      paintedPresentations: List<String>.unmodifiable(
        (json['surfaceFrames']! as List).cast<String>().skip(
          _paintObservationStart,
        ),
      ),
      revision: json['revision']! as int,
      sourceGeneration: json['sourceGeneration']! as int,
      scrollOffset: (json['scrollOffset']! as num).toDouble(),
      paintedCaretIdentities: List<bool>.unmodifiable(
        (json['surfaceCaretIdentities']! as List).cast<bool>().skip(
          _paintObservationStart,
        ),
      ),
      paintedSourceGenerations: List<int>.unmodifiable(
        (json['surfaceSourceGenerations']! as List).cast<int>().skip(
          _paintObservationStart,
        ),
      ),
      paintedSelectionBases: List<int>.unmodifiable(
        (json['surfaceSelectionBases']! as List).cast<int>().skip(
          _paintObservationStart,
        ),
      ),
      paintedSelectionExtents: List<int>.unmodifiable(
        (json['surfaceSelectionExtents']! as List).cast<int>().skip(
          _paintObservationStart,
        ),
      ),
      paintedCaretSources: List<int?>.unmodifiable(
        (json['surfaceCaretSources']! as List).cast<int?>().skip(
          _paintObservationStart,
        ),
      ),
      paintedCaretDisplays: List<int?>.unmodifiable(
        (json['surfaceCaretDisplays']! as List).cast<int?>().skip(
          _paintObservationStart,
        ),
      ),
      paintedVisibleSources: List<String>.unmodifiable(
        (json['surfaceVisibleSources']! as List).cast<String>().skip(
          _paintObservationStart,
        ),
      ),
      paintedStyledTexts: List<List<String>>.unmodifiable(
        (json['surfaceStyledTexts']! as List)
            .skip(_paintObservationStart)
            .map(
              (styles) =>
                  List<String>.unmodifiable((styles as List).cast<String>()),
            ),
      ),
      paintReceipts: _objects(json, 'paintReceipts', skip: _paintReceiptStart),
      frameTimingReceipts: _objects(
        json,
        'frameTimingReceipts',
        skip: _frameTimingStart,
      ),
      sourceEditPerformanceReceipts: _objects(
        json,
        'sourceEditPerformanceReceipts',
        skip: _sourcePerformanceStart,
      ),
      semanticEditPerformanceReceipts: _objects(
        json,
        'semanticEditPerformanceReceipts',
        skip: _semanticPerformanceStart,
      ),
      inputEvents: List<String>.unmodifiable(
        (json['inputEvents']! as List).cast<String>().skip(_inputEventStart),
      ),
      processLaunchEpochMicros: json['processLaunchEpochMicros'] as int?,
      openAcceptedEpochMicros: json['openAcceptedEpochMicros'] as int?,
      currentRssBytes: json['currentRssBytes']! as int,
      maximumRssBytes: json['maximumRssBytes']! as int,
      display: Map<String, Object?>.unmodifiable(
        (json['display']! as Map).cast<String, Object?>(),
      ),
      appProcessId: response['appPid']! as int,
      receiptEpochMicros: json['receiptEpochMicros']! as int,
      frameClockAnchor: Map<String, int>.unmodifiable(
        (json['frameClockAnchor']! as Map).cast<String, int>(),
      ),
    );
  }

  static List<Map<String, Object?>> _objects(
    Map<String, Object?> json,
    String name, {
    int skip = 0,
  }) => List<Map<String, Object?>>.unmodifiable(
    (json[name]! as List)
        .skip(skip)
        .map(
          (value) => Map<String, Object?>.unmodifiable(
            (value as Map).cast<String, Object?>(),
          ),
        ),
  );
}
