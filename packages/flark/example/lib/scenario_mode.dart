import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';

final class DogfoodScenarioMode {
  const DogfoodScenarioMode({
    required this.id,
    required this.source,
    required this.receiptPath,
  });

  static DogfoodScenarioMode? fromEnvironment() {
    final scenarioPath = Platform.environment['FLARK_SCENARIO_PATH'];
    final receiptPath = Platform.environment['FLARK_SCENARIO_RECEIPT_PATH'];
    if (scenarioPath == null || receiptPath == null) return null;
    final json =
        jsonDecode(File(scenarioPath).readAsStringSync())
            as Map<String, Object?>;
    return DogfoodScenarioMode(
      id: json['id']! as String,
      source: json['initialSource']! as String,
      receiptPath: receiptPath,
    );
  }

  final String id;
  final String source;
  final String receiptPath;
}

/// Opt-in dogfood instrumentation for native scenario runners. It retains
/// controller snapshots in memory during an interaction and publishes one
/// atomic receipt only after 100 ms of quiet, keeping file I/O off the typing
/// path that the scenario is trying to observe.
final class DogfoodScenarioReceiptWriter {
  DogfoodScenarioReceiptWriter(this.mode);

  final DogfoodScenarioMode mode;
  final List<String> _surfaceFrames = [];
  final List<String> _inputEvents = [];
  FlarkEditorController? _controller;
  Timer? _timer;
  int _writeGeneration = 0;
  bool _frameScheduled = false;

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
    if (_inputEvents.length == 128) _inputEvents.removeAt(0);
    _inputEvents.add('${DateTime.now().microsecondsSinceEpoch}:$event');
    _record();
  }

  void _record() {
    if (_frameScheduled) return;
    _frameScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _frameScheduled = false;
      _capturePaintedState();
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  void _capturePaintedState() {
    final controller = _controller;
    if (controller == null) return;
    final surface = controller.rows.isEmpty
        ? '<empty>'
        : controller.rows
              .map((row) {
                final presentation = controller.surfaceRow(row);
                return '${presentation.leadingText}${presentation.text}';
              })
              .join('\n');
    if (_surfaceFrames.isEmpty || _surfaceFrames.last != surface) {
      _surfaceFrames.add(surface);
    }
    _timer?.cancel();
    final generation = ++_writeGeneration;
    _timer = Timer(
      const Duration(milliseconds: 100),
      () => unawaited(_write(controller, generation)),
    );
  }

  Future<void> _write(FlarkEditorController controller, int generation) async {
    final source = await controller.readSource();
    if (!identical(controller, _controller) || generation != _writeGeneration) {
      return;
    }
    final receipt = <String, Object?>{
      'schemaVersion': 1,
      'scenarioId': mode.id,
      'status': controller.status.name,
      'revision': controller.revision,
      'source': source,
      'sourceUtf16Length': controller.sourceUtf16Length,
      'caretUtf16': controller.globalCaretOffset,
      'pendingEdits': controller.pendingEdits,
      'resyncCount': controller.resyncCount,
      'lastResyncReason': controller.lastResyncReason.name,
      'lastError': controller.lastError?.toString(),
      'surfaceFrames': List<String>.unmodifiable(_surfaceFrames),
      'inputEvents': List<String>.unmodifiable(_inputEvents),
    };
    final destination = File(mode.receiptPath);
    final temporary = File('${mode.receiptPath}.tmp');
    await temporary.writeAsString(jsonEncode(receipt), flush: true);
    await temporary.rename(destination.path);
  }
}
