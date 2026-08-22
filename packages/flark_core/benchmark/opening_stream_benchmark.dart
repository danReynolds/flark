// Cold-open receipt for the streamed admission path (RFC 029 A3, Dart
// layer). This is a development receipt, not the frozen product claim: it
// runs on the Dart VM JIT and an uncontrolled bench.
//
// Requires a library built with the opening-session cargo feature:
//   cargo build --manifest-path native/comrak_bridge/Cargo.toml \
//     --package flark-abi --release --features opening-session
//   FLARK_V4_LIBRARY_PATH=native/comrak_bridge/target/release/libflark_abi.dylib \
//     dart run benchmark/opening_stream_benchmark.dart [sizeMiB] [runs]
//
// Measurement points:
//   openToFirstCertifiedMs — from immediately before the
//     FlarkCoreDocument.openUtf8Stream call (isolate spawn, library load,
//     ABI negotiate, and opening create_begin all included) to the owner
//     isolate observing the first queryViewport answer that is
//     currentCertified with at least one semantic row. The poll queries a
//     bounded 4 KiB head window, the page discipline every parse-pending
//     consumer uses, and interleaves with the transport feed on the worker
//     mailbox — so the number carries real contention, as an application
//     poll would.
//   openToReadyMs — same origin to pumpUntilReady returning after the
//     stream closes (seal + full post-commit parse).
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flark_core/flark_core.dart';

Future<void> main(List<String> arguments) async {
  final sizeMiB = arguments.isEmpty ? 10 : int.parse(arguments.first);
  final runs = arguments.length > 1 ? int.parse(arguments[1]) : 5;
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  if (libraryPath == null) {
    stderr.writeln(
      'FLARK_V4_LIBRARY_PATH is required (opening-session feature build).',
    );
    exitCode = 64;
    return;
  }

  const block = 'Ordinary paragraph with **bold** text and plain words.\n\n';
  final targetBytes = sizeMiB * 1024 * 1024;
  final buffer = StringBuffer();
  while (buffer.length < targetBytes) {
    buffer.write(block);
  }
  final bytes = utf8.encode(buffer.toString());
  // Transport chunks cycle through the 8-64 KiB envelope.
  const chunkSizes = [8192, 16384, 32768, 65536];

  Stream<Uint8List> chunkStream() async* {
    var offset = 0;
    var index = 0;
    while (offset < bytes.length) {
      final end = math.min(
        offset + chunkSizes[index % chunkSizes.length],
        bytes.length,
      );
      yield Uint8List.sublistView(bytes, offset, end);
      offset = end;
      index++;
    }
  }

  for (var run = 0; run < runs; run++) {
    final watch = Stopwatch()..start();
    final document = await FlarkCoreDocument.openUtf8Stream(
      chunkStream(),
      libraryPath: libraryPath,
    );
    var firstCertifiedMicros = -1;
    var admittedAtCertified = -1;
    var certifiedRows = 0;
    var polls = 0;
    while (firstCertifiedMicros < 0) {
      await document.pump(workUnits: 512);
      final viewport = await document.queryViewport(
        endByte: math.min(document.sourceByteLength, 4096),
        maxRows: 64,
      );
      polls++;
      if (viewport.isCertified && viewport.rows.isNotEmpty) {
        firstCertifiedMicros = watch.elapsedMicroseconds;
        admittedAtCertified = document.sourceByteLength;
        certifiedRows = viewport.rows.length;
      }
      if (!document.isOpening && document.isReady) break;
    }
    await document.pumpUntilReady();
    final readyMicros = watch.elapsedMicroseconds;
    stdout.writeln(
      jsonEncode({
        'run': run,
        'sourceBytes': bytes.length,
        'chunkBytes': chunkSizes,
        'openToFirstCertifiedMs': firstCertifiedMicros / 1000,
        'admittedBytesAtCertified': admittedAtCertified,
        'certifiedRows': certifiedRows,
        'certifiedPollTurns': polls,
        'openToReadyMs': readyMicros / 1000,
        'finalSourceBytes': document.sourceByteLength,
        'rssMiB': (ProcessInfo.currentRss / (1024 * 1024)).round(),
      }),
    );
    await document.dispose();
  }
}
