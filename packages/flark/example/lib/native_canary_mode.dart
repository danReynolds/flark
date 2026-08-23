import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' show FramePhase, FrameTiming;

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';

final class DogfoodNativeCanaryMode {
  const DogfoodNativeCanaryMode({
    required this.receiptPath,
    required this.commandPath,
    required this.initialPresetName,
    required this.processLaunchEpochMicros,
  });

  static DogfoodNativeCanaryMode? fromEnvironment() {
    const receiptAtBuild = String.fromEnvironment('FLARK_CANARY_RECEIPT_PATH');
    const commandAtBuild = String.fromEnvironment('FLARK_CANARY_COMMAND_PATH');
    final receiptPath = receiptAtBuild.isNotEmpty
        ? receiptAtBuild
        : Platform.environment['FLARK_CANARY_RECEIPT_PATH'];
    if (receiptPath == null) return null;
    final commandPath = commandAtBuild.isNotEmpty
        ? commandAtBuild
        : Platform.environment['FLARK_CANARY_COMMAND_PATH'];
    if (commandPath == null) return null;
    final initialPresetName =
        Platform.environment['FLARK_CANARY_INITIAL_PRESET'];
    final processLaunchEpochMicros = int.tryParse(
      Platform.environment['FLARK_CANARY_PROCESS_LAUNCH_EPOCH_MICROS'] ?? '',
    );
    return DogfoodNativeCanaryMode(
      receiptPath: receiptPath,
      commandPath: commandPath,
      initialPresetName: initialPresetName,
      processLaunchEpochMicros: processLaunchEpochMicros,
    );
  }

  String get source => '';
  final String receiptPath;
  final String commandPath;
  final String? initialPresetName;
  final int? processLaunchEpochMicros;
}

final class DogfoodNativeCanaryCommand {
  DogfoodNativeCanaryCommand.fromJson(Map<String, Object?> json)
    : sequence = json['sequence']! as int,
      operation = json['operation']! as String,
      arguments = Map<String, Object?>.unmodifiable(
        (json['arguments']! as Map).cast<String, Object?>(),
      );

  final int sequence;
  final String operation;
  final Map<String, Object?> arguments;
}

/// One opt-in command slot for the real macOS actuator.
final class DogfoodNativeCanaryCommandMailbox {
  DogfoodNativeCanaryCommandMailbox({
    required this.path,
    required this.onCommand,
    required this.onError,
  });

  final String path;
  final Future<void> Function(DogfoodNativeCanaryCommand command) onCommand;
  final Future<void> Function(int sequence, Object error) onError;
  Timer? _timer;
  bool _polling = false;
  int _lastSequence = 0;

  void start() {
    _timer ??= Timer.periodic(
      const Duration(milliseconds: 20),
      (_) => unawaited(_poll()),
    );
  }

  void dispose() {
    _timer?.cancel();
    _timer = null;
  }

  Future<void> _poll() async {
    if (_polling) return;
    _polling = true;
    try {
      final file = File(path);
      if (!await file.exists()) return;
      final json = jsonDecode(await file.readAsString());
      if (json is! Map<String, Object?>) return;
      final command = DogfoodNativeCanaryCommand.fromJson(json);
      if (command.sequence <= _lastSequence) return;
      _lastSequence = command.sequence;
      try {
        await onCommand(command);
      } catch (error) {
        await onError(command.sequence, error);
      }
    } on FormatException {
      // The actuator writes atomically. A malformed request is ignored rather
      // than allowed to perturb the product isolate under test.
    } finally {
      _polling = false;
    }
  }
}

/// Opt-in dogfood instrumentation for native canaries. It retains
/// bounded observations in memory and publishes atomic receipts outside the
/// product input callback itself.
final class DogfoodNativeCanaryReceiptWriter {
  DogfoodNativeCanaryReceiptWriter(this.mode) {
    WidgetsBinding.instance.addTimingsCallback(_recordFrameTimings);
  }

