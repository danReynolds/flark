import 'dart:convert';
import 'dart:developer';
import 'dart:io';
import 'dart:ffi' show Abi;
import 'dart:ui';
import 'package:flark_dogfood/backend.dart';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_dogfood/main.dart' show dense, DogfoodApp;
import 'package:shared_preferences/shared_preferences.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

int p99(List<int> values) {
  final sorted = [...values]..sort();
  return sorted[(sorted.length * .99).ceil() - 1];
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
  testWidgets(
    'input to actual raster across admitted document shapes',
    (tester) async {
      final backend = await loadBackend();
      const appMode = bool.fromEnvironment('FLARK_PROFILE_APP');
      SharedPreferences? preferences;
      if (appMode) {
        SharedPreferences.setPrefix('flark.profile.');
        preferences = await SharedPreferences.getInstance();
        addTearDown(() async {
          await preferences!.remove('v5.active');
          await preferences.remove('v5.source.Tour');
        });
      }
      final frames = <int, FrameTiming>{};
      void timings(List<FrameTiming> batch) {
        for (final f in batch) {
          frames[f.frameNumber] = f;
        }
      }

      WidgetsBinding.instance.addTimingsCallback(timings);
      addTearDown(() => WidgetsBinding.instance.removeTimingsCallback(timings));
      final receipts = <Map<String, Object>>[];
      final failures = <String>[];
      var foregroundEstablished = false;
      var lostForeground = false;
      final lifecycleTransitions = <String>[];
      final lifecycle = AppLifecycleListener(
        onStateChange: (state) {
          lifecycleTransitions.add(state.name);
          if (foregroundEstablished && state != AppLifecycleState.resumed) {
            lostForeground = true;
          }
        },
      );
      addTearDown(lifecycle.dispose);
      void requireForeground() {
        expect(
          binding.lifecycleState,
          AppLifecycleState.resumed,
          reason:
              'The profile requires the actual foreground app, not a captured hidden window.',
        );
        expect(binding.framesEnabled, isTrue);
        expect(
          lostForeground,
          isFalse,
          reason: 'A foreground/lifecycle transition invalidates this run.',
        );
      }

      // A hidden live binding can wait indefinitely inside pumpWidget before
      // producing its first frame. Check the real lifecycle before any pump.
      final foregroundDeadline = DateTime.now().add(
        const Duration(seconds: 60),
      );
      while ((binding.lifecycleState != AppLifecycleState.resumed ||
              !binding.framesEnabled) &&
          DateTime.now().isBefore(foregroundDeadline)) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      foregroundEstablished = true;
      requireForeground();

      const bytes = int.fromEnvironment(
        'FLARK_PROFILE_BYTES',
        defaultValue: 32768,
      );
      const bounded = bool.fromEnvironment('FLARK_PROFILE_BOUNDED');
      const onlyShape = String.fromEnvironment('FLARK_PROFILE_SHAPE');
      const onlySite = String.fromEnvironment('FLARK_PROFILE_SITE');
      const iterations = int.fromEnvironment(
        'FLARK_PROFILE_ITERATIONS',
        defaultValue: 120,
      );
      expect(iterations, greaterThanOrEqualTo(120));
      for (final shape in profileCycles.keys) {
        if (onlyShape.isNotEmpty && shape != onlyShape) continue;
        for (final site
            in bounded ? ['start', 'largest block', 'end'] : ['end']) {
          if (onlySite.isNotEmpty && site != onlySite) continue;
          final cycle = profileCycles[shape]!;
          var text = shape == 'dense' ? dense(bytes - 4) : '';
          while (shape != 'dense' &&
              utf8.encode(text + cycle).length < bytes - 4) {
            text += cycle.replaceAll('__index__', '${text.length}');
          }
          text += ' ' * (bytes - 4 - utf8.encode(text).length);
          text +=
              '\n\nz'; // one byte remains available for each measured insertion.
          if (bounded) text = boundedProfileSource(backend, shape, bytes);
          var c = FlarkController(
            FlarkEditor(
              backend,
              text: text,
              caret: text.length,
              syncLimit: bytes,
              liveLimits: bounded
                  ? candidateLiveLimits
                  : const FlarkLiveLimits(lines: 4096, blocks: 4096),
            ),
          );
          if (c.editor.sourceMode) {
            failures.add('$shape admission');
            c.dispose();
            continue;
          }
          if (site != 'end') {
            final rows = c.editor.projection.rows;
            final row = site == 'start'
                ? rows.firstWhere((r) => r.text.isNotEmpty)
                : rows.reduce((a, b) => a.text.length > b.text.length ? a : b);
            final caret = row.sourceForDisplay(row.text.length ~/ 2);
            c.command(SetSelection(caret, caret));
          }
          final caret = c.editor.selection.extent;
          final insertedSource = text.replaceRange(caret, caret, 'x');
          final kernelUs = <int>[], parseUs = <int>[], projectUs = <int>[];
          for (var k = 0; k < 100; k++) {
            var start = Timeline.now;
            final model = backend.parse(text);
            parseUs.add(Timeline.now - start);
            start = Timeline.now;
            Projection.of(model, text);
            projectUs.add(Timeline.now - start);
            start = Timeline.now;
            expect(c.command(const InsertText('x')), isTrue);
            kernelUs.add(Timeline.now - start);
            expect(c.command(const DeleteBackward()), isTrue);
          }
          final paints = <FlarkPaintObservation>[];
          expect(c.editor.selection.extent, caret);
          if (appMode) {
            c.dispose();
            await preferences!.setString('v5.active', 'Tour');
            await preferences.setString('v5.source.Tour', text);
            await tester.pumpWidget(
              DogfoodApp(
                backend: backend,
                preferences: preferences,
                onPaint: paints.add,
              ),
            );
            await tester.pump();
            c = tester
                .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
                .controller;
            c.command(SetSelection.caret(caret));
          } else {
            await tester.pumpWidget(
              MaterialApp(
                home: Scaffold(
                  body: FlarkEditorWidget(
                    controller: c,
                    autofocus: true,
                    onPaint: paints.add,
                  ),
                ),
              ),
            );
          }
          for (var i = 0; i < 6; i++) {
            await tester.pump();
          }
          requireForeground();
          // ignore: avoid_print
          print('FLARK_FRAME_PROGRESS foreground $shape / $site');
          final samples = <(int, int, int)>[];
          final commands = <int>[];
          for (var i = 0; i < iterations; i++) {
            for (final insert in [true, false]) {
              requireForeground();
              paints.clear();
              final start = Timeline.now;
              if (insert) {
                final value = c.value;
                expect(
                  c.receiveDeltas([
                    TextEditingDeltaInsertion(
                      oldText: value.text,
                      textInserted: 'x',
                      insertionOffset: value.selection.extentOffset,
                      selection: TextSelection.collapsed(
                        offset: value.selection.extentOffset + 1,
                      ),
                      composing: TextRange.empty,
                    ),
                  ]),
                  isTrue,
                );
              } else {
                expect(c.command(const DeleteBackward()), isTrue);
              }
              if (i >= 20) commands.add(Timeline.now - start);
              expect(c.editor.sourceMode, isFalse);
              final revision = c.editor.revision;
              await tester.pump();
              requireForeground();
              expect(
                paints,
                isNotEmpty,
                reason: 'a post-input paint is required',
              );
              for (final p in paints) {
                expect(p.revision, revision);
                expect(
                  p.caret,
                  isNotNull,
                  reason: 'the input frame must draw its caret',
                );
                expect(p.caretSource, c.editor.selection.extent);
                expect(p.snapshot.source, insert ? insertedSource : text);
              }
              if (i >= 20) {
                samples.add((start, paints.last.frameNumber, insert ? 1 : 0));
              }
              await Future<void>.delayed(const Duration(milliseconds: 20));
            }
          }
          // Raster timings are delivered in batches. The proving paint was already
          // checked above; this wait only collects its matching engine receipt.
          final deadline = DateTime.now().add(const Duration(seconds: 5));
          while (samples.any((s) => !frames.containsKey(s.$2)) &&
              DateTime.now().isBefore(deadline)) {
            await tester.pump();
            await Future<void>.delayed(const Duration(milliseconds: 50));
          }
          final insertUs = <int>[], deleteUs = <int>[];
          final buildUs = <int>[], rasterUs = <int>[], inputToBuildUs = <int>[];
          final raw = <Map<String, int>>[];
          for (final s in samples) {
            expect(
              frames,
              contains(s.$2),
              reason: 'matching raster frame must exist',
            );
            final frame = frames[s.$2]!;
            final latency =
                frame.timestampInMicroseconds(FramePhase.rasterFinish) - s.$1;
            expect(latency, greaterThan(0), reason: 'clock calibration');
            (s.$3 == 1 ? insertUs : deleteUs).add(latency);
            buildUs.add(frame.buildDuration.inMicroseconds);
            rasterUs.add(frame.rasterDuration.inMicroseconds);
            inputToBuildUs.add(
              frame.timestampInMicroseconds(FramePhase.buildStart) - s.$1,
            );
            raw.add({
              'startUs': s.$1,
              'frameNumber': s.$2,
              'insert': s.$3,
              'latencyUs': latency,
              'buildUs': buildUs.last,
              'rasterUs': rasterUs.last,
              'inputToBuildUs': inputToBuildUs.last,
              'vsyncStartUs': frame.timestampInMicroseconds(
                FramePhase.vsyncStart,
              ),
              'buildStartUs': frame.timestampInMicroseconds(
                FramePhase.buildStart,
              ),
              'rasterStartUs': frame.timestampInMicroseconds(
                FramePhase.rasterStart,
              ),
            });
          }
          final receipt = <String, Object>{
            'lifecycle': binding.lifecycleState!.name,
            'lifecycleTransitions': List<String>.of(lifecycleTransitions),
            'shape': shape,
            'site': site,
            'bounded': bounded,
            'productionWorkbench': appMode,
            'blocks': c.editor.document.model.blockCount,
            'runs': c.editor.document.model.runCount,
            'maxRowCodeUnits': c.editor.projection.rows
                .map((r) => r.text.length)
                .reduce((a, b) => a > b ? a : b),
            'abi': Abi.current().toString(),
            'kernelOnlyP99Us': p99(kernelUs.skip(20).toList()),
            'parseP99Us': p99(parseUs.skip(20).toList()),
            'projectionP99Us': p99(projectUs.skip(20).toList()),
            'bytes': utf8.encode(text).length,
            'samples': samples.length,
            'commandP99Us': p99(commands),
            'insertP99Us': p99(insertUs),
            'deleteP99Us': p99(deleteUs),
            'buildP99Us': p99(buildUs),
            'rasterP99Us': p99(rasterUs),
            'inputToBuildP99Us': p99(inputToBuildUs),
            'displayHz':
                PlatformDispatcher.instance.views.first.display.refreshRate,
            'logicalViewport': {
              'width':
                  tester.view.physicalSize.width / tester.view.devicePixelRatio,
              'height':
                  tester.view.physicalSize.height /
                  tester.view.devicePixelRatio,
            },
            'devicePixelRatio': tester.view.devicePixelRatio,
            'raw': raw,
            'rss': ProcessInfo.currentRss,
            'pid': pid,
          };
          receipts.add(receipt);
          // ignore: avoid_print
          print(
            'FLARK_FRAME_RECEIPT ${jsonEncode({...receipt}..remove('raw'))}',
          );
          // Keep linked samples in the redirected process log even on failure.
          // ignore: avoid_print
          print(
            'FLARK_FRAME_SAMPLES ${jsonEncode({'shape': shape, 'site': site, 'samples': raw})}',
          );
          await tester.pumpWidget(const SizedBox());
          if (!appMode) c.dispose();
        }
      }
      binding.reportData = {'receipts': receipts};
      expect(failures, isEmpty);
      expect(
        receipts,
        isNotEmpty,
        reason: 'filters must select real workloads',
      );
      for (final r in receipts) {
        expect(
          r['insertP99Us'] as int,
          lessThan(16667),
          reason: '${r['shape']} insert frame',
        );
        expect(
          r['deleteP99Us'] as int,
          lessThan(16667),
          reason: '${r['shape']} delete frame',
        );
      }
    },
    timeout: const Timeout(Duration(minutes: 12)),
  );
}
