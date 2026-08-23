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
    const presentation = FlarkCorePresentationRow(
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
    const structuralSurface = FlarkCoreCommittedPresentationSurfaceV1(
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
      structuralSurfaces: const [
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
        const FlarkPendingStructuralSurface(surface: structuralSurface),
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
    const first = FlarkCorePresentationRow(
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
    const second = FlarkCorePresentationRow(
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
      presentations: const [first, second],
      replacedRowOrdinals: const {3, 4},
    );

    expect(dependency.presentations, const [first, second]);
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
