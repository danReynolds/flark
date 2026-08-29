import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
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
