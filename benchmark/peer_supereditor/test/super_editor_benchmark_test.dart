import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:super_editor/super_editor.dart';

// Peer calibration: super_editor (block-based WYSIWYG live editor — the closest
// architectural peer to Flark's live-rendered mode) per-edit render cost at
// matching block counts. Current git version (0.3.0-dev.*); same methodology as
// Flark's rebuild benchmark.
void main() {
  for (final blockCount in const [10, 20, 40, 80]) {
    testWidgets('super_editor per-edit cost at $blockCount blocks', (
      tester,
    ) async {
      final document = MutableDocument(
        nodes: [
          for (var i = 0; i < blockCount; i += 1)
            ParagraphNode(
              id: 'p$i',
              text: AttributedText(
                'task item number $i with a little inline text',
              ),
            ),
        ],
      );
      final composer = MutableDocumentComposer(
        initialSelection: const DocumentSelection.collapsed(
          position: DocumentPosition(
            nodeId: 'p0',
            nodePosition: TextNodePosition(offset: 5),
          ),
        ),
      );
      final editor = createDefaultDocumentEditor(
        document: document,
        composer: composer,
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 600,
              height: 600,
              child: SuperEditor(editor: editor),
            ),
          ),
        ),
      );
      await tester.pump();

      void edit() {
        editor.execute([
          InsertTextRequest(
            documentPosition: const DocumentPosition(
              nodeId: 'p0',
              nodePosition: TextNodePosition(offset: 5),
            ),
            textToInsert: 'x',
            attributions: const {},
          ),
        ]);
      }

      for (var i = 0; i < 5; i += 1) {
        edit();
        await tester.pump();
      }

      const iterations = 40;
      final samples = <Duration>[];
      for (var i = 0; i < iterations; i += 1) {
        edit();
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
      }

      samples.sort();
      final median = samples[samples.length ~/ 2];
      final p95 = samples[((samples.length - 1) * 0.95).ceil()];
      debugPrint(
        'peer_benchmark supereditor_${blockCount}blocks '
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
