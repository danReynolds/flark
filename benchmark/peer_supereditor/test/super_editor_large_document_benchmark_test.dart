import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:super_editor/super_editor.dart';

int _blackHole = 0;

void main() {
  for (final size in _sizes) {
    test('super_editor model build at ${size.label}', () {
      final paragraphs = _largeParagraphs(size.targetChars);
      final result = _measure(
        'supereditor_model_build_${size.label}_${_charCount(paragraphs)}chars',
        iterations: size.modelIterations,
        warmups: size.modelWarmups,
        body: () {
          final document = _documentFromParagraphs(paragraphs);
          return document.nodeCount;
        },
      );
      _report(result);
    });

    test('super_editor edit apply at ${size.label}', () {
      final paragraphs = _largeParagraphs(size.targetChars);
      final document = _documentFromParagraphs(paragraphs);
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
      final result = _measure(
        'supereditor_edit_apply_${size.label}_${_charCount(paragraphs)}chars',
        iterations: size.editIterations,
        warmups: size.editWarmups,
        body: () {
          _edit(editor);
          return document.nodeCount;
        },
      );
      _report(result);
    });

    testWidgets('super_editor viewport pump after edit at ${size.label}', (
      tester,
    ) async {
      final paragraphs = _largeParagraphs(size.targetChars);
      final document = _documentFromParagraphs(paragraphs);
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

      for (var i = 0; i < size.pumpWarmups; i += 1) {
        _edit(editor);
        await tester.pump();
      }

      final samples = <Duration>[];
      for (var i = 0; i < size.pumpIterations; i += 1) {
        _edit(editor);
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
      }
      _report(
        _BenchmarkResult(
          name:
              'supereditor_edit_pump_${size.label}_${_charCount(paragraphs)}chars',
          samples: samples,
        ),
      );
    });
  }
}

void _edit(Editor editor) {
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

MutableDocument _documentFromParagraphs(List<String> paragraphs) {
  return MutableDocument(
    nodes: [
      for (var i = 0; i < paragraphs.length; i += 1)
        ParagraphNode(id: 'p$i', text: AttributedText(paragraphs[i])),
    ],
  );
}

const _sizes = [
  _Size(
    label: '100KB',
    targetChars: 100000,
    modelIterations: 12,
    modelWarmups: 3,
    editIterations: 40,
    editWarmups: 8,
    pumpIterations: 20,
    pumpWarmups: 5,
  ),
  _Size(
    label: '1MB',
    targetChars: 1000000,
    modelIterations: 5,
    modelWarmups: 1,
    editIterations: 20,
    editWarmups: 4,
    pumpIterations: 10,
    pumpWarmups: 3,
  ),
];

final class _Size {
  const _Size({
    required this.label,
    required this.targetChars,
    required this.modelIterations,
    required this.modelWarmups,
    required this.editIterations,
    required this.editWarmups,
    required this.pumpIterations,
    required this.pumpWarmups,
  });

  final String label;
  final int targetChars;
  final int modelIterations;
  final int modelWarmups;
  final int editIterations;
  final int editWarmups;
  final int pumpIterations;
  final int pumpWarmups;
}

List<String> _largeParagraphs(int targetChars) {
  final paragraphs = <String>[];
  var chars = 0;
  var index = 0;
  while (chars < targetChars) {
    final paragraph =
        'paragraph $index ${'with enough text to make a realistic editor line. ' * 3}';
    paragraphs.add(paragraph);
    chars += paragraph.length + 1;
    index += 1;
  }
  return paragraphs;
}

int _charCount(List<String> paragraphs) {
  if (paragraphs.isEmpty) return 0;
  return paragraphs.fold<int>(0, (sum, text) => sum + text.length + 1);
}

_BenchmarkResult _measure(
  String name, {
  required int iterations,
  required int warmups,
  required int Function() body,
}) {
  for (var i = 0; i < warmups; i += 1) {
    _consume(body());
  }

  final samples = <Duration>[];
  for (var i = 0; i < iterations; i += 1) {
    final stopwatch = Stopwatch()..start();
    _consume(body());
    stopwatch.stop();
    samples.add(stopwatch.elapsed);
  }
  return _BenchmarkResult(name: name, samples: samples);
}

void _consume(int value) {
  _blackHole = (_blackHole + value) & 0x3fffffff;
}

void _report(_BenchmarkResult result) {
  debugPrint('peer_benchmark ${result.summary}');
}

final class _BenchmarkResult {
  _BenchmarkResult({required this.name, required Iterable<Duration> samples})
    : samples = List<Duration>.unmodifiable(
        [...samples]..sort((left, right) => left.compareTo(right)),
      );

  final String name;
  final List<Duration> samples;

  Duration get min => samples.first;
  Duration get median => samples[samples.length ~/ 2];
  Duration get p95 => samples[((samples.length - 1) * 0.95).ceil()];
  Duration get max => samples.last;

  String get summary {
    return '$name iterations=${samples.length} '
        'min=${_fmt(min)} median=${_fmt(median)} p95=${_fmt(p95)} '
        'max=${_fmt(max)} blackHole=$_blackHole';
  }
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
