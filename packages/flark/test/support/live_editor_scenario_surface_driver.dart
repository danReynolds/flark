import 'package:flark/src/render_surface.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'live_editor_scenario.dart';
import 'live_editor_scenario_executor.dart';
import 'live_editor_scenario_runner.dart';

final class FlutterSurfaceLiveEditorScenarioDriver
    extends NoWindowLiveEditorScenarioDriver {
  FlutterSurfaceLiveEditorScenarioDriver({
    required super.libraryPath,
    required this.tester,
  });

  final WidgetTester tester;
  final List<String> _paintedPresentations = [];
  final List<int> _paintedRenderPlanHashes = [];
  final List<int> _paintedVisualStateHashes = [];

  @override
  String get name => 'flutter-surface';

  @override
  bool get observesPaint => true;

  @override
  bool get observesScroll => true;

  RenderFlarkSurface get _surface => tester.renderObject<RenderFlarkSurface>(
    find.byType(FlarkRenderSurfaceWidget),
  );

  @override
  Future<void> start(LiveEditorScenarioPlan plan) async {
    await super.start(plan);
    _paintedPresentations.clear();
    _paintedRenderPlanHashes.clear();
    _paintedVisualStateHashes.clear();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox.expand(
          child: FlarkRenderSurfaceWidget(
            controller: activeController,
            textStyle: const TextStyle(fontSize: 17, height: 1.45),
            padding: EdgeInsets.zero,
            caretColor: const Color(0xff246bfd),
            selectionColor: const Color(0x40246bfd),
            debugPaintObserver: _recordPaint,
          ),
        ),
      ),
    );
    await tester.pump();
  }

  void _recordPaint(FlarkSurfacePaintObservation observation) {
    if (_paintedPresentations.isEmpty ||
        _paintedPresentations.last != observation.presentation ||
        _paintedRenderPlanHashes.last != observation.renderPlanHash ||
        _paintedVisualStateHashes.last != observation.visualStateHash) {
      if (_paintedPresentations.length == 128) {
        _paintedPresentations.removeAt(0);
        _paintedRenderPlanHashes.removeAt(0);
        _paintedVisualStateHashes.removeAt(0);
      }
      _paintedPresentations.add(observation.presentation);
      _paintedRenderPlanHashes.add(observation.renderPlanHash);
      _paintedVisualStateHashes.add(observation.visualStateHash);
    }
  }

  @override
  Future<void> activateAtUtf16(int offset) async {
    await super.activateAtUtf16(offset);
    await tester.pump();
    // Scenario paint assertions describe the edit, not the preceding focus
    // transition needed to place the caret.
    _paintedPresentations.clear();
    _paintedRenderPlanHashes.clear();
    _paintedVisualStateHashes.clear();
  }

  @override
  Future<void> scrollBy(int deltaY) async {
    _surface.scrollBy(deltaY.toDouble());
    await tester.pump();
  }

  @override
  Future<void> awaitBarrier(LiveEditorScenarioBarrier barrier) async {
    await super.awaitBarrier(barrier);
    await tester.pump();
  }

  @override
  Future<LiveEditorScenarioSnapshot> snapshot() async {
    await tester.pump();
    final controllerSnapshot = await super.snapshot();
    return LiveEditorScenarioSnapshot(
      source: controllerSnapshot.source,
      selectionBaseUtf16: controllerSnapshot.selectionBaseUtf16,
      selectionExtentUtf16: controllerSnapshot.selectionExtentUtf16,
      resyncCount: controllerSnapshot.resyncCount,
      faulted: controllerSnapshot.faulted,
      lastError: controllerSnapshot.lastError,
      settledPresentation: controllerSnapshot.settledPresentation,
      paintedPresentations: List.unmodifiable(_paintedPresentations),
      paintedRenderPlanHashes: List.unmodifiable(_paintedRenderPlanHashes),
      paintedVisualStateHashes: List.unmodifiable(_paintedVisualStateHashes),
      revision: controllerSnapshot.revision,
      scrollOffset: _surface.scrollOffset,
    );
  }

  @override
  Future<void> stop() async {
    await tester.pumpWidget(const SizedBox.shrink());
    await super.stop();
  }
}
