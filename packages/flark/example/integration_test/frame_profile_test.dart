import 'dart:convert';
import 'dart:io';
import 'dart:ui' show FramePhase, FrameTiming;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('profile-mode optimistic input and frame receipt', (
    tester,
  ) async {
    const configuredLibrary = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
    const fixtureShape = String.fromEnvironment(
      'FLARK_PROFILE_SHAPE',
      defaultValue: 'ordinary',
    );
    const workload = String.fromEnvironment(
      'FLARK_PROFILE_WORKLOAD',
      defaultValue: 'typing',
    );
    const startDelayMs = int.fromEnvironment(
      'FLARK_PROFILE_START_DELAY_MS',
      defaultValue: 0,
    );
    const sourceBytes = int.fromEnvironment(
      'FLARK_PROFILE_SOURCE_BYTES',
      defaultValue: 1024 * 1024,
    );
    final libraryPath = configuredLibrary.isNotEmpty
        ? configuredLibrary
        : File(
            '../../../native/comrak_bridge/target/release/libflark_abi.dylib',
          ).absolute.path;
    final controller = await FlarkEditorController.open(
      _fixture(sourceBytes, shape: fixtureShape),
      libraryPath: libraryPath,
    );
    await controller.continueParsing();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox.expand(
          child: FlarkEditor(controller: controller, autofocus: true),
        ),
      ),
    );
    await tester.pump();
    if (startDelayMs > 0) {
      await Future<void>.delayed(Duration(milliseconds: startDelayMs));
    }

    final frameTimings = <FrameTiming>[];
    void recordTimings(List<FrameTiming> timings) =>
        frameTimings.addAll(timings);
    binding.addTimingsCallback(recordTimings);
    final inputToFrameMicros = <int>[];
    final inputHandlingMicros = <int>[];
    final inputFrameBuildMicros = <int>[];
    final settleMicros = <int>[];
    final undoSettleMicros = <int>[];
    // Engine vsync stamp of the frame each sample proved, used after the run
    // to join samples to their FrameTiming without perturbing the cadence.
    final sampleFrameStamps = <int>[];
    switch (workload) {
      case 'typing':
        for (var index = 0; index < 120; index += 1) {
          final before = controller.inputValue;
          final offset = before.selection.extentOffset;
          final watch = Stopwatch()..start();
          final inputWatch = Stopwatch()..start();
          controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: index.isEven ? 'x' : 'y',
              insertionOffset: offset,
              selection: TextSelection.collapsed(offset: offset + 1),
              composing: TextRange.empty,
            ),
          ]);
          inputWatch.stop();
          inputHandlingMicros.add(inputWatch.elapsedMicroseconds);
          await tester.pump();
          watch.stop();
          inputToFrameMicros.add(watch.elapsedMicroseconds);
          sampleFrameStamps.add(
            binding.currentSystemFrameTimeStamp.inMicroseconds,
          );
        }
      case 'paste-32kib':
        final baseBytes = controller.sourceByteLength;
        final baseUtf16 = controller.sourceUtf16Length;
        final paste = List.filled(32 * 1024, 'p').join();
        for (var index = 0; index < 14; index += 1) {
          // The workload's claim is paste-during-active-session. An
          // adaptive-refresh display serves first-paint-after-idle from its
          // low-power cadence (measured up to ~54 ms on this hardware, a
          // platform characteristic recorded in the build plan), and only
          // sustained activity trains it back to full rate. Warmup cycles
          // plus a cadence train before each paste keep the measured
          // samples on the trained display.
          final measured = index >= 4;
          for (var primer = 0; primer < (measured ? 8 : 2); primer += 1) {
            await tester.pump();
          }
          final before = controller.inputValue;
          final offset = before.selection.extentOffset;
          final settleWatch = Stopwatch()..start();
          final frameWatch = Stopwatch()..start();
          final inputWatch = Stopwatch()..start();
          controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: paste,
              insertionOffset: offset,
              selection: TextSelection.collapsed(offset: offset + paste.length),
              composing: TextRange.empty,
            ),
          ]);
          inputWatch.stop();
          if (measured) {
            inputHandlingMicros.add(inputWatch.elapsedMicroseconds);
          }
          final timingStart = frameTimings.length;
          await tester.pump();
          frameWatch.stop();
          if (measured) {
            inputToFrameMicros.add(frameWatch.elapsedMicroseconds);
          }
          expect(
            controller.inputValue.text.length,
            lessThanOrEqualTo(16 * 1024),
          );
          expect(controller.visibleSource.length, lessThanOrEqualTo(16 * 1024));
          await _waitForPending(controller);
          settleWatch.stop();
          if (measured) {
            settleMicros.add(settleWatch.elapsedMicroseconds);
            await _captureInputFrameBuild(
              frameTimings,
              timingStart,
              inputFrameBuildMicros,
            );
          }
          expect(controller.sourceByteLength, baseBytes + paste.length);
          expect(controller.sourceUtf16Length, baseUtf16 + paste.length);
          final undoWatch = Stopwatch()..start();
          expect(await controller.undo(), isTrue);
          await _waitForPending(controller);
          undoWatch.stop();
          if (measured) undoSettleMicros.add(undoWatch.elapsedMicroseconds);
          await tester.pump();
          expect(controller.sourceByteLength, baseBytes);
          expect(controller.sourceUtf16Length, baseUtf16);
        }
      default:
        throw ArgumentError.value(
          workload,
          'FLARK_PROFILE_WORKLOAD',
          'unsupported workload',
        );
    }

    await _waitForPending(controller);
    await tester.pump();
    await Future<void>.delayed(const Duration(milliseconds: 100));
    binding.removeTimingsCallback(recordTimings);

    // Wall samples from a throttled (occluded or napping) app alternate with
    // ~50 ms display intervals, and a sleeping display leaves multi-second
    // holes of missing vsync. Either signature invalidates the entire run,
    // so the receipt records it and the harness fails loudly instead of
    // emitting rejectable numbers.
    final throttledSamples = inputToFrameMicros
        .where((value) => value >= 30000)
        .length;
    final throttledFraction = inputToFrameMicros.isEmpty
        ? 0.0
        : throttledSamples / inputToFrameMicros.length;
    final displayQuietSample = _maximum(inputToFrameMicros) >= 1000000;
    final foregroundValid = throttledFraction < 0.1 && !displayQuietSample;

    final buildMicros = frameTimings
        .map((timing) => timing.buildDuration.inMicroseconds)
        .toList();
    final rasterMicros = frameTimings
        .map((timing) => timing.rasterDuration.inMicroseconds)
        .toList();

    // Per-sample attribution. A threshold on wall time alone cannot tell an
    // editor-attributed over-budget frame from a display hole, which the
    // evidence contract requires distinguishing, so every over-budget sample
    // is joined to the frame it proved and classified from that frame's own
    // phases and the vsync gap preceding it.
    final orderedTimings = [...frameTimings]
      ..sort(
        (a, b) => a
            .timestampInMicroseconds(FramePhase.vsyncStart)
            .compareTo(b.timestampInMicroseconds(FramePhase.vsyncStart)),
      );
    final attributions = <Map<String, Object?>>[];
    var editorAttributedOverBudget = 0;
    var displayAttributedOverBudget = 0;
    var unexplainedOverBudget = 0;
    for (var index = 0; index < sampleFrameStamps.length; index += 1) {
      final wall = inputToFrameMicros[index];
      if (wall < 16000) continue;
      final stamp = sampleFrameStamps[index];
      var provingIndex = -1;
      for (var candidate = 0; candidate < orderedTimings.length; candidate += 1) {
        final vsync = orderedTimings[candidate].timestampInMicroseconds(
          FramePhase.vsyncStart,
        );
        if (vsync <= stamp) {
          provingIndex = candidate;
        } else {
          break;
        }
      }
      final proving = provingIndex >= 0 ? orderedTimings[provingIndex] : null;
      final previous = provingIndex > 0 ? orderedTimings[provingIndex - 1] : null;
      final editorMicros = proving == null
          ? 0
          : proving.buildDuration.inMicroseconds +
              proving.rasterDuration.inMicroseconds;
      final gapMicros = proving == null || previous == null
          ? 0
          : proving.timestampInMicroseconds(FramePhase.vsyncStart) -
              previous.timestampInMicroseconds(FramePhase.vsyncStart);
      final String verdict;
      if (editorMicros >= 16000) {
        verdict = 'editor';
        editorAttributedOverBudget += 1;
      } else if (gapMicros >= wall * 0.8) {
        verdict = 'display';
        displayAttributedOverBudget += 1;
      } else {
        verdict = 'unexplained';
        unexplainedOverBudget += 1;
      }
      attributions.add({
        'sample': index,
        'wallMs': wall / 1000,
        'editorMs': editorMicros / 1000,
        'vsyncGapMs': gapMicros / 1000,
        'verdict': verdict,
      });
    }

    // Distinguishes a quiet display (large inter-frame vsync gap) from a
    // starved await (steady vsync while a sample stalled).
    final vsyncStarts = frameTimings
        .map((timing) => timing.timestampInMicroseconds(FramePhase.vsyncStart))
        .toList();
    final vsyncGaps = <List<num>>[];
    for (var index = 1; index < vsyncStarts.length; index += 1) {
      vsyncGaps.add([vsyncStarts[index] - vsyncStarts[index - 1], index]);
    }
    vsyncGaps.sort((a, b) => b.first.compareTo(a.first));
    final vsyncGapTopMs = vsyncGaps
        .take(5)
        .map((gap) => [(gap.first as int) / 1000, gap.last])
        .toList();
    stdout.writeln(
      'FLARK_PROFILE_RECEIPT ${jsonEncode({'fixtureShape': fixtureShape, 'workload': workload, 'sourceBytes': controller.sourceByteLength, 'inputSamples': inputToFrameMicros.length, 'inputHandlingRawMs': inputHandlingMicros.map((value) => value / 1000).toList(), 'inputHandlingP50Ms': _percentile(inputHandlingMicros, 50) / 1000, 'inputHandlingP99Ms': _percentile(inputHandlingMicros, 99) / 1000, 'inputHandlingMaxMs': _maximum(inputHandlingMicros) / 1000, 'inputToFrameRawMs': inputToFrameMicros.map((value) => value / 1000).toList(), 'inputToFrameP50Ms': _percentile(inputToFrameMicros, 50) / 1000, 'inputToFrameP99Ms': _percentile(inputToFrameMicros, 99) / 1000, 'inputToFrameMaxMs': _maximum(inputToFrameMicros) / 1000, 'inputFrameBuildRawMs': inputFrameBuildMicros.map((value) => value / 1000).toList(), 'inputFrameBuildP50Ms': _percentile(inputFrameBuildMicros, 50) / 1000, 'inputFrameBuildP99Ms': _percentile(inputFrameBuildMicros, 99) / 1000, 'inputFrameBuildMaxMs': _maximum(inputFrameBuildMicros) / 1000, 'settleRawMs': settleMicros.map((value) => value / 1000).toList(), 'settleP50Ms': _percentile(settleMicros, 50) / 1000, 'settleP99Ms': _percentile(settleMicros, 99) / 1000, 'settleMaxMs': _maximum(settleMicros) / 1000, 'undoSettleRawMs': undoSettleMicros.map((value) => value / 1000).toList(), 'undoSettleMaxMs': _maximum(undoSettleMicros) / 1000, 'frameSamples': frameTimings.length, 'buildP99Ms': _percentile(buildMicros, 99) / 1000, 'buildMaxMs': _maximum(buildMicros) / 1000, 'rasterP99Ms': _percentile(rasterMicros, 99) / 1000, 'rasterMaxMs': _maximum(rasterMicros) / 1000, 'vsyncGapTopMs': vsyncGapTopMs, 'overBudgetAttribution': attributions, 'editorAttributedOverBudget': editorAttributedOverBudget, 'displayAttributedOverBudget': displayAttributedOverBudget, 'unexplainedOverBudget': unexplainedOverBudget, 'throttledFrameFraction': throttledFraction, 'foregroundValid': foregroundValid, 'pendingEdits': controller.pendingEdits})}',
    );

    expect(
      foregroundValid,
      isTrue,
      reason:
          'wall samples show a throttled or quiet display '
          '(${(throttledFraction * 100).toStringAsFixed(1)}% >= 30 ms, '
          'max ${(_maximum(inputToFrameMicros) / 1000).toStringAsFixed(1)} ms); '
          'the display was not live for the whole run, so it is not evidence',
    );
    expect(controller.pendingEdits, 0);
    expect(controller.lastError, isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    await controller.close();
  });
}

