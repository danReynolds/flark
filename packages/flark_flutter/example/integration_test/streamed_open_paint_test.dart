// Development receipt for the RFC 029 A3 open: time from the public open
// call to the first painted frame carrying parser-certified rows, measured
// through the real Flutter surface on a streamed admission.
//
// This is not the frozen five-run claim — it runs the product path on this
// machine and prints one JSON line per run. Drive it with
//   scripts/profile_v4_streamed_open_macos.sh
// which foregrounds the app and holds the display awake, because a frame
// receipt taken against a sleeping display has no vsync to measure.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const _block = 'Ordinary paragraph with **bold** text and plain words.\n\n';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('streamed open to first certified painted frame', (tester) async {
    const configuredLibrary = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
    const sourceBytes = int.fromEnvironment(
      'FLARK_PROFILE_SOURCE_BYTES',
      defaultValue: 10 * 1024 * 1024,
    );
    const runCount = int.fromEnvironment(
      'FLARK_PROFILE_RUN_COUNT',
      defaultValue: 5,
    );
    final libraryPath = configuredLibrary.isEmpty ? null : configuredLibrary;

    expect(
      await FlarkEditorController.streamedOpenSupported(
        libraryPath: libraryPath,
      ),
      isTrue,
      reason:
          'this receipt needs a library built with the opening-session '
          'cargo feature',
    );

    // One warm pass takes the first-open cost of the renderer, the dynamic
    // library, and the isolate out of the measured runs; run 0 of a cold
    // process is reported separately by the caller if it wants it.
    final warmup = await FlarkEditorController.open(
      'Warm **renderer** and native runtime.\n',
      libraryPath: libraryPath,
    );
    await warmup.continueParsing();
    await tester.pumpWidget(
      MaterialApp(home: Scaffold(body: FlarkEditor(controller: warmup))),
    );
    await tester.pump();
    await tester.pumpWidget(const SizedBox.shrink());
    await warmup.close();

    final source = _fixture(sourceBytes);
    for (var run = 0; run < runCount; run++) {
      final watch = Stopwatch()..start();
      final controller = await FlarkEditorController.openUtf8Stream(
        _chunks(source),
        libraryPath: libraryPath,
      );
      final openCall = watch.elapsed;
      await tester.pumpWidget(
        MaterialApp(home: Scaffold(body: FlarkEditor(controller: controller))),
      );
      unawaited(controller.continueParsing());

      // The certified head becomes publishable asynchronously; pump frames
      // until the controller reports its first certified publication, then
      // take the very next presented frame as the painted receipt.
      final deadline = DateTime.now().add(const Duration(seconds: 60));
      while (controller.firstCertifiedPublicationEpochMicros == null) {
        if (!DateTime.now().isBefore(deadline)) {
          fail('no certified publication within 60s (run $run)');
        }
        await tester.pump(const Duration(milliseconds: 1));
      }
      final published = watch.elapsed;
      final painted = Completer<Duration>();
      SchedulerBinding.instance.addPostFrameCallback((_) {
        painted.complete(watch.elapsed);
      });
      await tester.pump();
      final paintedAt = await painted.future;
      watch.stop();

      final admittedAtPublication = controller.sourceByteLength;
      final rows = controller.rows.length;
      final status = controller.status.name;
      await tester.pumpWidget(const SizedBox.shrink());
      await controller.close();

      stdout.writeln(
        'FLARK_STREAMED_OPEN_RECEIPT ${jsonEncode({
          'run': run,
          'sourceBytes': source.length,
          'openCallMicros': openCall.inMicroseconds,
          'firstCertifiedPublicationMicros': published.inMicroseconds,
          'firstCertifiedPaintedFrameMicros': paintedAt.inMicroseconds,
          'admittedBytesAtPublication': admittedAtPublication,
          'certifiedRows': rows,
          'statusAtPublication': status,
        })}',
      );
    }
  }, timeout: const Timeout(Duration(minutes: 10)));
}

String _fixture(int targetBytes) {
  final buffer = StringBuffer();
  while (buffer.length < targetBytes) {
    buffer.write(_block);
  }
  return buffer.toString();
}

/// Transport-sized chunks encoded one slice at a time, so the harness never
/// holds a second complete copy of the fixture.
Stream<Uint8List> _chunks(String source) async* {
  const sizes = [8192, 65536, 24576, 49152, 16384, 32768];
  var start = 0;
  var next = 0;
  while (start < source.length) {
    final end = (start + sizes[next % sizes.length]).clamp(0, source.length);
    yield Uint8List.fromList(utf8.encode(source.substring(start, end)));
    start = end;
    next++;
    await Future<void>.delayed(Duration.zero);
  }
}
