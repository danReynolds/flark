import 'dart:convert';
import 'dart:developer';
import 'dart:io';
import 'dart:ui';

import 'package:flark/render_model.dart';
import 'package:flark_dogfood/backend.dart';
import 'package:flark_dogfood/main.dart';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/code.dart';
// Qualification observes the host's bounded page without expanding its API.
// ignore: implementation_imports
import 'package:flark_flutter/src/source_window.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

class _CountedBackend implements FlarkParseBackend {
  _CountedBackend(this.backend);
  final FlarkParseBackend backend;
  int calls = 0;
  @override
  int get schemaVersion => backend.schemaVersion;
  @override
  RenderModel parse(String source) {
    calls++;
    return backend.parse(source);
  }
}

Map<String, String> workbenchProfileSources(FlarkParseBackend backend) => {
  for (final shape in profileCycles.keys)
    shape: boundedProfileSource(backend, shape, candidateLiveBytes),
  'source ceiling': '${'word\n' * (candidateSourceBytes ~/ 5)}zzz',
  'long line': 'a' * 4097,
  'line count': 'a\n' * 1025,
  'block count': '- a\n' * 257,
  'run count': '${'${'*a* ' * 700}\n\n' * 4}z',
  'container depth': '${'> ' * 9}a',
};

// Known first editable content in these literal fixture prefixes. Opening at
// source zero must legalize past a heading/list/quote marker. Reference
// definitions remain editable source rows and therefore start at zero.
int workbenchOpeningCaret(String shape) => switch (shape) {
  'dense' => 3,
  'list' || 'table' => 2,
  'nested' => 16,
  'code' => 8,
  _ => 0,
};

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
  testWidgets(
    'production workbench opening, boundaries and sustained use',
    (tester) async {
      final backend = _CountedBackend(await loadBackend());
      final code = await FlarkTreeSitter.load();
      addTearDown(code.dispose);
      final sources = workbenchProfileSources(backend);
      SharedPreferences.setPrefix('flark.workbenchProfile.');
      final preferences = await SharedPreferences.getInstance();
      addTearDown(() async {
        await preferences.remove('v5.active');
        await preferences.remove('v5.source.Tour');
      });
      final paints = <FlarkPaintObservation>[];
      final frames = <int, FrameTiming>{};
      void timings(List<FrameTiming> batch) {
        for (final frame in batch) {
          frames[frame.frameNumber] = frame;
        }
      }

      binding.addTimingsCallback(timings);
      addTearDown(() => binding.removeTimingsCallback(timings));
      var established = false, lostForeground = false;
      final transitions = <String>[];
      final lifecycle = AppLifecycleListener(
        onStateChange: (state) {
          transitions.add(state.name);
          if (established && state != AppLifecycleState.resumed) {
            lostForeground = true;
          }
        },
      );
      addTearDown(lifecycle.dispose);
      void requireForeground() {
        expect(binding.lifecycleState, AppLifecycleState.resumed);
        expect(binding.framesEnabled, isTrue);
        expect(lostForeground, isFalse);
      }

      FlarkController controller() => tester
          .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
          .controller;
      Future<void> mount() => tester.pumpWidget(
        DogfoodApp(
          backend: backend,
          code: code,
          preferences: preferences,
          onPaint: paints.add,
        ),
      );
      Future<void> close() async {
        // The production exit handler drains its serialized save queue. Do not
        // discard an app with pending persistence and call that a lifecycle pass.
        expect(await binding.handleRequestAppExit(), AppExitResponse.exit);
        await tester.pumpWidget(const SizedBox());
      }

      await preferences.setString('v5.active', 'Tour');
      await preferences.setString('v5.source.Tour', tour);
      // Wait before mount: a hidden live binding may not produce its first frame.
      final deadline = DateTime.now().add(const Duration(seconds: 60));
      while ((binding.lifecycleState != AppLifecycleState.resumed ||
              !binding.framesEnabled) &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      established = true;
      requireForeground();
      await mount();
      await tester.pump();
      requireForeground();
      await close();

      final samples = <Map<String, Object>>[];
      final memory = <int>[];
      var completedCycles = 0, sustainedInputs = 0;
      // An interrupted run is rejected, but retain its progress and linked
      // measurements so diagnosis does not depend on a final success callback.
      addTearDown(() {
        for (final sample in samples) {
          final frame = frames[sample['frameNumber']];
          if (frame != null) {
            sample['latencyUs'] =
                frame.timestampInMicroseconds(FramePhase.rasterFinish) -
                (sample['startUs'] as int);
          }
        }
        // ignore: avoid_print
        print(
          'FLARK_WORKBENCH_SAMPLES ${jsonEncode({'completedCycles': completedCycles, 'sustainedInputs': sustainedInputs, 'lostForeground': lostForeground, 'lifecycleTransitions': transitions, 'samples': samples})}',
        );
      });
      var peakRss = 0;
      var measured = false;
      var shape = '';
      Future<void> paintAfter(
        String operation,
        Future<void> Function() action, {
        required String source,
        required bool sourceMode,
        bool opening = false,
        int maximumParses = 1,
      }) async {
        requireForeground();
        paints.clear();
        final parseStart = backend.calls;
        final start = Timeline.now;
        await action();
        await tester.pump();
        requireForeground();
        expect(tester.takeException(), isNull);
        final c = controller();
        expect(c.text, source);
        expect(c.editor.sourceMode, sourceMode);
        expect(
          c.editor.selection,
          FlarkSelection.collapsed(
            opening ? workbenchOpeningCaret(shape) : source.length,
          ),
        );
        expect(
          paints,
          isNotEmpty,
          reason: '$operation requires an actual paint',
        );
        // Opening may paint before autofocus. All paints must have exact source;
        // the first editable viewport (including its caret) defines open latency.
        for (final paint in paints) {
          expect(paint.snapshot.source, source);
          expect(paint.revision, c.editor.revision);
          expect(paint.caretSource, c.editor.selection.extent);
          if (!opening) {
            expect(paint.caret, isNotNull);
            final lineStart = source.lastIndexOf('\n') + 1;
            var tailStart = source.length - 32;
            if (sourceMode) {
              final page = SourceWindow.at(source, source.length);
              if (page.start > tailStart) tailStart = page.start;
              expect(
                find.text('Source page ${page.index + 1} of ${page.count}'),
                page.count > 1 ? findsOneWidget : findsNothing,
              );
            }
            expect(
              paint.rows.last,
              endsWith(
                source.substring(tailStart > lineStart ? tailStart : lineStart),
              ),
            );
          }
        }
        final editable = paints.where((paint) => paint.caret != null).toList();
        expect(editable, isNotEmpty, reason: '$operation must remain editable');
        final parseCalls = backend.calls - parseStart;
        expect(parseCalls, lessThanOrEqualTo(maximumParses));
        if (measured) {
          final rss = ProcessInfo.currentRss;
          peakRss = ProcessInfo.maxRss;
          samples.add({
            'shape': shape,
            'operation': operation,
            'startUs': start,
            'frameNumber': editable.first.frameNumber,
            'sourceMode': sourceMode,
            'parseCalls': parseCalls,
            'rss': rss,
            'budgetUs': opening ? 200000 : (sourceMode ? 50000 : 16667),
          });
        }
      }

      Future<void> command(
        String operation,
        FlarkCommand command,
        String source,
        bool sourceMode, {
        int maximumParses = 1,
      }) => paintAfter(
        operation,
        () async => expect(controller().command(command), isTrue),
        source: source,
        sourceMode: sourceMode,
        maximumParses: maximumParses,
      );

      // Warm every shape twice, then retain every sample of 100 open/edit/close
      // cycles. This includes real preferences writes and actual surface disposal.
      final warmups = sources.length * 2;
      for (var cycle = 0; cycle < warmups + 100; cycle++) {
        measured = cycle >= warmups;
        final entry = sources.entries.elementAt(cycle % sources.length);
        shape = entry.key;
        final source = entry.value;
        final live = profileCycles.containsKey(shape);
        final cheapFallback = const {
          'source ceiling',
          'long line',
          'line count',
        }.contains(shape);
        await preferences.setString('v5.source.Tour', source);
        await paintAfter(
          'open',
          mount,
          source: source,
          sourceMode: !live,
          opening: true,
          maximumParses: cheapFallback ? 0 : 1,
        );
        final c = controller();
        expect(c.command(SetSelection.caret(source.length)), isTrue);
        await tester.pump();
        await command(
          'insert',
          const InsertText('x'),
          '${source}x',
          !live,
          maximumParses: cheapFallback ? 0 : 1,
        );
        if (live) {
          await command(
            'enter source at byte boundary',
            const InsertText('y'),
            '${source}xy',
            true,
            maximumParses: 0,
          );
          // A real caret excursion ends the typing group before measuring a
          // separately undoable deletion. Adjacent typing and deletion coalesce.
          c.command(SetSelection.caret(source.length + 1));
          c.command(SetSelection.caret(source.length + 2));
          await tester.pump();
          await command(
            'return to live',
            const DeleteBackward(),
            '${source}x',
            false,
          );
          await command(
            'undo boundary',
            const Undo(),
            '${source}xy',
            true,
            maximumParses: 0,
          );
          await command('redo boundary', const Redo(), '${source}x', false);
        }
        await command(
          'delete',
          const DeleteBackward(),
          source,
          !live,
          maximumParses: cheapFallback ? 0 : 1,
        );
        // This is the workbench's real inspection toggle: it halves the editor's
        // width and restores it. Native window dragging remains a separate canary.
        expect(
          tester.view.physicalSize.width / tester.view.devicePixelRatio,
          greaterThan(650),
          reason: 'inspection must actually reflow the editor',
        );
        for (var toggle = 0; toggle < 2; toggle++) {
          await paintAfter(
            'inspection reflow',
            () => tester.tap(find.byTooltip('Inspect Markdown')),
            source: source,
            sourceMode: !live,
            maximumParses: 0,
          );
          expect(c.editor.selection.extent, source.length);
        }
        await close();
        expect(preferences.getString('v5.source.Tour'), source);
        if (cycle >= warmups - 1) memory.add(ProcessInfo.currentRss);
        if (measured) completedCycles++;
      }
      // ignore: avoid_print
      print('FLARK_WORKBENCH_PROGRESS completedCycles=$completedCycles');

      // Five minutes of continuous ordinary typing/deletion, with an exact source
      // oracle after every character. No settle before the proving paint.
      shape = 'sustained prose';
      var source = sources['prose']!;
      source = source.substring(0, source.length - 40);
      await preferences.setString('v5.source.Tour', source);
      await mount();
      await tester.pump();
      controller().command(SetSelection.caret(source.length));
      await tester.pump();
      const sentence = 'alpha beta  gamma!';
      var expected = source;
      for (var tick = 0; tick < 3000; tick++) {
        final phase = tick % (sentence.length * 2);
        if (phase < sentence.length) {
          expected += sentence[phase];
          await command(
            'sustained insert',
            InsertText(sentence[phase]),
            expected,
            false,
          );
        } else {
          expected = expected.substring(0, expected.length - 1);
          await command(
            'sustained delete',
            const DeleteBackward(),
            expected,
            false,
          );
        }
        sustainedInputs++;
        if (sustainedInputs % 500 == 0) {
          // ignore: avoid_print
          print('FLARK_WORKBENCH_PROGRESS sustainedInputs=$sustainedInputs');
        }
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      await close();
      expect(preferences.getString('v5.source.Tour'), expected);
      await Future<void>.delayed(const Duration(seconds: 5));
      requireForeground();
      final retainedRss = ProcessInfo.currentRss;
      final baselineRss = memory.first;
      final failures = <String>[];
      final grouped = <String, List<int>>{};
      for (final sample in samples) {
        final frame = frames[sample['frameNumber']];
        expect(
          frame,
          isNotNull,
          reason: 'matching engine raster frame required',
        );
        final latency =
            frame!.timestampInMicroseconds(FramePhase.rasterFinish) -
            (sample['startUs'] as int);
        expect(latency, greaterThan(0), reason: 'clock calibration');
        sample['latencyUs'] = latency;
        final key = '${sample['shape']}: ${sample['operation']}';
        (grouped[key] ??= []).add(latency);
      }
      final summaries = <Map<String, Object>>[];
      for (final entry in grouped.entries) {
        final values = entry.value..sort();
        final p99 = values[(values.length * .99).ceil() - 1];
        final budget =
            samples.firstWhere(
                  (s) => '${s['shape']}: ${s['operation']}' == entry.key,
                )['budgetUs']
                as int;
        summaries.add({
          'operation': entry.key,
          'samples': values.length,
          'p99Us': p99,
          'maximumUs': values.last,
          'budgetUs': budget,
        });
        if (p99 >= budget) failures.add('${entry.key}: $p99 >= $budget us');
      }
      if (peakRss - baselineRss > 64 * 1024 * 1024) failures.add('peak RSS');
      if (retainedRss - baselineRss > 16 * 1024 * 1024) {
        failures.add('retained RSS');
      }
      final report = <String, Object>{
        'productionWorkbench': true,
        'treeSitterEditing': true,
        'treeSitterColors': true,
        'lifecycleTransitions': transitions,
        'logicalViewport': {
          'width':
              tester.view.physicalSize.width / tester.view.devicePixelRatio,
          'height':
              tester.view.physicalSize.height / tester.view.devicePixelRatio,
        },
        'displayHz':
            PlatformDispatcher.instance.views.first.display.refreshRate,
        'devicePixelRatio': tester.view.devicePixelRatio,
        'baselineRss': baselineRss,
        'peakRss': peakRss,
        'retainedRss': retainedRss,
        'cycleRss': memory,
        'summaries': summaries,
        'samples': samples,
        'failures': failures,
      };
      binding.reportData = report;
      // Raw linked samples are written by integrationDriver into its JSON result.
      // ignore: avoid_print
      print(
        'FLARK_WORKBENCH_RECEIPT ${jsonEncode({...report}..remove('samples'))}',
      );
      expect(failures, isEmpty);
    },
    timeout: const Timeout(Duration(minutes: 15)),
  );
}
