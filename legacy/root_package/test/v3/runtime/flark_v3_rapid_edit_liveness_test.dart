// Cross-platform regression for continuously superseded live-edit staging.
import 'dart:async';

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3InlineDemandDisposition,
        FlarkV3InlineFactKind;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

const _prefix = '[flark]: /target\n';
const _mib = 1024 * 1024;
const _burstTail = '**seed**\n';
const _burstReplacements = 'abcdefghijkl';
const _tail =
    'Edit this bounded input island and watch the engine receipts. '
    '**Bold**, _emphasis_, and `code` render live without visible Markdown '
    'delimiters while canonical source remains intact.';

void main() {
  for (final editDelay in <Duration>[
    Duration.zero,
    Duration(milliseconds: 4),
    Duration(milliseconds: 16),
  ]) {
    test(
      'thirteen projected-region edits converge at ${editDelay.inMilliseconds}ms cadence',
      () async {
        final runtime = await openFlarkV3PublicRuntimeForTest('$_prefix$_tail');
        FlarkV3DocumentRuntimeAdapterLease? lease;
        try {
          await runtime.initialReady.timeout(const Duration(seconds: 10));
          lease = FlarkV3DocumentRuntimeAdapter.borrow(
            runtime,
            inlineDemandOwner: true,
          );
          final boldStart = _prefix.length + _tail.indexOf('Bold');
          await _ensureStrongInline(
            runtime: runtime,
            lease: lease,
            positionUtf16: boldStart + 2,
          );

          var currentBold = 'Bold';
          for (var index = currentBold.length - 1; index >= 0; index -= 1) {
            _replaceBold(runtime, boldStart, currentBold, index, index + 1, '');
            currentBold = currentBold.substring(0, index);
            await Future<void>.delayed(editDelay);
          }
          _replaceBold(runtime, boldStart, currentBold, 0, 0, 'x');
          currentBold = 'x';
          await Future<void>.delayed(editDelay);
          _replaceBold(runtime, boldStart, currentBold, 0, 1, '');
          currentBold = '';
          await Future<void>.delayed(editDelay);
          for (final character in 'instant'.split('')) {
            _replaceBold(
              runtime,
              boldStart,
              currentBold,
              currentBold.length,
              currentBold.length,
              character,
            );
            currentBold += character;
            await Future<void>.delayed(editDelay);
          }

          expect(runtime.sourceRevision, 14);
          expect(
            runtime.exportMarkdown(),
            '$_prefix${_tail.replaceFirst('Bold', 'instant')}',
          );

          final current = runtime.status.structureCurrent
              ? runtime.status
              : await runtime.statuses
                    .firstWhere(
                      (status) =>
                          status.sourceRevision == runtime.sourceRevision &&
                          status.sourceCurrent &&
                          status.structureCurrent,
                    )
                    .timeout(
                      const Duration(seconds: 10),
                      onTimeout: () => throw StateError(
                        'final structure did not converge: '
                        '${_statusSummary(runtime.status)}',
                      ),
                    );
          expect(current.structureRevision, runtime.sourceRevision);

          await _ensureStrongInline(
            runtime: runtime,
            lease: lease,
            positionUtf16: boldStart + 3,
          );
        } finally {
          lease?.release();
          await runtime.close().timeout(
            const Duration(seconds: 10),
            onTimeout: () => throw StateError(
              'close did not settle: ${_statusSummary(runtime.status)}',
            ),
          );
        }
      },
    );
  }

  test(
    'status delivery coalesces a source burst and retains exact inline close',
    () async {
      final runtime = await openFlarkV3PublicRuntimeForTest('**seed**\n');
      StreamSubscription<FlarkV3DocumentRuntimeStatus>? subscription;
      final observed = <FlarkV3DocumentRuntimeStatus>[];
      final streamDone = Completer<void>();
      var closed = false;
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 10));
        await Future<void>.delayed(Duration.zero);
        final beforeInline = runtime.status;
        subscription = runtime.statuses.listen(
          observed.add,
          onDone: streamDone.complete,
        );
        final initialQuery = await runtime
            .queryInlineAtUtf16(3)
            .timeout(const Duration(seconds: 10));
        expect(switch (initialQuery) {
          FlarkV3DocumentStructuralQuery(:final inlineFacts) => inlineFacts,
          FlarkV3RecursiveGreenPointQuery(:final inlineFacts) => inlineFacts,
          _ => null,
        }, isNotNull);
        await Future<void>.delayed(Duration.zero);
        final initial = runtime.status;
        expect(initial.structureCurrent, isTrue);
        expect(
          observed.any(
            (status) =>
                status.sourceRevision == beforeInline.sourceRevision &&
                status.inlinePresentationGeneration >
                    beforeInline.inlinePresentationGeneration,
          ),
          isTrue,
          reason: 'inline presentation is a semantic delivery barrier',
        );
        final callbackCountBeforeBurst = observed.length;
        const replacements = 'abcde';
        for (var index = 0; index < replacements.length; index += 1) {
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: runtime.sourceRevision,
              operation: FlarkV3SourceEdit(
                startUtf16: 2,
                endUtf16: 3,
                replacement: replacements[index],
              ),
            ),
          );
          expect(receipt.sourceRevision, initial.sourceRevision + index + 1);
          expect(
            runtime.status.sourceRevision,
            receipt.sourceRevision,
            reason: 'the synchronous status must never be coalesced',
          );
          expect(runtime.status.structureCurrent, isFalse);
        }
        expect(
          observed,
          hasLength(callbackCountBeforeBurst),
          reason: 'status callbacks remain asynchronous to source mutation',
        );

        final latestRevision = initial.sourceRevision + replacements.length;
        await runtime.statuses
            .firstWhere(
              (status) =>
                  status.sourceRevision == latestRevision &&
                  status.structureRevision == latestRevision &&
                  status.structureCurrent,
            )
            .timeout(const Duration(seconds: 10));
        await Future<void>.delayed(Duration.zero);

        final pendingBurst = observed
            .where(
              (status) =>
                  status.state == initial.state &&
                  status.sourceRevision > initial.sourceRevision &&
                  status.certifiedSourceRevision ==
                      initial.certifiedSourceRevision &&
                  !status.sourceCurrent &&
                  status.structureRevision == initial.structureRevision &&
                  status.structureGeneration == initial.structureGeneration &&
                  !status.structureCurrent &&
                  status.inlinePresentationGeneration ==
                      initial.inlinePresentationGeneration &&
                  status.inlineAttemptOutcomeGeneration ==
                      initial.inlineAttemptOutcomeGeneration &&
                  status.viewportPresentationGeneration ==
                      initial.viewportPresentationGeneration &&
                  status.viewportPresentationAttemptOutcomeGeneration ==
                      initial.viewportPresentationAttemptOutcomeGeneration &&
                  status.viewportPresentationUnavailableReason ==
                      initial.viewportPresentationUnavailableReason &&
                  status.recoveryAvailable == initial.recoveryAvailable,
            )
            .toList(growable: false);
        expect(pendingBurst, hasLength(1));
        expect(pendingBurst.single.sourceRevision, latestRevision);
        expect(
          observed,
          contains(
            isA<FlarkV3DocumentRuntimeStatus>()
                .having(
                  (status) => status.sourceRevision,
                  'sourceRevision',
                  latestRevision,
                )
                .having(
                  (status) => status.structureRevision,
                  'structureRevision',
                  latestRevision,
                )
                .having(
                  (status) => status.structureCurrent,
                  'structureCurrent',
                  isTrue,
                ),
          ),
        );
        await runtime.close().timeout(const Duration(seconds: 10));
        closed = true;
        await streamDone.future.timeout(const Duration(seconds: 1));
        final closingIndex = observed.indexWhere(
          (status) => status.state == FlarkV3DocumentRuntimeState.closing,
        );
        final closedIndex = observed.indexWhere(
          (status) => status.state == FlarkV3DocumentRuntimeState.closed,
        );
        expect(closingIndex, greaterThanOrEqualTo(0));
        expect(closedIndex, greaterThan(closingIndex));
      } finally {
        await subscription?.cancel();
        if (!closed) {
          await runtime.close().timeout(const Duration(seconds: 10));
        }
      }
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    '1 MiB document coalesces a synchronous local edit burst exactly',
    () async {
      final source = _largeBurstDocument();
      final editStart = source.length - _burstTail.length + 2;
      final expected = source.replaceRange(
        editStart,
        editStart + 1,
        _burstReplacements[_burstReplacements.length - 1],
      );
      final runtime = await openFlarkV3PublicRuntimeForTest(source);
      StreamSubscription<FlarkV3DocumentRuntimeStatus>? subscription;
      final observed = <FlarkV3DocumentRuntimeStatus>[];
      try {
        await runtime.initialReady.timeout(const Duration(seconds: 20));
        final initial = runtime.status;
        expect(initial.structureCurrent, isTrue);
        expect(source.length, greaterThanOrEqualTo(_mib));

        subscription = runtime.statuses.listen(observed.add);
        var maximumApply = Duration.zero;
        var totalApplyMicroseconds = 0;
        for (var index = 0; index < _burstReplacements.length; index += 1) {
          final applyClock = Stopwatch()..start();
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: runtime.sourceRevision,
              operation: FlarkV3SourceEdit(
                startUtf16: editStart,
                endUtf16: editStart + 1,
                replacement: _burstReplacements[index],
              ),
            ),
          );
          applyClock.stop();
          expect(receipt.changed, isTrue);
          expect(receipt.sourceRevision, initial.sourceRevision + index + 1);
          totalApplyMicroseconds += applyClock.elapsedMicroseconds;
          if (applyClock.elapsed > maximumApply) {
            maximumApply = applyClock.elapsed;
          }
        }

        final latestRevision =
            initial.sourceRevision + _burstReplacements.length;
        expect(runtime.sourceRevision, latestRevision);
        expect(runtime.status.structureCurrent, isFalse);
        expect(runtime.exportMarkdown(), expected);

        final current = await runtime.statuses
            .firstWhere(
              (status) =>
                  status.state == FlarkV3DocumentRuntimeState.faulted ||
                  status.sourceRevision == latestRevision &&
                      status.structureCurrent,
            )
            .timeout(
              const Duration(seconds: 20),
              onTimeout: () => throw StateError(
                'large burst did not converge: '
                '${_statusSummary(runtime.status)}',
              ),
            );
        await Future<void>.delayed(Duration.zero);

        expect(current.state, FlarkV3DocumentRuntimeState.open);
        expect(current.sourceCurrent, isTrue);
        expect(current.certifiedSourceRevision, latestRevision);
        expect(current.structureRevision, latestRevision);
        expect(
          current.structureGeneration,
          initial.structureGeneration + 1,
          reason:
              'The synchronous burst must coalesce into one fresh structural '
              'publication.',
        );
        final currentCommits = observed
            .where((status) => status.structureCurrent)
            .toList();
        expect(currentCommits, isNotEmpty);
        expect(
          currentCommits,
          everyElement(
            isA<FlarkV3DocumentRuntimeStatus>()
                .having(
                  (status) => status.sourceRevision,
                  'sourceRevision',
                  latestRevision,
                )
                .having(
                  (status) => status.structureRevision,
                  'structureRevision',
                  latestRevision,
                ),
          ),
          reason:
              'No superseded source revision may become structurally current.',
        );
        expect(runtime.readSourceRange(editStart, editStart + 4), 'leed');
        expect(runtime.exportMarkdown(), expected);

        expect(
          maximumApply,
          lessThan(const Duration(microseconds: 16667)),
          reason:
              'One local source cut must return within one 60 Hz frame on the '
              'native/Web regression host; parser convergence stays off the '
              'caller isolate.',
        );
        expect(
          Duration(microseconds: totalApplyMicroseconds),
          lessThan(const Duration(milliseconds: 50)),
          reason:
              'Twelve zero-cadence local edits must remain a small bounded '
              'caller-isolate burst.',
        );
        // ignore: avoid_print
        print(
          'flark_v3_large_rapid_burst '
          'source_utf16=${source.length} '
          'edits=${_burstReplacements.length} '
          'apply_max_us=${maximumApply.inMicroseconds} '
          'apply_total_us=$totalApplyMicroseconds',
        );
      } finally {
        await subscription?.cancel();
        await runtime.close().timeout(
          const Duration(seconds: 10),
          onTimeout: () => throw StateError(
            'close did not settle: ${_statusSummary(runtime.status)}',
          ),
        );
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );
}

