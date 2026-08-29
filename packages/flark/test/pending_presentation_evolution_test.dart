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