  final DogfoodNativeCanaryMode mode;
  final List<String> _surfaceFrames = [];
  final List<int> _surfaceFrameHashes = [];
  final List<int> _surfaceVisualStateHashes = [];
  final List<bool> _surfaceCaretIdentities = [];
  final List<int> _surfaceSourceGenerations = [];
  final List<int> _surfaceSelectionBases = [];
  final List<int> _surfaceSelectionExtents = [];
  final List<int?> _surfaceCaretSources = [];
  final List<int?> _surfaceCaretDisplays = [];
  final List<String> _surfaceVisibleSources = [];
  final List<List<String>> _surfaceStyledTexts = [];
  final List<Map<String, Object?>> _paintReceipts = [];
  final List<Map<String, Object?>> _frameTimingReceipts = [];
  final List<String> _inputEvents = [];
  FlarkEditorController? _controller;
  Timer? _timer;
  int _writeGeneration = 0;
  bool _frameScheduled = false;
  String _canaryId = 'native-canary';
  String _settledPresentation = '<empty>';
  int _commandSequence = 0;
  Object? _commandError;
  int? _sourcePointOffset;
  FlarkEditorDebugGeometry? _sourcePointGeometry;
  int? _taskActionTarget;
  FlarkEditorDebugGeometry? _taskActionGeometry;
  DateTime? _lastInputEventAt;
  double _lastScrollOffset = 0;

  void beginCanary(String id) {
    _canaryId = id;
    _surfaceFrames.clear();
    _surfaceFrameHashes.clear();
    _surfaceVisualStateHashes.clear();
    _surfaceCaretIdentities.clear();
    _surfaceSourceGenerations.clear();
    _surfaceSelectionBases.clear();
    _surfaceSelectionExtents.clear();
    _surfaceCaretSources.clear();
    _surfaceCaretDisplays.clear();
    _surfaceVisibleSources.clear();
    _surfaceStyledTexts.clear();
    _paintReceipts.clear();
    _frameTimingReceipts.clear();
    _inputEvents.clear();
    _settledPresentation = '<empty>';
    _commandError = null;
    _sourcePointOffset = null;
    _sourcePointGeometry = null;
    _taskActionTarget = null;
    _taskActionGeometry = null;
    _lastInputEventAt = null;
    _lastScrollOffset = 0;
  }

  void attach(FlarkEditorController controller) {
    detach();
    _controller = controller;
    controller.addListener(_record);
    _record();
  }

  void detach() {
    _timer?.cancel();
    _timer = null;
    _controller?.removeListener(_record);
    _controller = null;
  }

  void dispose() {
    WidgetsBinding.instance.removeTimingsCallback(_recordFrameTimings);
    detach();
  }

  void _recordFrameTimings(List<FrameTiming> timings) {
    for (final timing in timings) {
      if (_frameTimingReceipts.length == 1024) {
        _frameTimingReceipts.removeAt(0);
      }
      _frameTimingReceipts.add({
        'frameNumber': timing.frameNumber,
        'vsyncStartMicros': timing.timestampInMicroseconds(
          FramePhase.vsyncStart,
        ),
        'buildStartMicros': timing.timestampInMicroseconds(
          FramePhase.buildStart,
        ),
        'buildFinishMicros': timing.timestampInMicroseconds(
          FramePhase.buildFinish,
        ),
        'rasterStartMicros': timing.timestampInMicroseconds(
          FramePhase.rasterStart,
        ),
        'rasterFinishMicros': timing.timestampInMicroseconds(
          FramePhase.rasterFinish,
        ),
        'buildMicros': timing.buildDuration.inMicroseconds,
        'rasterMicros': timing.rasterDuration.inMicroseconds,
        'totalSpanMicros': timing.totalSpan.inMicroseconds,
      });
    }
  }

  void recordInputEvent(String event) {
    _lastInputEventAt = DateTime.now();
    if (_inputEvents.length == 1024) _inputEvents.removeAt(0);
    _inputEvents.add('${DateTime.now().microsecondsSinceEpoch}:$event');
    _record();
  }

