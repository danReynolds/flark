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
  final portableViewportPager = File(
    '../flark/lib/src/editor_viewport_pager.dart',
  ).readAsStringSync();
  final portableInputWindow = File(
    '../flark/lib/src/editor_input_window.dart',
  ).readAsStringSync();
  final inputTransactionState = File(
    'lib/src/input_transaction_state.dart',
  ).readAsStringSync();
  final inputState = File('lib/src/editor_input_state.dart').readAsStringSync();

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

  test('viewport query and page-path effects stay below Flutter', () {
    expect(portableViewportPager, isNot(contains('package:flutter')));
    expect(portableViewportPager, isNot(contains('dart:ui')));
    expect(controller, contains('final FlarkEditorViewportPager'));
    expect(controller, isNot(contains('_viewportNavigation')));
    expect(controller, isNot(contains('_queryViewportAtAnchor')));
    expect(controller, isNot(contains('queryViewportNext(')));
  });

  test('Flutter consumes parser-authored semantic command capabilities', () {
    expect(controller, contains('semanticCapabilities'));
    expect(controller, isNot(contains('_supportsSemanticParagraphBreakV1')));
    expect(controller, isNot(contains('_supportsSemanticDeleteBackwardV1')));
    expect(controller, isNot(contains('_isPlainParagraphRow')));
    expect(controller, isNot(contains('_isTopLevelThematicBreak')));
  });

  test('semantic successor lineage has one Flutter state owner', () {
    expect(inputTransactionState, contains('classifySemanticSuccessor'));
    expect(inputTransactionState, contains('reserveSemanticSuccessor'));
    expect(inputTransactionState, contains('takePendingSemantic'));
    expect(controller, isNot(contains('before.composing != TextRange.empty')));
    expect(
      controller,
      isNot(contains('successors.length < _maximumSemanticSuccessors')),
    );
  });

  test('bounded platform input has one invariant-owning state boundary', () {
    expect(controller, contains('final FlarkEditorInputState _inputState'));
    expect(controller, isNot(contains('TextEditingValue _inputValue')));
    expect(controller, isNot(contains('int _inputGlobalUtf16Start')));
    expect(controller, isNot(contains('int? _activeOrdinal')));
    expect(controller, isNot(contains('int _globalSelectionBase')));
    expect(controller, isNot(contains('int _globalSelectionExtent')));
    expect(controller, isNot(contains('bool _crossRowSelection')));
    expect(controller, isNot(contains('bool _oversizedSelection')));
    expect(controller, isNot(contains('bool _semanticEditV1Active')));
    expect(
      controller,
      isNot(contains('FlarkCoreInlineContinuationV1? _inlineContinuation')),
    );
    expect(inputState, contains('activateWindow'));
    expect(inputState, contains('activateCollapsedWindow'));
    expect(inputState, contains('markOversizedSelection'));
    expect(inputState, contains('FlarkEditorInputWindowPlanner.activate'));
    expect(inputState, contains('FlarkEditorInputWindowPlanner.collapsed'));
    expect(inputState, contains('installWindowPlan'));
    expect(portableInputWindow, contains('restoreCollapsed'));
    expect(portableInputWindow, contains('paragraphGap'));
    expect(portableInputWindow, contains('caretBoundary'));
    expect(portableInputWindow, contains('neutralLine'));
    expect(controller, isNot(contains('_activateWindowWithoutNotification')));
    expect(portableInputWindow, isNot(contains('package:flutter')));
    expect(portableInputWindow, isNot(contains('dart:ui')));
    expect(inputState, isNot(contains("import 'controller.dart'")));
    expect(inputState, isNot(contains('FlarkEditorController')));
    expect(inputState, isNot(contains(RegExp(r'\n\s+set [A-Za-z]'))));
  });
}
