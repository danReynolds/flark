import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// The streamed-open controller surface only exists against libraries
// compiled with the opening-session cargo feature, so these tests key off
// their own library path — mirroring packages/flark_core's gated
// opening_stream_test — and skip instead of failing the everyday gate.
// Build the library with
//   FLARK_V4_FEATURES=opening-session ./scripts/verify_v4.sh
const _librarySkipMessage =
    'Set FLARK_V4_OPENING_LIBRARY_PATH to a flark_abi library built with '
    'the opening-session cargo feature.';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_OPENING_LIBRARY_PATH'];

  String buildSource(int paragraphs) {
    final buffer = StringBuffer();
    for (var index = 0; index < paragraphs; index++) {
      buffer.write(
        'Ordinary paragraph $index with **bold** text and plain words.\n\n',
      );
    }
    return buffer.toString();
  }

  /// Feeds [bytes[start..end)] into [chunks] in transport-realistic ragged
  /// sizes inside the 8-64 KiB envelope.
  int feedRange(
    StreamController<Uint8List> chunks,
    Uint8List bytes,
    int start,
    int end,
  ) {
    const chunkSizes = [8192, 65536, 24576, 49152, 16384, 32768];
    var offset = start;
    var next = 0;
    while (offset < end) {
      final sliceEnd = (offset + chunkSizes[next % chunkSizes.length]).clamp(
        0,
        end,
      );
      chunks.add(Uint8List.sublistView(bytes, offset, sliceEnd));
      offset = sliceEnd;
      next++;
    }
    return offset;
  }

  Future<void> waitUntil(
    bool Function() condition, {
    Duration timeout = const Duration(seconds: 30),
    String? reason,
  }) async {
    final deadline = DateTime.now().add(timeout);
    while (!condition()) {
      if (!DateTime.now().isBefore(deadline)) {
        fail(reason ?? 'condition not reached within $timeout');
      }
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
  }

  test(
    'a streamed open publishes the certified head, reports streaming, and '
    'converges to ready',
    () async {
      final source = buildSource(6000);
      final bytes = Uint8List.fromList(utf8.encode(source));
      expect(bytes.length, greaterThan(300 * 1024));

      final chunks = StreamController<Uint8List>();
      final controller = await FlarkEditorController.openUtf8Stream(
        chunks.stream,
        libraryPath: libraryPath,
      );
      addTearDown(controller.close);
      final statuses = <FlarkEditorStatus>{controller.status};
      controller.addListener(() => statuses.add(controller.status));
      final parseTask = controller.continueParsing();

      // Feed the first half only: certification must land while the stream
      // is demonstrably still open.
      final fed = feedRange(chunks, bytes, 0, bytes.length ~/ 2);
      await waitUntil(
        () =>
            controller.firstCertifiedPublicationEpochMicros != null ||
            controller.lastError != null,
        reason: 'no certified viewport was published mid-stream',
      );
      expect(
        controller.lastError,
        isNull,
        reason: 'status=${controller.status} rows=${controller.rows.length} '
            'bytes=${controller.sourceByteLength}',
      );
      expect(controller.status, FlarkEditorStatus.streaming);
      expect(controller.rows, isNotEmpty);
      expect(controller.semanticsCurrent, isTrue);
      expect(controller.rows.first.sourceBytes.start, 0);
      expect(controller.visibleSource, isNotEmpty);
      expect(source.startsWith(controller.visibleSource), isTrue);
      // The tail is not admitted yet, so this is genuinely mid-load.
      expect(controller.sourceByteLength, lessThan(bytes.length));
      expect(statuses, contains(FlarkEditorStatus.streaming));
      expect(statuses, isNot(contains(FlarkEditorStatus.ready)));

      // Seal the stream and converge through the ordinary ready flow.
      feedRange(chunks, bytes, fed, bytes.length);
      await chunks.close();
      await parseTask;
      expect(controller.status, FlarkEditorStatus.ready);
      expect(controller.lastError, isNull);
      expect(controller.sourceByteLength, bytes.length);
      expect(controller.sourceUtf16Length, source.length);
      expect(statuses, containsAll(<FlarkEditorStatus>{
        FlarkEditorStatus.streaming,
        FlarkEditorStatus.ready,
      }));
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'admission appends never resync the input window mid-typing',
    () async {
      // The stance under test: an append can only add bytes after the
      // admitted frontier, so it cannot disturb an input window that lies
      // inside certified text. Typing through a live admission must keep
      // the same window epochs, take no resync, and land the character.
      final source = buildSource(6000);
      final bytes = Uint8List.fromList(utf8.encode(source));
      final chunks = StreamController<Uint8List>();
      final controller = await FlarkEditorController.openUtf8Stream(
        chunks.stream,
        libraryPath: libraryPath,
      );
      addTearDown(controller.close);
      final parseTask = controller.continueParsing();
      var fed = feedRange(chunks, bytes, 0, 96 * 1024);
      await waitUntil(
        () => controller.firstCertifiedPublicationEpochMicros != null,
        reason: 'no certified head to type into',
      );
      expect(controller.status, FlarkEditorStatus.streaming);

      final row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);
      final resyncBefore = controller.resyncCount;
      final connectionEpochBefore = controller.connectionEpoch;
      final before = controller.inputValue;

      // Admit more source while the window is active, then type into it.
      fed = feedRange(chunks, bytes, fed, fed + 96 * 1024);
      await waitUntil(
        () => controller.sourceByteLength > 96 * 1024,
        reason: 'admission did not advance under an active input window',
      );
      controller.updateEditingValue(
        TextEditingValue(
          text:
              '${before.text.substring(0, before.selection.extentOffset)}Z'
              '${before.text.substring(before.selection.extentOffset)}',
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
        ),
      );
      await controller.debugWaitForMutationSettled();
      await controller.debugWaitForPresentationSettled();

      expect(
        controller.resyncCount,
        resyncBefore,
        reason: 'admission is epoch-neutral: appends must not force a resync',
      );
      expect(controller.connectionEpoch, connectionEpochBefore);
      expect(controller.inputValue.text, contains('Z'));

      feedRange(chunks, bytes, fed, bytes.length);
      await chunks.close();
      await parseTask;
      expect(controller.status, FlarkEditorStatus.ready);
      expect(await controller.readSource(), contains('Z'));
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'openStreaming round-trips a large source through the streamed path',
    () async {
      final source = buildSource(3000);
      final controller = await FlarkEditorController.openStreaming(
        source,
        libraryPath: libraryPath,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      expect(controller.status, FlarkEditorStatus.ready);
      expect(controller.sourceUtf16Length, source.length);
      expect(await controller.readSource(), source);
      expect(controller.rows, isNotEmpty);
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'streamedOpenSupported reports the feature library as capable',
    () async {
      expect(
        await FlarkEditorController.streamedOpenSupported(
          libraryPath: libraryPath,
        ),
        isTrue,
      );
    },
    skip: libraryPath == null ? _librarySkipMessage : false,
  );

  testWidgets(
    'the editor widget paints the certified head while streaming and '
    'settles to ready',
    (tester) async {
      final source = buildSource(6000);
      final bytes = Uint8List.fromList(utf8.encode(source));
      final chunks = StreamController<Uint8List>();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.openUtf8Stream(
          chunks.stream,
          libraryPath: libraryPath,
        ),
      ))!;
      addTearDown(controller.close);

      var fed = 0;
      await tester.runAsync(() async {
        unawaited(controller.continueParsing());
        fed = feedRange(chunks, bytes, 0, bytes.length ~/ 2);
        await waitUntil(
          () => controller.firstCertifiedPublicationEpochMicros != null,
          reason: 'no certified viewport was published mid-stream',
        );
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 900,
            height: 600,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();
      expect(controller.status, FlarkEditorStatus.streaming);
      expect(controller.rows, isNotEmpty);
      final head = controller.surfaceRow(controller.rows.first);
      expect(head.text, contains('Ordinary paragraph 0'));
      // The certified head projects — the bold delimiters must not paint.
      expect(head.text, isNot(contains('**')));

      await tester.runAsync(() async {
        feedRange(chunks, bytes, fed, bytes.length);
        await chunks.close();
        await waitUntil(
          () => controller.status == FlarkEditorStatus.ready,
          reason: 'the sealed stream did not converge to ready',
        );
      });
      await tester.pump();
      expect(controller.status, FlarkEditorStatus.ready);
      expect(controller.lastError, isNull);
      expect(controller.sourceByteLength, bytes.length);
    },
    // testWidgets' skip is bool-only; the message lives on the plain tests.
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
