import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';

final class DogfoodNativeCanaryMode {
  const DogfoodNativeCanaryMode({
    required this.receiptPath,
    required this.commandPath,
  });

  static DogfoodNativeCanaryMode? fromEnvironment() {
    final receiptPath = Platform.environment['FLARK_CANARY_RECEIPT_PATH'];
    if (receiptPath == null) return null;
    final commandPath = Platform.environment['FLARK_CANARY_COMMAND_PATH'];
    if (commandPath == null) return null;
    return DogfoodNativeCanaryMode(
      receiptPath: receiptPath,
      commandPath: commandPath,
    );
  }

  String get source => '';
  final String receiptPath;
  final String commandPath;
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
  DogfoodNativeCanaryReceiptWriter(this.mode);

  final DogfoodNativeCanaryMode mode;
  final List<String> _surfaceFrames = [];
  final List<int> _surfaceFrameHashes = [];
  final List<int> _surfaceVisualStateHashes = [];
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

  void dispose() => detach();

  void recordInputEvent(String event) {
    _lastInputEventAt = DateTime.now();
    if (_inputEvents.length == 128) _inputEvents.removeAt(0);
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
    final surface = observation.presentation;
    if (_surfaceFrames.isEmpty ||
        _surfaceFrames.last != surface ||
        _surfaceFrameHashes.last != observation.renderPlanHash ||
        _surfaceVisualStateHashes.last != observation.visualStateHash) {
      if (_surfaceFrames.length == 128) {
        _surfaceFrames.removeAt(0);
        _surfaceFrameHashes.removeAt(0);
        _surfaceVisualStateHashes.removeAt(0);
      }
      _surfaceFrames.add(surface);
      _surfaceFrameHashes.add(observation.renderPlanHash);
      _surfaceVisualStateHashes.add(observation.visualStateHash);
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

  void _record() {
    if (_frameScheduled) return;
    _frameScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _frameScheduled = false;
      _captureSettledPresentation();
    });
    WidgetsBinding.instance.ensureVisualUpdate();
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
      'schemaVersion': 2,
      'canaryId': _canaryId,
      'commandSequence': _commandSequence,
      'commandError': _commandError?.toString(),
      'status': controller.status.name,
      'revision': controller.revision,
      'source': source,
      'sourceUtf16Length': controller.sourceUtf16Length,
      'selectionBaseUtf16': selection?.base ?? controller.globalCaretOffset,
      'selectionExtentUtf16': selection?.extent ?? controller.globalCaretOffset,
      'caretUtf16': selection?.extent ?? controller.globalCaretOffset,
      'pendingEdits': controller.pendingEdits,
      'resyncCount': controller.resyncCount,
      'lastResyncReason': controller.lastResyncReason.name,
      'lastError': controller.lastError?.toString(),
      'settledPresentation': _settledPresentation,
      'surfaceFrames': List<String>.unmodifiable(_surfaceFrames),
      'surfaceFrameHashes': List<int>.unmodifiable(_surfaceFrameHashes),
      'surfaceVisualStateHashes': List<int>.unmodifiable(
        _surfaceVisualStateHashes,
      ),
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
    final destination = File(mode.receiptPath);
    final temporary = File('${mode.receiptPath}.tmp');
    await temporary.writeAsString(jsonEncode(receipt), flush: true);
    await temporary.rename(destination.path);
  }
}
