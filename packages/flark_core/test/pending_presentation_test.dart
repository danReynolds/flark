import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  const cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: FlarkSourceRange(0, 4),
    affectedUtf16: FlarkSourceRange(0, 4),
    triggerBytes: FlarkSourceRange(0, 4),
    triggerUtf16: FlarkSourceRange(0, 4),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  const envelope = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.asciiWordInsertion,
    sourceBytes: FlarkSourceRange(0, 4),
    sourceUtf16: FlarkSourceRange(0, 4),
  );

  FlarkCoreEditIntentReceiptV1 receipt({
    required FlarkCoreEditPresentationTransitionV1 transition,
    required int baseStart,
    required int baseEnd,
    String replacement = '',
  }) {
    final delta = replacement.length - (baseEnd - baseStart);
    return FlarkCoreEditIntentReceiptV1(
      disposition: FlarkCoreEditIntentDispositionV1.applied,
      baseRevision: 1,
      resultRevision: 2,
      baseByteStart: baseStart,
      baseByteEnd: baseEnd,
      baseUtf16Start: baseStart,
      baseUtf16End: baseEnd,
      resultByteStart: baseStart,
      resultByteEnd: baseStart + replacement.length,
      resultUtf16Start: baseStart,
      resultUtf16End: baseStart + replacement.length,
      replacement: replacement,
      resultSelectionUtf16: baseStart + replacement.length,
      resultSourceByteLength: 100 + delta,
      resultSourceUtf16Length: 100 + delta,
      historyToken: null,
      parserPending: true,
      presentationProven: true,
      logicalEditId: 1,
      requestDigest: 1,
      telemetry: const FlarkCoreEditIntentTelemetryV1(
        coreQueueMicros: 0,
        workerRoundTripMicros: 0,
        workerQueueMicros: 0,
        nativeFfiMicros: 0,
        coreAdoptionMicros: 0,
      ),
      presentationTransition: transition,
    );
  }

  FlarkCoreCommittedPresentationSurfaceV1 structuralSurface({
    required int rowOrdinal,
    required int start,
    required int end,
    bool projectionCurrent = true,
    int? removedRowOrdinal,
  }) => FlarkCoreCommittedPresentationSurfaceV1(
    rowOrdinal: rowOrdinal,
    sourceUtf16: FlarkSourceRange(start, end),
    presentation: FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(start, end),
      leadingText: '',
      text: List<String>.filled(end - start, 'x', growable: false).join(),
      globalUtf16Start: start,
      kind: 5,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: rowOrdinal,
      runs: const [],
    ),
    projectionCurrent: projectionCurrent,
    removedRowOrdinal: removedRowOrdinal,
  );

  FlarkViewport exactEmptyViewport({
    int revision = 7,
    FlarkCertification certification = FlarkCertification.currentCertified,
    FlarkSourceRange requestedBytes = const FlarkSourceRange(0, 0),
    FlarkSourceRange coveredBytes = const FlarkSourceRange(0, 0),
    FlarkSourceRange coveredUtf16 = const FlarkSourceRange(0, 0),
    String? neutralSource = '',
    int continuation = 0,
    List<FlarkCertificationRange> certificationRanges = const [],
  }) => FlarkViewport(
    revision: revision,
    snapshot: 1,
    requestedBytes: requestedBytes,
    coveredBytes: coveredBytes,
    coveredUtf16: coveredUtf16,
    certification: certification,
    rows: const [],
    neutralSource: neutralSource,
    continuation: continuation,
    certificationRanges: certificationRanges,
  );

  FlarkViewport exactPendingViewport({
    int revision = 7,
    FlarkCertification certification = FlarkCertification.pendingNeutral,
    FlarkSourceRange requestedBytes = const FlarkSourceRange(0, 3),
    FlarkSourceRange coveredBytes = const FlarkSourceRange(0, 3),
    FlarkSourceRange coveredUtf16 = const FlarkSourceRange(0, 2),
    String? neutralSource = 'éx',
    int continuation = 0,
    List<FlarkCertificationRange> certificationRanges = const [],
  }) => FlarkViewport(
    revision: revision,
    snapshot: 1,
    requestedBytes: requestedBytes,
    coveredBytes: coveredBytes,
    coveredUtf16: coveredUtf16,
    certification: certification,
    rows: const [],
    neutralSource: neutralSource,
    continuation: continuation,
    certificationRanges: certificationRanges,
  );

  FlarkViewport certifiedRowViewport({int revision = 7}) => FlarkViewport(
    revision: revision,
    snapshot: 1,
    requestedBytes: const FlarkSourceRange(0, 1),
    coveredBytes: const FlarkSourceRange(0, 1),
    coveredUtf16: const FlarkSourceRange(0, 1),
    certification: FlarkCertification.currentCertified,
    rows: [
      FlarkViewportRow(
        ordinal: 0,
        kind: 5,
        sourceBytes: const FlarkSourceRange(0, 1),
        sourceUtf16: const FlarkSourceRange(0, 1),
        editableBytes: const FlarkSourceRange(0, 1),
        editableUtf16: const FlarkSourceRange(0, 1),
        editCapability: FlarkViewportRowEditCapability.contiguous,
        headingLevel: null,
        headingStyle: null,
        listItem: null,
        blockQuote: null,
        codeBlock: null,
        thematicBreak: false,
        pathDepth: 0,
        inlineFacts: const [],
      ),
    ],
    neutralSource: null,
    continuation: 0,
  );

  FlarkViewport readyBlankViewport() => FlarkViewport(
    revision: 7,
    snapshot: 1,
    requestedBytes: const FlarkSourceRange(0, 1),
    coveredBytes: const FlarkSourceRange(0, 1),
    coveredUtf16: const FlarkSourceRange(0, 1),
    certification: FlarkCertification.currentCertified,
    rows: const [],
    neutralSource: '\n',
    continuation: 0,
  );

  test('edit publication proof is phase-bound to installed authority', () {
    bool proves(
      FlarkViewport viewport, {
      required bool opening,
      bool? ready,
      int revision = 7,
      int bytes = 3,
      int utf16 = 2,
    }) => viewport.provesEditPublication(
      documentRevision: revision,
      documentSourceByteLength: bytes,
      documentSourceUtf16Length: utf16,
      documentOpening: opening,
      documentReady: ready ?? !opening,
      allowExactPending: opening,
    );

    expect(
      proves(certifiedRowViewport(), opening: true, bytes: 1, utf16: 1),
      isTrue,
    );
    expect(
      proves(certifiedRowViewport(), opening: false, bytes: 1, utf16: 1),
      isTrue,
    );
    expect(
      proves(
        certifiedRowViewport(),
        opening: false,
        ready: false,
        bytes: 1,
        utf16: 1,
      ),
      isFalse,
      reason: 'a sealed parser cannot report ready before convergence',
    );
    expect(
      proves(readyBlankViewport(), opening: false, bytes: 1, utf16: 1),
      isTrue,
      reason: 'a Ready blank document is safe despite having no semantic row',
    );
    expect(
      proves(readyBlankViewport(), opening: true, bytes: 1, utf16: 1),
      isFalse,
      reason: 'the same blank viewport is not certified opening authority',
    );
    expect(
      proves(exactEmptyViewport(), opening: true, bytes: 0, utf16: 0),
      isTrue,
    );
    expect(
      proves(exactEmptyViewport(), opening: false, bytes: 0, utf16: 0),
      isTrue,
    );
    expect(proves(exactPendingViewport(), opening: true), isTrue);
    expect(proves(exactPendingViewport(), opening: false), isFalse);
    expect(
      proves(exactPendingViewport(), opening: true, bytes: 4, utf16: 3),
      isFalse,
      reason:
          'same-revision stream growth invalidates complete pending coverage',
    );
    expect(proves(certifiedRowViewport(revision: 6), opening: false), isFalse);
  });

  test('exact empty viewport proof rejects every stale shape sampled', () {
    bool proves(FlarkViewport viewport, {int revision = 7}) =>
        viewport.provesExactEmptyDocument(
          documentRevision: revision,
          documentSourceByteLength: 0,
          documentSourceUtf16Length: 0,
        );

    expect(proves(exactEmptyViewport()), isTrue);
    expect(proves(exactEmptyViewport(revision: 6)), isFalse);
    expect(
      proves(
        exactEmptyViewport(certification: FlarkCertification.pendingNeutral),
      ),
      isFalse,
    );
    expect(
      proves(exactEmptyViewport(requestedBytes: const FlarkSourceRange(0, 1))),
      isFalse,
    );
    expect(proves(exactEmptyViewport(neutralSource: null)), isFalse);
    expect(proves(exactEmptyViewport(continuation: 1)), isFalse);
    expect(
      proves(
        exactEmptyViewport(
          certificationRanges: const [
            FlarkCertificationRange(
              certification: FlarkCertification.currentCertified,
              sourceBytes: FlarkSourceRange(0, 0),
              sourceUtf16: FlarkSourceRange(0, 0),
            ),
          ],
        ),
      ),
      isFalse,
    );
  });

  test('exact pending viewport proof rejects partial or mixed authority', () {
    bool proves(FlarkViewport viewport, {int bytes = 3, int utf16 = 2}) =>
        viewport.provesExactPendingDocument(
          documentRevision: 7,
          documentSourceByteLength: bytes,
          documentSourceUtf16Length: utf16,
        );

    expect(proves(exactPendingViewport()), isTrue);
    expect(proves(exactPendingViewport(revision: 6)), isFalse);
    expect(
      proves(
        exactPendingViewport(
          certification: FlarkCertification.currentCertified,
        ),
      ),
      isFalse,
    );
    expect(
      proves(
        exactPendingViewport(requestedBytes: const FlarkSourceRange(0, 2)),
      ),
      isFalse,
    );
    expect(
      proves(exactPendingViewport(coveredBytes: const FlarkSourceRange(0, 2))),
      isFalse,
    );
    expect(
      proves(exactPendingViewport(coveredUtf16: const FlarkSourceRange(0, 1))),
      isFalse,
    );
    expect(proves(exactPendingViewport(neutralSource: 'xx')), isFalse);
    expect(proves(exactPendingViewport(continuation: 1)), isFalse);
    // A stream append can preserve revision while making an earlier exact
    // pending query partial. Document lengths are therefore part of proof.
    expect(proves(exactPendingViewport(), bytes: 4, utf16: 3), isFalse);
  });

  test('published structural and table collections own their snapshots', () {
    final styles = <FlarkCorePresentationInlineStyle>{
      FlarkCorePresentationInlineStyle.emphasis,
    };
    final run = FlarkCorePresentationRun(
      text: 'x',
      sourceUtf16Start: 0,
      sourceUtf16End: 1,
      sourceExact: true,
      styles: styles,
    );
    final runs = <FlarkCorePresentationRun>[run];
    final presentation = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(0, 1),
      leadingText: '',
      text: 'x',
      globalUtf16Start: 0,
      kind: 5,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: 0,
      runs: runs,
    );
    styles.clear();
    runs.clear();

    expect(presentation.runs, [run]);
    expect(presentation.runs.single.styles, {
      FlarkCorePresentationInlineStyle.emphasis,
    });
    expect(() => presentation.runs.clear(), throwsUnsupportedError);
    expect(
      () => presentation.runs.single.styles.clear(),
      throwsUnsupportedError,
    );
    final projectionCells = <FlarkProjectionEditCell>[cell];
    final surface = FlarkCoreCommittedPresentationSurfaceV1(
      rowOrdinal: 0,
      sourceUtf16: const FlarkSourceRange(0, 1),
      presentation: presentation,
      projectionEditCells: projectionCells,
    );
    projectionCells.clear();

    expect(surface.projectionEditCells, const [cell]);
    expect(() => surface.projectionEditCells.clear(), throwsUnsupportedError);

    const tableCell = FlarkTableCellPresentation(
      alignment: FlarkTableAlignment.left,
      header: false,
      autocompleted: false,
      sourceBytes: FlarkSourceRange(0, 1),
      sourceUtf16: FlarkSourceRange(0, 1),
      contentBytes: FlarkSourceRange(0, 1),
      contentUtf16: FlarkSourceRange(0, 1),
    );
    final tableRow = <FlarkTableCellPresentation>[tableCell];
    final tableRows = <List<FlarkTableCellPresentation>>[tableRow];
    final table = FlarkTablePresentation(rows: tableRows);
    tableRow.clear();
    tableRows.clear();

    expect(table.rows, hasLength(1));
    expect(table.rows.single, const [tableCell]);
    expect(() => table.rows.clear(), throwsUnsupportedError);
    expect(() => table.rows.single.clear(), throwsUnsupportedError);
  });

  test('one binder normalizes edit cells ahead of legacy envelopes', () {
    final authority = bindPendingDependencyAuthority(
      revision: 7,
      cells: const [cell],
      envelopes: const [envelope],
      authorizedContentUtf16: const FlarkSourceRange(0, 4),
      startUtf16: 2,
      endUtf16: 2,
      replacement: 'x',
    );

    expect(authority, isA<FlarkProjectionEditCellReceipt>());
    expect(authority?.resultRevision, 8);
    expect(authority?.presentsExactIsland, isTrue);

    final successor = authority?.continueWith(
      startUtf16: 3,
      endUtf16: 3,
      replacement: 'y',
    );
    expect(successor, isA<FlarkProjectionEditCellReceipt>());
    expect(successor?.resultRevision, 9);
  });

  test('binder retains projected outcome when only an envelope matches', () {
    final authority = bindPendingDependencyAuthority(
      revision: 4,
      cells: const [],
      envelopes: const [envelope],
      authorizedContentUtf16: const FlarkSourceRange(0, 4),
      startUtf16: 2,
      endUtf16: 2,
      replacement: 'x',
    );

    expect(authority, isA<FlarkProjectionContinuityReceipt>());
    expect(authority?.presentsExactIsland, isFalse);
  });

  test(
    'committed adoption chains structural state and reports row removals',
    () {
      final first = structuralSurface(rowOrdinal: 1, start: 0, end: 2);
      final active = structuralSurface(rowOrdinal: 2, start: 2, end: 4);
      final successor = structuralSurface(
        rowOrdinal: 3,
        start: 4,
        end: 5,
        removedRowOrdinal: 9,
      );
      final snapshot = FlarkPendingPresentationSnapshot(
        structuralSurfaces: [
          FlarkPendingStructuralSurface(surface: first),
          FlarkPendingStructuralSurface(surface: active),
        ],
        taskChecks: const {7: true},
      );
      final adoption = snapshot.adoptCommittedTransition(
        receipt: receipt(
          transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
          baseStart: 4,
          baseEnd: 4,
          replacement: '\n',
        ),
        transition: FlarkCoreCommittedPresentationTransitionV1(
          surfaces: [successor],
          removedRowOrdinals: const [10],
        ),
      );

      expect(
        adoption.snapshot.structuralSurfaces.map(
          (state) => state.surface.rowOrdinal,
        ),
        [1, 3],
      );
      expect(adoption.snapshot.taskChecks, const {7: true});
      expect(adoption.removedRowOrdinals, {9, 10});
      expect(() => adoption.removedRowOrdinals.add(11), throwsUnsupportedError);
      expect(adoption.requiresParserCertification, isFalse);
    },
  );

  test('committed adoption owns caret-boundary retirement policy', () {
    final boundary = FlarkPendingCaretBoundary(rowOrdinal: 2, rowEndUtf16: 4);
    final snapshot = FlarkPendingPresentationSnapshot(
      paragraphGap: const FlarkCoreCommittedPresentationGapV1(
        rowOrdinal: 2,
        rowEndUtf16: 4,
      ),
      caretBoundary: boundary,
    );
    final transition = FlarkCoreCommittedPresentationTransitionV1(
      clearPriorGap: true,
    );

    final interiorMerge = snapshot.adoptCommittedTransition(
      receipt: receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
        baseStart: 5,
        baseEnd: 6,
      ),
      transition: transition,
    );
    final boundaryMerge = snapshot.adoptCommittedTransition(
      receipt: receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
        baseStart: 4,
        baseEnd: 5,
      ),
      transition: transition,
    );

    expect(interiorMerge.snapshot.paragraphGap, isNull);
    expect(interiorMerge.snapshot.caretBoundary, same(boundary));
    expect(boundaryMerge.snapshot.caretBoundary, isNull);
    expect(interiorMerge.requiresParserCertification, isTrue);
  });

  test('committed adoption classifies unsafe provisional surfaces in Core', () {
    final unproved = structuralSurface(
      rowOrdinal: 1,
      start: 0,
      end: 1,
      projectionCurrent: false,
    );
    final unprovedAdoption = const FlarkPendingPresentationSnapshot.empty()
        .adoptCommittedTransition(
          receipt: receipt(
            transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
            baseStart: 0,
            baseEnd: 0,
            replacement: '\n',
          ),
          transition: FlarkCoreCommittedPresentationTransitionV1(
            surfaces: [unproved],
          ),
        );
    final gapOnlyContinuation = const FlarkPendingPresentationSnapshot.empty()
        .adoptCommittedTransition(
          receipt: receipt(
            transition: FlarkCoreEditPresentationTransitionV1.continueList,
            baseStart: 0,
            baseEnd: 0,
            replacement: '\n- ',
          ),
          transition: FlarkCoreCommittedPresentationTransitionV1(
            gap: const FlarkCoreCommittedPresentationGapV1(
              rowOrdinal: 1,
              rowEndUtf16: 1,
            ),
          ),
        );

    expect(unprovedAdoption.requiresParserCertification, isTrue);
    expect(gapOnlyContinuation.requiresParserCertification, isTrue);

    final absentTransition =
        FlarkPendingPresentationSnapshot(
          structuralSurfaces: [
            FlarkPendingStructuralSurface(surface: unproved),
          ],
          taskChecks: const {4: false},
        ).adoptCommittedTransition(
          receipt: receipt(
            transition: FlarkCoreEditPresentationTransitionV1.none,
            baseStart: 0,
            baseEnd: 0,
          ),
          transition: null,
        );
    expect(absentTransition.snapshot.structuralSurfaces, isEmpty);
    expect(absentTransition.snapshot.taskChecks, const {4: false});
    expect(absentTransition.requiresParserCertification, isTrue);
  });

  test('snapshot owns dependency, structure, gap, and actions immutably', () {
    final authority = bindPendingDependencyAuthority(
      revision: 4,
      cells: const [cell],
      envelopes: const [],
      authorizedContentUtf16: const FlarkSourceRange(0, 4),
      startUtf16: 2,
      endUtf16: 2,
      replacement: 'x',
    )!;
    final presentation = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(0, 5),
      leadingText: '',
      text: 'abxcd',
      globalUtf16Start: 0,
      kind: 5,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: 3,
      runs: [
        FlarkCorePresentationRun(
          text: 'abxcd',
          sourceUtf16Start: 0,
          sourceUtf16End: 5,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final structuralSurface = FlarkCoreCommittedPresentationSurfaceV1(
      rowOrdinal: 3,
      sourceUtf16: FlarkSourceRange(0, 5),
      presentation: presentation,
      projectionCurrent: true,
    );
    final snapshot = FlarkPendingPresentationSnapshot(
      dependency: FlarkPendingDependencyPresentation(
        rowOrdinal: 3,
        authority: authority,
        presentation: presentation,
      ),
      paragraphGap: const FlarkCoreCommittedPresentationGapV1(
        rowOrdinal: 3,
        rowEndUtf16: 5,
      ),
      structuralSurfaces: [
        FlarkPendingStructuralSurface(surface: structuralSurface),
      ],
    ).withTaskCheck(9, true);

    expect(snapshot.isEmpty, isFalse);
    expect(snapshot.hasPresentationAuthority, isTrue);
    expect(snapshot.dependency?.rowOrdinal, 3);
    expect(snapshot.paragraphGap?.rowEndUtf16, 5);
    expect(snapshot.structuralSurfaces.single.surface.rowOrdinal, 3);
    expect(snapshot.taskChecks, {9: true});
    expect(
      () => snapshot.structuralSurfaces.add(
        FlarkPendingStructuralSurface(surface: structuralSurface),
      ),
      throwsUnsupportedError,
    );
    expect(() => snapshot.taskChecks[10] = false, throwsUnsupportedError);

    for (final part in FlarkPendingPresentationPart.values) {
      final retired = snapshot.retire({part});
      expect(
        retired.dependency == null,
        part == FlarkPendingPresentationPart.dependency,
        reason: '$part dependency retirement',
      );
      expect(
        retired.paragraphGap == null,
        part == FlarkPendingPresentationPart.paragraphGap,
        reason: '$part gap retirement',
      );
      expect(
        retired.structuralSurfaces.isEmpty,
        part == FlarkPendingPresentationPart.structuralSurfaces,
        reason: '$part structural retirement',
      );
      expect(
        retired.taskChecks.isEmpty,
        part == FlarkPendingPresentationPart.taskChecks,
        reason: '$part task retirement',
      );
    }

    final cleared = snapshot.clear();
    expect(cleared.isEmpty, isTrue);
    expect(snapshot.isEmpty, isFalse, reason: 'snapshots are immutable');
  });

  test('dependency publication owns an ordered multi-row result immutably', () {
    final authority = bindPendingDependencyAuthority(
      revision: 4,
      cells: const [cell],
      envelopes: const [],
      authorizedContentUtf16: const FlarkSourceRange(0, 4),
      startUtf16: 2,
      endUtf16: 2,
      replacement: 'x',
    )!;
    final first = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(0, 5),
      leadingText: '',
      text: 'code\n',
      globalUtf16Start: 0,
      kind: 7,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: FlarkCodeBlockPresentation(
        style: FlarkCodeBlockStyle.fencedBacktick,
        minimumClosingLength: 3,
        fenceOffset: 0,
        closed: true,
      ),
      thematicBreak: false,
      ordinal: 3,
      runs: [
        FlarkCorePresentationRun(
          text: 'code\n',
          sourceUtf16Start: 0,
          sourceUtf16End: 5,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final second = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(5, 13),
      leadingText: '',
      text: 'sentinel',
      globalUtf16Start: 5,
      kind: 5,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: 4,
      runs: [
        FlarkCorePresentationRun(
          text: 'sentinel',
          sourceUtf16Start: 5,
          sourceUtf16End: 13,
          sourceExact: true,
          styles: {FlarkCorePresentationInlineStyle.strong},
        ),
      ],
    );
    final dependency = FlarkPendingDependencyPresentation.multi(
      rowOrdinal: 3,
      authority: authority,
      presentations: [first, second],
      replacedRowOrdinals: const {3, 4},
    );

    expect(dependency.presentations, [first, second]);
    expect(dependency.replacedRowOrdinals, const {3, 4});
    expect(dependency.sourceUtf16.start, 0);
    expect(dependency.sourceUtf16.end, 13);
    expect(() => dependency.presentation, throwsStateError);
    expect(() => dependency.presentations.add(first), throwsUnsupportedError);
    expect(() => dependency.replacedRowOrdinals.add(5), throwsUnsupportedError);
  });

  test('bounded plan selects and materializes each exact parser step', () {
    final plan = FlarkPendingPresentationPlan(
      sequence: 'ab',
      triggerBytes: const FlarkSourceRange(0, 0),
      triggerUtf16: const FlarkSourceRange(0, 0),
      affectedBytes: const FlarkSourceRange(0, 6),
      affectedUtf16: const FlarkSourceRange(0, 6),
      replacedRowCount: 1,
      steps: [
        FlarkPendingPresentationStep(
          prefixLength: 1,
          affectedBytes: const FlarkSourceRange(0, 7),
          affectedUtf16: const FlarkSourceRange(0, 7),
          rows: [_strongPlanRow(prefixLength: 1)],
        ),
        FlarkPendingPresentationStep(
          prefixLength: 2,
          affectedBytes: const FlarkSourceRange(0, 8),
          affectedUtf16: const FlarkSourceRange(0, 8),
          rows: [_strongPlanRow(prefixLength: 2)],
        ),
      ],
    );

    final first = bindPendingDependencyAuthority(
      revision: 7,
      plans: [plan],
      cells: const [],
      envelopes: const [],
      authorizedContentUtf16: const FlarkSourceRange(0, 6),
      startUtf16: 0,
      endUtf16: 0,
      replacement: 'a',
    );
    expect(first, isA<FlarkBoundedPendingPresentationPlanReceipt>());
    final firstPlan = first! as FlarkBoundedPendingPresentationPlanReceipt;
    expect(firstPlan.resultRevision, 8);
    expect(firstPlan.prefixLength, 1);
    final firstPresentation = materializeBoundedPendingPresentationPlan(
      authority: firstPlan,
      rowOrdinal: 4,
      visibleSource: 'a**x**\n',
      visibleUtf16Start: 0,
    )!;
    expect(firstPresentation.presentations.single.text, 'ax\n');
    expect(firstPresentation.presentations.single.ordinal, 4);
    expect(
      firstPresentation.presentations.single.runs
          .where((run) => run.text == 'x')
          .single
          .styles,
      {FlarkCorePresentationInlineStyle.strong},
    );

    final second = firstPlan.continueWith(
      startUtf16: 1,
      endUtf16: 1,
      replacement: 'b',
    )!;
    expect(second.resultRevision, 9);
    expect(second.prefixLength, 2);
    expect(
      materializeBoundedPendingPresentationPlan(
        authority: second,
        rowOrdinal: 4,
        visibleSource: 'ab**x**\n',
        visibleUtf16Start: 0,
      )!.presentations.single.text,
      'abx\n',
    );
    expect(
      second.continueWith(startUtf16: 2, endUtf16: 2, replacement: 'c'),
      isNull,
    );
    expect(
      authorizeBoundedPendingPresentationPlan(
        revision: 7,
        plans: [plan],
        startUtf16: 0,
        endUtf16: 0,
        replacement: 'x',
      ),
      isNull,
    );
    expect(
      authorizeBoundedPendingPresentationPlan(
        revision: 7,
        plans: [plan, plan],
        startUtf16: 0,
        endUtf16: 0,
        replacement: 'a',
      ),
      isNull,
      reason: 'ambiguous parser plans fail closed',
    );
    expect(
      materializeBoundedPendingPresentationPlan(
        authority: firstPlan,
        rowOrdinal: 4,
        visibleSource: 'short',
        visibleUtf16Start: 0,
      ),
      isNull,
      reason: 'a plan cannot escape the exact materialized source window',
    );
  });
}

FlarkViewportRow _strongPlanRow({required int prefixLength}) =>
    FlarkViewportRow(
      ordinal: 0,
      kind: 5,
      sourceBytes: FlarkSourceRange(0, 6 + prefixLength),
      sourceUtf16: FlarkSourceRange(0, 6 + prefixLength),
      editableBytes: FlarkSourceRange(0, 6 + prefixLength),
      editableUtf16: FlarkSourceRange(0, 6 + prefixLength),
      editCapability: FlarkViewportRowEditCapability.contiguous,
      headingLevel: null,
      headingStyle: null,
      listItem: null,
      blockQuote: null,
      codeBlock: null,
      thematicBreak: false,
      pathDepth: 1,
      inlineFacts: [
        FlarkInlineFact(
          kind: FlarkInlineFactKind.strong,
          flags: 0,
          sourceBytes: FlarkSourceRange(prefixLength, prefixLength + 5),
          sourceUtf16: FlarkSourceRange(prefixLength, prefixLength + 5),
          contentBytes: FlarkSourceRange(prefixLength + 2, prefixLength + 3),
          contentUtf16: FlarkSourceRange(prefixLength + 2, prefixLength + 3),
        ),
      ],
    );
