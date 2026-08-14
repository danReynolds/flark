import 'dart:async';
import 'dart:convert';
import 'dart:io';

final class MacosNativeCanarySnapshot {
  const MacosNativeCanarySnapshot({
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
    required this.scrollOffset,
  });

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
  final double scrollOffset;
}

/// Thin actuator for the few macOS facts that headless Flutter cannot prove:
/// real key routing, pointer selection, clipboard shortcuts, and scrolling.
/// Product semantics remain in the ordinary transition and core tests.
final class MacosNativeCanaryDriver {
  MacosNativeCanaryDriver({
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
  Map<String, Object?>? _lastRawSnapshot;

  String get debugLastReceipt =>
      const JsonEncoder.withIndent('  ').convert(_lastRawSnapshot);

  Future<MacosNativeCanarySnapshot> reset({
    required String id,
    required String source,
  }) async {
    if (_process == null) await _startActuator();
    _paintObservationStart = 0;
    return _snapshot(
      await _request('reset', {'canaryId': id, 'source': source}),
    );
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

  Future<void> activateAtUtf16(int offset) async {
    final response = await _request('activateAtUtf16', {'utf16Offset': offset});
    final json = (response['snapshot']! as Map).cast<String, Object?>();
    _paintObservationStart = (json['surfaceFrames']! as List).length;
  }

  void beginPaintObservation() => _paintObservationStart = 0;

  Future<void> typeText(
    String text, {
    Duration cadence = const Duration(milliseconds: 2),
  }) => _request('insertText', {
    'text': text,
    'cadenceMs': cadence.inMilliseconds,
  });

  Future<void> pressKey(String key) => _request('key', {'key': key});

  Future<void> selectSourceRange({required int base, required int extent}) =>
      _request('selectSourceRange', {'baseUtf16': base, 'extentUtf16': extent});

  Future<void> pasteText(String text) => _request('pasteText', {'text': text});

  Future<void> scrollBy(int deltaY) => _request('scrollBy', {'deltaY': deltaY});

  Future<MacosNativeCanarySnapshot> settle() async =>
      _snapshot(await _request('settle'));

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
      const Duration(seconds: 20),
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
    return response;
  }

  MacosNativeCanarySnapshot _snapshot(Map<String, Object?> response) {
    final json = (response['snapshot']! as Map).cast<String, Object?>();
    _lastRawSnapshot = json;
    return MacosNativeCanarySnapshot(
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
      scrollOffset: (json['scrollOffset']! as num).toDouble(),
    );
  }
}