String _largeBurstDocument() {
  final line = '${'x' * 79}\n';
  return '${line * ((_mib ~/ line.length) + 1)}$_burstTail';
}

void _replaceBold(
  FlarkV3DocumentRuntime runtime,
  int boldStart,
  String before,
  int localStart,
  int localEnd,
  String replacement,
) {
  final receipt = runtime.apply(
    FlarkV3SourceTransaction.single(
      baseRevision: runtime.sourceRevision,
      operation: FlarkV3SourceEdit(
        startUtf16: boldStart + localStart,
        endUtf16: boldStart + localEnd,
        replacement: replacement,
      ),
    ),
  );
  expect(receipt.changed, isTrue);
  expect(
    runtime.readSourceRange(
      boldStart,
      boldStart + before.length - (localEnd - localStart) + replacement.length,
    ),
    before.replaceRange(localStart, localEnd, replacement),
  );
}

Future<void> _ensureStrongInline({
  required FlarkV3DocumentRuntime runtime,
  required FlarkV3DocumentRuntimeAdapterLease lease,
  required int positionUtf16,
}) async {
  final query = lease.queryAtUtf16(positionUtf16);
  expect(query, isA<FlarkV3DocumentStructuralQuery>());
  final structural = query as FlarkV3DocumentStructuralQuery;
  if (structural.inlineFacts == null) {
    final presentation = runtime.status.inlinePresentationGeneration;
    final outcome = runtime.status.inlineAttemptOutcomeGeneration;
    final settled = runtime.statuses.firstWhere(
      (status) =>
          status.inlinePresentationGeneration > presentation ||
          status.inlineAttemptOutcomeGeneration > outcome ||
          status.state == FlarkV3DocumentRuntimeState.faulted,
    );
    expect(
      lease.ensureInlineAtUtf16(positionUtf16, structuralQuery: structural),
      FlarkV3InlineDemandDisposition.scheduled,
    );
    final status = await settled.timeout(
      const Duration(seconds: 10),
      onTimeout: () => throw StateError(
        'inline demand did not settle: ${_statusSummary(runtime.status)}',
      ),
    );
    expect(status.state, FlarkV3DocumentRuntimeState.open);
    expect(status.inlineAttemptOutcomeGeneration, outcome + 1);
    expect(status.inlinePresentationGeneration, presentation + 1);
  }
  final refined = lease.queryAtUtf16(positionUtf16);
  expect(refined, isA<FlarkV3DocumentStructuralQuery>());
  final inline = (refined as FlarkV3DocumentStructuralQuery).inlineFacts;
  expect(inline, isNotNull);
  expect(inline!.facts, isNotEmpty);
  expect(
    inline.facts.map((fact) => fact.kind),
    contains(FlarkV3InlineFactKind.strong),
  );
}

String _statusSummary(FlarkV3DocumentRuntimeStatus status) =>
    'state=${status.state.name}, '
    'source=${status.sourceRevision}/${status.sourceCurrent}, '
    'certified=${status.certifiedSourceRevision}, '
    'structure=${status.structureRevision}/${status.structureCurrent}, '
    'inline=${status.inlinePresentationGeneration}/'
    '${status.inlineAttemptOutcomeGeneration}, '
    'recovery=${status.recoveryAvailable}';