  /// Waits for native delivery and any async selector work to enter the
  /// controller before the harness evaluates the authoritative edit barrier.
  Future<void> waitForPlatformInputQuiescence() async {
    final notBefore = DateTime.now().add(const Duration(milliseconds: 50));
    while (true) {
      final now = DateTime.now();
      final lastInput = _lastInputEventAt;
      final quiet =
          lastInput == null ||
          now.difference(lastInput) >= const Duration(milliseconds: 50);
      if (!now.isBefore(notBefore) && quiet) return;
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }
  }

  void recordPaintObservation(FlarkSurfacePaintObservation observation) {
    _lastScrollOffset = observation.scrollOffset;
    if (_paintReceipts.length == 1024) _paintReceipts.removeAt(0);
    _paintReceipts.add({
      'paintEpochMicros': DateTime.now().microsecondsSinceEpoch,
      'frameStampMicros':
          WidgetsBinding.instance.currentSystemFrameTimeStamp.inMicroseconds,
      'revision': observation.revision,
      'sourceGeneration': observation.sourceGeneration,
      'semanticsCurrent': observation.semanticsCurrent,
      'viewportPageIndex': observation.viewportPageIndex,
      'visibleUtf16Start': observation.visibleUtf16Start,
      'visibleUtf16Length': observation.visibleUtf16Length,
      'visibleSourceSha256': flarkWindowTextSha256(observation.visibleSource),
      'renderPlanHash': observation.renderPlanHash,
      'visualStateHash': observation.visualStateHash,
      'canonicalSelectionBaseUtf16': observation.canonicalSelectionBaseUtf16,
      'canonicalSelectionExtentUtf16':
          observation.canonicalSelectionExtentUtf16,
      'caretSourceUtf16': observation.caretSourceUtf16,
      'caretDisplayUtf16': observation.caretDisplayUtf16,
      'selectionRectCount': observation.selectionRects.length,
      'neutralRowCount': observation.rows.where((row) => row.neutral).length,
      'activeNeutralRowCount': observation.rows
          .where((row) => row.active && row.neutral)
          .length,
      'exactRunRanges': observation.rows
          .expand((row) => row.runs)
          .where((run) => run.sourceExact)
          .map(
            (run) => {
              'startUtf16': run.sourceUtf16Start,
              'endUtf16': run.sourceUtf16End,
            },
          )
          .toList(growable: false),
    });
    final surface = observation.presentation;
    if (_surfaceFrames.isEmpty ||
        _surfaceFrames.last != surface ||
        _surfaceFrameHashes.last != observation.renderPlanHash ||
        _surfaceVisualStateHashes.last != observation.visualStateHash ||
        _surfaceSourceGenerations.last != observation.sourceGeneration ||
        _surfaceSelectionBases.last !=
            observation.canonicalSelectionBaseUtf16 ||
        _surfaceSelectionExtents.last !=
            observation.canonicalSelectionExtentUtf16 ||
        _surfaceCaretSources.last != observation.caretSourceUtf16 ||
        _surfaceCaretDisplays.last != observation.caretDisplayUtf16 ||
        _surfaceVisibleSources.last != observation.visibleSource) {
      if (_surfaceFrames.length == 1024) {
        _surfaceFrames.removeAt(0);
        _surfaceFrameHashes.removeAt(0);
        _surfaceVisualStateHashes.removeAt(0);
        _surfaceCaretIdentities.removeAt(0);
        _surfaceSourceGenerations.removeAt(0);
        _surfaceSelectionBases.removeAt(0);
        _surfaceSelectionExtents.removeAt(0);
        _surfaceCaretSources.removeAt(0);
        _surfaceCaretDisplays.removeAt(0);
        _surfaceVisibleSources.removeAt(0);
        _surfaceStyledTexts.removeAt(0);
      }
      _surfaceFrames.add(surface);
      _surfaceFrameHashes.add(observation.renderPlanHash);
      _surfaceVisualStateHashes.add(observation.visualStateHash);
      final hasVisibleActiveRow = observation.rows.any((row) => row.active);
      _surfaceCaretIdentities.add(
        (!hasVisibleActiveRow &&
                observation.caretSourceUtf16 == null &&
                observation.caretDisplayUtf16 == null) ||
            (observation.caretSourceUtf16 != null &&
                observation.caretDisplayUtf16 != null &&
                observation.caretSourceUtf16 ==
                    observation.canonicalSelectionExtentUtf16),
      );
      _surfaceSourceGenerations.add(observation.sourceGeneration);
      _surfaceSelectionBases.add(observation.canonicalSelectionBaseUtf16);
      _surfaceSelectionExtents.add(observation.canonicalSelectionExtentUtf16);
      _surfaceCaretSources.add(observation.caretSourceUtf16);
      _surfaceCaretDisplays.add(observation.caretDisplayUtf16);
      _surfaceVisibleSources.add(observation.visibleSource);
      _surfaceStyledTexts.add(
        observation.rows
            .expand((row) => row.runs)
            .expand(
              (run) => run.styles
                  .where((style) {
                    return switch (style) {
                      FlarkSurfaceInlineStyle.strong =>
                        run.resolvedStyle.fontWeight == FontWeight.w700,
                      FlarkSurfaceInlineStyle.emphasis =>
                        run.resolvedStyle.fontStyle == FontStyle.italic,
                      FlarkSurfaceInlineStyle.code =>
                        run.resolvedStyle.fontFamily == 'Menlo',
                      FlarkSurfaceInlineStyle.strikethrough =>
                        run.resolvedStyle.decoration?.contains(
                              TextDecoration.lineThrough,
                            ) ==
                            true,
                      FlarkSurfaceInlineStyle.link =>
                        run.resolvedStyle.decoration?.contains(
                              TextDecoration.underline,
                            ) ==
                            true,
                    };
                  })
                  .map((style) => '${style.name}:${run.text}'),
            )
            .toList(growable: false),
      );
    }
    final controller = _controller;
    if (controller == null) return;
    _timer?.cancel();
    final generation = ++_writeGeneration;
    _timer = Timer(
      const Duration(milliseconds: 100),
      () => unawaited(_write(controller, generation)),
    );
  }

