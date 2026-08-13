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

  test('paragraph merge preserves mapped styling without Flutter', () {
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

    final surface = transition?.surface;
    expect(surface?.removedRowOrdinal, 5);
    expect(surface?.sourceUtf16.start, 0);
    expect(surface?.sourceUtf16.end, 18);
    expect(surface?.presentation.text, 'left boldright');
    expect(surface?.presentation.runs.last.sourceUtf16Start, 13);
    expect(surface?.presentation.runs[1].styles, {
      FlarkCorePresentationInlineStyle.strong,
    });
  });

  test('list lift removes presentation prefix and maps content runs', () {
    final listRow = _row(
      ordinal: 2,
      sourceStart: 0,
      sourceEnd: 10,
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
      sourceStart: 4,
      sourceEnd: 10,
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
    expect(presentation?.sourceUtf16.start, 2);
    expect(presentation?.runs.single.sourceUtf16Start, 2);
  });

  test('projected quote Return hides the new certified prefix', () {
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

    final presentation = transition?.surface?.presentation;
    expect(transition?.gap, isNull);
    expect(presentation?.leadingText, '│ ');
    expect(presentation?.text, 'first\nsec\nond');
    expect(presentation?.sourceUtf16.end, 20);
    expect(
      presentation?.runs.map(
        (run) => (run.text, run.sourceUtf16Start, run.sourceUtf16End),
      ),
      [('first\n', 2, 8), ('sec', 10, 13), ('\n', 13, 14), ('ond', 16, 19)],
    );
  });

  test('projected quote lift publishes ordered quote and plain surfaces', () {
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

    expect(transition?.surface, isNull);
    expect(transition?.surfaces, hasLength(2));
    final quoted = transition!.surfaces.first.presentation;
    final plain = transition.surfaces.last.presentation;
    expect(quoted.leadingText, '│ ');
    expect(quoted.text, 'first\n');
    expect((quoted.sourceUtf16.start, quoted.sourceUtf16.end), (0, 8));
    expect(plain.leadingText, isEmpty);
    expect(plain.blockQuoteDepth, isNull);
    expect(plain.text, '\nsecond');
    expect((plain.sourceUtf16.start, plain.sourceUtf16.end), (8, 16));
    expect(
      plain.runs.map(
        (run) => (run.text, run.sourceUtf16Start, run.sourceUtf16End),
      ),
      [('\n', 8, 9), ('second', 9, 15)],
    );
  });

  test('literal successor maps one temporary surface without losing peers', () {
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.liftBlockQuote,
        baseStart: 8,
        baseEnd: 10,
        replacement: '\n',
      ),
      activeOrdinal: 8,
      active: _row(
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
      ),
    );

    final mapped = mapCommittedPresentationSurfacesThroughLiteralSpliceV1(
      surfaces: transition!.surfaces,
      startUtf16: 9,
      endUtf16: 9,
      replacement: 'X',
    );

    expect(mapped, hasLength(2));
    expect(mapped!.first.presentation.text, 'first\n');
    expect(mapped.last.presentation.text, '\nXsecond');
    expect(mapped.first.sourceUtf16.end, 8);
    expect(mapped.last.sourceUtf16.end, 17);
    expect(
      mapped.last.presentation.runs.map(
        (run) => (run.text, run.sourceUtf16Start, run.sourceUtf16End),
      ),
      [('\n', 8, 9), ('X', 9, 10), ('second', 10, 16)],
    );
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
  }) => resolveCommittedPresentationTransitionV1(
    receipt: receipt,
    priorActiveOrdinal: activeOrdinal,
    activeRow: active,
    precedingRow: preceding,
    priorGapPending: false,
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
}) => FlarkCorePresentationRow(
  sourceUtf16: FlarkSourceRange(sourceStart, sourceEnd),
  leadingText: leadingText,
  text: text,
  globalUtf16Start: globalStart ?? sourceStart,
  kind: kind,
  headingLevel: null,
  blockQuoteDepth: blockQuoteDepth,
  codeBlock: null,
  thematicBreak: false,
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