Future<void> _waitForPending(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  expect(controller.pendingEdits, 0);
  expect(controller.lastError, isNull);
}

Future<void> _captureInputFrameBuild(
  List<FrameTiming> timings,
  int start,
  List<int> output,
) async {
  final deadline = DateTime.now().add(const Duration(milliseconds: 100));
  while (timings.length == start && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  if (timings.length > start) {
    output.add(timings[start].buildDuration.inMicroseconds);
  }
}

int _percentile(List<int> values, int percentile) {
  if (values.isEmpty) return 0;
  final sorted = [...values]..sort();
  final index = ((sorted.length * percentile + 99) ~/ 100 - 1).clamp(
    0,
    sorted.length - 1,
  );
  return sorted[index];
}

int _maximum(List<int> values) =>
    values.fold(0, (maximum, value) => value > maximum ? value : maximum);

String _fixture(int targetBytes, {required String shape}) {
  if (shape == 'giant-line') {
    // One physical line filling the whole target: the hostile case for
    // bounded windows, paging, and fragment layout.
    final buffer = StringBuffer();
    const token = 'giantword ';
    while (buffer.length < targetBytes - 1) {
      buffer.write(token);
    }
    return '${buffer.toString().substring(0, targetBytes - 1)}\n';
  }
  final (prefix, block) = switch (shape) {
    'ordinary' => (
      '',
      '## Section\n\nA quick paragraph with **bold text**.\n\n',
    ),
    'dense-inline' => (
      '[id]: /target\n\n',
      '## Section\n\n\\* &ngE; *em* **strong** ~~strike~~ ` a ` '
          '[direct](https://a.test) [ref][id] ![alt](image.png) '
          '<https://b.test>  \nnext\n\n',
    ),
    'tiny-blocks' => ('', 'x.\n\n'),
    _ => throw ArgumentError.value(shape, 'shape', 'unsupported fixture shape'),
  };
  final buffer = StringBuffer(prefix);
  final remainingBytes = targetBytes - prefix.length;
  final fullBlocks = remainingBytes ~/ block.length;
  final remainder = remainingBytes % block.length;
  for (var index = 0; index < fullBlocks; index += 1) {
    buffer.write(block);
  }
  buffer.write(block.substring(0, remainder));
  return buffer.toString();
}