  Future<void> writeNow({
    required int commandSequence,
    int? sourcePointOffset,
    FlarkEditorDebugGeometry? sourcePointGeometry,
    int? taskActionTarget,
    FlarkEditorDebugGeometry? taskActionGeometry,
  }) async {
    _timer?.cancel();
    _timer = null;
    _commandSequence = commandSequence;
    _commandError = null;
    _sourcePointOffset = sourcePointOffset;
    _sourcePointGeometry = sourcePointGeometry;
    _taskActionTarget = taskActionTarget;
    _taskActionGeometry = taskActionGeometry;
    _captureSettledPresentation();
    _timer?.cancel();
    _timer = null;
    final controller = _controller;
    if (controller == null) {
      throw StateError('canary receipt writer has no controller');
    }
    // FrameTiming is delivered asynchronously after the painted frame. Wait
    // for the final paint's real engine timestamp rather than inventing a
    // duration or silently dropping the proving frame from the receipt.
    await _waitForFinalFrameTiming();
    final generation = ++_writeGeneration;
    await _write(controller, generation);
  }

  Future<void> writeCommandError(int commandSequence, Object error) async {
    _timer?.cancel();
    _timer = null;
    _commandSequence = commandSequence;
    _commandError = error;
    final controller = _controller;
    if (controller == null) return;
    final generation = ++_writeGeneration;
    await _write(controller, generation);
  }

