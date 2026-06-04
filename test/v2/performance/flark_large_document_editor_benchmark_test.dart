@Tags(<String>['benchmark'])
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

int _blackHole = 0;

void main() {
  for (final size in _sizes) {
    test('flark controller build at ${size.label}', () {
      final text = _largePlainText(size.targetChars);
      final result = _measure(
        'flark_controller_build_${size.label}_${text.length}chars',
        iterations: size.modelIterations,
        warmups: size.modelWarmups,
        body: () {
          final controller = FlarkFlutterController.fromMarkdown(text);
          final value = controller.markdown.length;
          controller.dispose();
          return value;
        },
      );
      _report(result);
    });

    test('flark source edit apply at ${size.label}', () {
      final text = _largePlainText(size.targetChars);
      final state = FlarkEditorState.fromMarkdown(text);
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(5, 'x'),
        metadata: const FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'benchmark.largeDocInsert',
        ),
      );
      final result = _measure(
        'flark_source_edit_apply_${size.label}_${text.length}chars',
        iterations: size.editIterations,
        warmups: size.editWarmups,
        body: () {
          final next = state.applyTransaction(transaction);
          return next.document.length + next.selection.extentOffset;
        },
      );
      _report(result);
    });

    testWidgets('flark source viewport pump after edit at ${size.label}', (
      tester,
    ) async {
      final text = _largePlainText(size.targetChars);
      final controller = FlarkFlutterController.fromMarkdown(text);
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: FlarkEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14),
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
          name: 'flark_source_edit_pump_${size.label}_${text.length}chars',
          samples: samples,
        ),
      );
    });

    testWidgets('raw EditableText viewport pump after edit at ${size.label}', (
      tester,
    ) async {
      final text = _largePlainText(size.targetChars);
      final textController = TextEditingController(text: text);
      final focusNode = FocusNode();
      addTearDown(textController.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: EditableText(
              controller: textController,
              focusNode: focusNode,
              style: const TextStyle(fontSize: 14),
              cursorColor: const Color(0xFF006ADC),
              backgroundCursorColor: const Color(0x00000000),
              maxLines: null,
              keyboardType: TextInputType.multiline,
              textInputAction: TextInputAction.newline,
            ),
          ),
        ),
      );
      await tester.pump();

      for (var i = 0; i < size.pumpWarmups; i += 1) {
        _editRawText(textController);
        await tester.pump();
      }

      final samples = <Duration>[];
      for (var i = 0; i < size.pumpIterations; i += 1) {
        _editRawText(textController);
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
      }
      _report(
        _BenchmarkResult(
          name: 'raw_editable_text_pump_${size.label}_${text.length}chars',
          samples: samples,
        ),
      );
    });

    testWidgets('flark live rendered viewport pump after edit at ${size.label}', (
      tester,
    ) async {
      final backend = FlarkNativeComrakParseBackend.tryLoad();
      if (backend == null) {
        debugPrint(
          'flark_benchmark flark_live_edit_pump_${size.label} skipped=no_bridge',
        );
        return;
      }
      final text = _largePlainText(size.targetChars);
      final controller = FlarkFlutterController.fromMarkdown(text);
      addTearDown(controller.dispose);
      final parsed = await tester.runAsync(
        () => backend.parse(
          FlarkMarkdownParseRequest(
            revision: controller.state.revision,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        ),
      );
      expect(controller.applyParseResult(parsed!), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: FlarkLiveRenderedEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14),
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
          name: 'flark_live_edit_pump_${size.label}_${text.length}chars',
          samples: samples,
        ),
      );
    });
  }
}

void _edit(FlarkFlutterController controller) {
  controller.applyTransaction(
    FlarkTransaction.single(
      FlarkSourceOperation.insert(5, 'x'),
      metadata: const FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'benchmark.largeDocInsert',
      ),
    ),
  );
}

void _editRawText(TextEditingController controller) {
  final current = controller.value;
  controller.value = TextEditingValue(
    text: current.text.replaceRange(5, 5, 'x'),
    selection: const TextSelection.collapsed(offset: 6),
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
  debugPrint('flark_benchmark ${result.summary}');
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
