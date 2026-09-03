import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  const editCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.anyNoCrLfSplice,
    affectedBytes: FlarkSourceRange(0, 3),
    affectedUtf16: FlarkSourceRange(0, 3),
    triggerBytes: FlarkSourceRange(0, 3),
    triggerUtf16: FlarkSourceRange(0, 3),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  const literalEnvelope = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.asciiWordInsertion,
    sourceBytes: FlarkSourceRange(0, 3),
    sourceUtf16: FlarkSourceRange(0, 3),
  );

  late FlarkEditorCoordinator coordinator;
  late FlarkEditorViewportState viewportState;
  late FlarkEditorSourceEditPlanner planner;

  setUp(() {
    coordinator = FlarkEditorCoordinator();
    viewportState = FlarkEditorViewportState();
    planner = FlarkEditorSourceEditPlanner(
      coordinator: coordinator,
      viewportState: viewportState,
    );
  });

  FlarkEditorSourceEditPlan plan({
    int revision = 0,
    required int start,
    required int end,
    required String replacement,
    required String inputText,
    int? activeOrdinal,
    bool compositionUsesExactFallback = false,
  }) => planner.plan(
    FlarkEditorSourceEditPlanningRequest(
      revision: revision,
      startUtf16: start,
      endUtf16: end,
      replacement: replacement,
      inputGlobalUtf16Start: viewportState.visibleUtf16Start,
      inputValue: FlarkEditorInputValue(
        text: inputText,
        selection: FlarkTextSelection.collapsed(offset: inputText.length),
      ),
      activeOrdinal: activeOrdinal,
      selectionBaseUtf16: inputText.length,
      selectionExtentUtf16: inputText.length,
      crossRowSelection: false,
      compositionUsesExactFallback: compositionUsesExactFallback,
      requiresStructuralCertification: false,
    ),
  );

  FlarkViewportRow row({
    List<FlarkLiteralSafeEnvelope> envelopes = const [],
    List<FlarkProjectionEditCell> cells = const [],
  }) => FlarkViewportRow(
    ordinal: 0,
    kind: 5,
    sourceBytes: const FlarkSourceRange(0, 3),
    sourceUtf16: const FlarkSourceRange(0, 3),
    editableBytes: const FlarkSourceRange(0, 3),
    editableUtf16: const FlarkSourceRange(0, 3),
    editCapability: FlarkViewportRowEditCapability.contiguous,
    headingLevel: null,
    headingStyle: null,
    listItem: null,
    blockQuote: null,
    codeBlock: null,
    thematicBreak: false,
    pathDepth: 0,
    inlineFacts: const [],
    literalSafeEnvelopes: envelopes,
    projectionEditCells: cells,
  );

  void installRow(FlarkViewportRow row) {
    viewportState.install(
      FlarkViewport(
        revision: 1,
        snapshot: 1,
        requestedBytes: const FlarkSourceRange(0, 3),
        coveredBytes: const FlarkSourceRange(0, 3),
        coveredUtf16: const FlarkSourceRange(0, 3),
        certification: FlarkCertification.currentCertified,
        rows: [row],
        neutralSource: null,
        continuation: 0,
      ),
      'abc',
    );
  }

  FlarkPendingStructuralSurface structuralSurface() =>
      FlarkPendingStructuralSurface(
        surface: FlarkCoreCommittedPresentationSurfaceV1(
          rowOrdinal: 0,
          sourceUtf16: const FlarkSourceRange(0, 3),
          presentation: FlarkCorePresentationRow(
            sourceUtf16: const FlarkSourceRange(0, 3),
            leadingText: '',
            text: 'abc',
            globalUtf16Start: 0,
            kind: 5,
            headingLevel: null,
            blockQuoteDepth: null,
            codeBlock: null,
            thematicBreak: false,
            ordinal: 0,
            runs: [
              FlarkCorePresentationRun(
                text: 'abc',
                sourceUtf16Start: 0,
                sourceUtf16End: 3,
                sourceExact: true,
                styles: {},
              ),
            ],
          ),
          projectionCurrent: true,
          projectionEditCells: const [editCell],
        ),
      );

  test('an unproved ordinary source edit fails closed to certification', () {
    viewportState.adoptUncertifiedSourceWindow(source: 'abc', startUtf16: 0);

    final result = plan(start: 3, end: 3, replacement: 'x', inputText: 'abc');

    expect(
      result.publication,
      FlarkQueuedEditPublication.retainPublishedUntilCertified,
    );
    expect(result.projectionReceipt, isNull);
    expect(result.usesExactFallback, isFalse);
  });

  test('invalid source ranges are rejected before state evolves', () {
    viewportState.adoptUncertifiedSourceWindow(source: 'abc', startUtf16: 0);

    expect(
      () => plan(start: 2, end: 1, replacement: 'x', inputText: 'abc'),
      throwsArgumentError,
    );
    expect(coordinator.pendingPresentation.isEmpty, isTrue);
  });

  test(
    'a neutral exact input island can publish without guessed semantics',
    () {
      viewportState.adoptUncertifiedSourceWindow(source: '', startUtf16: 0);

      final result = plan(
        start: 0,
        end: 0,
        replacement: 'x',
        inputText: '',
        activeOrdinal: -1,
      );

      expect(
        result.publication,
        FlarkQueuedEditPublication.publishOptimistically,
      );
      expect(result.usesExactFallback, isTrue);
      expect(coordinator.pendingPresentation.isEmpty, isTrue);
    },
  );

  test('a publication barrier overrides an otherwise exact fallback', () {
    viewportState.adoptUncertifiedSourceWindow(source: '', startUtf16: 0);
    coordinator.beginPublicationBarrier();

    final result = plan(
      start: 0,
      end: 0,
      replacement: 'x',
      inputText: '',
      activeOrdinal: -1,
    );

    expect(
      result.publication,
      FlarkQueuedEditPublication.retainPublishedUntilCertified,
    );
    expect(result.usesExactFallback, isTrue);
  });

  test('typing into a retained blank boundary retires that boundary', () {
    viewportState.adoptUncertifiedSourceWindow(source: '\n', startUtf16: 0);
    coordinator.setPendingCaretBoundary(
      FlarkPendingCaretBoundary(rowOrdinal: 0, rowEndUtf16: 0),
    );

    final result = plan(start: 0, end: 0, replacement: 'x', inputText: '\n');

    expect(result.usesExactFallback, isTrue);
    expect(coordinator.pendingPresentation.caretBoundary, isNull);
    expect(
      result.publication,
      FlarkQueuedEditPublication.publishOptimistically,
    );
  });

  test('parser-authorized continuity publishes and becomes pending truth', () {
    installRow(row(envelopes: const [literalEnvelope]));

    final result = plan(
      revision: 1,
      start: 1,
      end: 1,
      replacement: 'x',
      inputText: 'abc',
      activeOrdinal: 0,
    );

    expect(
      result.publication,
      FlarkQueuedEditPublication.publishOptimistically,
    );
    expect(result.projectionReceipt, isNull);
    expect(coordinator.pendingPresentation.dependency, isNotNull);
    expect(
      coordinator.pendingPresentation.dependency!.presentation.text,
      'axbc',
    );
  });

  test('one structural edit-cell advances exactly one pending surface', () {
    viewportState.adoptUncertifiedSourceWindow(source: 'abc', startUtf16: 0);
    coordinator.setPendingStructuralSurfaces([structuralSurface()]);

    final result = plan(
      revision: 1,
      start: 1,
      end: 1,
      replacement: 'x',
      inputText: 'abc',
    );

    expect(
      result.publication,
      FlarkQueuedEditPublication.publishOptimistically,
    );
    expect(result.projectionReceipt, isNotNull);
    expect(coordinator.pendingPresentation.structuralSurfaces, hasLength(1));
    expect(
      coordinator
          .pendingPresentation
          .structuralSurfaces
          .single
          .surface
          .presentation
          .text,
      'axbc',
    );
  });

  test('ambiguous structural edit-cell authority fails closed', () {
    viewportState.adoptUncertifiedSourceWindow(source: 'abc', startUtf16: 0);
    coordinator.setPendingStructuralSurfaces([
      structuralSurface(),
      structuralSurface(),
    ]);

    final result = plan(
      revision: 1,
      start: 1,
      end: 1,
      replacement: 'x',
      inputText: 'abc',
    );

    expect(
      result.publication,
      FlarkQueuedEditPublication.retainPublishedUntilCertified,
    );
    expect(result.projectionReceipt, isNull);
    expect(coordinator.pendingPresentation.structuralSurfaces, isEmpty);
  });
}