  Future<void> writeClosed({
    required int commandSequence,
    required int closeRequestedEpochMicros,
    required int closeRequestedRssBytes,
    required int closeRequestedMaximumRssBytes,
    required FlarkNativeGlobalLiveStateInspection globalLiveState,
  }) async {
    _timer?.cancel();
    _timer = null;
    _commandSequence = commandSequence;
    _commandError = null;
    final receipt = <String, Object?>{
      'schemaVersion': 5,
      'receiptEpochMicros': DateTime.now().microsecondsSinceEpoch,
      'canaryId': _canaryId,
      'commandSequence': commandSequence,
      'commandError': null,
      'status': 'disposed',
      'processLaunchEpochMicros': mode.processLaunchEpochMicros,
      'closeRequestedEpochMicros': closeRequestedEpochMicros,
      'closeRequestedRssBytes': closeRequestedRssBytes,
      'closeRequestedMaximumRssBytes': closeRequestedMaximumRssBytes,
      'postCloseEpochMicros': DateTime.now().microsecondsSinceEpoch,
      'postCloseRssBytes': ProcessInfo.currentRss,
      'maximumRssBytes': ProcessInfo.maxRss,
      'display': _displayReceipt(),
      'globalLiveState': {
        'sessions': globalLiveState.liveSessions,
        'transactions': globalLiveState.liveTransactions,
        'continuations': globalLiveState.liveContinuations,
        'anchors': globalLiveState.liveAnchors,
        'historyTokens': globalLiveState.liveHistoryTokens,
      },
    };
    await _writeReceipt(receipt);
  }

