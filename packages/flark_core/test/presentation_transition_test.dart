import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  const frontend = _FakeDartFrontend();

  test('paragraph split publishes a receipt-backed neutral gap', () {
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 4,
        baseEnd: 4,
        replacement: '\n\n',
      ),
      activeOrdinal: 7,
    );

    expect(transition?.gap?.rowOrdinal, 7);
    expect(transition?.gap?.rowEndUtf16, 5);
    expect(transition?.surface, isNull);
  });

  test('empty successor split extends the existing gap by one row', () {
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 4,
        baseEnd: 4,
        replacement: '\n',
      ),
      activeOrdinal: 7,
    );

    expect(transition?.gap?.rowOrdinal, 7);
    expect(transition?.gap?.rowEndUtf16, 4);
  });

  test('partial blank paragraph Backspace retains the prior gap', () {
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.retainParagraphGap,
        baseStart: 5,
        baseEnd: 6,
        replacement: '',
      ),
      activeOrdinal: -2,
      priorGapPending: true,
    );

    expect(transition?.retainPriorGap, isTrue);
    expect(transition?.clearPriorGap, isFalse);
  });

  test('paragraph merge fails closed around predecessor inline styling', () {
    final left = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 14,
      text: 'left bold',
      runs: const [
        FlarkCorePresentationRun(
          text: 'left ',
          sourceUtf16Start: 0,
          sourceUtf16End: 5,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'bold',
          sourceUtf16Start: 7,
          sourceUtf16End: 11,
          sourceExact: true,
          styles: {FlarkCorePresentationInlineStyle.strong},
        ),
      ],
    );
    final right = _row(
      ordinal: 5,
      sourceStart: 14,
      sourceEnd: 19,
      text: 'right',
      runs: const [
        FlarkCorePresentationRun(
          text: 'right',
          sourceUtf16Start: 14,
          sourceUtf16End: 19,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
        baseStart: 13,
        baseEnd: 14,
        replacement: '',
      ),
      activeOrdinal: 5,
      active: right,
      preceding: left,
    );

    expect(transition?.clearPriorGap, isTrue);
    expect(transition?.surfaces, isEmpty);
  });

  test('paragraph merge fails closed around hidden unstyled inline syntax', () {
    final image = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 12,
      text: 'foo',
      runs: const [
        FlarkCorePresentationRun(
          text: 'foo',
          sourceUtf16Start: 2,
          sourceUtf16End: 5,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final right = _row(
      ordinal: 5,
      sourceStart: 12,
      sourceEnd: 17,
      text: 'right',
      runs: const [
        FlarkCorePresentationRun(
          text: 'right',
          sourceUtf16Start: 12,
          sourceUtf16End: 17,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
        baseStart: 11,
        baseEnd: 12,
        replacement: '',
      ),
      activeOrdinal: 5,
      active: right,
      preceding: image,
    );

    expect(transition?.clearPriorGap, isTrue);
    expect(transition?.surfaces, isEmpty);
  });

  test('block-prefix gap cannot conceal an unstyled image projection', () {
    final quoteWithImage = _row(
      ordinal: 8,
      sourceStart: 0,
      sourceEnd: 22,
      globalStart: 2,
      text: 'first\nfoo',
      leadingText: '│ ',
      blockQuoteDepth: 1,
      runs: const [
        FlarkCorePresentationRun(
          text: 'first\n',
          sourceUtf16Start: 2,
          sourceUtf16End: 8,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'foo',
          sourceUtf16Start: 12,
          sourceUtf16End: 15,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.continueBlockQuote,
        baseStart: 13,
        baseEnd: 13,
        replacement: '\n> ',
      ),
      activeOrdinal: 8,
      active: quoteWithImage,
    );

    expect(transition?.surface, isNull);
    expect(transition?.gap?.rowOrdinal, 8);
  });

  test('paragraph merge fails closed for an empty projected image', () {
    final emptyImage = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 7,
      text: '',
      runs: const [],
    );
    final right = _row(
      ordinal: 5,
      sourceStart: 7,
      sourceEnd: 12,
      text: 'right',
      runs: const [
        FlarkCorePresentationRun(
          text: 'right',
          sourceUtf16Start: 7,
          sourceUtf16End: 12,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
        baseStart: 6,
        baseEnd: 7,
        replacement: '',
      ),
      activeOrdinal: 5,
      active: right,
      preceding: emptyImage,
    );

    expect(transition?.clearPriorGap, isTrue);
    expect(transition?.surfaces, isEmpty);
  });

  test('list lift removes presentation prefix and maps content runs', () {
    final listRow = _row(
      ordinal: 2,
      sourceStart: 0,
      sourceEnd: 10,
      globalStart: 2,
      text: 'item',
      leadingText: '- ',
      kind: 12,
      runs: const [
        FlarkCorePresentationRun(
          text: 'item',
          sourceUtf16Start: 2,
          sourceUtf16End: 6,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.liftList,
        baseStart: 0,
        baseEnd: 2,
        replacement: '',
      ),
      activeOrdinal: 2,
      active: listRow,
    );

    final presentation = transition?.surface?.presentation;
    expect(presentation?.leadingText, isEmpty);
    expect(presentation?.kind, 5);
    expect(presentation?.sourceUtf16.start, 0);
    expect(presentation?.sourceUtf16.end, 8);
    expect(presentation?.runs.single.sourceUtf16Start, 0);
  });

  test('list outdent removes one certified visual indentation level', () {
    final nested = _row(
      ordinal: 3,
      sourceStart: 0,
      sourceEnd: 10,
      globalStart: 4,
      text: 'child',
      leadingText: '  - ',
      runs: const [
        FlarkCorePresentationRun(
          text: 'child',
          sourceUtf16Start: 4,
          sourceUtf16End: 9,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.outdentList,
        baseStart: 0,
        baseEnd: 2,
        replacement: '',
      ),
      activeOrdinal: 3,
      active: nested,
    );

    final presentation = transition?.surface?.presentation;
    expect(presentation?.leadingText, '- ');
    expect(presentation?.sourceUtf16.start, 0);
    expect(presentation?.globalUtf16Start, 2);
    expect(presentation?.runs.single.sourceUtf16Start, 2);
  });

  test('list indent adds the receipt-certified visual indentation level', () {
    final item = _row(
      ordinal: 3,
      sourceStart: 0,
      sourceEnd: 8,
      globalStart: 2,
      text: 'child',
      leadingText: '- ',
      runs: const [
        FlarkCorePresentationRun(
          text: 'child',
          sourceUtf16Start: 2,
          sourceUtf16End: 7,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.indentList,
        baseStart: 0,
        baseEnd: 0,
        replacement: '  ',
      ),
      activeOrdinal: 3,
      active: item,
    );

    final presentation = transition?.surface?.presentation;
    expect(presentation?.leadingText, '  - ');
    expect(presentation?.sourceUtf16.start, 0);
    expect(presentation?.globalUtf16Start, 4);
    expect(presentation?.runs.single.sourceUtf16Start, 4);
  });

  test('deeper list outdent preserves preceding visual indentation', () {
    final nested = _row(
      ordinal: 4,
      sourceStart: 6,
      sourceEnd: 12,
      text: 'leaf',
      leadingText: '    - ',
      runs: const [
        FlarkCorePresentationRun(
          text: 'leaf',
          sourceUtf16Start: 6,
          sourceUtf16End: 10,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.outdentList,
        baseStart: 2,
        baseEnd: 4,
        replacement: '',
      ),
      activeOrdinal: 4,
      active: nested,
    );

    expect(transition?.surface?.presentation.leadingText, '  - ');
    expect(transition?.surface?.presentation.sourceUtf16.start, 2);
  });

  test('projected quote Return fails closed to a neutral gap', () {
    final quote = _row(
      ordinal: 8,
      sourceStart: 0,
      sourceEnd: 17,
      globalStart: 2,
      text: 'first\nsecond',
      leadingText: '│ ',
      blockQuoteDepth: 1,
      runs: const [
        FlarkCorePresentationRun(
          text: 'first\n',
          sourceUtf16Start: 2,
          sourceUtf16End: 8,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'second',
          sourceUtf16Start: 10,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.continueBlockQuote,
        baseStart: 13,
        baseEnd: 13,
        replacement: '\n> ',
      ),
      activeOrdinal: 8,
      active: quote,
    );

    expect(transition?.surface, isNull);
    expect(transition?.gap?.rowOrdinal, 8);
    expect(transition?.gap?.rowEndUtf16, 14);
  });

  test('projected quote lift fails closed around hidden prefix gaps', () {
    final quote = _row(
      ordinal: 8,
      sourceStart: 0,
      sourceEnd: 17,
      globalStart: 2,
      text: 'first\nsecond',
      leadingText: '│ ',
      blockQuoteDepth: 1,
      runs: const [
        FlarkCorePresentationRun(
          text: 'first\n',
          sourceUtf16Start: 2,
          sourceUtf16End: 8,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'second',
          sourceUtf16Start: 10,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.liftBlockQuote,
        baseStart: 8,
        baseEnd: 10,
        replacement: '\n',
      ),
      activeOrdinal: 8,
      active: quote,
    );

    expect(transition, isNull);
  });

  test('indented code Return fails closed around hidden prefix gaps', () {
    final code = _row(
      ordinal: 9,
      sourceStart: 0,
      sourceEnd: 16,
      globalStart: 4,
      text: 'one\ntwo\n',
      codeBlock: const FlarkCodeBlockPresentation(
        style: FlarkCodeBlockStyle.indented,
        minimumClosingLength: 0,
        fenceOffset: 0,
        closed: false,
      ),
      runs: const [
        FlarkCorePresentationRun(
          text: 'one\n',
          sourceUtf16Start: 4,
          sourceUtf16End: 8,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'two\n',
          sourceUtf16Start: 12,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.continueIndentedCode,
        baseStart: 14,
        baseEnd: 14,
        replacement: '\n    ',
      ),
      activeOrdinal: 9,
      active: code,
    );

    expect(transition?.surface, isNull);
    expect(transition?.gap?.rowOrdinal, 9);
    expect(transition?.gap?.rowEndUtf16, 15);
  });

  test('indented code Backspace fails closed around hidden prefix gaps', () {
    final code = _row(
      ordinal: 9,
      sourceStart: 0,
      sourceEnd: 16,
      globalStart: 4,
      text: 'one\ntwo\n',
      codeBlock: const FlarkCodeBlockPresentation(
        style: FlarkCodeBlockStyle.indented,
        minimumClosingLength: 0,
        fenceOffset: 0,
        closed: false,
      ),
      runs: const [
        FlarkCorePresentationRun(
          text: 'one\n',
          sourceUtf16Start: 4,
          sourceUtf16End: 8,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'two\n',
          sourceUtf16Start: 12,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.joinIndentedCode,
        baseStart: 7,
        baseEnd: 12,
        replacement: '',
      ),
      activeOrdinal: 9,
      active: code,
    );

    expect(transition, isNull);
  });

  test('thematic break deletion removes the certified semantic atom', () {
    final atom = _row(
      ordinal: 12,
      sourceStart: 0,
      sourceEnd: 4,
      text: '',
      runs: const [],
      thematicBreak: true,
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.deleteThematicBreak,
        baseStart: 0,
        baseEnd: 4,
        replacement: '',
      ),
      activeOrdinal: 12,
      active: atom,
    );

    expect(transition?.surfaces, isEmpty);
    expect(transition?.removedRowOrdinals, [12]);
    expect(transition?.clearPriorGap, isTrue);
  });

  test('nested quote outdent fails closed around hidden prefix gaps', () {
    final quote = _row(
      ordinal: 20,
      sourceStart: 0,
      sourceEnd: 21,
      globalStart: 4,
      text: 'first\nsecond',
      leadingText: '│ │ ',
      blockQuoteDepth: 2,
      runs: const [
        FlarkCorePresentationRun(
          text: 'first\n',
          sourceUtf16Start: 4,
          sourceUtf16End: 10,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'second',
          sourceUtf16Start: 14,
          sourceUtf16End: 20,
          sourceExact: true,
          styles: {},
        ),
      ],
    );

    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.outdentBlockQuote,
        baseStart: 10,
        baseEnd: 14,
        replacement: '\n> ',
      ),
      activeOrdinal: 20,
      active: quote,
    );

    expect(transition, isNull);
  });
}

/// Deliberately has no Flutter dependency. A future Dart UI adapter receives
/// the same transition state that the production Flutter adapter consumes.
final class _FakeDartFrontend {
  const _FakeDartFrontend();

  FlarkCoreCommittedPresentationTransitionV1? adopt({
    required FlarkCoreEditIntentReceiptV1 receipt,
    required int activeOrdinal,
    FlarkCorePresentationRow? active,
    FlarkCorePresentationRow? preceding,
    bool priorGapPending = false,
  }) => resolveCommittedPresentationTransitionV1(
    receipt: receipt,
    priorActiveOrdinal: activeOrdinal,
    activeRow: active,
    precedingRow: preceding,
    priorGapPending: priorGapPending,
  );
}

FlarkCorePresentationRow _row({
  required int ordinal,
  required int sourceStart,
  required int sourceEnd,
  required String text,
  required List<FlarkCorePresentationRun> runs,
  String leadingText = '',
  int kind = 5,
  int? globalStart,
  int? blockQuoteDepth,
  FlarkCodeBlockPresentation? codeBlock,
  bool thematicBreak = false,
}) => FlarkCorePresentationRow(
  sourceUtf16: FlarkSourceRange(sourceStart, sourceEnd),
  leadingText: leadingText,
  text: text,
  globalUtf16Start: globalStart ?? sourceStart,
  kind: kind,
  headingLevel: null,
  blockQuoteDepth: blockQuoteDepth,
  codeBlock: codeBlock,
  thematicBreak: thematicBreak,
  ordinal: ordinal,
  runs: runs,
);

FlarkCoreEditIntentReceiptV1 _receipt({
  required FlarkCoreEditPresentationTransitionV1 transition,
  required int baseStart,
  required int baseEnd,
  required String replacement,
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
