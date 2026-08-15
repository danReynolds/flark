import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  test(
    'adapter facade coalesces, queries, bounds, and invalidates one exact page',
    () async {
      final markdown = _viewportMarkdown();
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        leafProjectionDemandOwner: true,
        viewportPresentationDemandOwner: true,
      );
      final passiveLease = FlarkV3DocumentRuntimeAdapter.borrow(runtime);
      addTearDown(() async {
        passiveLease.release();
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      });
      expect(
        () => FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          viewportPresentationDemandOwner: true,
        ),
        throwsStateError,
      );

      await runtime.initialReady.timeout(const Duration(seconds: 5));
      final initial = runtime.status;
      final demand = FlarkV3ViewportPresentationDemand(
        sourceRevision: initial.sourceRevision,
        structureGeneration: initial.structureGeneration,
        startUtf16: 0,
        endUtf16: runtime.sourceLengthUtf16,
        startBlockOrdinal: 0,
      );
      expect(
        () => passiveLease.ensureViewportPresentation(demand),
        throwsStateError,
      );
      final wrongStructure = FlarkV3ViewportPresentationDemand(
        sourceRevision: demand.sourceRevision,
        structureGeneration: demand.structureGeneration + 1,
        startUtf16: demand.startUtf16,
        endUtf16: demand.endUtf16,
        startBlockOrdinal: demand.startBlockOrdinal,
      );
      expect(
        lease.ensureViewportPresentation(wrongStructure).disposition,
        FlarkV3ViewportPresentationDemandDisposition.stale,
      );
      expect(
        (lease.queryViewportPresentation(wrongStructure)
                as FlarkV3UnavailableViewportPresentationPage)
            .reason,
        FlarkV3ViewportPresentationUnavailableReason.structureChanged,
      );
      final settled = runtime.statuses.firstWhere(
        (status) =>
            status.viewportPresentationAttemptOutcomeGeneration >
            initial.viewportPresentationAttemptOutcomeGeneration,
      );

      final scheduled = lease.ensureViewportPresentation(demand);
      expect(
        scheduled.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );
      expect(scheduled.viewportGeneration, isNotNull);
      expect(
        scheduled.attemptOutcomeGeneration,
        initial.viewportPresentationAttemptOutcomeGeneration,
      );

      final coalesced = lease.ensureViewportPresentation(demand);
      expect(
        coalesced.disposition,
        FlarkV3ViewportPresentationDemandDisposition.coalesced,
      );
      expect(coalesced.viewportGeneration, scheduled.viewportGeneration);

      final completed = await settled.timeout(const Duration(seconds: 10));
      expect(
        completed.viewportPresentationGeneration,
        scheduled.viewportGeneration,
        reason:
            'The status completion edge must identify the installed parser '
            'generation without adapter polling '
            '(unavailable: '
            '${completed.viewportPresentationUnavailableReason}).',
      );
      expect(completed.viewportPresentationUnavailableReason, isNull);

      final current = lease.ensureViewportPresentation(demand);
      expect(
        current.disposition,
        FlarkV3ViewportPresentationDemandDisposition.current,
      );
      expect(current.viewportGeneration, scheduled.viewportGeneration);

      final queried = lease.queryViewportPresentation(demand);
      expect(queried, isA<FlarkV3ExactViewportPresentationPage>());
      final exact = queried as FlarkV3ExactViewportPresentationPage;
      expect(exact.page.ack.baseAck, exact.currentStructuralAck);
      expect(exact.structureGeneration, demand.structureGeneration);
      expect(exact.sourceDocument.revision, demand.sourceRevision);
      expect(exact.page.entryCount, 24);

      final undersized = lease.queryViewportPresentation(
        demand,
        maximumEncodedBytes: exact.page.encodedPage.lengthInBytes - 1,
      );
      expect(undersized, isA<FlarkV3UnavailableViewportPresentationPage>());
      expect(
        (undersized as FlarkV3UnavailableViewportPresentationPage).reason,
        FlarkV3ViewportPresentationUnavailableReason.queryBoundExceeded,
      );

      final preemptedDemand = FlarkV3ViewportPresentationDemand(
        sourceRevision: demand.sourceRevision,
        structureGeneration: demand.structureGeneration,
        startUtf16: 0,
        endUtf16: _paragraph(0).length,
        startBlockOrdinal: 0,
      );
      final passiveAttempt = lease.ensureViewportPresentation(preemptedDemand);
      expect(
        passiveAttempt.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );
      final point = lease.queryAtUtf16(1) as FlarkV3DocumentStructuralQuery;
      expect(
        lease.ensureLeafProjectionAtUtf16(1, structuralQuery: point),
        FlarkV3LeafProjectionDemandDisposition.scheduled,
      );
      final rescheduled = lease.ensureViewportPresentation(preemptedDemand);
      expect(
        rescheduled.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
        reason:
            'Focused refinement must clear passive caller-side coalescing as '
            'well as the executor request.',
      );
      expect(
        rescheduled.viewportGeneration,
        greaterThan(passiveAttempt.viewportGeneration!),
      );

      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: runtime.sourceLengthUtf16,
            endUtf16: runtime.sourceLengthUtf16,
            replacement: '\n\nnew block',
          ),
        ),
      );
      final staleDemand = lease.ensureViewportPresentation(demand);
      expect(
        staleDemand.disposition,
        FlarkV3ViewportPresentationDemandDisposition.stale,
      );
      expect(
        staleDemand.unavailableReason,
        FlarkV3ViewportPresentationUnavailableReason.sourceChanged,
      );
      final stalePage = lease.queryViewportPresentation(demand);
      expect(stalePage, isA<FlarkV3UnavailableViewportPresentationPage>());
      expect(
        (stalePage as FlarkV3UnavailableViewportPresentationPage).reason,
        FlarkV3ViewportPresentationUnavailableReason.sourceChanged,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'adapter facade presents exact partial viewport windows in a large document',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _largeViewportMarkdown(),
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        viewportPresentationDemandOwner: true,
      );
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      });

      await runtime.initialReady.timeout(const Duration(seconds: 20));
      for (final (startBlockOrdinal, structuralEntryCount, inlineLeafCount)
          in <(int, int, int)>[(0, 24, 12), (4084, 24, 12), (4064, 64, 32)]) {
        final current = runtime.status;
        final located = lease.queryBlockOrdinalWindow(
          FlarkV3DocumentOrdinalWindowDemand(
            sourceRevision: current.sourceRevision,
            structureGeneration: current.structureGeneration,
            startBlockOrdinal: startBlockOrdinal,
          ),
          budget: FlarkV3DocumentOrdinalWindowBudget(
            maximumEntries: structuralEntryCount,
          ),
        );
        expect(located, isA<FlarkV3ExactDocumentOrdinalWindow>());
        final exactWindow = located as FlarkV3ExactDocumentOrdinalWindow;
        expect(exactWindow.startBlockOrdinal, startBlockOrdinal);
        expect(
          exactWindow.nextBlockOrdinal,
          startBlockOrdinal + structuralEntryCount,
        );

        final demand = FlarkV3ViewportPresentationDemand(
          sourceRevision: current.sourceRevision,
          structureGeneration: current.structureGeneration,
          startUtf16: exactWindow.coveredSource.startUtf16,
          endUtf16: exactWindow.coveredSource.endUtf16,
          startBlockOrdinal: exactWindow.startBlockOrdinal,
        );
        final completion = runtime.statuses.firstWhere(
          (status) =>
              status.viewportPresentationAttemptOutcomeGeneration >
              current.viewportPresentationAttemptOutcomeGeneration,
        );
        final receipt = lease.ensureViewportPresentation(demand);
        expect(
          receipt.disposition,
          FlarkV3ViewportPresentationDemandDisposition.scheduled,
        );
        final completed = await completion.timeout(const Duration(seconds: 10));
        expect(
          completed.viewportPresentationGeneration,
          receipt.viewportGeneration,
        );
        expect(completed.viewportPresentationUnavailableReason, isNull);

        final queried = lease.queryViewportPresentation(demand);
        expect(queried, isA<FlarkV3ExactViewportPresentationPage>());
        final page = (queried as FlarkV3ExactViewportPresentationPage).page;
        expect(
          page.ack.envelope.visitedStructuralEntries,
          structuralEntryCount,
        );
        expect(page.ack.envelope.orderedLeafCount, inlineLeafCount);
        expect(page.entryCount, inlineLeafCount);
        expect(page.ack.binding.start.blockOrdinal.lowWord, startBlockOrdinal);
        expect(
          page.ack.binding.next.blockOrdinal.lowWord,
          startBlockOrdinal + structuralEntryCount,
        );
      }
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  test(
    'default viewport profile admits 64 moderately dense inline leaves',
    () async {
      final markdown = _moderatelyDenseViewportMarkdown();
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        viewportPresentationDemandOwner: true,
      );
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      });

      await runtime.initialReady.timeout(const Duration(seconds: 20));
      final current = runtime.status;
      final located = lease.queryBlockOrdinalWindow(
        FlarkV3DocumentOrdinalWindowDemand(
          sourceRevision: current.sourceRevision,
          structureGeneration: current.structureGeneration,
          startBlockOrdinal: 0,
        ),
        budget: const FlarkV3DocumentOrdinalWindowBudget(maximumEntries: 64),
      );
      expect(located, isA<FlarkV3ExactDocumentOrdinalWindow>());
      final exactWindow = located as FlarkV3ExactDocumentOrdinalWindow;
      expect(exactWindow.nextBlockOrdinal, 64);

      final demand = FlarkV3ViewportPresentationDemand(
        sourceRevision: current.sourceRevision,
        structureGeneration: current.structureGeneration,
        startUtf16: exactWindow.coveredSource.startUtf16,
        endUtf16: exactWindow.coveredSource.endUtf16,
        startBlockOrdinal: exactWindow.startBlockOrdinal,
      );
      final completion = runtime.statuses.firstWhere(
        (status) =>
            status.viewportPresentationAttemptOutcomeGeneration >
            current.viewportPresentationAttemptOutcomeGeneration,
      );
      final receipt = lease.ensureViewportPresentation(demand);
      expect(
        receipt.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );
      final completed = await completion.timeout(const Duration(seconds: 10));
      expect(
        completed.viewportPresentationGeneration,
        receipt.viewportGeneration,
      );
      expect(completed.viewportPresentationUnavailableReason, isNull);

      final queried = lease.queryViewportPresentation(demand);
      expect(queried, isA<FlarkV3ExactViewportPresentationPage>());
      final page = (queried as FlarkV3ExactViewportPresentationPage).page;
      expect(page.ack.envelope.visitedStructuralEntries, 64);
      expect(page.ack.envelope.orderedLeafCount, 64);
      // 192 facts crossed the retired logical-page * maximum-record
      // approximation even though the actual bounded VPB1 transport fits.
      expect(page.ack.envelope.factCount, 64 * 3);
      expect(page.ack.envelope.parserTransitions, lessThanOrEqualTo(250000));
      expect(page.entryCount, 64);
      expect(page.encodedPage.lengthInBytes, lessThan(64 * 1024));
      expect(page.ack.actualEncodedFrameBytes, lessThanOrEqualTo(512 * 1024));
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  test(
    'viewport unavailability applies only to its exact tracked demand',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _sixtyFiveHeadingViewportMarkdown(),
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        viewportPresentationDemandOwner: true,
      );
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(const Duration(seconds: 5));
      });

      await runtime.initialReady.timeout(const Duration(seconds: 20));
      final current = runtime.status;
      FlarkV3ExactDocumentOrdinalWindow locate(int maximumEntries) {
        final result = lease.queryBlockOrdinalWindow(
          FlarkV3DocumentOrdinalWindowDemand(
            sourceRevision: current.sourceRevision,
            structureGeneration: current.structureGeneration,
            startBlockOrdinal: 0,
          ),
          budget: FlarkV3DocumentOrdinalWindowBudget(
            maximumEntries: maximumEntries,
          ),
        );
        expect(result, isA<FlarkV3ExactDocumentOrdinalWindow>());
        return result as FlarkV3ExactDocumentOrdinalWindow;
      }

      FlarkV3ViewportPresentationDemand demandFor(
        FlarkV3ExactDocumentOrdinalWindow window,
      ) => FlarkV3ViewportPresentationDemand(
        sourceRevision: current.sourceRevision,
        structureGeneration: current.structureGeneration,
        startUtf16: window.coveredSource.startUtf16,
        endUtf16: window.coveredSource.endUtf16,
        startBlockOrdinal: window.startBlockOrdinal,
      );

      final oversized = locate(65);
      final admitted = locate(64);
      expect(oversized.nextBlockOrdinal, 65);
      expect(admitted.nextBlockOrdinal, 64);
      final oversizedDemand = demandFor(oversized);
      final admittedDemand = demandFor(admitted);
      final failed = runtime.statuses.firstWhere(
        (status) =>
            status.viewportPresentationAttemptOutcomeGeneration >
            current.viewportPresentationAttemptOutcomeGeneration,
      );
      final oversizedReceipt = lease.ensureViewportPresentation(
        oversizedDemand,
      );
      expect(
        oversizedReceipt.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );

      final failedStatus = await failed.timeout(const Duration(seconds: 10));
      expect(
        failedStatus.viewportPresentationUnavailableReason,
        FlarkV3ViewportPresentationUnavailableReason.budgetExceeded,
      );
      final unavailable = lease.ensureViewportPresentation(oversizedDemand);
      expect(
        unavailable.disposition,
        FlarkV3ViewportPresentationDemandDisposition.unavailable,
      );
      expect(
        unavailable.unavailableReason,
        FlarkV3ViewportPresentationUnavailableReason.budgetExceeded,
      );

      final unrelatedQuery = lease.queryViewportPresentation(admittedDemand);
      expect(unrelatedQuery, isA<FlarkV3UnavailableViewportPresentationPage>());
      expect(
        (unrelatedQuery as FlarkV3UnavailableViewportPresentationPage).reason,
        FlarkV3ViewportPresentationUnavailableReason.notInstalled,
        reason: 'A failed generation must not poison a different exact window.',
      );

      final completed = runtime.statuses.firstWhere(
        (status) =>
            status.viewportPresentationAttemptOutcomeGeneration >
            failedStatus.viewportPresentationAttemptOutcomeGeneration,
      );
      final admittedReceipt = lease.ensureViewportPresentation(admittedDemand);
      expect(
        admittedReceipt.disposition,
        FlarkV3ViewportPresentationDemandDisposition.scheduled,
      );
      expect(runtime.status.viewportPresentationUnavailableReason, isNull);
      final completedStatus = await completed.timeout(
        const Duration(seconds: 10),
      );
      expect(
        completedStatus.viewportPresentationGeneration,
        admittedReceipt.viewportGeneration,
      );
      expect(completedStatus.viewportPresentationUnavailableReason, isNull);
      expect(
        lease.queryViewportPresentation(admittedDemand),
        isA<FlarkV3ExactViewportPresentationPage>(),
      );
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );
}

String _viewportMarkdown() {
  final output = StringBuffer();
  for (var ordinal = 0; ordinal < 24; ordinal += 1) {
    if (ordinal != 0) output.write('\n\n');
    output.write(_paragraph(ordinal));
  }
  return output.toString();
}

String _paragraph(int ordinal) {
  final suffix = ordinal.toString().padLeft(2, '0');
  return '**bold$suffix** *em$suffix* `code$suffix`';
}

String _largeViewportMarkdown() => List<String>.generate(
  4096,
  (index) => index == 2048
      ? '**β😀** and _em_.'
      : 'Paragraph ${index.toString().padLeft(4, '0')} is canonical.',
).join('\n\n');

String _moderatelyDenseViewportMarkdown() =>
    List<String>.filled(64, '# **strong** *emphasis* `code`').join('\n');

String _sixtyFiveHeadingViewportMarkdown() =>
    List<String>.generate(65, (index) => '# heading $index').join('\n');