  void _record() {
    if (_frameScheduled) return;
    _frameScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _frameScheduled = false;
      _captureSettledPresentation();
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  Future<void> _waitForFinalFrameTiming() async {
    if (_paintReceipts.isEmpty) return;
    final paintStamp = _paintReceipts.last['frameStampMicros']! as int;
    final refreshHz = _displayReceipt()['refreshHz']! as num;
    final framePeriodMicros = (1000000 / refreshHz).round();
    for (var attempt = 0; attempt < 20; attempt += 1) {
      final joined = _frameTimingReceipts.any(
        (timing) =>
            ((timing['vsyncStartMicros']! as int) +
                    framePeriodMicros -
                    paintStamp)
                .abs() <=
            32,
      );
      if (joined) return;
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
  }

  void _captureSettledPresentation() {
    final controller = _controller;
    if (controller == null) return;
    _settledPresentation = controller.rows.isEmpty
        ? '<empty>'
        : controller.rows
              .map(controller.surfaceRow)
              .map((row) => '${row.leadingText}${row.text}')
              .join('\n');
    _timer?.cancel();
    final generation = ++_writeGeneration;
    _timer = Timer(
      const Duration(milliseconds: 100),
      () => unawaited(_write(controller, generation)),
    );
  }

  Future<void> _write(FlarkEditorController controller, int generation) async {
    final source = await controller.readSource();
    final selection = await controller.resolveCanonicalSelection();
    if (!identical(controller, _controller) || generation != _writeGeneration) {
      return;
    }
    final geometry = _sourcePointGeometry;
    final taskGeometry = _taskActionGeometry;
    final receipt = <String, Object?>{
      'schemaVersion': 5,
      'receiptEpochMicros': DateTime.now().microsecondsSinceEpoch,
      'processLaunchEpochMicros': mode.processLaunchEpochMicros,
      'canaryId': _canaryId,
      'commandSequence': _commandSequence,
      'commandError': _commandError?.toString(),
      'status': controller.status.name,
      'revision': controller.revision,
      'sourceGeneration': controller.sourceGeneration,
      'source': source,
      'sourceUtf16Length': controller.sourceUtf16Length,
      'selectionBaseUtf16': selection?.base ?? controller.globalCaretOffset,
      'selectionExtentUtf16': selection?.extent ?? controller.globalCaretOffset,
      'caretUtf16': selection?.extent ?? controller.globalCaretOffset,
      'pendingEdits': controller.pendingEdits,
      'currentRssBytes': ProcessInfo.currentRss,
      'maximumRssBytes': ProcessInfo.maxRss,
      'display': _displayReceipt(),
      'resyncCount': controller.resyncCount,
      'lastResyncReason': controller.lastResyncReason.name,
      'lastError': controller.lastError?.toString(),
      'settledPresentation': _settledPresentation,
      'surfaceFrames': List<String>.unmodifiable(_surfaceFrames),
      'surfaceFrameHashes': List<int>.unmodifiable(_surfaceFrameHashes),
      'surfaceVisualStateHashes': List<int>.unmodifiable(
        _surfaceVisualStateHashes,
      ),
      'surfaceCaretIdentities': List<bool>.unmodifiable(
        _surfaceCaretIdentities,
      ),
      'surfaceSourceGenerations': List<int>.unmodifiable(
        _surfaceSourceGenerations,
      ),
      'surfaceSelectionBases': List<int>.unmodifiable(_surfaceSelectionBases),
      'surfaceSelectionExtents': List<int>.unmodifiable(
        _surfaceSelectionExtents,
      ),
      'surfaceCaretSources': List<int?>.unmodifiable(_surfaceCaretSources),
      'surfaceCaretDisplays': List<int?>.unmodifiable(_surfaceCaretDisplays),
      'surfaceVisibleSources': List<String>.unmodifiable(
        _surfaceVisibleSources,
      ),
      'surfaceStyledTexts': _surfaceStyledTexts
          .map(List<String>.unmodifiable)
          .toList(growable: false),
      'paintReceipts': List<Map<String, Object?>>.unmodifiable(_paintReceipts),
      'frameTimingReceipts': List<Map<String, Object?>>.unmodifiable(
        _frameTimingReceipts,
      ),
      'sourceEditPerformanceReceipts': controller.sourceEditPerformanceReceipts
          .map(
            (receipt) => {
              'kind': receipt.kind.name,
              'sourceGeneration': receipt.sourceGeneration,
              'coreQueueMicros': receipt.coreQueueMicros,
              'workerRoundTripMicros': receipt.workerRoundTripMicros,
              'workerQueueMicros': receipt.workerQueueMicros,
              'nativeFfiMicros': receipt.nativeFfiMicros,
              'coreAdoptionMicros': receipt.coreAdoptionMicros,
              'flutterReceiptAdoptionMicros':
                  receipt.flutterReceiptAdoptionMicros,
              'acceptanceToReceiptMicros': receipt.acceptanceToReceiptMicros,
            },
          )
          .toList(growable: false),
      'semanticEditPerformanceReceipts': controller
          .semanticEditPerformanceReceipts
          .map(
            (receipt) => {
              'sourceGeneration': receipt.sourceGeneration,
              'platformCallbackMicros': receipt.platformCallbackMicros,
              'coreQueueMicros': receipt.coreQueueMicros,
              'workerRoundTripMicros': receipt.workerRoundTripMicros,
              'workerQueueMicros': receipt.workerQueueMicros,
              'nativeFfiMicros': receipt.nativeFfiMicros,
              'coreAdoptionMicros': receipt.coreAdoptionMicros,
              'flutterReceiptAdoptionMicros':
                  receipt.flutterReceiptAdoptionMicros,
              'callbackToReceiptMicros': receipt.callbackToReceiptMicros,
            },
          )
          .toList(growable: false),
      'scrollOffset': _lastScrollOffset,
      'inputEvents': List<String>.unmodifiable(_inputEvents),
      if (geometry != null)
        'sourcePoint': {
          'utf16Offset': _sourcePointOffset,
          'globalX': geometry.globalPosition.dx,
          'globalY': geometry.globalPosition.dy,
          'rootWidth': geometry.rootLogicalSize.width,
          'rootHeight': geometry.rootLogicalSize.height,
        },
      if (taskGeometry != null)
        'taskActionPoint': {
          'targetUtf16': _taskActionTarget,
          'globalX': taskGeometry.globalPosition.dx,
          'globalY': taskGeometry.globalPosition.dy,
          'rootWidth': taskGeometry.rootLogicalSize.width,
          'rootHeight': taskGeometry.rootLogicalSize.height,
        },
    };
    await _writeReceipt(receipt);
  }

  Future<void> _writeReceipt(Map<String, Object?> receipt) async {
    final destination = File(mode.receiptPath);
    final temporary = File('${mode.receiptPath}.tmp');
    await temporary.writeAsString(jsonEncode(receipt), flush: true);
    await temporary.rename(destination.path);
  }

  Map<String, Object> _displayReceipt() {
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    final display = view.display;
    return {
      'refreshHz': display.refreshRate,
      'devicePixelRatio': view.devicePixelRatio,
      'widthLogical': view.physicalSize.width / view.devicePixelRatio,
      'heightLogical': view.physicalSize.height / view.devicePixelRatio,
    };
  }
}
