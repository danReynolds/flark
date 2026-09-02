import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flark/flark.dart';

Future<void> main(List<String> arguments) async {
  final sizeMiB = arguments.isEmpty ? 1 : int.parse(arguments.first);
  final editDuringOpen = arguments.length > 1 && arguments[1] == 'early-edit';
  final queryProfile = arguments.length > 1 && arguments[1] == 'query-profile';
  final burstProfile = arguments.length > 1 && arguments[1] == 'burst-profile';
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  if (libraryPath == null) {
    stderr.writeln('FLARK_V4_LIBRARY_PATH is required.');
    exitCode = 64;
    return;
  }
  final targetBytes = sizeMiB * 1024 * 1024;
  final source = _fixture(targetBytes);
  final rssBefore = ProcessInfo.currentRss;

  final openWatch = Stopwatch()..start();
  final document = await FlarkCoreDocument.open(
    source,
    libraryPath: libraryPath,
  );
  openWatch.stop();

  final pendingWatch = Stopwatch()..start();
  final initialPage = await document.queryViewport(
    endByte: math.min(document.sourceByteLength, 4 * 1024),
    maxRows: 256,
  );
  pendingWatch.stop();

  if (editDuringOpen) {
    final quick = source.indexOf('quick');
    final editWatch = Stopwatch()..start();
    await document.applyEditUtf16(quick, quick + 'quick'.length, 'swift');
    editWatch.stop();
    final editedPageWatch = Stopwatch()..start();
    final editedPage = await document.queryViewport(
      endByte: math.min(document.sourceByteLength, 4 * 1024),
      maxRows: 256,
    );
    editedPageWatch.stop();
    stdout.writeln(
      jsonEncode({
        'sizeMiB': sizeMiB,
        'sourceBytes': document.sourceByteLength,
        'openMs': _milliseconds(openWatch),
        'pendingViewportMs': _milliseconds(pendingWatch),
        'editDuringOpenAckMs': _milliseconds(editWatch),
        'editedPendingViewportMs': _milliseconds(editedPageWatch),
        'editedPageWasNeutral': !editedPage.isCertified,
        'revision': document.revision,
      }),
    );
    await document.dispose();
    return;
  }

  final initialPump = await _pumpToReady(document, scheduledTurns: true);
  if (burstProfile) {
    final offset = source.indexOf('quick');
    final samples = <int>[];
    for (var index = 0; index < 120; index += 1) {
      final watch = Stopwatch()..start();
      await document.applyEditUtf16(offset + index, offset + index, 'x');
      watch.stop();
      samples.add(watch.elapsedMicroseconds);
    }
    samples.sort();
    stdout.writeln(
      jsonEncode({
        'sizeMiB': sizeMiB,
        'editSamples': samples.length,
        'editP50Ms': samples[samples.length ~/ 2] / 1000,
        'editP99Ms': samples[(samples.length * 99 ~/ 100) - 1] / 1000,
        'editMaxMs': samples.last / 1000,
      }),
    );
    await document.dispose();
    return;
  }
  if (queryProfile) {
    stdout.writeln(jsonEncode({'pid': pid, 'phase': 'query-profile'}));
    await stdout.flush();
    final samples = <int>[];
    for (var index = 0; index < 200; index += 1) {
      final watch = Stopwatch()..start();
      await document.queryViewport(
        endByte: document.sourceByteLength,
        maxRows: 32,
      );
      watch.stop();
      samples.add(watch.elapsedMicroseconds);
    }
    samples.sort();
    stdout.writeln(
      jsonEncode({
        'sizeMiB': sizeMiB,
        'querySamples': samples.length,
        'queryP50Ms': samples[samples.length ~/ 2] / 1000,
        'queryP99Ms': samples[(samples.length * 99 ~/ 100) - 1] / 1000,
        'queryMaxMs': samples.last / 1000,
      }),
    );
    await document.dispose();
    return;
  }
  final certifiedWatch = Stopwatch()..start();
  final certifiedPage = await document.queryViewport(
    endByte: math.min(document.sourceByteLength, 4 * 1024),
    maxRows: 256,
  );
  certifiedWatch.stop();

  final quick = source.indexOf('quick');
  final editWatch = Stopwatch()..start();
  await document.applyEditUtf16(quick, quick + 'quick'.length, 'swift');
  editWatch.stop();

  final pendingEditWatch = Stopwatch()..start();
  final pendingEditPage = await document.queryViewport(
    endByte: math.min(document.sourceByteLength, 4 * 1024),
    maxRows: 256,
  );
  pendingEditWatch.stop();
  final editPump = await _pumpToReady(document, scheduledTurns: true);
  final rssAfter = ProcessInfo.currentRss;

  stdout.writeln(
    jsonEncode({
      'sizeMiB': sizeMiB,
      'sourceBytes': document.sourceByteLength,
      'openMs': _milliseconds(openWatch),
      'openReturnedReady': initialPage.isCertified,
      'pendingViewportMs': _milliseconds(pendingWatch),
      'initialCertificationMs': initialPump.totalMilliseconds,
      'initialPumpTurns': initialPump.turns,
      'initialMaximumPumpTurnMs': initialPump.maximumTurnMilliseconds,
      'certifiedViewportMs': _milliseconds(certifiedWatch),
      'certifiedViewportRows': certifiedPage.rows.length,
      'editAckMs': _milliseconds(editWatch),
      'pendingEditViewportMs': _milliseconds(pendingEditWatch),
      'pendingEditWasNeutral': !pendingEditPage.isCertified,
      'editCertificationMs': editPump.totalMilliseconds,
      'editPumpTurns': editPump.turns,
      'editMaximumPumpTurnMs': editPump.maximumTurnMilliseconds,
      'rssDeltaMiB': (rssAfter - rssBefore) / (1024 * 1024),
    }),
  );

  await document.dispose();
}

