import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'live_editor_scenario.dart';
import 'live_editor_scenario_executor.dart';

final class MacosNativeLiveEditorScenarioDriver
    implements LiveEditorScenarioDriver {
  MacosNativeLiveEditorScenarioDriver({
    required this.appExecutable,
    required this.libraryPath,
    required this.actuatorScript,
  });

  final String appExecutable;
  final String libraryPath;
  final String actuatorScript;
  Process? _process;
  StreamIterator<String>? _responses;
  int _sequence = 0;
  int _paintObservationStart = 0;
  LiveEditorScenarioSnapshot? _lastSnapshot;
  Map<String, Object?>? _lastRawSnapshot;

  String get debugLastReceipt =>
      const JsonEncoder.withIndent('  ').convert(_lastRawSnapshot);

  @override
  String get name => 'macos-native';

  @override
  bool get observesPaint => true;

  @override
  bool get observesScroll => true;

  @override
  Future<void> start(LiveEditorScenarioPlan plan) async {
    if (_process == null) await _startActuator();
    final response = await _request('reset', {
      'scenarioId': plan.qualifiedId,
      'source': plan.initialSource,
    });
    _paintObservationStart = 0;
    _lastSnapshot = _snapshot(response);
  }

  Future<void> _startActuator() async {
    final process = await Process.start('swift', [
      actuatorScript,
      appExecutable,
      libraryPath,
    ]);
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

  @override
  Future<void> activateAtUtf16(int offset) async {
    final response = await _request('activateAtUtf16', {'utf16Offset': offset});
    final json = (response['snapshot']! as Map).cast<String, Object?>();
    _paintObservationStart = (json['surfaceFrames']! as List).length;
    _lastSnapshot = null;
  }

  @override
  Future<void> insertText(String text, {required Duration cadence}) async {
    await _request('insertText', {
      'text': text,
      'cadenceMs': cadence.inMilliseconds,
    });
  }

  @override
  Future<void> pressKey(LiveEditorScenarioKey key) async {
    await _request('key', {'key': key.name});
  }

  @override
  Future<void> selectSourceRange({required int base, required int extent}) {
    return _request('selectSourceRange', {
      'baseUtf16': base,
      'extentUtf16': extent,
    });
  }

  @override
  Future<void> pasteText(String text) {
    return _request('pasteText', {'text': text});
  }

  @override
  Future<void> toggleTaskAtUtf16(int targetUtf16) {
    return _request('toggleTaskAtUtf16', {'targetUtf16': targetUtf16});
  }

  @override
  Future<void> scrollBy(int deltaY) {
    return _request('scrollBy', {'deltaY': deltaY});
  }

  @override
  Future<void> pause(Duration duration) async {
    await _request('pause', {'milliseconds': duration.inMilliseconds});
  }

  @override
  Future<void> awaitBarrier(LiveEditorScenarioBarrier barrier) async {
    final response = await _request('settle');
    _lastSnapshot = _snapshot(response);
  }

  @override
  Future<LiveEditorScenarioSnapshot> snapshot() async {
    if (_lastSnapshot case final snapshot?) return snapshot;
    final response = await _request('settle');
    return _lastSnapshot = _snapshot(response);
  }

  @override
  Future<void> stop() async {
    _lastSnapshot = null;
  }

  /// Ends the one app process shared by all plans in a native canary run.
  Future<void> close() async {
    final process = _process;
    final responses = _responses;
    _process = null;
    _responses = null;
    _lastSnapshot = null;
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
    }
  }

  Future<Map<String, Object?>> _request(
    String operation, [
    Map<String, Object?> arguments = const {},
  ]) {
    final process =
        _process ?? (throw StateError('macOS scenario driver is not started'));
    final responses = _responses!;
    return _requestOn(process, responses, operation, arguments);
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
      const Duration(seconds: 20),
    );
    if (!hasResponse) {
      throw StateError('macOS actuator exited during $operation');
    }
    final response = jsonDecode(responses.current) as Map<String, Object?>;
    if (response['sequence'] != sequence) {
      throw StateError(
        'macOS actuator response sequence ${response['sequence']} '
        'did not match $sequence',
      );
    }
    if (response['ok'] != true) {
      throw StateError(
        'macOS actuator $operation failed: ${response['error']}',
      );
    }
    return response;
  }

  LiveEditorScenarioSnapshot _snapshot(Map<String, Object?> response) {
    final json = (response['snapshot']! as Map).cast<String, Object?>();
    _lastRawSnapshot = json;
    return LiveEditorScenarioSnapshot(
      source: json['source']! as String,
      selectionBaseUtf16: json['selectionBaseUtf16']! as int,
      selectionExtentUtf16: json['selectionExtentUtf16']! as int,
      resyncCount: json['resyncCount']! as int,
      faulted: json['status'] == 'faulted',
      lastError: json['lastError'],
      settledPresentation: json['settledPresentation']! as String,
      paintedPresentations: List<String>.unmodifiable(
        (json['surfaceFrames']! as List).cast<String>().skip(
          _paintObservationStart,
        ),
      ),
      paintedRenderPlanHashes: List<int>.unmodifiable(
        ((json['surfaceFrameHashes'] as List?) ?? const <Object?>[])
            .cast<int>()
            .skip(_paintObservationStart),
      ),
      paintedVisualStateHashes: List<int>.unmodifiable(
        ((json['surfaceVisualStateHashes'] as List?) ?? const <Object?>[])
            .cast<int>()
            .skip(_paintObservationStart),
      ),
      revision: json['revision']! as int,
      scrollOffset: (json['scrollOffset']! as num).toDouble(),
    );
  }
}
