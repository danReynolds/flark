import 'package:flutter/material.dart';
import 'package:flutter_quill/flutter_quill.dart';
import 'package:flutter_test/flutter_test.dart';

// Peer calibration: flutter_quill per-edit render cost at matching block counts,
// using the SAME methodology as Flark's rebuild benchmark (N line-paragraphs in
// a 600px viewport, one-character insert near the start, 40 timed pumps, debug
// test-VM). Quill renders the document as a single rich-text layout rather than
// per-block widgets, so this calibrates the "single-layout" end of the spectrum.
void main() {
  for (final blockCount in const [10, 20, 40, 80]) {
    testWidgets('quill per-edit cost at $blockCount blocks', (tester) async {
      final lines = List.generate(
        blockCount,
        (i) => 'task item number $i with a little inline text',
      ).join('\n');
      final document = Document()..insert(0, lines);
      final controller = QuillController(
        document: document,
        selection: const TextSelection.collapsed(offset: 0),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates:
              FlutterQuillLocalizations.localizationsDelegates,
          supportedLocales: FlutterQuillLocalizations.supportedLocales,
          home: Scaffold(
            body: SizedBox(
              width: 600,
              height: 600,
              child: QuillEditor.basic(controller: controller),
            ),
          ),
        ),
      );
      await tester.pump();

      void edit(int i) =>
          controller.replaceText(5, 0, 'x', const TextSelection.collapsed(offset: 6));

      for (var i = 0; i < 5; i += 1) {
        edit(i);
        await tester.pump();
      }

      const iterations = 40;
      final samples = <Duration>[];
      for (var i = 0; i < iterations; i += 1) {
        edit(i);
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
      }

      samples.sort();
      final median = samples[samples.length ~/ 2];
      final p95 = samples[((samples.length - 1) * 0.95).ceil()];
      debugPrint(
        'peer_benchmark quill_${blockCount}blocks '
        'pump_median=${_fmt(median)} pump_p95=${_fmt(p95)}',
      );
    });
  }
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