final class _PumpMeasurement {
  const _PumpMeasurement({
    required this.totalMilliseconds,
    required this.turns,
    required this.maximumTurnMilliseconds,
  });

  final double totalMilliseconds;
  final int turns;
  final double maximumTurnMilliseconds;
}

Future<_PumpMeasurement> _pumpToReady(
  FlarkCoreDocument document, {
  required bool scheduledTurns,
}) async {
  final total = Stopwatch()..start();
  if (!scheduledTurns) {
    await document.pumpUntilReady(workUnits: 512);
    total.stop();
    return _PumpMeasurement(
      totalMilliseconds: total.elapsedMicroseconds / 1000,
      turns: 1,
      maximumTurnMilliseconds: total.elapsedMicroseconds / 1000,
    );
  }
  var turns = 0;
  var maximumMicros = 0;
  while (!document.isReady) {
    final turn = Stopwatch()..start();
    await document.pump(workUnits: 512);
    turn.stop();
    maximumMicros = math.max(maximumMicros, turn.elapsedMicroseconds);
    turns += 1;
  }
  total.stop();
  return _PumpMeasurement(
    totalMilliseconds: total.elapsedMicroseconds / 1000,
    turns: turns,
    maximumTurnMilliseconds: maximumMicros / 1000,
  );
}

double _milliseconds(Stopwatch watch) => watch.elapsedMicroseconds / 1000;

String _fixture(int targetBytes) {
  const block = '''
## Section

A quick brown fox crosses the incremental Markdown editor with **bold text**,
`inline code`, and a [link](https://example.com).

- first item
- second item
- third item

> A bounded block quote keeps the fixture structurally representative.

''';
  final fullBlocks = targetBytes ~/ block.length;
  final remainder = targetBytes % block.length;
  final buffer = StringBuffer();
  for (var index = 0; index < fullBlocks; index += 1) {
    buffer.write(block);
  }
  buffer.write(block.substring(0, remainder));
  return buffer.toString();
}
