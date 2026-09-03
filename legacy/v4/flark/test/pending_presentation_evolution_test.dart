import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  FlarkCorePresentationRow row({
    required String text,
    Set<FlarkCorePresentationInlineStyle> styles = const {},
  }) => FlarkCorePresentationRow(
    sourceUtf16: FlarkSourceRange(0, text.length),
    leadingText: '',
    text: text,
    globalUtf16Start: 0,
    kind: 5,
    headingLevel: null,
    blockQuoteDepth: null,
    codeBlock: null,
    thematicBreak: false,
    ordinal: 7,
    runs: [
      FlarkCorePresentationRun(
        text: text,
        sourceUtf16Start: 0,
        sourceUtf16End: text.length,
        sourceExact: true,
        styles: styles,
      ),
    ],
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
    required int ordinal,
    required FlarkSourceRange range,
    required FlarkCoreCommittedPresentationSurfaceRole role,
    required String text,
  }) => FlarkCoreCommittedPresentationSurfaceV1(
    rowOrdinal: ordinal,
    sourceUtf16: range,
    presentation: FlarkCorePresentationRow(
      sourceUtf16: range,
      leadingText: '',
      text: text,
      globalUtf16Start: range.start,
      kind: 5,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: ordinal,
      runs: const [],
    ),
    projectionCurrent: true,
    role: role,
  );

  FlarkViewportRow viewportRow({
    int kind = 5,
    List<FlarkInlineFact>? inlineFacts,
  }) => FlarkViewportRow(
    ordinal: 7,
    kind: kind,
    sourceBytes: const FlarkSourceRange(0, 4),
    sourceUtf16: const FlarkSourceRange(0, 4),
    editableBytes: const FlarkSourceRange(0, 4),
    editableUtf16: const FlarkSourceRange(0, 4),
    editCapability: FlarkViewportRowEditCapability.contiguous,
    headingLevel: null,
    headingStyle: null,
    listItem: null,
    blockQuote: null,
    codeBlock: null,
    thematicBreak: false,
    pathDepth: 0,
    inlineFacts: inlineFacts,
  );

  FlarkViewport certifiedViewport({
    required FlarkViewportRow viewportRow,
    int revision = 2,
  }) => FlarkViewport(
    revision: revision,
    snapshot: 1,
    requestedBytes: const FlarkSourceRange(0, 4),
    coveredBytes: const FlarkSourceRange(0, 4),
    coveredUtf16: const FlarkSourceRange(0, 4),
    certification: FlarkCertification.currentCertified,
    rows: [viewportRow],
    neutralSource: null,
    continuation: 0,
  );

  test(
    'viewport supersession fails closed until parser facts replace proof',
    () {
      final authority = authorizeRowProjectionContinuity(
        revision: 1,
        envelopes: const [
          FlarkLiteralSafeEnvelope(
            editClass: FlarkLiteralEditClass.asciiWordInsertion,
            sourceBytes: FlarkSourceRange(0, 3),
            sourceUtf16: FlarkSourceRange(0, 3),
          ),
        ],
        authorizedContentUtf16: const FlarkSourceRange(0, 3),
        startUtf16: 1,
        endUtf16: 1,
        replacement: 'x',
      )!;
      final pending = FlarkPendingPresentationSnapshot(
        dependency: FlarkPendingDependencyPresentation(
          rowOrdinal: 7,
          authority: authority,
          presentation: row(
            text: 'axbc',
            styles: const {FlarkCorePresentationInlineStyle.emphasis},
          ),
        ),
      );

      expect(
        certifiedViewportSupersedesPendingDependency(
          viewport: certifiedViewport(
            viewportRow: viewportRow(inlineFacts: null),
          ),
          pendingPresentation: pending,
        ),
        isFalse,
      );
      expect(
        certifiedViewportSupersedesPendingDependency(
          viewport: certifiedViewport(
            viewportRow: viewportRow(inlineFacts: const []),
            revision: 1,
          ),
          pendingPresentation: pending,
        ),
        isFalse,
      );
      expect(
        certifiedViewportSupersedesPendingDependency(
          viewport: certifiedViewport(
            viewportRow: viewportRow(inlineFacts: const []),
          ),
          pendingPresentation: pending,
        ),
        isTrue,
      );
      expect(
        certifiedViewportSupersedesPendingDependency(
          viewport: certifiedViewport(viewportRow: viewportRow(kind: 2)),
          pendingPresentation: pending,
        ),
        isTrue,
      );
    },
  );

  test('transition ownership recovers one containing parser row', () {
    final active = row(text: 'abc');

    final transition = resolvePendingPresentationTransition(
      receipt: receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 2,
        baseEnd: 2,
        replacement: '\n\n',
      ),
      pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
      activeOrdinal: -2,
      priorRows: [active],
    );

    expect(transition?.surfaces, hasLength(2));
    expect(transition?.surfaces.first.rowOrdinal, 7);
  });

  test('structural caret boundary begins before a painted separator', () {
    final boundary = caretBoundaryForStructuralSurfaces([
      FlarkPendingStructuralSurface(
        surface: structuralSurface(
          ordinal: 7,
          range: const FlarkSourceRange(3, 4),
          role: FlarkCoreCommittedPresentationSurfaceRole.blockSeparator,
          text: '\n',
        ),
      ),
      FlarkPendingStructuralSurface(
        surface: structuralSurface(
          ordinal: 8,
          range: const FlarkSourceRange(4, 4),
          role: FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
          text: '',
        ),
      ),
    ]);

    expect(boundary?.rowOrdinal, 8);
    expect(boundary?.rowEndUtf16, 3);
    expect(boundary?.authorizedContentUtf16, const FlarkSourceRange(4, 4));
  });

  test('parser-authorized continuity evolves one immutable core row', () {
    final authority = authorizeRowProjectionContinuity(
      revision: 4,
      envelopes: const [
        FlarkLiteralSafeEnvelope(
          editClass: FlarkLiteralEditClass.asciiWordInsertion,
          sourceBytes: FlarkSourceRange(0, 3),
          sourceUtf16: FlarkSourceRange(0, 3),
        ),
      ],
      authorizedContentUtf16: const FlarkSourceRange(0, 3),
      startUtf16: 1,
      endUtf16: 1,
      replacement: 'x',
    );
    final base = row(
      text: 'abc',
      styles: const {FlarkCorePresentationInlineStyle.emphasis},
    );

    final result = advancePendingPresentationRow(
      presentation: base,
      authority: authority!,
      visibleSource: 'abc',
      visibleUtf16Start: 0,
      startUtf16: 1,
      endUtf16: 1,
      replacement: 'x',
    );

    expect(result!.text, 'axbc');
    expect((result.sourceUtf16.start, result.sourceUtf16.end), (0, 4));
    expect(result.runs.single.styles, {
      FlarkCorePresentationInlineStyle.emphasis,
    });
    expect(base.text, 'abc');
    expect((base.sourceUtf16.start, base.sourceUtf16.end), (0, 3));
  });

  test('an edit-cell evolves only its exact parser-declared closure', () {
    const cell = FlarkProjectionEditCell(
      matcher: FlarkProjectionEditMatcher.anyNoCrLfSplice,
      affectedBytes: FlarkSourceRange(1, 2),
      affectedUtf16: FlarkSourceRange(1, 2),
      triggerBytes: FlarkSourceRange(1, 2),
      triggerUtf16: FlarkSourceRange(1, 2),
      retainBlockShell: true,
      retainOutsideClosure: true,
      presentClosureExact: true,
      chainResultCell: true,
    );
    final authority = authorizeProjectionEditCell(
      revision: 8,
      cells: const [cell],
      authorizedContentUtf16: const FlarkSourceRange(0, 3),
      authorizedBlockUtf16: const FlarkSourceRange(0, 3),
      startUtf16: 1,
      endUtf16: 2,
      replacement: 'xy',
    );

    final result = advancePendingPresentationRow(
      presentation: row(text: 'abc'),
      authority: authority!,
      visibleSource: 'abc',
      visibleUtf16Start: 0,
      startUtf16: 1,
      endUtf16: 2,
      replacement: 'xy',
    );

    expect(result!.text, 'axyc');
    expect((result.sourceUtf16.start, result.sourceUtf16.end), (0, 4));
    expect(
      result.runs.map(
        (run) => (run.text, run.sourceUtf16Start, run.sourceUtf16End),
      ),
      [('a', 0, 1), ('xy', 1, 3), ('c', 3, 4)],
    );
  });

  test('edit-cell evolution fails closed outside the bounded source', () {
    const cell = FlarkProjectionEditCell(
      matcher: FlarkProjectionEditMatcher.anyNoCrLfSplice,
      affectedBytes: FlarkSourceRange(1, 2),
      affectedUtf16: FlarkSourceRange(1, 2),
      triggerBytes: FlarkSourceRange(1, 2),
      triggerUtf16: FlarkSourceRange(1, 2),
      retainBlockShell: true,
      retainOutsideClosure: true,
      presentClosureExact: true,
      chainResultCell: true,
    );
    final authority = authorizeProjectionEditCell(
      revision: 8,
      cells: const [cell],
      authorizedContentUtf16: const FlarkSourceRange(0, 3),
      authorizedBlockUtf16: const FlarkSourceRange(0, 3),
      startUtf16: 1,
      endUtf16: 2,
      replacement: 'xy',
    );

    expect(
      advancePendingPresentationRow(
        presentation: row(text: 'abc'),
        authority: authority!,
        visibleSource: 'bc',
        visibleUtf16Start: 1,
        startUtf16: 1,
        endUtf16: 2,
        replacement: 'xy',
      ),
      isNull,
    );
  });
}
