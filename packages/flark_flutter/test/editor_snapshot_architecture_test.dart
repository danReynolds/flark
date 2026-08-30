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
  final portableCommandExecutor = File(
    '../flark/lib/src/editor_command_executor.dart',
  ).readAsStringSync();
  final portableParseDriver = File(
    '../flark/lib/src/editor_parse_driver.dart',
  ).readAsStringSync();
  final portableSourceEditPlanner = File(
    '../flark/lib/src/editor_source_edit_planner.dart',
  ).readAsStringSync();
  final portableSemanticReceiptAdopter = File(
    '../flark/lib/src/editor_semantic_receipt_adopter.dart',
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
    expect(inputTransactionState, contains('capturePendingObservation'));
    expect(inputTransactionState, contains('captureLateObservation'));
    expect(
      inputTransactionState,
      contains('captureCertificationDeferredObservation'),
    );
    expect(inputTransactionState, contains('deferSuccessor'));
    expect(controller, isNot(contains('before.composing != TextRange.empty')));
    expect(
      controller,
      isNot(contains('successors.length < _maximumSemanticSuccessors')),
    );
    expect(controller, isNot(contains('successors.add(')));
    expect(controller, isNot(contains('successors.addAll(')));
    expect(controller, isNot(contains('successors.insert(')));
    expect(controller, isNot(contains('provisionalTail =')));
    expect(controller, isNot(contains('certificationPromotion =')));
    expect(controller, isNot(contains('fallbackWhenNotApplied =')));
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
    expect(portableInputWindow, contains('FlarkEditorInputMutationPlanner'));
    expect(controller, contains('FlarkEditorInputMutationPlanner.plan('));
    expect(controller, isNot(contains('candidateContinuationRewrite')));
    expect(controller, isNot(contains('boundedReplacementWindow(')));
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

  test('native editor commands have one portable execution boundary', () {
    expect(portableCommandExecutor, isNot(contains('package:flutter')));
    expect(portableCommandExecutor, isNot(contains('dart:ui')));
    expect(controller, contains('FlarkEditorCommandExecutor _commands'));
    expect(controller, isNot(contains('_coordinator.admitCommand(')));
    expect(controller, isNot(contains('_coordinator.completeCommand(')));
    expect(controller, isNot(contains('_coordinator.failCommand(')));
    expect(controller, isNot(contains('_session.applyEditUtf16(')));
    expect(controller, isNot(contains('_session.applyEditIntentOutcomeV1(')));
    expect(controller, isNot(contains('_session.applySemanticActionV1(')));
    expect(controller, isNot(contains('_session.undo(')));
    expect(controller, isNot(contains('_session.redo(')));
    expect(controller, isNot(contains('_session.cancelComposition(')));
    expect(portableCommandExecutor, contains('executeSourceEdit'));
    expect(portableCommandExecutor, contains('executeSemanticEdit'));
    expect(portableCommandExecutor, contains('executeSemanticAction'));
    expect(portableCommandExecutor, contains('executeHistory'));
    expect(portableCommandExecutor, contains('executeCompositionCancel'));
  });

  test('native parse progression has one portable execution boundary', () {
    expect(portableParseDriver, isNot(contains('package:flutter')));
    expect(portableParseDriver, isNot(contains('dart:ui')));
    expect(controller, contains('FlarkEditorParseDriver _parseDriver'));
    expect(controller, contains('switch (await _parseDriver.next())'));
    expect(portableParseDriver, contains('_coordinator.editTail'));
    expect(
      portableParseDriver,
      contains('_coordinator.sourceEditAdoptionTail'),
    );
    expect(portableParseDriver, contains('_document.pump('));
    expect(portableParseDriver, contains('_document.queryViewport('));
    expect(portableParseDriver, contains('adoptOpening('));
    expect(portableParseDriver, contains('awaitEditPublication('));
    expect(portableParseDriver, contains('adoptEditPublication('));
    expect(portableParseDriver, contains('currentOpeningEditPublication('));
    expect(controller, isNot(contains('_awaitEditPublicationCertification')));
    expect(
      controller,
      isNot(contains('_installedViewportProvesEditPublication')),
    );

    final finishParsing = RegExp(
      r'Future<void> _finishParsing\(\) async \{[\s\S]*?\n  void _retainOptimisticRefreshAnchor',
    ).firstMatch(controller)!.group(0)!;
    expect(finishParsing, isNot(contains('_document.pump(')));
    expect(finishParsing, isNot(contains('_document.queryViewport(')));
    expect(finishParsing, isNot(contains('_coordinator.editTail')));
    expect(
      finishParsing,
      isNot(contains('_coordinator.sourceEditAdoptionTail')),
    );
  });

  test('source-edit presentation planning stays below Flutter', () {
    expect(portableSourceEditPlanner, isNot(contains('package:flutter')));
    expect(portableSourceEditPlanner, isNot(contains('dart:ui')));
    expect(controller, contains('FlarkEditorSourceEditPlanner'));
    expect(controller, contains('_sourceEditPlanner.plan('));
    expect(controller, isNot(contains('_prepareProjectionContinuity')));
    expect(controller, isNot(contains('_advanceCommittedStructuralSurfaces')));
    expect(controller, isNot(contains('_advanceCommittedCaretBoundary')));
    expect(
      portableSourceEditPlanner,
      contains('structuralSuccessorRequiresCertification'),
    );
    expect(
      portableSourceEditPlanner,
      contains('lacksResultPresentationAuthority'),
    );
  });

  test('semantic receipt publication state stays below Flutter', () {
    expect(portableSemanticReceiptAdopter, isNot(contains('package:flutter')));
    expect(portableSemanticReceiptAdopter, isNot(contains('dart:ui')));
    expect(controller, contains('FlarkEditorSemanticReceiptAdopter'));
    expect(controller, contains('_semanticReceiptAdopter.adopt('));
    expect(
      controller,
      isNot(contains('resolvePendingPresentationTransition(')),
    );
    expect(
      controller,
      isNot(contains('_commands.adoptCommittedPresentation(')),
    );
    expect(controller, isNot(contains('_viewportPager.pinRefreshAnchor(')));
    expect(
      portableSemanticReceiptAdopter,
      contains('_commands.publishSource('),
    );
    expect(
      portableSemanticReceiptAdopter,
      contains('resolvePendingPresentationTransition('),
    );
    expect(
      portableSemanticReceiptAdopter,
      contains('_commands.adoptCommittedPresentation('),
    );
    expect(
      portableSemanticReceiptAdopter,
      contains('_viewportState.applyOptimisticEdit('),
    );
    expect(controller, contains('afterCommittedSplice('));
  });

  test('Flutter callback shapes converge before editor policy', () {
    expect(controller, contains('_applyPlatformObservation(observation)'));
    expect(controller, contains('_platformInput.observeValue('));
    expect(controller, isNot(contains('_isPlatformNewlineMutation')));
    expect(controller, isNot(contains('_isPlatformNewlineValue')));
    expect(controller, isNot(contains('_isPlatformDeleteBackwardMutation')));
    expect(controller, isNot(contains('_isPlatformDeleteBackwardValue')));
    expect(controller, isNot(contains('_updateEditingValueFromPlatform')));
    expect(controller, isNot(contains('_applyOversizedPlatformDeltas')));
    expect(controller, isNot(contains('_validateDeltaBatch')));
    expect(controller, isNot(contains('_mutationFor(')));
    expect(controller, contains('_captureSemanticSuccessorObservation('));
    expect(controller, contains('_captureLateSemanticObservation('));
    expect(
      controller,
      contains('_capturePlatformObservationBehindCertification('),
    );
  });
}
