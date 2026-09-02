import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  group('editor viewport pager', () {
    test('owns query, source read, and ordered page navigation', () async {
      final first = _viewport(start: 0, end: 4, continuation: 1);
      final second = _viewport(start: 4, end: 8);
      final source = _FakeViewportSource(
        text: 'aaa\nbbb\nccc',
        pages: {0: first, 4: second},
        successors: {first: second},
      );
      final coordinator = FlarkEditorCoordinator();
      final pager = FlarkEditorViewportPager(
        source: source,
        coordinator: coordinator,
        maximumVisibleBytes: 4,
        rowsPerPage: 1,
      );

      final initial = await pager.refresh(
        const FlarkViewportRefreshRequest(
          previousViewport: null,
          visibleUtf16Start: 0,
          visibleSource: '',
          optimisticEditsStartAtOrAfterPreviousStart: true,
          caretUtf16: 0,
          ensureCaretVisible: false,
        ),
      );
      expect(initial, isNotNull);
      expect(initial!.source, 'aaa\n');
      expect(pager.adopt(initial), isTrue);
      expect(pager.pageIndex, 0);

      final next = await pager.nextPage(initial.viewport);
      expect(next, isNotNull);
      expect(next!.source, 'bbb\n');
      expect(pager.adopt(next), isTrue);
      expect(pager.pageIndex, 1);
      expect(pager.canPageBackward, isTrue);

      final previous = await pager.previousPage(next.viewport);
      expect(previous, isNotNull);
      expect(pager.adopt(previous!), isTrue);
      expect(
        (
          previous.viewport.coveredBytes.start,
          previous.viewport.coveredBytes.end,
        ),
        (0, 4),
      );
      expect(pager.pageIndex, 0);
      expect(pager.canPageBackward, isFalse);
      expect(source.queryStarts, [0, 0]);
      expect(source.successorQueries, [first]);
    });

    test(
      'stale page result is released and cannot advance navigation',
      () async {
        final first = _viewport(start: 0, end: 4, continuation: 1);
        final stale = _viewport(start: 4, end: 8, continuation: 2);
        final coordinator = FlarkEditorCoordinator();
        FlarkEditorCommandTicket? newerCommand;
        final source = _FakeViewportSource(
          text: 'aaa\nbbb\n',
          pages: {0: first},
          successors: {first: stale},
          onQueryNext: () {
            newerCommand = coordinator.admitCommand(
              FlarkEditorCommandKind.sourceEdit,
              publishSourceImmediately: true,
            );
          },
        );
        final pager = FlarkEditorViewportPager(
          source: source,
          coordinator: coordinator,
          maximumVisibleBytes: 4,
          rowsPerPage: 1,
        );

        final result = await pager.nextPage(first);

        expect(result, isNull);
        expect(pager.pageIndex, 0);
        expect(source.released, [stale]);
        coordinator.completeCommand(newerCommand!);
      },
    );

    test('receipt adoption closes the async generation gap', () async {
      final first = _viewport(start: 0, end: 4, continuation: 1);
      final queried = _viewport(start: 4, end: 8);
      final coordinator = FlarkEditorCoordinator();
      final source = _FakeViewportSource(
        text: 'aaa\nbbb\n',
        pages: {0: first},
        successors: {first: queried},
      );
      final pager = FlarkEditorViewportPager(
        source: source,
        coordinator: coordinator,
        maximumVisibleBytes: 4,
        rowsPerPage: 1,
      );

      final receipt = await pager.nextPage(first);
      final newerCommand = coordinator.admitCommand(
        FlarkEditorCommandKind.sourceEdit,
        publishSourceImmediately: true,
      );

      expect(pager.adopt(receipt!), isFalse);
      expect(pager.discard(receipt), isNull);
      expect(pager.pageIndex, 0);
      expect(source.released, isEmpty);
      coordinator.completeCommand(newerCommand);
    });

    test('receipts are owned and settle exactly once', () async {
      final first = _viewport(start: 0, end: 4, continuation: 1);
      final queried = _viewport(start: 4, end: 8);
      final coordinator = FlarkEditorCoordinator();
      final source = _FakeViewportSource(
        text: 'aaa\nbbb\n',
        pages: {0: first},
        successors: {first: queried},
      );
      final owner = FlarkEditorViewportPager(
        source: source,
        coordinator: coordinator,
        maximumVisibleBytes: 4,
        rowsPerPage: 1,
      );
      final stranger = FlarkEditorViewportPager(
        source: source,
        coordinator: coordinator,
        maximumVisibleBytes: 4,
        rowsPerPage: 1,
      );

      final receipt = (await owner.nextPage(first))!;

      expect(() => stranger.adopt(receipt), throwsStateError);
      expect(owner.adopt(receipt), isTrue);
      expect(() => owner.adopt(receipt), throwsStateError);
      expect(() => owner.discard(receipt), throwsStateError);
    });
  });

  group('editor viewport adopter', () {
    test(
      'atomically publishes current viewport and retires certified state',
      () async {
        final viewport = _viewport(start: 0, end: 4);
        final source = _FakeViewportSource(
          text: 'aaa\n',
          pages: {0: viewport},
          successors: const {},
        );
        final coordinator = FlarkEditorCoordinator()
          ..setPendingTaskCheck(0, true);
        final pager = FlarkEditorViewportPager(
          source: source,
          coordinator: coordinator,
          maximumVisibleBytes: 4,
          rowsPerPage: 1,
        );
        final state = FlarkEditorViewportState();
        final adopter = FlarkEditorViewportAdopter(
          coordinator: coordinator,
          pager: pager,
          state: state,
        );
        final result = await pager.refresh(
          const FlarkViewportRefreshRequest(
            previousViewport: null,
            visibleUtf16Start: 0,
            visibleSource: '',
            optimisticEditsStartAtOrAfterPreviousStart: true,
            caretUtf16: 0,
            ensureCaretVisible: false,
            expectedEditGeneration: 0,
          ),
        );

        final adoption = adopter.adopt(result!, caretUtf16: 0);

        expect(adoption, isNotNull);
        expect(adoption!.installation.installsFreshRows, isTrue);
        expect(adoption.hasFirstCertifiedEvidence, isTrue);
        expect(state.viewport, same(viewport));
        expect(state.visibleSource, 'aaa\n');
        expect(state.semanticCurrent, isTrue);
        expect(coordinator.interactionGeneration, 1);
        expect(coordinator.publishedDocumentRevision, viewport.revision);
        expect(coordinator.pendingPresentation.taskChecks, isEmpty);
        expect(pager.pageIndex, 0);
      },
    );

    test('rejects a stale receipt before any portable state mutates', () async {
      final viewport = _viewport(start: 0, end: 4);
      final source = _FakeViewportSource(
        text: 'aaa\n',
        pages: {0: viewport},
        successors: const {},
      );
      final coordinator = FlarkEditorCoordinator();
      final pager = FlarkEditorViewportPager(
        source: source,
        coordinator: coordinator,
        maximumVisibleBytes: 4,
        rowsPerPage: 1,
      );
      final state = FlarkEditorViewportState();
      final adopter = FlarkEditorViewportAdopter(
        coordinator: coordinator,
        pager: pager,
        state: state,
      );
      final result = (await pager.refresh(
        const FlarkViewportRefreshRequest(
          previousViewport: null,
          visibleUtf16Start: 0,
          visibleSource: '',
          optimisticEditsStartAtOrAfterPreviousStart: true,
          caretUtf16: 0,
          ensureCaretVisible: false,
          expectedEditGeneration: 0,
        ),
      ))!;
      final newer = coordinator.admitCommand(
        FlarkEditorCommandKind.sourceEdit,
        publishSourceImmediately: true,
      );
      final interactionGeneration = coordinator.interactionGeneration;

      expect(adopter.adopt(result, caretUtf16: 0), isNull);
      expect(adopter.discard(result), isNull);
      expect(state.viewport, isNull);
      expect(state.visibleSource, isEmpty);
      expect(coordinator.interactionGeneration, interactionGeneration);
      expect(pager.pageIndex, 0);
      coordinator.completeCommand(newer);
    });
  });
}

