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
}
