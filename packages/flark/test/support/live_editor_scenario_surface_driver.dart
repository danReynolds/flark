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

  @override
  String get name => 'flutter-surface';

  @override
  bool get observesPaint => true;

  @override
  Future<void> start(LiveEditorScenarioPlan plan) async {
    await super.start(plan);
    _paintedPresentations.clear();
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
        _paintedPresentations.last != observation.presentation) {
      if (_paintedPresentations.length == 128) {
        _paintedPresentations.removeAt(0);
      }
      _paintedPresentations.add(observation.presentation);
    }
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
      revision: controllerSnapshot.revision,
    );
  }

  @override
  Future<void> stop() async {
    await tester.pumpWidget(const SizedBox.shrink());
    await super.stop();
  }
}
