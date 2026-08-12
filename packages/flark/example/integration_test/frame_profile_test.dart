import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
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
          child: FlarkEditor(
            controller: controller,
            autofocus: workload != 'semantic-burst',
          ),
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
    final semanticPlatformCallbackMicros = <int>[];
    final semanticCoreQueueMicros = <int>[];
    final semanticWorkerRoundTripMicros = <int>[];
    final semanticWorkerQueueMicros = <int>[];
    final semanticNativeFfiMicros = <int>[];
    final semanticCoreAdoptionMicros = <int>[];
    final semanticFlutterAdoptionMicros = <int>[];
    final semanticCallbackToReceiptMicros = <int>[];
    var projectedContinuitySamples = 0;
    var rawProjectionFrames = 0;
    var missingActiveProjectionFrames = 0;
    var markerProjectionFrames = 0;
    var missingCaretInsideSourceRowFrames = 0;
    var missingCaretOutsideSourceRowsFrames = 0;
    // Engine vsync stamp of the frame each sample proved, used after the run
    // to join samples to their FrameTiming without perturbing the cadence.
    final sampleFrameStamps = <int>[];
    switch (workload) {
      case 'typing':
        // The evidence contract prescribes 20 warmups per sustained-typing
        // run, excluded from the distribution. They also carry the adaptive
        // display from its idle cadence to full rate, so the measured
        // samples describe sustained typing rather than the ramp.
        const typingWarmups = 20;
        for (var index = 0; index < typingWarmups + 120; index += 1) {
          final measured = index >= typingWarmups;
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
          await tester.pump();
          watch.stop();
          if (!measured) continue;
          inputHandlingMicros.add(inputWatch.elapsedMicroseconds);
          inputToFrameMicros.add(watch.elapsedMicroseconds);
          sampleFrameStamps.add(
            binding.currentSystemFrameTimeStamp.inMicroseconds,
          );
        }
      case 'inline-typing':
        final inlineRow = controller.rows.firstWhere(
          (row) =>
              row.inlineFacts?.any((fact) => fact.kind.name == 'strong') ??
              false,
        );
        final strong = inlineRow.inlineFacts!.firstWhere(
          (fact) => fact.kind.name == 'strong',
        );
        controller.activateRow(inlineRow, strong.contentUtf16.start + 2);
        await tester.pump();
        const typingWarmups = 20;
        for (var index = 0; index < typingWarmups + 120; index += 1) {
          final measured = index >= typingWarmups;
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
          await tester.pump();
          watch.stop();
          FlarkSurfaceRow? presentation;
          for (final row in controller.rows) {
            final candidate = controller.surfaceRow(row);
            if (candidate.active) {
              presentation = candidate;
              break;
            }
          }
          if (measured) {
            projectedContinuitySamples += 1;
            if (presentation == null) {
              missingActiveProjectionFrames += 1;
              final caret = controller.globalCaretOffset;
              final caretInsideSourceRow = controller.rows.any(
                (row) =>
                    row.sourceUtf16.start <= caret &&
                    caret < row.sourceUtf16.end,
              );
              if (caretInsideSourceRow) {
                missingCaretInsideSourceRowFrames += 1;
              } else {
                missingCaretOutsideSourceRowsFrames += 1;
              }
              rawProjectionFrames += 1;
            } else if (presentation.text.contains('**')) {
              markerProjectionFrames += 1;
              rawProjectionFrames += 1;
            }
            inputHandlingMicros.add(inputWatch.elapsedMicroseconds);
            inputToFrameMicros.add(watch.elapsedMicroseconds);
            sampleFrameStamps.add(
              binding.currentSystemFrameTimeStamp.inMicroseconds,
            );
          }
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
      case 'semantic-burst':
        final target = controller.rows.firstWhere(
          (row) =>
              row.kind == 5 &&
              row.editableUtf16 != null &&
              row.inlineFacts?.any((fact) => fact.kind.name == 'strong') ==
                  true,
        );
        final selectionGeneration = controller.canonicalSelectionGeneration;
        controller.activateRow(target, target.editableUtf16!.end);
        await _waitForCanonicalSelection(controller, selectionGeneration);
        await tester.pump();
        stdout.writeln(
          'FLARK_SEMANTIC_TARGET ${jsonEncode({'ordinal': target.ordinal, 'kind': target.kind, 'editableStart': target.editableUtf16!.start, 'editableEnd': target.editableUtf16!.end, 'inputStart': controller.inputWindowShadow.globalUtf16Start, 'inputCaret': controller.inputValue.selection.extentOffset, 'inputLength': controller.inputValue.text.length, 'surfaceActive': controller.surfaceRow(target).active})}',
        );
        var previousSourceBytes = controller.sourceByteLength;
        const warmups = 20;
        for (var index = 0; index < warmups + 120; index += 1) {
          final measured = index >= warmups;
          final priorPerformance = controller.lastSemanticEditPerformance;
          final before = controller.inputValue;
          final offset = before.selection.extentOffset;
          final provisionalText = before.text.replaceRange(
            offset,
            offset,
            '\n',
          );
          final watch = Stopwatch()..start();
          controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: '\n',
              insertionOffset: offset,
              selection: TextSelection.collapsed(offset: offset + 1),
              composing: TextRange.empty,
            ),
          ]);
          controller.observePlatformNewlineAction();
          controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: provisionalText,
              textInserted: 'x',
              insertionOffset: offset + 1,
              selection: TextSelection.collapsed(offset: offset + 2),
              composing: TextRange.empty,
            ),
          ]);
          if (index == 0) {
            stdout.writeln(
              'FLARK_SEMANTIC_DISPATCH ${jsonEncode({'pendingEdits': controller.pendingEdits, 'revision': controller.revision, 'resyncCount': controller.resyncCount, 'resyncReason': controller.lastResyncReason.name, 'inputLength': controller.inputValue.text.length, 'inputCaret': controller.inputValue.selection.extentOffset})}',
            );
          }
          final performance = await _waitForSemanticReceipt(
            controller,
            priorPerformance,
          );
          await _waitForPending(controller);
          await tester.pump();
          watch.stop();
          if (measured) {
            inputHandlingMicros.add(performance.platformCallbackMicros);
            inputToFrameMicros.add(watch.elapsedMicroseconds);
            sampleFrameStamps.add(
              binding.currentSystemFrameTimeStamp.inMicroseconds,
            );
            semanticPlatformCallbackMicros.add(
              performance.platformCallbackMicros,
            );
            semanticCoreQueueMicros.add(performance.coreQueueMicros);
            semanticWorkerRoundTripMicros.add(
              performance.workerRoundTripMicros,
            );
            semanticWorkerQueueMicros.add(performance.workerQueueMicros);
            semanticNativeFfiMicros.add(performance.nativeFfiMicros);
            semanticCoreAdoptionMicros.add(performance.coreAdoptionMicros);
            semanticFlutterAdoptionMicros.add(
              performance.flutterReceiptAdoptionMicros,
            );
            semanticCallbackToReceiptMicros.add(
              performance.callbackToReceiptMicros,
            );
          }
          expect(controller.sourceByteLength, greaterThan(previousSourceBytes));
          previousSourceBytes = controller.sourceByteLength;
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
    // The editor's own per-keystroke cost: input handling plus the proving
    // frame's build and raster. Unlike wall time this does not include
    // waiting for the display, so it stays meaningful when an adaptive
    // panel is serving below the frame budget.
    final editorLatencyMicros = <int>[];
    final servedIntervals = <int>[];
    var editorAttributedOverBudget = 0;
    var displayAttributedOverBudget = 0;
    var unexplainedOverBudget = 0;
    for (var index = 0; index < sampleFrameStamps.length; index += 1) {
      final stamp = sampleFrameStamps[index];
      var provingForLatency = -1;
      for (
        var candidate = 0;
        candidate < orderedTimings.length;
        candidate += 1
      ) {
        if (orderedTimings[candidate].timestampInMicroseconds(
              FramePhase.vsyncStart,
            ) <=
            stamp) {
          provingForLatency = candidate;
        } else {
          break;
        }
      }
      if (provingForLatency >= 0) {
        final frame = orderedTimings[provingForLatency];
        editorLatencyMicros.add(
          inputHandlingMicros[index] +
              frame.buildDuration.inMicroseconds +
              frame.rasterDuration.inMicroseconds,
        );
        if (provingForLatency > 0) {
          servedIntervals.add(
            frame.timestampInMicroseconds(FramePhase.vsyncStart) -
                orderedTimings[provingForLatency - 1].timestampInMicroseconds(
                  FramePhase.vsyncStart,
                ),
          );
        }
      }
      final wall = inputToFrameMicros[index];
      if (wall < 16000) continue;
      var provingIndex = -1;
      for (
        var candidate = 0;
        candidate < orderedTimings.length;
        candidate += 1
      ) {
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
      final previous = provingIndex > 0
          ? orderedTimings[provingIndex - 1]
          : null;
      final editorMicros = proving == null
          ? 0
          : proving.buildDuration.inMicroseconds +
                proving.rasterDuration.inMicroseconds;
      final gapMicros = proving == null || previous == null
          ? 0
          : proving.timestampInMicroseconds(FramePhase.vsyncStart) -
                previous.timestampInMicroseconds(FramePhase.vsyncStart);
      // A single preceding gap misjudges a display that delivers frames in
      // bursts: the second frame of a pair has a small gap even though the
      // sample waited on the burst cadence. The served rate over the window
      // leading into the sample is the honest measure.
      final windowStart = math.max(0, provingIndex - 8);
      final windowSpan = provingIndex > windowStart
          ? proving!.timestampInMicroseconds(FramePhase.vsyncStart) -
                orderedTimings[windowStart].timestampInMicroseconds(
                  FramePhase.vsyncStart,
                )
          : 0;
      final windowFrames = provingIndex - windowStart;
      final servedIntervalMicros = windowFrames > 0
          ? windowSpan ~/ windowFrames
          : 0;
      final String verdict;
      if (editorMicros >= 16000) {
        verdict = 'editor';
        editorAttributedOverBudget += 1;
      } else if (gapMicros >= wall * 0.8 || servedIntervalMicros >= 16000) {
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
        'servedIntervalMs': servedIntervalMicros / 1000,
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
    final finalViewport = controller.viewport;
    final finalInputWindow = controller.inputWindowShadow;
    stdout.writeln(
      'FLARK_PROFILE_STATE ${jsonEncode({'visibleUtf16Start': controller.visibleUtf16Start, 'visibleUtf16Length': controller.visibleSource.length, 'inputUtf16Start': finalInputWindow.globalUtf16Start, 'inputUtf16Length': finalInputWindow.windowUtf16Length, 'viewportCoveredUtf16Start': finalViewport?.coveredUtf16.start, 'viewportCoveredUtf16End': finalViewport?.coveredUtf16.end, 'viewportRowCount': finalViewport?.rows.length, 'viewportRevision': finalViewport?.revision, 'documentRevision': controller.revision})}',
    );
    stdout.writeln(
      'FLARK_PROFILE_RECEIPT ${jsonEncode({'fixtureShape': fixtureShape, 'workload': workload, 'sourceBytes': controller.sourceByteLength, 'inputSamples': inputToFrameMicros.length, 'projectedContinuitySamples': projectedContinuitySamples, 'rawProjectionFrames': rawProjectionFrames, 'missingActiveProjectionFrames': missingActiveProjectionFrames, 'markerProjectionFrames': markerProjectionFrames, 'missingCaretInsideSourceRowFrames': missingCaretInsideSourceRowFrames, 'missingCaretOutsideSourceRowsFrames': missingCaretOutsideSourceRowsFrames, 'finalCaretUtf16': controller.globalCaretOffset, 'finalRowCount': controller.rows.length, 'inputHandlingRawMs': inputHandlingMicros.map((value) => value / 1000).toList(), 'inputHandlingP50Ms': _percentile(inputHandlingMicros, 50) / 1000, 'inputHandlingP99Ms': _percentile(inputHandlingMicros, 99) / 1000, 'inputHandlingMaxMs': _maximum(inputHandlingMicros) / 1000, 'inputToFrameRawMs': inputToFrameMicros.map((value) => value / 1000).toList(), 'inputToFrameP50Ms': _percentile(inputToFrameMicros, 50) / 1000, 'inputToFrameP99Ms': _percentile(inputToFrameMicros, 99) / 1000, 'inputToFrameMaxMs': _maximum(inputToFrameMicros) / 1000, 'inputFrameBuildRawMs': inputFrameBuildMicros.map((value) => value / 1000).toList(), 'inputFrameBuildP50Ms': _percentile(inputFrameBuildMicros, 50) / 1000, 'inputFrameBuildP99Ms': _percentile(inputFrameBuildMicros, 99) / 1000, 'inputFrameBuildMaxMs': _maximum(inputFrameBuildMicros) / 1000, 'settleRawMs': settleMicros.map((value) => value / 1000).toList(), 'settleP50Ms': _percentile(settleMicros, 50) / 1000, 'settleP99Ms': _percentile(settleMicros, 99) / 1000, 'settleMaxMs': _maximum(settleMicros) / 1000, 'undoSettleRawMs': undoSettleMicros.map((value) => value / 1000).toList(), 'undoSettleMaxMs': _maximum(undoSettleMicros) / 1000, 'frameSamples': frameTimings.length, 'buildP99Ms': _percentile(buildMicros, 99) / 1000, 'buildMaxMs': _maximum(buildMicros) / 1000, 'rasterP99Ms': _percentile(rasterMicros, 99) / 1000, 'rasterMaxMs': _maximum(rasterMicros) / 1000, 'vsyncGapTopMs': vsyncGapTopMs, 'editorLatencyP50Ms': _percentile(editorLatencyMicros, 50) / 1000, 'editorLatencyP99Ms': _percentile(editorLatencyMicros, 99) / 1000, 'editorLatencyMaxMs': _maximum(editorLatencyMicros) / 1000, 'servedIntervalP50Ms': _percentile(servedIntervals, 50) / 1000, 'servedDisplayHz': servedIntervals.isEmpty ? 0 : (1000000 / _percentile(servedIntervals, 50)).round(), 'overBudgetAttribution': attributions, 'editorAttributedOverBudget': editorAttributedOverBudget, 'displayAttributedOverBudget': displayAttributedOverBudget, 'unexplainedOverBudget': unexplainedOverBudget, 'throttledFrameFraction': throttledFraction, 'foregroundValid': foregroundValid, 'pendingEdits': controller.pendingEdits})}',
    );
    if (semanticCallbackToReceiptMicros.isNotEmpty) {
      stdout.writeln(
        'FLARK_SEMANTIC_RECEIPT ${jsonEncode({'platformCallback': _distribution(semanticPlatformCallbackMicros), 'coreQueue': _distribution(semanticCoreQueueMicros), 'workerRoundTrip': _distribution(semanticWorkerRoundTripMicros), 'workerQueue': _distribution(semanticWorkerQueueMicros), 'nativeFfi': _distribution(semanticNativeFfiMicros), 'coreAdoption': _distribution(semanticCoreAdoptionMicros), 'flutterAdoption': _distribution(semanticFlutterAdoptionMicros), 'callbackToReceipt': _distribution(semanticCallbackToReceiptMicros)})}',
      );
    }

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
    if (workload == 'inline-typing') {
      expect(projectedContinuitySamples, 120);
      expect(
        rawProjectionFrames,
        0,
        reason: 'ordinary inline typing painted raw Markdown markers',
      );
    }
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

Future<void> _waitForCanonicalSelection(
  FlarkEditorController controller,
  int previousGeneration,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (controller.canonicalSelectionGeneration == previousGeneration &&
      controller.lastError == null &&
      DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  expect(controller.lastError, isNull);
  expect(
    controller.canonicalSelectionGeneration,
    greaterThan(previousGeneration),
  );
}

Future<FlarkSemanticEditPerformance> _waitForSemanticReceipt(
  FlarkEditorController controller,
  FlarkSemanticEditPerformance? previous,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (identical(controller.lastSemanticEditPerformance, previous) &&
      controller.lastError == null &&
      DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  expect(controller.lastError, isNull);
  final receipt = controller.lastSemanticEditPerformance;
  expect(receipt, isNotNull);
  expect(
    identical(receipt, previous),
    isFalse,
    reason:
        'semantic receipt did not advance; pending=${controller.pendingEdits}, '
        'revision=${controller.revision}, resync=${controller.resyncCount}/'
        '${controller.lastResyncReason.name}, input=${controller.inputValue}',
  );
  return receipt!;
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

Map<String, Object> _distribution(List<int> values) => {
  'rawMs': values.map((value) => value / 1000).toList(),
  'p50Ms': _percentile(values, 50) / 1000,
  'p99Ms': _percentile(values, 99) / 1000,
  'maxMs': _maximum(values) / 1000,
};

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
