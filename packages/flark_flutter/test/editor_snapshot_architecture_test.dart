import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final controller = File('lib/src/controller.dart').readAsStringSync();
  final renderer = File('lib/src/render_surface.dart').readAsStringSync();
  final portableSnapshotSources = [
    '../flark/lib/src/editor_snapshot.dart',
    '../flark/lib/src/editor_text.dart',
    '../flark/lib/src/surface_projection.dart',
    '../flark/lib/src/surface_projector.dart',
  ].map((path) => File(path).readAsStringSync()).join('\n');
  final portableViewportState = File(
    '../flark/lib/src/editor_viewport_state.dart',
  ).readAsStringSync();

  test('controller has one immutable outward publication function', () {
    expect(
      RegExp(r'super\.notifyListeners\(').allMatches(controller),
      hasLength(1),
    );
    expect(
      RegExp(
        r'_snapshot\s*=\s*_captureEditorSnapshot\(',
      ).allMatches(controller),
      hasLength(1),
    );
    expect(controller, isNot(contains('FlarkSurfacePublication')));
    expect(controller, isNot(contains('surfacePublication')));
  });

  test('renderer reads visual truth only from the captured snapshot', () {
    final reads = RegExp(
      r'_controller\.([A-Za-z][A-Za-z0-9_]*)',
    ).allMatches(renderer).map((match) => match.group(1)!).toSet();

    expect(reads, {
      'addListener',
      'removeListener',
      'nextViewportPage',
      'previousViewportPage',
      'snapshot',
      'toggleTaskChecked',
    });
  });

  test('snapshot and deterministic projection remain host neutral', () {
    expect(portableSnapshotSources, isNot(contains('package:flutter')));
    expect(portableSnapshotSources, isNot(contains('dart:ui')));
    expect(controller, isNot(contains('class FlarkEditorSnapshot')));
  });

  test('pending presentation evolution remains below Flutter', () {
    expect(controller, isNot(contains('_spliceContinuityPresentation')));
    expect(
      controller,
      isNot(contains('_spliceProjectionEditCellPresentation')),
    );
    expect(
      controller,
      isNot(
        contains(
          'switch (authority) {\n'
          '    FlarkProjectionContinuityReceipt',
        ),
      ),
    );
    expect(
      controller,
      isNot(contains('_prepareCommittedPresentationTransition')),
    );
    expect(controller, isNot(contains('_caretBoundaryForStructuralSurfaces')));
    expect(
      controller,
      isNot(contains('resolveCommittedPresentationTransitionV1(')),
    );
    expect(
      controller,
      isNot(contains('_viewportSupersedesProjectionContinuity')),
    );
  });

  test('bounded viewport publication has one portable state owner', () {
    expect(portableViewportState, isNot(contains('package:flutter')));
    expect(portableViewportState, isNot(contains('dart:ui')));
    expect(controller, contains('final FlarkEditorViewportState'));
    expect(controller, isNot(contains('FlarkViewport? _viewport')));
    expect(controller, isNot(contains('List<FlarkViewportRow> _cachedRows')));
    expect(controller, isNot(contains('String _visibleSource')));
    expect(controller, isNot(contains('FlarkOptimisticRangeMap')));
  });
}
