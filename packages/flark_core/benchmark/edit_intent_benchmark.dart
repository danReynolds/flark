import 'dart:convert';
import 'dart:io';

import 'package:flark_core/flark_core.dart';

Future<void> main(List<String> arguments) async {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  if (libraryPath == null) {
    stderr.writeln('FLARK_V4_LIBRARY_PATH is required.');
    exitCode = 64;
    return;
  }
  final sizeMiB = arguments.isEmpty ? 1 : int.parse(arguments[0]);
  final shape = arguments.length < 2 ? 'ordinary' : arguments[1];
  final lane = arguments.length < 3 ? 'sequential' : arguments[2];
  final samples = arguments.length < 4 ? 120 : int.parse(arguments[3]);
  final document = await FlarkCoreDocument.open(
    _fixture(sizeMiB * 1024 * 1024, shape),
    libraryPath: libraryPath,
  );
  await document.pumpUntilReady();
  final session = FlarkCoreEditorSession(document);
  await session.setSelectionUtf16(8, 8);

  final total = <int>[];
  final coreQueue = <int>[];
  final workerRoundTrip = <int>[];
  final workerQueue = <int>[];
  final nativeFfi = <int>[];
  final coreAdoption = <int>[];
  for (var index = 0; index < samples; index += 1) {
    final watch = Stopwatch()..start();
    if (lane == 'pump-contention') {
      final pump = document.pump(workUnits: 512);
      final receipt = await session.applyEditIntentV1(
        FlarkCoreEditIntentV1.insertParagraphBreak,
        compositionActive: false,
      );
      _requireCommit(receipt);
      await pump;
      _record(
        receipt.telemetry,
        coreQueue,
        workerRoundTrip,
        workerQueue,
        nativeFfi,
        coreAdoption,
      );
      watch.stop();
      await session.undo();
      await session.clearHistory();
    } else if (lane == 'queued-pair') {
      final first = session.applyEditIntentV1(
        FlarkCoreEditIntentV1.insertParagraphBreak,
        compositionActive: false,
      );
      final second = session.applyEditIntentV1(
        FlarkCoreEditIntentV1.deleteBackward,
        compositionActive: false,
      );
      final receipts = await Future.wait([first, second]);
      for (final receipt in receipts) {
        _requireCommit(receipt);
        _record(
          receipt.telemetry,
          coreQueue,
          workerRoundTrip,
          workerQueue,
          nativeFfi,
          coreAdoption,
        );
      }
      watch.stop();
      await session.undo();
      await session.undo();
      await session.clearHistory();
    } else {
      final receipt = await session.applyEditIntentV1(
        FlarkCoreEditIntentV1.insertParagraphBreak,
        compositionActive: false,
      );
      _requireCommit(receipt);
      _record(
        receipt.telemetry,
        coreQueue,
        workerRoundTrip,
        workerQueue,
        nativeFfi,
        coreAdoption,
      );
      watch.stop();
      await session.undo();
      await session.clearHistory();
    }
    total.add(watch.elapsedMicroseconds);
  }

  stdout.writeln(
    jsonEncode({
      'sizeMiB': sizeMiB,
      'shape': shape,
      'lane': lane,
      'samples': samples,
      'commandSamples': nativeFfi.length,
      'cycleP50Ms': _percentile(total, 50) / 1000,
      'cycleP95Ms': _percentile(total, 95) / 1000,
      'cycleP99Ms': _p99(total) / 1000,
      'cycleMaxMs': _maximum(total) / 1000,
      'coreQueueP50Ms': _percentile(coreQueue, 50) / 1000,
      'coreQueueP99Ms': _p99(coreQueue) / 1000,
      'workerRoundTripP50Ms': _percentile(workerRoundTrip, 50) / 1000,
      'workerRoundTripP99Ms': _p99(workerRoundTrip) / 1000,
      'workerQueueP50Ms': _percentile(workerQueue, 50) / 1000,
      'workerQueueP99Ms': _p99(workerQueue) / 1000,
      'nativeFfiP50Ms': _percentile(nativeFfi, 50) / 1000,
      'nativeFfiP99Ms': _p99(nativeFfi) / 1000,
      'coreAdoptionP50Ms': _percentile(coreAdoption, 50) / 1000,
      'coreAdoptionP99Ms': _p99(coreAdoption) / 1000,
      'nativeFfiMaxMs': _maximum(nativeFfi) / 1000,
    }),
  );
  await session.dispose();
  await document.dispose();
}

void _requireCommit(FlarkCoreEditIntentReceiptV1 receipt) {
  if (!receipt.hasCommit) {
    throw StateError(
      'benchmark command did not commit: ${receipt.disposition.name}',
    );
  }
}

void _record(
  FlarkCoreEditIntentTelemetryV1 telemetry,
  List<int> coreQueue,
  List<int> workerRoundTrip,
  List<int> workerQueue,
  List<int> nativeFfi,
  List<int> coreAdoption,
) {
  coreQueue.add(telemetry.coreQueueMicros);
  workerRoundTrip.add(telemetry.workerRoundTripMicros);
  workerQueue.add(telemetry.workerQueueMicros);
  nativeFfi.add(telemetry.nativeFfiMicros);
  coreAdoption.add(telemetry.coreAdoptionMicros);
}

int _p99(List<int> values) {
  return _percentile(values, 99);
}

int _percentile(List<int> values, int percentile) {
  final sorted = [...values]..sort();
  return sorted[((sorted.length * percentile + 99) ~/ 100 - 1).clamp(
    0,
    sorted.length - 1,
  )];
}

int _maximum(List<int> values) =>
    values.fold(0, (maximum, value) => value > maximum ? value : maximum);

String _fixture(int targetBytes, String shape) {
  const prefix = '- target\n\n';
  final block = switch (shape) {
    'ordinary' =>
      'A representative paragraph with **bold**, `code`, and a link.\n\n',
    'tiny-blocks' => 'x.\n\n',
    'giant-line' => 'x',
    _ => throw ArgumentError.value(shape, 'shape'),
  };
  final buffer = StringBuffer(prefix);
  if (shape == 'giant-line') {
    buffer.write(List.filled(targetBytes - prefix.length - 1, 'x').join());
    buffer.write('\n');
    return buffer.toString();
  }
  while (buffer.length + block.length <= targetBytes) {
    buffer.write(block);
  }
  buffer.write(block.substring(0, targetBytes - buffer.length));
  return buffer.toString();
}
