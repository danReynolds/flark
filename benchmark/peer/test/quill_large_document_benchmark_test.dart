import 'package:flutter/material.dart';
import 'package:flutter_quill/flutter_quill.dart';
import 'package:flutter_test/flutter_test.dart';

int _blackHole = 0;

void main() {
  for (final size in _sizes) {
    test('quill model build at ${size.label}', () {
      final text = _largePlainText(size.targetChars);
      final result = _measure(
        'quill_model_build_${size.label}_${text.length}chars',
        iterations: size.modelIterations,
        warmups: size.modelWarmups,
        body: () {
          final document = Document()..insert(0, text);
          return document.length;
        },
      );
      _report(result);
    });

    test('quill edit apply at ${size.label}', () {
      final text = _largePlainText(size.targetChars);
      final document = Document()..insert(0, text);
      final controller = QuillController(
        document: document,
        selection: const TextSelection.collapsed(offset: 0),
      );
      addTearDown(controller.dispose);
      final result = _measure(
        'quill_edit_apply_${size.label}_${text.length}chars',
        iterations: size.editIterations,
        warmups: size.editWarmups,
        body: () {
          controller.replaceText(
            5,
            0,
            'x',
            const TextSelection.collapsed(offset: 6),
          );
          return controller.document.length;
        },
      );
      _report(result);
    });

    testWidgets('quill viewport pump after edit at ${size.label}', (
      tester,
    ) async {
      final text = _largePlainText(size.targetChars);
      final document = Document()..insert(0, text);
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

      for (var i = 0; i < size.pumpWarmups; i += 1) {
        _edit(controller);
        await tester.pump();
      }

      final samples = <Duration>[];
      for (var i = 0; i < size.pumpIterations; i += 1) {
        _edit(controller);
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
      }
      _report(
        _BenchmarkResult(
          name: 'quill_edit_pump_${size.label}_${text.length}chars',
          samples: samples,
        ),
      );
    });
  }
}

void _edit(QuillController controller) {
  controller.replaceText(5, 0, 'x', const TextSelection.collapsed(offset: 6));
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

String _largePlainText(int targetChars) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetChars) {
    buffer
      ..write('paragraph ')
      ..write(index)
      ..write(' ')
      ..write('with enough text to make a realistic editor line. ' * 3)
      ..writeln();
    index += 1;
  }
  return buffer.toString();
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