FlarkViewport _viewport({
  required int start,
  required int end,
  int continuation = 0,
}) {
  final range = FlarkSourceRange(start, end);
  return FlarkViewport(
    revision: 1,
    snapshot: start + 1,
    requestedBytes: range,
    coveredBytes: range,
    coveredUtf16: range,
    certification: FlarkCertification.currentCertified,
    rows: [
      FlarkViewportRow(
        ordinal: start,
        kind: 5,
        sourceBytes: range,
        sourceUtf16: range,
        editableBytes: range,
        editableUtf16: range,
        editCapability: FlarkViewportRowEditCapability.contiguous,
        headingLevel: null,
        headingStyle: null,
        listItem: null,
        blockQuote: null,
        codeBlock: null,
        thematicBreak: false,
        pathDepth: 0,
        inlineFacts: const [],
      ),
    ],
    neutralSource: null,
    continuation: continuation,
  );
}

final class _FakeViewportSource implements FlarkViewportSource {
  _FakeViewportSource({
    required this.text,
    required this.pages,
    required this.successors,
    this.onQueryNext,
  });

  final String text;
  final Map<int, FlarkViewport> pages;
  final Map<FlarkViewport, FlarkViewport> successors;
  final void Function()? onQueryNext;
  final List<int> queryStarts = [];
  final List<FlarkViewport> successorQueries = [];
  final List<FlarkViewport> released = [];

  @override
  int get sourceByteLength => text.length;

  @override
  Future<FlarkViewport> queryViewport({
    int startByte = 0,
    int? endByte,
    int maxRows = 256,
  }) async {
    queryStarts.add(startByte);
    return pages[startByte]!;
  }

  @override
  Future<FlarkViewport> queryViewportNext(
    FlarkViewport previous, {
    int maxRows = 256,
  }) async {
    successorQueries.add(previous);
    onQueryNext?.call();
    return successors[previous]!;
  }

  @override
  Future<void> releaseViewportContinuation(FlarkViewport viewport) async {
    released.add(viewport);
  }

  @override
  Future<String> readSourceRange(int startByte, int endByte) async =>
      text.substring(startByte, endByte);
}
