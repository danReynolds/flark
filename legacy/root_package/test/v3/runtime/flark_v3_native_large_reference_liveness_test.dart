@TestOn('vm')
library;

import 'dart:async';

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

String _destinationFor(int ordinal) => '/u/$ordinal';

String _referenceDense(int definitions, String tail) {
  final output = StringBuffer();
  for (var ordinal = 0; ordinal < definitions; ordinal += 1) {
    output.writeln('[label-$ordinal]: ${_destinationFor(ordinal)}');
  }
  output.write(tail);
  return output.toString();
}

Future<FlarkV3DocumentRuntimeStatus> _awaitCurrent(
  FlarkV3DocumentRuntime runtime,
) async {
  if (runtime.status.structureCurrent) return runtime.status;
  final status = await runtime.statuses
      .firstWhere(
        (status) =>
            status.structureCurrent ||
            status.state == FlarkV3DocumentRuntimeState.closed,
      )
      .timeout(const Duration(seconds: 60));
  if (status.structureCurrent) return status;
  await runtime.close();
  throw StateError(
    'The native runtime closed before structural recertification.',
  );
}

Future<void> _closeIfOpen(FlarkV3DocumentRuntime runtime) async {
  if (runtime.status.state == FlarkV3DocumentRuntimeState.closed) return;
  await runtime.close().timeout(const Duration(seconds: 30));
}

