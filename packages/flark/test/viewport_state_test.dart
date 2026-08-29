import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  group('editor viewport state', () {
    FlarkViewportRow row() => FlarkViewportRow(
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
    );

    FlarkViewport viewport({
      required FlarkCertification certification,
      List<FlarkViewportRow> rows = const [],
    }) => FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: const FlarkSourceRange(0, 3),
      coveredBytes: const FlarkSourceRange(0, 3),
      coveredUtf16: const FlarkSourceRange(0, 3),
      certification: certification,
      rows: rows,
      neutralSource: rows.isEmpty ? 'abc' : null,
      continuation: 0,
    );

    test('installs source rows and certification as one state', () {
      final state = FlarkEditorViewportState();

      final installation = state.install(
        viewport(
          certification: FlarkCertification.currentCertified,
          rows: [row()],
        ),
        'abc',
      );

      expect(installation.installsCertifiedSurface, isTrue);
      expect(state.semanticCurrent, isTrue);
      expect(state.visibleSource, 'abc');
      expect(state.rows, hasLength(1));

      final adoption = state.applyOptimisticEdit(
        globalStart: 1,
        globalEnd: 1,
        replacement: 'x',
        fallbackSource: 'axbc',
        fallbackUtf16Start: 0,
        focusUtf16: 2,
        maximumVisibleCodeUnits: 16,
      );

      expect(
        adoption.disposition,
        FlarkOptimisticViewportEditDisposition.retainedMappedSurface,
      );
      expect(state.semanticCurrent, isFalse);
      expect(state.visibleSource, 'axbc');
      expect(state.rows, hasLength(1));
      expect(state.mapRange(const FlarkSourceRange(0, 3)).end, 4);
    });

    test('matching pending source retains the installed row shell', () {
      final state = FlarkEditorViewportState()
        ..install(
          viewport(
            certification: FlarkCertification.currentCertified,
            rows: [row()],
          ),
          'abc',
        );

      final installation = state.install(
        viewport(certification: FlarkCertification.pendingNeutral),
        'abc',
      );

      expect(installation.retainsExistingSurface, isTrue);
      expect(state.rows, hasLength(1));
      expect(state.visibleSource, 'abc');
      expect(state.semanticCurrent, isFalse);
    });

    test('an out-of-window edit falls back atomically to host input', () {
      final state = FlarkEditorViewportState()
        ..install(
          viewport(
            certification: FlarkCertification.currentCertified,
            rows: [row()],
          ),
          'abc',
        );

      final adoption = state.applyOptimisticEdit(
        globalStart: 9,
        globalEnd: 9,
        replacement: 'x',
        fallbackSource: 'input',
        fallbackUtf16Start: 8,
        focusUtf16: 10,
        maximumVisibleCodeUnits: 16,
      );

      expect(
        adoption.disposition,
        FlarkOptimisticViewportEditDisposition.replacedByInputWindow,
      );
      expect(state.viewport, isNull);
      expect(state.rows, isEmpty);
      expect(state.visibleSource, 'input');
      expect(state.visibleUtf16Start, 8);
    });

    test('bounded replacement never returns an unpaired surrogate', () {
      final window = boundedReplacementWindow(
        source: 'a😀b',
        start: 1,
        end: 1,
        replacement: '',
        focus: 2,
        maximumCodeUnits: 1,
      );

      expect(window.text, isEmpty);
      expect(window.start, 3);
    });
  });

  group('viewport installation plan', () {
    FlarkViewport viewport({
      required FlarkCertification certification,
      required String source,
    }) => FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: FlarkSourceRange(0, source.length),
      coveredBytes: FlarkSourceRange(0, source.length),
      coveredUtf16: FlarkSourceRange(0, source.length),
      certification: certification,
      rows: const [],
      neutralSource: source,
      continuation: 0,
    );

    test('certified empty result is current without installing rows', () {
      final plan = FlarkViewportInstallationPlan.evaluate(
        viewport: viewport(
          certification: FlarkCertification.currentCertified,
          source: '',
        ),
        source: '',
        previousVisibleUtf16Start: 0,
        previousVisibleSource: 'old',
        mappedCachedRowRanges: const [],
      );

      expect(plan.installsFreshRows, isFalse);
      expect(plan.installsCertifiedSurface, isTrue);
      expect(plan.retainsExistingSurface, isFalse);
    });

    test('pending exact source can retain a matching certified shell', () {
      final plan = FlarkViewportInstallationPlan.evaluate(
        viewport: viewport(
          certification: FlarkCertification.pendingNeutral,
          source: 'abc',
        ),
        source: 'abc',
        previousVisibleUtf16Start: 0,
        previousVisibleSource: 'abc',
        mappedCachedRowRanges: [FlarkSourceRange(0, 3)],
      );

      expect(plan.retainsExistingSurface, isTrue);
      expect(plan.installsFreshRows, isFalse);
      expect(plan.installsCertifiedSurface, isFalse);
    });
  });

  group('viewport navigation state', () {
    const pageA = FlarkViewportPageAnchor(byte: 100, utf16: 80);
    const pageB = FlarkViewportPageAnchor(byte: 220, utf16: 170);
    const replacementB = FlarkViewportPageAnchor(byte: 210, utf16: 165);

    FlarkViewport viewportAt(FlarkViewportPageAnchor anchor) => FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: FlarkSourceRange(anchor.byte, anchor.byte + 10),
      coveredBytes: FlarkSourceRange(anchor.byte, anchor.byte + 10),
      coveredUtf16: FlarkSourceRange(anchor.utf16, anchor.utf16 + 10),
      certification: FlarkCertification.currentCertified,
      rows: const [],
      neutralSource: '0123456789',
      continuation: 0,
    );

    test('page path advances, rewinds, and replaces abandoned history', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      expect(navigation.pageIndex, 2);
      expect(navigation.previousAnchor, pageA);
      expect(navigation.currentPageMatches(viewportAt(pageB)), isTrue);
      expect(() => navigation.advanceTo(pageB), throwsStateError);

      navigation.moveBackwardTo(pageA);
      navigation.advanceTo(replacementB);
      expect(navigation.pagePath, const [
        FlarkViewportPageAnchor.zero,
        pageA,
        replacementB,
      ]);
    });

    test('backward adoption normalizes a rewound enclosing-row anchor', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB)
        ..moveBackwardTo(const FlarkViewportPageAnchor(byte: 90, utf16: 70));

      expect(navigation.pagePath, const [
        FlarkViewportPageAnchor.zero,
        FlarkViewportPageAnchor(byte: 90, utf16: 70),
      ]);
      expect(navigation.pageIndex, 1);
    });

    test('refresh path rejects torn or nonmonotone history', () {
      final navigation = FlarkViewportNavigationState();

      expect(
        () => navigation.installRefreshPath(const [pageA]),
        throwsArgumentError,
      );
      expect(
        () => navigation.installRefreshPath(const [
          FlarkViewportPageAnchor.zero,
          pageB,
          replacementB,
        ]),
        throwsArgumentError,
      );
    });

    test('refresh origin retains the earliest affected edit', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      navigation.retainRefreshAnchorForEdit(
        editStart: 160,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      navigation.retainRefreshAnchorForEdit(
        editStart: 240,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      expect(navigation.refreshAnchor, pageA);

      navigation.retainRefreshAnchorForEdit(
        editStart: 60,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      expect(navigation.refreshAnchor, FlarkViewportPageAnchor.zero);
      expect(
        navigation.refreshAnchorForCaret(20),
        FlarkViewportPageAnchor.zero,
      );
      navigation.clearRefreshAnchor();
      expect(navigation.refreshAnchor, isNull);
    });

    test('input rewind and caret byte windows preserve line boundaries', () {
      final navigation = FlarkViewportNavigationState();

      navigation.retainRefreshAnchorForEdit(
        editStart: 7,
        deriveFromInput: true,
        currentViewport: viewportAt(
          const FlarkViewportPageAnchor(byte: 10, utf16: 10),
        ),
        inputGlobalUtf16Start: 0,
        inputText: 'aaaa\nbbbb\ncccc',
      );
      expect(
        (navigation.refreshAnchor!.byte, navigation.refreshAnchor!.utf16),
        (5, 5),
      );

      final window = navigation.byteWindowForCaret(
        origin: FlarkViewportPageAnchor.zero,
        visibleUtf16Start: 0,
        visibleSource: 'abc\ndef',
        caret: 4,
        sourceByteLength: 7,
        maximumVisibleBytes: 16,
      );
      expect(window, isNotNull);
      expect(
        (window!.startByte, window.startUtf16, window.caretByte),
        (0, 0, 4),
      );
    });

    test('known edit anchor selects the nearest certified origin', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      expect(navigation.knownAnchorFor(160), pageA);
      final selected = navigation.knownAnchorFor(
        205,
        currentViewport: viewportAt(
          const FlarkViewportPageAnchor(byte: 240, utf16: 190),
        ),
      );
      expect((selected.byte, selected.utf16), (240, 190));
    });
  });

  group('optimistic range map', () {
    test('maps following ranges without preserving a touched range', () {
      final ranges = FlarkOptimisticRangeMap()
        ..add(
          const FlarkOptimisticViewportEdit(
            start: 2,
            end: 2,
            replacementLength: 3,
          ),
        );

      final mapped = ranges.mapRange(FlarkSourceRange(5, 8));
      expect((mapped.start, mapped.end), (8, 11));
      expect(ranges.leavesRangeUnchanged(FlarkSourceRange(5, 8)), isTrue);
      expect(ranges.leavesRangeUnchanged(FlarkSourceRange(0, 4)), isFalse);
    });

    test('container retention fails closed for structural receipts', () {
      final ranges = FlarkOptimisticRangeMap()
        ..add(
          const FlarkOptimisticViewportEdit(
            start: 3,
            end: 4,
            replacementLength: 0,
            preservesMappedRowFacts: false,
          ),
        );

      expect(ranges.staysWithin(FlarkSourceRange(0, 8)), isFalse);
    });
  });
}
