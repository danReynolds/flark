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
  int _windowWidth = 800;
  int _windowHeight = 632;
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

  Future<MacosNativeCanarySnapshot> activateAtUtf16(
    int offset, {
    int windowWidth = 800,
    int windowHeight = 632,
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
    _paintObservationStart = (json['surfaceFrames']! as List).length;
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
      'cadenceMs': cadence.inMilliseconds,
      ...selection,
    });
  }

  Future<void> pressKey(String key) =>
      _request('key', {'key': key, ..._expectedSelectionArguments()});

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
    );
  }
}
