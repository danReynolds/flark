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
}) => FlarkCorePresentationRow(
  sourceUtf16: FlarkSourceRange(sourceStart, sourceEnd),
  leadingText: leadingText,
  text: text,
  globalUtf16Start: sourceStart,
  kind: kind,
  headingLevel: null,
  blockQuoteDepth: null,
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
