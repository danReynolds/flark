import 'dart:convert';
import 'dart:developer';
import 'dart:io';
import 'dart:ffi' show Abi;
import 'dart:ui';
import 'package:flark_dogfood/backend.dart';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_dogfood/main.dart' show dense, DogfoodApp;
import 'package:shared_preferences/shared_preferences.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/code.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import '../test_driver/frame_gate.dart';
import 'native_profile.dart';

int p99(List<int> values) => nearestRankP99(values);

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
  setUpAll(() => waitForNativeProfile(binding));
  testWidgets(
    'edit work and next-frame presentation across admitted shapes',
    (tester) async {
      final backend = await loadBackend();
      final code = await FlarkTreeSitter.load();
      addTearDown(code.dispose);
      const appMode = bool.fromEnvironment('FLARK_PROFILE_APP');
      SharedPreferences? preferences;
      if (appMode) {
        SharedPreferences.setPrefix('flark.profile.');
        // Bound once: a nullable local captured by a closure does not stay
        // promoted across an await, so `!` on the first call does not carry.
        final store = preferences = await SharedPreferences.getInstance();
        addTearDown(() async {
          await store.remove('v5.active');
          await store.remove('v5.source.Tour');
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
        expect(binding.platformDispatcher.semanticsEnabled, isTrue);
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
          if (bounded) {
            text = boundedProfileSource(
              backend,
              shape,
              site == 'start' ? bytes - profileStartHeadroom : bytes,
            );
          }
          var c = FlarkController(
            FlarkEditor(
              backend,
              codeEditing: code,
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
            final caret = profileCaret(c.editor.projection, site, text);
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
            final store = preferences!;
            await store.setString('v5.active', 'Tour');
            await store.setString('v5.source.Tour', text);
            await tester.pumpWidget(
              DogfoodApp(
                backend: backend,
                code: code,
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
          final samples = <(int, int, int, String)>[];
          final operations = profileOperations(site);
          for (var i = 0; i < iterations; i++) {
            for (final (operation, command) in operations) {
              final insert = operation == 'insert';
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
                expect(c.command(command), isTrue, reason: operation);
              }
              final commandUs = Timeline.now - start;
              expect(c.editor.sourceMode, isFalse);
              if (insert) expect(c.text, insertedSource);
              if (operation == 'delete' || operation.startsWith('undo')) {
                expect(c.text, text, reason: operation);
              }
              final revision = c.editor.revision, expected = c.text;
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
                expect(p.snapshot.source, expected);
              }
              if (i >= 20) {
                samples.add((
                  start,
                  paints.last.frameNumber,
                  commandUs,
                  operation,
                ));
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
          final raw = <Map<String, Object>>[];
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
            if (s.$4 == 'insert') insertUs.add(latency);
            if (s.$4 == 'delete') deleteUs.add(latency);
            buildUs.add(frame.buildDuration.inMicroseconds);
            rasterUs.add(frame.rasterDuration.inMicroseconds);
            inputToBuildUs.add(
              frame.timestampInMicroseconds(FramePhase.buildStart) - s.$1,
            );
            raw.add({
              'operation': s.$4,
              'startUs': s.$1,
              'frameNumber': s.$2,
              'commandUs': s.$3,
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
          final displayHz =
              PlatformDispatcher.instance.views.first.display.refreshRate;
          final gates = {
            for (final (operation, _) in operations)
              operation: FrameGateResult(
                [
                  for (final sample in raw)
                    if (sample['operation'] == operation) sample,
                ],
                displayHz,
                budgetUs: frameBudgetUs,
              ),
          };
          final receipt = <String, Object>{
            'lifecycle': binding.lifecycleState!.name,
            'lifecycleTransitions': List<String>.of(lifecycleTransitions),
            'shape': shape,
            'site': site,
            'bounded': bounded,
            'productionWorkbench': appMode,
            'nativeSemantics': binding.platformDispatcher.semanticsEnabled,
            'treeSitterEditing': true,
            'treeSitterColors': c.codeColors != null,
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
            'commandP99Us': p99([for (final s in samples) s.$3]),
            // Input-to-raster latency: reported, not gated.
            'insertP99Us': p99(insertUs),
            'deleteP99Us': p99(deleteUs),
            'gates': {
              for (final MapEntry(:key, :value) in gates.entries)
                key: value.toJson(),
            },
            'buildP99Us': p99(buildUs),
            'rasterP99Us': p99(rasterUs),
            'inputToBuildP99Us': p99(inputToBuildUs),
            'displayHz': displayHz,
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
      final gateFailures = <String>[];
      for (final r in receipts) {
        final raw = (r['raw'] as List).cast<Map>();
        for (final operation in {for (final s in raw) s['operation']}) {
          gateFailures.addAll(
            FrameGateResult(
              [
                for (final s in raw)
                  if (s['operation'] == operation) s,
              ],
              r['displayHz'] as num,
              budgetUs: frameBudgetUs,
            ).failures(
              '${r['shape']} ${r['site']} $operation',
              nextFrame: true,
            ),
          );
        }
      }
      expect(gateFailures, isEmpty);
    },
    semanticsEnabled: false,
    timeout: const Timeout(Duration(minutes: 12)),
  );
}
