import 'package:flark/flark.dart';
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

  test(
    'inline-owner deletion removes its styled run without exposing markers',
    () {
      final row = _row(
        ordinal: 3,
        sourceStart: 0,
        sourceEnd: 7,
        text: 'A t Z',
        runs: [
          FlarkCorePresentationRun(
            text: 'A ',
            sourceUtf16Start: 0,
            sourceUtf16End: 2,
            sourceExact: true,
            styles: {},
          ),
          FlarkCorePresentationRun(
            text: 't',
            sourceUtf16Start: 3,
            sourceUtf16End: 4,
            sourceExact: true,
            styles: {FlarkCorePresentationInlineStyle.emphasis},
          ),
          FlarkCorePresentationRun(
            text: ' Z',
            sourceUtf16Start: 5,
            sourceUtf16End: 7,
            sourceExact: true,
            styles: {},
          ),
        ],
      );

      final transition = frontend.adopt(
        receipt: _receipt(
          transition: FlarkCoreEditPresentationTransitionV1.deleteInlineOwner,
          baseStart: 2,
          baseEnd: 5,
          replacement: '',
        ),
        activeOrdinal: 3,
        active: row,
      );

      final surface = transition!.surfaces.single;
      expect(surface.sourceUtf16.start, 0);
      expect(surface.sourceUtf16.end, 4);
      expect(surface.presentation.text, 'A  Z');
      expect(surface.presentation.runs, hasLength(2));
      expect(surface.presentation.runs.last.sourceUtf16Start, 2);
      expect(surface.presentation.runs.last.sourceUtf16End, 4);
      expect(
        surface.presentation.runs.expand((run) => run.styles),
        isNot(contains(FlarkCorePresentationInlineStyle.emphasis)),
      );
    },
  );

  test('paragraph merge fails closed around predecessor inline styling', () {
    final left = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 14,
      text: 'left bold',
      runs: [
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
      runs: [
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

  test('parser-proven paragraph merge retains predecessor inline styling', () {
    final left = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 14,
      text: 'left bold',
      runs: [
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
      runs: [
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
        presentationProven: true,
      ),
      activeOrdinal: 5,
      active: right,
      preceding: left,
    );

    expect(transition?.surfaces, hasLength(1));
    expect(transition?.surfaces.single.projectionCurrent, isTrue);
    expect(
      transition?.surfaces.single.presentation.runs[1].styles,
      contains(FlarkCorePresentationInlineStyle.strong),
    );
  });

  test(
    'parser-proven terminal split retains the row and authors a successor cell',
    () {
      final active = _row(
        ordinal: 7,
        sourceStart: 0,
        sourceEnd: 17,
        text: 'Before bold.',
        runs: [
          FlarkCorePresentationRun(
            text: 'Before ',
            sourceUtf16Start: 0,
            sourceUtf16End: 7,
            sourceExact: true,
            styles: {},
          ),
          FlarkCorePresentationRun(
            text: 'bold',
            sourceUtf16Start: 9,
            sourceUtf16End: 13,
            sourceExact: true,
            styles: {FlarkCorePresentationInlineStyle.strong},
          ),
          FlarkCorePresentationRun(
            text: '.',
            sourceUtf16Start: 15,
            sourceUtf16End: 16,
            sourceExact: true,
            styles: {},
          ),
        ],
      );
      final transition = frontend.adopt(
        receipt: _receipt(
          transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
          baseStart: 16,
          baseEnd: 16,
          replacement: '\n\n',
          presentationProven: true,
        ),
        activeOrdinal: 7,
        active: active,
      );

      expect(transition?.gap, isNull);
      expect(transition?.surfaces, hasLength(3));
      expect(transition?.surfaces.first.presentation.text, 'Before bold.');
      expect(transition?.surfaces.first.projectionCurrent, isTrue);
      expect(transition?.surfaces[1].presentation.text, isEmpty);
      expect(transition?.surfaces[1].presentation.kind, 0);
      expect(transition?.surfaces[1].sourceUtf16.start, 17);
      expect(transition?.surfaces[1].sourceUtf16.end, 18);
      expect(transition?.surfaces.last.presentation.kind, 5);
      expect(transition?.surfaces.last.projectionEditCells, hasLength(1));
      expect(
        transition?.surfaces.last.projectionEditCells.single.matcher,
        FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
      );
      final cell = transition!.surfaces.last.projectionEditCells.single;
      expect(cell.affectedBytes.start, 18);
      expect(cell.affectedBytes.end, 18);
      expect(cell.affectedUtf16.start, 18);
      expect(cell.affectedUtf16.end, 18);
    },
  );

  test('unproven inline split retains an exact two-row partition', () {
    final active = _row(
      ordinal: 7,
      sourceStart: 0,
      sourceEnd: 17,
      text: 'Before bold.',
      runs: [
        FlarkCorePresentationRun(
          text: 'Before ',
          sourceUtf16Start: 0,
          sourceUtf16End: 7,
          sourceExact: true,
          styles: {},
        ),
        FlarkCorePresentationRun(
          text: 'bold',
          sourceUtf16Start: 9,
          sourceUtf16End: 13,
          sourceExact: true,
          styles: {FlarkCorePresentationInlineStyle.strong},
        ),
        FlarkCorePresentationRun(
          text: '.',
          sourceUtf16Start: 15,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 13,
        baseEnd: 13,
        replacement: '\n\n',
      ),
      activeOrdinal: 7,
      active: active,
    );

    expect(transition?.gap, isNull);
    expect(transition?.surfaces, hasLength(2));
    expect(transition?.surfaces.first.sourceUtf16.start, 0);
    expect(transition?.surfaces.first.sourceUtf16.end, 14);
    expect(transition?.surfaces.last.sourceUtf16.start, 15);
    expect(transition?.surfaces.last.sourceUtf16.end, 19);
    expect(
      transition?.surfaces.every((surface) => !surface.projectionCurrent),
      isTrue,
    );
  });

  test('parser-proven embedded split publishes one durable blank row', () {
    final active = _row(
      ordinal: 7,
      sourceStart: 0,
      sourceEnd: 16,
      text: '| a\n | b |\nmore\n',
      runs: [
        FlarkCorePresentationRun(
          text: '| a\n | b |\nmore\n',
          sourceUtf16Start: 0,
          sourceUtf16End: 16,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 4,
        baseEnd: 4,
        replacement: '\n',
        presentationProven: true,
      ),
      activeOrdinal: 7,
      active: active,
    );

    expect(transition?.gap, isNull);
    expect(transition?.surfaces, hasLength(3));
    expect(transition?.surfaces.first.presentation.text, '| a');
    final neutral = transition!.surfaces[1];
    expect(
      neutral.role,
      FlarkCoreCommittedPresentationSurfaceRole.visibleBlankSeparator,
    );
    expect(neutral.sourceUtf16.start, 4);
    expect(neutral.sourceUtf16.end, 5);
    expect(neutral.presentation.text, isEmpty);
    expect(neutral.projectionCurrent, isTrue);
    final successor = transition.surfaces.last;
    expect(
      successor.role,
      FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
    );
    expect(successor.sourceUtf16.start, 5);
    expect(successor.presentation.globalUtf16Start, 6);
    expect(successor.presentation.text, '| b |\nmore');
    expect(successor.projectionEditCells, hasLength(1));
    final cell = successor.projectionEditCells.single;
    expect(cell.matcher, FlarkProjectionEditMatcher.anyNoCrLfSplice);
    expect(cell.affectedUtf16.start, 5);
    expect(cell.affectedUtf16.end, 6);
    expect(cell.triggerUtf16.start, 6);
    expect(cell.triggerUtf16.end, 6);
    expect(cell.chainResultCell, isFalse);
  });

  test('parser-proven fenced split retains one rendered code surface', () {
    final active = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(0, 29),
      leadingText: '',
      text: 'final value = 1;\n',
      globalUtf16Start: 8,
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
      ordinal: 7,
      runs: [
        FlarkCorePresentationRun(
          text: 'final value = 1;\n',
          sourceUtf16Start: 8,
          sourceUtf16End: 25,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 24,
        baseEnd: 24,
        replacement: '\n',
        presentationProven: true,
      ),
      activeOrdinal: 7,
      active: active,
    );

    expect(transition?.surfaces, hasLength(1));
    final surface = transition!.surfaces.single;
    expect(surface.projectionCurrent, isTrue);
    expect(surface.sourceUtf16.start, 0);
    expect(surface.sourceUtf16.end, 30);
    expect(surface.presentation.text, 'final value = 1;\n\n');
    expect(surface.presentation.codeBlock?.closed, isTrue);
    expect(surface.projectionEditCells, hasLength(1));
    final cell = surface.projectionEditCells.single;
    expect(
      cell.matcher,
      FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd,
    );
    expect(cell.affectedUtf16.start, 25);
    expect(cell.affectedUtf16.end, 25);
    expect(cell.chainResultCell, isTrue);
  });

  test('parser-proven fenced join removes only the visible line ending', () {
    final active = FlarkCorePresentationRow(
      sourceUtf16: FlarkSourceRange(0, 30),
      leadingText: '',
      text: 'final value = 1;\n\n',
      globalUtf16Start: 8,
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
      ordinal: 7,
      runs: [
        FlarkCorePresentationRun(
          text: 'final value = 1;\n\n',
          sourceUtf16Start: 8,
          sourceUtf16End: 26,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.joinFencedCode,
        baseStart: 24,
        baseEnd: 25,
        replacement: '',
        presentationProven: true,
      ),
      activeOrdinal: 7,
      active: active,
    );

    final surface = transition?.surfaces.single;
    expect(surface?.projectionCurrent, isTrue);
    expect(surface?.sourceUtf16.start, 0);
    expect(surface?.sourceUtf16.end, 29);
    expect(surface?.presentation.text, 'final value = 1;\n');
    expect(surface?.presentation.codeBlock?.closed, isTrue);
    expect(surface?.projectionEditCells, hasLength(1));
    expect(
      surface?.projectionEditCells.single.matcher,
      FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd,
    );
    expect(surface?.projectionEditCells.single.affectedUtf16.start, 24);
  });

  test(
    'parser-proven heading split retains rendered heading and plain successor',
    () {
      final active = _row(
        ordinal: 4,
        sourceStart: 0,
        sourceEnd: 7,
        globalStart: 3,
        text: 'Head',
        kind: 12,
        headingLevel: 2,
        runs: [
          FlarkCorePresentationRun(
            text: 'Head',
            sourceUtf16Start: 3,
            sourceUtf16End: 7,
            sourceExact: true,
            styles: {},
          ),
        ],
      );
      final transition = frontend.adopt(
        receipt: _receipt(
          transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
          baseStart: 7,
          baseEnd: 7,
          replacement: '\n\n',
          presentationProven: true,
        ),
        activeOrdinal: 4,
        active: active,
      );

      expect(transition?.gap, isNull);
      expect(transition?.surfaces, hasLength(3));
      expect(transition?.surfaces.first.presentation.text, 'Head');
      expect(transition?.surfaces.first.presentation.headingLevel, 2);
      expect(transition?.surfaces.first.projectionCurrent, isTrue);
      expect(
        transition?.surfaces[1].role,
        FlarkCoreCommittedPresentationSurfaceRole.visibleBlankSeparator,
      );
      expect(transition?.surfaces.last.presentation.kind, 5);
      expect(transition?.surfaces.last.presentation.text, isEmpty);
    },
  );

  test(
    'parser-proven terminal list continuation publishes one empty successor',
    () {
      final active = _row(
        ordinal: 4,
        sourceStart: 0,
        sourceEnd: 12,
        globalStart: 2,
        leadingText: '- ',
        text: 'list item',
        listItem: true,
        runs: [
          FlarkCorePresentationRun(
            text: 'list item',
            sourceUtf16Start: 2,
            sourceUtf16End: 11,
            sourceExact: true,
            styles: {},
          ),
        ],
      );
      final transition = frontend.adopt(
        receipt: _receipt(
          transition: FlarkCoreEditPresentationTransitionV1.continueList,
          baseStart: 11,
          baseEnd: 11,
          replacement: '\n- ',
          presentationProven: true,
        ),
        activeOrdinal: 4,
        active: active,
      );

      expect(transition?.surfaces, hasLength(2));
      expect(transition?.surfaces.first.presentation.leadingText, '- ');
      expect(transition?.surfaces.first.presentation.text, 'list item');
      expect(transition?.surfaces.last.sourceUtf16.start, 12);
      expect(transition?.surfaces.last.sourceUtf16.end, 14);
      expect(transition?.surfaces.last.presentation.leadingText, '- ');
      expect(transition?.surfaces.last.presentation.text, isEmpty);
      expect(transition?.surfaces.last.presentation.listItem, isTrue);
      expect(
        transition?.surfaces.last.projectionEditCells.single.affectedUtf16,
        isA<FlarkSourceRange>(),
      );
      expect(
        transition!
            .surfaces
            .last
            .projectionEditCells
            .single
            .affectedUtf16
            .start,
        14,
      );
    },
  );

  test('parser-proven empty list exit authors a plain insertion cell', () {
    final active = _row(
      ordinal: 4,
      sourceStart: 6,
      sourceEnd: 9,
      globalStart: 8,
      leadingText: '- ',
      text: '',
      listItem: true,
      runs: const [],
    );
    final transition = frontend.adopt(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.exitList,
        baseStart: 6,
        baseEnd: 8,
        replacement: '\n',
        presentationProven: true,
      ),
      activeOrdinal: 4,
      active: active,
    );

    expect(transition?.clearPriorGap, isTrue);
    expect(transition?.surfaces, hasLength(2));
    expect(
      transition?.surfaces.first.role,
      FlarkCoreCommittedPresentationSurfaceRole.blockSeparator,
    );
    expect(transition?.surfaces.first.sourceUtf16.start, 6);
    expect(transition?.surfaces.first.sourceUtf16.end, 7);
    final surface = transition!.surfaces.last;
    expect(surface.sourceUtf16.start, 7);
    expect(surface.sourceUtf16.end, 8);
    expect(surface.projectionCurrent, isTrue);
    expect(
      surface.role,
      FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
    );
    expect(surface.presentation.leadingText, isEmpty);
    expect(surface.presentation.text, isEmpty);
    expect(surface.presentation.listItem, isFalse);
    expect(surface.projectionEditCells, hasLength(1));
    expect(surface.projectionEditCells.single.affectedUtf16.start, 7);
  });

  test(
    'parser-proven terminal quote continuation authors a rendered successor',
    () {
      final active = _row(
        ordinal: 4,
        sourceStart: 0,
        sourceEnd: 14,
        globalStart: 2,
        leadingText: '│ ',
        text: 'quoted text',
        blockQuoteDepth: 1,
        runs: [
          FlarkCorePresentationRun(
            text: 'quoted text',
            sourceUtf16Start: 2,
            sourceUtf16End: 13,
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
          presentationProven: true,
        ),
        activeOrdinal: 4,
        active: active,
      );

      expect(transition?.surfaces, hasLength(2));
      expect(transition?.surfaces.first.presentation.text, 'quoted text');
      final successor = transition!.surfaces.last;
      expect(successor.sourceUtf16.start, 14);
      expect(successor.sourceUtf16.end, 16);
      expect(successor.presentation.leadingText, '│ ');
      expect(successor.presentation.text, isEmpty);
      expect(successor.presentation.blockQuoteDepth, 1);
      expect(successor.projectionEditCells.single.affectedUtf16.start, 16);
    },
  );

  test('a carried successor cannot authorize another structural split', () {
    final successor = _row(
      ordinal: 7,
      sourceStart: 18,
      sourceEnd: 19,
      text: 'x',
      runs: [
        FlarkCorePresentationRun(
          text: 'x',
          sourceUtf16Start: 18,
          sourceUtf16End: 19,
          sourceExact: true,
          styles: {},
        ),
      ],
    );
    final transition = resolveCommittedPresentationTransitionV1(
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.splitParagraph,
        baseStart: 19,
        baseEnd: 19,
        replacement: '\n\n',
      ),
      priorActiveOrdinal: 7,
      activeRow: successor,
      precedingRow: null,
      priorGapPending: false,
      activeRowTransitional: true,
    );

    expect(transition?.surfaces, isEmpty);
    expect(transition?.gap, isNotNull);
  });

  test('paragraph merge fails closed around hidden unstyled inline syntax', () {
    final image = _row(
      ordinal: 4,
      sourceStart: 0,
      sourceEnd: 12,
      text: 'foo',
      runs: [
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
      runs: [
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
      runs: [
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
      runs: [
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
      runs: [
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
      runs: [
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
        presentationProven: true,
      ),
      activeOrdinal: 3,
      active: nested,
    );

    final presentation = transition?.surface?.presentation;
    expect(presentation?.leadingText, '- ');
    expect(presentation?.sourceUtf16.start, 0);
    expect(presentation?.globalUtf16Start, 2);
    expect(presentation?.runs.single.sourceUtf16Start, 2);
    expect(transition?.surface?.projectionCurrent, isTrue);
  });

  test('list indent adds the receipt-certified visual indentation level', () {
    final item = _row(
      ordinal: 3,
      sourceStart: 0,
      sourceEnd: 8,
      globalStart: 2,
      text: 'child',
      leadingText: '- ',
      runs: [
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
        presentationProven: true,
      ),
      activeOrdinal: 3,
      active: item,
    );

    final presentation = transition?.surface?.presentation;
    expect(presentation?.leadingText, '  - ');
    expect(presentation?.sourceUtf16.start, 0);
    expect(presentation?.globalUtf16Start, 4);
    expect(presentation?.runs.single.sourceUtf16Start, 4);
    expect(transition?.surface?.projectionCurrent, isTrue);
  });

  test('deeper list outdent preserves preceding visual indentation', () {
    final nested = _row(
      ordinal: 4,
      sourceStart: 6,
      sourceEnd: 12,
      text: 'leaf',
      leadingText: '    - ',
      runs: [
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
    expect(transition?.surface?.projectionCurrent, isFalse);
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
      runs: [
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
      runs: [
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
      runs: [
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
      runs: [
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
      runs: [
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
  int? headingLevel,
  int? globalStart,
  int? blockQuoteDepth,
  FlarkCodeBlockPresentation? codeBlock,
  bool thematicBreak = false,
  bool listItem = false,
}) => FlarkCorePresentationRow(
  sourceUtf16: FlarkSourceRange(sourceStart, sourceEnd),
  leadingText: leadingText,
  text: text,
  globalUtf16Start: globalStart ?? sourceStart,
  kind: kind,
  headingLevel: headingLevel,
  blockQuoteDepth: blockQuoteDepth,
  codeBlock: codeBlock,
  thematicBreak: thematicBreak,
  listItem: listItem,
  ordinal: ordinal,
  runs: runs,
);

FlarkCoreEditIntentReceiptV1 _receipt({
  required FlarkCoreEditPresentationTransitionV1 transition,
  required int baseStart,
  required int baseEnd,
  required String replacement,
  bool presentationProven = false,
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
    presentationProven: presentationProven,
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