FlarkV3RecursiveGreenPointQuery _expectExactRecursiveGreenParagraph({
  required FlarkV3DocumentRuntime runtime,
  required int pointUtf16,
  required int revision,
  required String expectedEditableSource,
}) {
  final result = runtime.queryAtUtf16(pointUtf16);
  expect(result, isA<FlarkV3RecursiveGreenPointQuery>());
  final query = result as FlarkV3RecursiveGreenPointQuery;
  expect(query.sourceRevision, revision);
  expect(query.structureRevision, revision);
  expect(query.owner.kind, FlarkV3RecursiveGreenKind.paragraph);

  final range = runtime.queryBlockRange(pointUtf16, pointUtf16 + 1);
  if (range is FlarkV3DocumentSourceGapBlockRange) {
    fail('100k recursive-Green row query gap: ${range.reason}');
  }
  expect(range, isA<FlarkV3RecursiveGreenRowRange>());
  final greenRange = range as FlarkV3RecursiveGreenRowRange;
  expect(greenRange.sourceRevision, revision);
  expect(greenRange.structureRevision, revision);
  expect(greenRange.structuralAck.sourceVersion.revision, revision);
  expect(greenRange.selectedRow, isNotNull);
  final row = greenRange.selectedRow!;
  expect(row.selected, isTrue);
  expect(row.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(row.frameId, query.owner.frameId);
  expect(row.editCapability, FlarkV3RecursiveGreenRowEditCapability.contiguous);
  expect(row.editableSource, isNotNull);
  expect(
    runtime.readSourceRange(
      row.editableSource!.startUtf16,
      row.editableSource!.endUtf16,
    ),
    expectedEditableSource,
  );
  expect(
    query.ancestry.map((ancestor) => (ancestor.frameId, ancestor.kind)),
    row.path.map((frame) => (frame.frameId, frame.kind)),
  );
  return query;
}

Future<FlarkV3RecursiveGreenPointQuery> _queryAndExpectReferenceLinks({
  required FlarkV3DocumentRuntime runtime,
  required int pointUtf16,
  required int revision,
  required List<int> referenceOrdinals,
}) async {
  final result = await runtime
      .queryInlineAtUtf16(pointUtf16)
      .timeout(const Duration(seconds: 60));
  expect(result, isA<FlarkV3RecursiveGreenPointQuery>());
  final query = result as FlarkV3RecursiveGreenPointQuery;
  expect(query.sourceRevision, revision);
  expect(query.structureRevision, revision);
  expect(query.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
  expect(query.paragraphSource, isNotNull);
  expect(query.inlineSource, isNotNull);
  expect(query.inlineFacts, isNotNull);
  final inline = query.inlineFacts!;
  expect(inline.sourceVersion.revision, revision);
  expect(inline.disposition, FlarkV3InlineFactsDisposition.authoritative);
  expect(
    inline.facts.map((fact) => fact.kind),
    List<FlarkV3InlineFactKind>.filled(
      referenceOrdinals.length,
      FlarkV3InlineFactKind.referenceLink,
    ),
  );
  final destinationStarts = <int>[];
  for (var index = 0; index < referenceOrdinals.length; index += 1) {
    final expectedDestination = _destinationFor(referenceOrdinals[index]);
    final link = inline.facts[index].linkAnnotation;
    expect(link, isNotNull);
    expect(link!.kind, FlarkV3InlineLinkKind.reference);
    expect(
      link.targetRecipe,
      FlarkV3InlineLinkTargetRecipe.companionCookedValue,
    );
    expect(link.destination, expectedDestination);
    expect(link.title, isNull);
    expect(
      runtime.readSourceRange(
        link.destinationSource.startUtf16,
        link.destinationSource.endUtf16,
      ),
      expectedDestination,
    );
    destinationStarts.add(link.destinationSource.startUtf16);
  }
  expect(destinationStarts, orderedEquals(destinationStarts.toList()..sort()));
  return query;
}

void main() {
  test(
    'reference-definition destination edit recertifies unchanged recursive Paragraph fanout',
    () async {
      const initial = '[id]: /old\n\nFirst [one][id].\n\nSecond [two][id].\n';
      const edited = '[id]: /new\n\nFirst [one][id].\n\nSecond [two][id].\n';
      final firstPointUtf16 = initial.indexOf('one') + 1;
      final secondPointUtf16 = initial.indexOf('two') + 1;
      final destinationStart = initial.indexOf('/old');
      final runtime = await FlarkV3DocumentRuntime.open(
        initial,
      ).timeout(const Duration(seconds: 20));
      addTearDown(() => _closeIfOpen(runtime));
      await runtime.initialReady.timeout(const Duration(seconds: 20));

      Future<FlarkV3RecursiveGreenPointQuery> expectReference({
        required int pointUtf16,
        required String editableSource,
        required int revision,
        required String destination,
      }) async {
        final structural = _expectExactRecursiveGreenParagraph(
          runtime: runtime,
          pointUtf16: pointUtf16,
          revision: revision,
          expectedEditableSource: editableSource,
        );
        final result = await runtime
            .queryInlineAtUtf16(pointUtf16)
            .timeout(const Duration(seconds: 20));
        expect(result, isA<FlarkV3RecursiveGreenPointQuery>());
        final refined = result as FlarkV3RecursiveGreenPointQuery;
        expect(refined.sourceRevision, revision);
        expect(refined.structureRevision, revision);
        expect(refined.owner.frameId, structural.owner.frameId);
        expect(refined.inlineFacts, isNotNull);
        final inline = refined.inlineFacts!;
        expect(inline.sourceVersion.revision, revision);
        expect(inline.disposition, FlarkV3InlineFactsDisposition.authoritative);
        expect(inline.facts, hasLength(1));
        final fact = inline.facts.single;
        expect(fact.kind, FlarkV3InlineFactKind.referenceLink);
        final link = fact.linkAnnotation;
        expect(link, isNotNull);
        expect(link!.kind, FlarkV3InlineLinkKind.reference);
        expect(
          link.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        );
        expect(link.destination, destination);
        expect(link.title, isNull);
        expect(
          runtime.readSourceRange(
            link.destinationSource.startUtf16,
            link.destinationSource.endUtf16,
          ),
          destination,
        );
        return refined;
      }

      final initialFirst = await expectReference(
        pointUtf16: firstPointUtf16,
        editableSource: 'First [one][id].',
        revision: 1,
        destination: '/old',
      );
      final initialSecond = await expectReference(
        pointUtf16: secondPointUtf16,
        editableSource: 'Second [two][id].',
        revision: 1,
        destination: '/old',
      );
      final receipt = runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: 1,
          operation: FlarkV3SourceEdit(
            startUtf16: destinationStart,
            endUtf16: destinationStart + '/old'.length,
            replacement: '/new',
          ),
        ),
      );
      expect(receipt.changed, isTrue);
      expect(receipt.sourceRevision, 2);
      expect(runtime.exportMarkdown(), edited);
      final current = await _awaitCurrent(runtime);
      expect(current.sourceRevision, 2);
      expect(current.structureRevision, 2);

      final editedFirst = await expectReference(
        pointUtf16: firstPointUtf16,
        editableSource: 'First [one][id].',
        revision: 2,
        destination: '/new',
      );
      final editedSecond = await expectReference(
        pointUtf16: secondPointUtf16,
        editableSource: 'Second [two][id].',
        revision: 2,
        destination: '/new',
      );
      expect(editedFirst.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
      expect(editedSecond.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
      expect(editedFirst.inlineSource, isNotNull);
      expect(editedSecond.inlineSource, isNotNull);
      expect(initialFirst.inlineFacts!.sourceVersion.revision, 1);
      expect(initialSecond.inlineFacts!.sourceVersion.revision, 1);
      expect(runtime.exportMarkdown(), edited);
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    '100,000 definitions cross the native isolate and FFI host without caller-thread stalls',
    () async {
      const definitions = 100_000;
      const referenceOrdinals = [0, definitions ~/ 2, definitions - 1];
      final tail =
          '[early][label-${referenceOrdinals[0]}] '
          '[middle][label-${referenceOrdinals[1]}] '
          '[last][label-${referenceOrdinals[2]}] visible tail\n';
      final source = _referenceDense(definitions, tail);
      final tailStart = source.length - tail.length;
      final tailPoint = tailStart + tail.indexOf('visible') + 1;
      final clock = Stopwatch()..start();
      var previousHeartbeat = Duration.zero;
      var maximumHeartbeatGap = Duration.zero;
      var heartbeatCount = 0;
      final heartbeat = Timer.periodic(const Duration(milliseconds: 8), (_) {
        final now = clock.elapsed;
        final gap = now - previousHeartbeat;
        previousHeartbeat = now;
        heartbeatCount += 1;
        if (gap > maximumHeartbeatGap) maximumHeartbeatGap = gap;
      });

      FlarkV3DocumentRuntime? runtime;
      Object? primaryFailure;
      StackTrace? primaryStack;
      Object? cleanupFailure;
      StackTrace? cleanupStack;
      try {
        final coldStarted = clock.elapsed;
        final openCallClock = Stopwatch()..start();
        final openFuture = FlarkV3DocumentRuntime.open(source);
        openCallClock.stop();
        runtime = await openFuture.timeout(const Duration(seconds: 60));
        await runtime.initialReady.timeout(const Duration(seconds: 60));
        final coldElapsed = clock.elapsed - coldStarted;
        final initialRevision = runtime.sourceRevision;

        final initialPointClock = Stopwatch()..start();
        final initial = _expectExactRecursiveGreenParagraph(
          runtime: runtime,
          pointUtf16: tailPoint,
          revision: initialRevision,
          expectedEditableSource: tail.substring(0, tail.length - 1),
        );
        initialPointClock.stop();
        final initialInlineClock = Stopwatch()..start();
        final initialRefined = await _queryAndExpectReferenceLinks(
          runtime: runtime,
          pointUtf16: tailPoint,
          revision: initialRevision,
          referenceOrdinals: referenceOrdinals,
        );
        initialInlineClock.stop();
        expect(initialRefined.owner.frameId, initial.owner.frameId);
        expect(initialRefined.inlineSource, isNotNull);
        final initialDestinationSources = [
          for (final fact in initialRefined.inlineFacts!.facts)
            (
              fact.linkAnnotation!.destinationSource.startUtf16,
              fact.linkAnnotation!.destinationSource.endUtf16,
            ),
        ];

        final applyClock = Stopwatch()..start();
        final editStart = tailStart + tail.indexOf('visible');
        final edit = runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: runtime.sourceRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: editStart,
              endUtf16: editStart + 1,
              replacement: 'V',
            ),
          ),
        );
        applyClock.stop();
        expect(edit.changed, isTrue);
        final replacementStarted = clock.elapsed;
        final replacement = await _awaitCurrent(runtime);
        final replacementElapsed = clock.elapsed - replacementStarted;
        expect(replacement.structureRevision, runtime.sourceRevision);
        expect(runtime.readSourceRange(editStart, editStart + 1), 'V');

        final editedTail = tail.replaceFirst('visible', 'Visible');
        final replacementPointClock = Stopwatch()..start();
        final replaced = _expectExactRecursiveGreenParagraph(
          runtime: runtime,
          pointUtf16: tailPoint,
          revision: runtime.sourceRevision,
          expectedEditableSource: editedTail.substring(
            0,
            editedTail.length - 1,
          ),
        );
        replacementPointClock.stop();
        final replacementInlineClock = Stopwatch()..start();
        final replacedRefined = await _queryAndExpectReferenceLinks(
          runtime: runtime,
          pointUtf16: tailPoint,
          revision: runtime.sourceRevision,
          referenceOrdinals: referenceOrdinals,
        );
        replacementInlineClock.stop();
        expect(replacedRefined.owner.frameId, replaced.owner.frameId);
        expect(
          [
            for (final fact in replacedRefined.inlineFacts!.facts)
              (
                fact.linkAnnotation!.destinationSource.startUtf16,
                fact.linkAnnotation!.destinationSource.endUtf16,
              ),
          ],
          initialDestinationSources,
          reason:
              'A literal tail edit must retain early, middle, and final '
              'definition authority.',
        );

        final closeClock = Stopwatch()..start();
        await runtime.close().timeout(const Duration(seconds: 30));
        closeClock.stop();
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
        expect(applyClock.elapsed, lessThan(const Duration(milliseconds: 50)));
        expect(heartbeatCount, greaterThan(10));
        expect(
          maximumHeartbeatGap,
          lessThan(const Duration(seconds: 1)),
          reason: 'Native packet admission and host polls must stay bounded.',
        );
        // ignore: avoid_print
        print(
          'flark_v3_native_100k_references '
          'source_bytes=${source.length} cold_us=${coldElapsed.inMicroseconds} '
          'open_call_us=${openCallClock.elapsedMicroseconds} '
          'initial_point_us=${initialPointClock.elapsedMicroseconds} '
          'initial_inline_us=${initialInlineClock.elapsedMicroseconds} '
          'apply_us=${applyClock.elapsedMicroseconds} '
          'replacement_us=${replacementElapsed.inMicroseconds} '
          'replacement_point_us='
          '${replacementPointClock.elapsedMicroseconds} '
          'replacement_inline_us='
          '${replacementInlineClock.elapsedMicroseconds} '
          'close_us=${closeClock.elapsedMicroseconds} '
          'heartbeat_max_us=${maximumHeartbeatGap.inMicroseconds}',
        );
      } catch (error, stackTrace) {
        primaryFailure = error;
        primaryStack = stackTrace;
      } finally {
        heartbeat.cancel();
        if (runtime != null) {
          try {
            await _closeIfOpen(runtime);
          } catch (error, stackTrace) {
            cleanupFailure = error;
            cleanupStack = stackTrace;
          }
        }
      }
      if (primaryFailure != null) {
        if (cleanupFailure != null) {
          // ignore: avoid_print
          print(
            'flark_v3_native_100k_references cleanup_after_failure='
            '$cleanupFailure',
          );
        }
        Error.throwWithStackTrace(primaryFailure, primaryStack!);
      }
      if (cleanupFailure != null) {
        Error.throwWithStackTrace(cleanupFailure, cleanupStack!);
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
