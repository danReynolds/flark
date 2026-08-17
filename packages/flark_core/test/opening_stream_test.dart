import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

// The streamed-open surface only exists in libraries compiled with the
// opening-session cargo feature, so these tests key off their own library
// path: pointing FLARK_V4_LIBRARY_PATH at a default-feature build must skip
// them instead of failing the everyday gate. Build the library with
//   cargo build --manifest-path native/comrak_bridge/Cargo.toml \
//     --package flark-abi --release --features opening-session
// or run scripts/verify_v4.sh with FLARK_V4_FEATURES=opening-session.
const _librarySkipMessage =
    'Set FLARK_V4_OPENING_LIBRARY_PATH to a flark_abi library built with '
    'the opening-session cargo feature.';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_OPENING_LIBRARY_PATH'];

  /// Polls bounded pump-and-query turns until [accept] admits a viewport.
  /// Queries stay within a bounded head window, the same page discipline
  /// every parse-pending consumer applies.
  Future<FlarkViewport> pollViewport(
    FlarkCoreDocument document,
    bool Function(FlarkViewport viewport) accept, {
    int attempts = 5000,
  }) async {
    for (var attempt = 0; attempt < attempts; attempt++) {
      await document.pump(workUnits: 512);
      final viewport = await document.queryViewport(
        endByte: math.min(document.sourceByteLength, 4096),
        maxRows: 64,
      );
      if (accept(viewport)) return viewport;
    }
    fail('viewport condition not reached within $attempts poll turns');
  }

  test(
    'streamed open serves certified rows and accepts edits before the '
    'stream closes',
    () async {
      final buffer = StringBuffer();
      for (var index = 0; index < 6000; index++) {
        buffer.write(
          'Paragraph $index on the café terrace has **bold** words.\n\n',
        );
      }
      final source = buffer.toString();
      final bytes = utf8.encode(source);
      expect(bytes.length, greaterThan(300 * 1024));

      final chunks = StreamController<Uint8List>();
      final document = await FlarkCoreDocument.openUtf8Stream(
        chunks.stream,
        libraryPath: libraryPath,
      );
      addTearDown(document.dispose);
      expect(document.isOpening, isTrue);
      expect(document.isReady, isFalse);

      // One chunk boundary lands inside the two-byte 'é' of "café": the
      // runtime carries the split scalar, and the carried byte counts as
      // admitted only once its scalar completes.
      final splitAt = bytes.indexOf(0xc3) + 1;
      chunks.add(Uint8List.sublistView(bytes, 0, splitAt));
      var offset = splitAt;
      // Transport-realistic ragged chunk sizes, 8 to 64 KiB.
      const chunkSizes = [8192, 65536, 24576, 49152, 16384, 32768];
      var next = 0;
      while (offset < bytes.length ~/ 2) {
        final end = (offset + chunkSizes[next % chunkSizes.length])
            .clamp(0, bytes.length ~/ 2);
        chunks.add(Uint8List.sublistView(bytes, offset, end));
        offset = end;
        next++;
      }

      // Certified semantic rows appear while the stream is still open and
      // cover the document head; the tail stays pending.
      final early = await pollViewport(
        document,
        (viewport) => viewport.isCertified && viewport.rows.isNotEmpty,
      );
      expect(document.isOpening, isTrue);
      expect(document.isReady, isFalse);
      expect(early.rows.first.sourceBytes.start, 0);
      expect(early.coveredBytes.start, 0);
      expect(document.sourceByteLength, lessThan(bytes.length));

      // A literal edit mid-load commits against the current revision and
      // bumps it; recertification then returns.
      final editAt = source.indexOf('bold');
      final revisionBefore = document.revision;
      final receipt = await document.applyEditUtf16(
        editAt,
        editAt + 'bold'.length,
        'BOLD',
      );
      expect(receipt.revision, greaterThan(revisionBefore));
      expect(document.isOpening, isTrue);
      await pollViewport(
        document,
        (viewport) => viewport.isCertified && viewport.rows.isNotEmpty,
      );

      // Stream the tail, close, and finish the ordinary post-commit parse.
      while (offset < bytes.length) {
        final end = (offset + 65536).clamp(0, bytes.length);
        chunks.add(Uint8List.sublistView(bytes, offset, end));
        offset = end;
      }
      await chunks.close();
      await document.pumpUntilReady();
      expect(document.isOpening, isFalse);
      expect(document.isReady, isTrue);

      final edited = source.replaceRange(editAt, editAt + 'bold'.length, 'BOLD');
      expect(document.sourceByteLength, utf8.encode(edited).length);
      expect(document.sourceUtf16Length, edited.length);
      expect(await document.readSource(), edited);

      final finalViewport = await document.queryViewport(maxRows: 64);
      expect(finalViewport.isCertified, isTrue);
      expect(finalViewport.rows, isNotEmpty);
      expect(
        await document.readSourceUtf16Range(editAt, editAt + 'BOLD'.length),
        'BOLD',
      );
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'pre-certification streamed queries answer pending-neutral, not '
    'exceptions',
    () async {
      final chunks = StreamController<Uint8List>();
      final document = await FlarkCoreDocument.openUtf8Stream(
        chunks.stream,
        libraryPath: libraryPath,
      );
      addTearDown(document.dispose);

      // Nothing admitted yet: the empty-range query answers without rows
      // and without an exception (an empty range is vacuously certified).
      final empty = await document.queryViewport();
      expect(empty.rows, isEmpty);
      expect(empty.coveredBytes.length, 0);

      // A partial unsealed line can never certify; the answer stays the
      // familiar pending-neutral shape while the parser waits for source.
      chunks.add(utf8.encode('An unterminated opening paragraph without'));
      var pending = false;
      for (var attempt = 0; attempt < 32; attempt++) {
        await document.pump(workUnits: 512);
        final viewport = await document.queryViewport(maxRows: 16);
        if (viewport.certification == FlarkCertification.pendingNeutral) {
          pending = true;
        }
        expect(viewport.rows, isEmpty);
      }
      expect(pending, isTrue);

      // Finish the line, then a real body: the native opening session can
      // only seal after capturing a first compact slice, so the stream must
      // grow past that threshold before it closes (a tiny streamed document
      // fails its seal with a typed PARSER_FAULT — a known limitation of
      // the experiment surface, covered by the API documentation).
      chunks.add(utf8.encode(' its line ever ending.\n\n'));
      final body = StringBuffer();
      for (var index = 0; index < 400; index++) {
        body.write('Body paragraph $index keeps the seal above the '
            'first-slice threshold.\n\n');
      }
      chunks.add(utf8.encode(body.toString()));
      await chunks.close();
      await document.pumpUntilReady();
      expect(document.isReady, isTrue);
      expect(
        await document.readSource(),
        startsWith(
          'An unterminated opening paragraph without its line ever '
          'ending.\n\nBody paragraph 0',
        ),
      );
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
  );

  test(
    'openStreaming encodes at scalar boundaries and round-trips the source',
    () async {
      final buffer = StringBuffer();
      for (var index = 0; index < 3000; index++) {
        buffer.write('Blocco $index — café, 🌍 pair, **bold** text.\n\n');
      }
      final source = buffer.toString();
      final document = await FlarkCoreDocument.openStreaming(
        source,
        libraryPath: libraryPath,
      );
      addTearDown(document.dispose);
      await document.pumpUntilReady();
      expect(document.isOpening, isFalse);
      expect(document.isReady, isTrue);
      expect(document.sourceByteLength, utf8.encode(source).length);
      expect(document.sourceUtf16Length, source.length);
      expect(await document.readSource(), source);
      final viewport = await document.queryViewport(maxRows: 32);
      expect(viewport.isCertified, isTrue);
      expect(viewport.rows, isNotEmpty);
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
