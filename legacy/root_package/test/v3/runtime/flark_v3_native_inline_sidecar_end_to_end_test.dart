@TestOn('vm')
library;

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3InlineDemandDisposition,
        FlarkV3InlineFactKind,
        FlarkV3InlineFactsDisposition,
        FlarkV3InlineLinkKind,
        FlarkV3InlineLinkTargetRecipe,
        FlarkV3InlineMarkerPolicy,
        FlarkV3InlineProjection;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'real native runtime promotes exact styles and autolinks after demand',
    () async {
      const markdown =
          '**bold** ~~gone~~ <https://example.test/a> <me@example.test>';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        expect(runtime.status.sourceCurrent, isTrue);
        expect(runtime.status.structureCurrent, isTrue);

        final schema1 = runtime.queryAtUtf16(3);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(
          structural.inlineFacts,
          isNull,
          reason:
              'the exact structural publication must not eagerly install '
              'canonical inline facts',
        );
        expect(runtime.exportMarkdown(), markdown);

        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        final initialGeneration = runtime.status.inlinePresentationGeneration;
        final committed = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialGeneration ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(3, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        expect(
          lease.ensureInlineAtUtf16(3, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.coalesced,
          reason: 'repeat demand cannot consume the bounded retry early',
        );
        final committedStatus = await committed;
        expect(
          committedStatus.state,
          isNot(FlarkV3DocumentRuntimeState.faulted),
          reason:
              'the native endpoint must keep the exact-source runtime live '
              'while fulfilling late inline demand',
        );
        expect(
          committedStatus.inlinePresentationGeneration,
          initialGeneration + 1,
          reason: 'the presentation generation advances at host commit',
        );

        final schema8 = lease.queryAtUtf16(3);
        expect(schema8, isA<FlarkV3DocumentStructuralQuery>());
        final refined = schema8 as FlarkV3DocumentStructuralQuery;
        final inline = refined.inlineFacts;
        expect(
          inline,
          isNotNull,
          reason: 'the queried structure must join the demanded sidecar facts',
        );
        expect(
          inline!.disposition,
          FlarkV3InlineFactsDisposition.authoritative,
        );
        expect(inline.facts, hasLength(4));

        final strong = inline.facts[0];
        expect(strong.kind, FlarkV3InlineFactKind.strong);
        _expectUtf16Span(strong.source, 0, 8);
        _expectUtf16Span(strong.opener, 0, 2);
        _expectUtf16Span(strong.content, 2, 6);
        _expectUtf16Span(strong.closer, 6, 8);
        final strike = inline.facts[1];
        expect(strike.kind, FlarkV3InlineFactKind.strikethrough);
        _expectUtf16Span(strike.source, 9, 17);
        _expectUtf16Span(strike.opener, 9, 11);
        _expectUtf16Span(strike.content, 11, 15);
        _expectUtf16Span(strike.closer, 15, 17);
        final uri = inline.facts[2];
        expect(uri.kind, FlarkV3InlineFactKind.autolinkUri);
        _expectUtf16Span(uri.source, 18, 42);
        _expectUtf16Span(uri.opener, 18, 19);
        _expectUtf16Span(uri.content, 19, 41);
        _expectUtf16Span(uri.closer, 41, 42);
        expect(uri.linkAnnotation?.kind, FlarkV3InlineLinkKind.uri);
        expect(
          uri.linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.exactContent,
        );
        expect(uri.linkAnnotation?.destination, 'https://example.test/a');
        final email = inline.facts[3];
        expect(email.kind, FlarkV3InlineFactKind.autolinkEmail);
        _expectUtf16Span(email.source, 43, 60);
        _expectUtf16Span(email.opener, 43, 44);
        _expectUtf16Span(email.content, 44, 59);
        _expectUtf16Span(email.closer, 59, 60);
        expect(email.linkAnnotation?.kind, FlarkV3InlineLinkKind.email);
        expect(
          email.linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
        );
        expect(email.linkAnnotation?.destination, 'mailto:me@example.test');
        expect(
          runtime.exportMarkdown(),
          markdown,
          reason: 'late presentation facts never rewrite exact source truth',
        );
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime carries direct links and images through marker-free projection',
    () async {
      const markdown =
          '[*link*](&bsol;* "link&amp;title") '
          '![alt](&bsol;* "image&amp;title")';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );

        final schema1 = lease.queryAtUtf16(3);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(structural.inlineFacts, isNull);

        final initialPresentation = runtime.status.inlinePresentationGeneration;
        final initialOutcome = runtime.status.inlineAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(3, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final settledStatus = await settled;
        expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
        expect(
          settledStatus.inlinePresentationGeneration,
          initialPresentation + 1,
        );
        expect(
          settledStatus.inlineAttemptOutcomeGeneration,
          initialOutcome + 1,
        );

        final schema8 = lease.queryAtUtf16(3);
        expect(schema8, isA<FlarkV3DocumentStructuralQuery>());
        final inline = (schema8 as FlarkV3DocumentStructuralQuery).inlineFacts!;
        expect(inline.disposition, FlarkV3InlineFactsDisposition.authoritative);
        expect(inline.facts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.directLink,
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.directImage,
        ]);

        final directLink = inline.facts.first;
        final directImage = inline.facts.last;
        final link = directLink.linkAnnotation!;
        final image = directImage.imageAnnotation!;
        expect(link.kind, FlarkV3InlineLinkKind.direct);
        expect(
          link.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        );
        expect(link.destination, '*');
        expect(link.title, 'link&title');
        expect(directLink.imageAnnotation, isNull);
        expect(directImage.linkAnnotation, isNull);
        expect(image.destination, '*');
        expect(image.title, 'image&title');

        final source = lease.document.source;
        expect(
          source.readRange(
            link.destinationSource.startUtf16,
            link.destinationSource.endUtf16,
          ),
          '&bsol;*',
        );
        expect(
          source.readRange(
            link.titleSource!.startUtf16,
            link.titleSource!.endUtf16,
          ),
          '"link&amp;title"',
        );
        expect(
          source.readRange(
            image.destinationSource.startUtf16,
            image.destinationSource.endUtf16,
          ),
          '&bsol;*',
        );
        expect(
          source.readRange(
            image.titleSource!.startUtf16,
            image.titleSource!.endUtf16,
          ),
          '"image&amp;title"',
        );

        final markerFree = FlarkV3InlineProjection.fromValidatedFacts(
          sourceDocument: source,
          expectedSource: inline.sourceVersion,
          facts: inline,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        );
        expect(markerFree.sourceText, markdown);
        expect(markerFree.displayText, 'link alt');
        expect(markerFree.displayText, isNot(contains('[')));
        expect(markerFree.displayText, isNot(contains('(')));
        expect(
          markerFree.runs
              .where(
                (run) =>
                    run.linkAnnotation?.kind == FlarkV3InlineLinkKind.direct,
              )
              .map((run) => run.text)
              .join(),
          'link',
        );
        expect(
          markerFree.runs
              .where((run) => run.imageAnnotation != null)
              .map((run) => run.text)
              .join(),
          'alt',
        );
        expect(markerFree.imageAnnotations.single.destination, '*');
        expect(runtime.exportMarkdown(), markdown);
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime carries strict GFM bare autolinks markerlessly',
    () async {
      const scheme = 'https://example.test/a';
      const www = 'www.example.test/b';
      const email = 'me@example.test';
      const markdown = 'before $scheme $www $email after';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        final initial = lease.queryAtUtf16(markdown.indexOf(scheme));
        expect(initial, isA<FlarkV3DocumentStructuralQuery>());
        final structural = initial as FlarkV3DocumentStructuralQuery;
        expect(structural.inlineFacts, isNull);

        final initialPresentation = runtime.status.inlinePresentationGeneration;
        final initialOutcome = runtime.status.inlineAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(
            markdown.indexOf(scheme),
            structuralQuery: structural,
          ),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final settledStatus = await settled;
        expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
        expect(
          settledStatus.inlinePresentationGeneration,
          initialPresentation + 1,
        );

        final refined = lease.queryAtUtf16(markdown.indexOf(scheme));
        expect(refined, isA<FlarkV3DocumentStructuralQuery>());
        final inline = (refined as FlarkV3DocumentStructuralQuery).inlineFacts!;
        expect(inline.disposition, FlarkV3InlineFactsDisposition.authoritative);
        expect(inline.facts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.autolinkUri,
          FlarkV3InlineFactKind.autolinkUri,
          FlarkV3InlineFactKind.autolinkEmail,
        ]);

        final expected = <({String source, String destination})>[
          (source: scheme, destination: scheme),
          (source: www, destination: 'http://$www'),
          (source: email, destination: 'mailto:$email'),
        ];
        for (var index = 0; index < expected.length; index += 1) {
          final fact = inline.facts[index];
          final value = expected[index];
          final start = markdown.indexOf(value.source);
          final end = start + value.source.length;
          _expectUtf16Span(fact.source, start, end);
          _expectUtf16Span(fact.content, start, end);
          _expectUtf16Span(fact.opener, start, start);
          _expectUtf16Span(fact.closer, end, end);
          expect(fact.linkAnnotation?.destination, value.destination);
        }
        expect(
          inline.facts[1].linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent,
        );

        final projection = FlarkV3InlineProjection.fromValidatedFacts(
          sourceDocument: lease.document.source,
          expectedSource: inline.sourceVersion,
          facts: inline,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        );
        expect(projection.displayText, markdown);
        expect(
          projection.runs
              .where((run) => run.linkAnnotation != null)
              .map((run) => run.text),
          [scheme, www, email],
        );
        expect(runtime.exportMarkdown(), markdown);
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime refines bold after a reference definition',
    () async {
      const markdown = '[ref]: /target\n**x**';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      final observedStatuses = <FlarkV3DocumentRuntimeStatus>[runtime.status];
      final statusSubscription = runtime.statuses.listen(observedStatuses.add);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.open);
        expect(runtime.status.sourceCurrent, isTrue);
        expect(runtime.status.structureCurrent, isTrue);

        final schema1 = runtime.queryAtUtf16(18);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(structural.structure.referenceDefinitionCount, 1);
        _expectUtf16Span(structural.structure.source, 0, 20);
        _expectUtf16Span(structural.structure.visibleSource, 15, 20);
        _expectUtf16Span(structural.projection.source, 0, 20);
        _expectUtf16Span(structural.projection.projectedSource, 15, 20);
        expect(
          structural.inlineFacts,
          isNull,
          reason:
              'the exact structural publication must remain schema 1 until '
              'the active paragraph is explicitly demanded',
        );
        expect(runtime.exportMarkdown(), markdown);

        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        final initialPresentation = runtime.status.inlinePresentationGeneration;
        final initialOutcome = runtime.status.inlineAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(18, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final settledStatus = await settled;
        expect(
          settledStatus.state,
          FlarkV3DocumentRuntimeState.open,
          reason: 'late inline demand must not fault the exact-source runtime',
        );
        expect(
          settledStatus.inlineAttemptOutcomeGeneration,
          initialOutcome + 1,
          reason: 'one demanded refinement reaches one terminal host outcome',
        );
        expect(
          settledStatus.inlinePresentationGeneration,
          initialPresentation + 1,
          reason: 'that terminal outcome must be a successful host commit',
        );

        final schema8 = lease.queryAtUtf16(18);
        expect(schema8, isA<FlarkV3DocumentStructuralQuery>());
        final refined = schema8 as FlarkV3DocumentStructuralQuery;
        _expectUtf16Span(refined.structure.visibleSource, 15, 20);
        _expectUtf16Span(refined.projection.projectedSource, 15, 20);
        final inline = refined.inlineFacts;
        expect(inline, isNotNull);
        expect(
          inline!.disposition,
          FlarkV3InlineFactsDisposition.authoritative,
        );
        expect(inline.facts, hasLength(1));
        final strong = inline.facts.single;
        expect(strong.kind, FlarkV3InlineFactKind.strong);
        _expectUtf16Span(strong.source, 15, 20);
        _expectUtf16Span(strong.opener, 15, 17);
        _expectUtf16Span(strong.content, 17, 18);
        _expectUtf16Span(strong.closer, 18, 20);
        expect(runtime.exportMarkdown(), markdown);
        expect(
          observedStatuses
              .where((status) => status.structureCurrent)
              .every(
                (status) => status.state == FlarkV3DocumentRuntimeState.open,
              ),
          isTrue,
          reason:
              'an exact structural status must not precede the open runtime '
              'state: ${_statusTrace(observedStatuses)}',
        );
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await statusSubscription.cancel();
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime refines Unicode after a committed sidecar and edits',
    () async {
      const initialMarkdown = '[ref]: /target\n**x**';
      const finalMarkdown = '[ref]: /target\n**y日本🌍**';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        initialMarkdown,
      ).timeout(_functionalTimeout);
      final observedStatuses = <FlarkV3DocumentRuntimeStatus>[runtime.status];
      final statusSubscription = runtime.statuses.listen(observedStatuses.add);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );

        final initialQuery = runtime.queryAtUtf16(18);
        expect(initialQuery, isA<FlarkV3DocumentStructuralQuery>());
        final initialStructural =
            initialQuery as FlarkV3DocumentStructuralQuery;
        expect(initialStructural.inlineFacts, isNull);
        final initialPresentation = runtime.status.inlinePresentationGeneration;
        final initialOutcome = runtime.status.inlineAttemptOutcomeGeneration;
        final firstCommit = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(18, structuralQuery: initialStructural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final firstCommitStatus = await firstCommit;
        expect(firstCommitStatus.state, FlarkV3DocumentRuntimeState.open);
        expect(
          firstCommitStatus.inlinePresentationGeneration,
          initialPresentation + 1,
        );
        expect(
          firstCommitStatus.inlineAttemptOutcomeGeneration,
          initialOutcome + 1,
        );

        final exactRevisionFour = _awaitStatus(
          runtime,
          (status) =>
              status.sourceRevision == 4 &&
                  status.sourceCurrent &&
                  status.structureCurrent ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          runtime
              .apply(
                FlarkV3SourceTransaction.single(
                  baseRevision: 1,
                  operation: const FlarkV3SourceEdit(
                    startUtf16: 17,
                    endUtf16: 18,
                    replacement: 'y',
                  ),
                ),
              )
              .sourceRevision,
          2,
        );
        expect(
          runtime
              .apply(
                FlarkV3SourceTransaction.single(
                  baseRevision: 2,
                  operation: const FlarkV3SourceEdit(
                    startUtf16: 18,
                    endUtf16: 18,
                    replacement: '日本',
                  ),
                ),
              )
              .sourceRevision,
          3,
        );
        expect(
          runtime
              .apply(
                FlarkV3SourceTransaction.single(
                  baseRevision: 3,
                  operation: const FlarkV3SourceEdit(
                    startUtf16: 20,
                    endUtf16: 20,
                    replacement: '🌍',
                  ),
                ),
              )
              .sourceRevision,
          4,
        );
        expect(runtime.exportMarkdown(), finalMarkdown);

        final revisionFourStatus = await exactRevisionFour;
        expect(
          revisionFourStatus.state,
          FlarkV3DocumentRuntimeState.open,
          reason:
              'coalesced edits after sidecar commit must retain runtime '
              'liveness: ${_statusTrace(observedStatuses)}',
        );
        expect(revisionFourStatus.sourceRevision, 4);
        expect(revisionFourStatus.structureRevision, 4);

        final schema1 = runtime.queryAtUtf16(22);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(structural.structure.referenceDefinitionCount, 1);
        _expectUtf16Span(structural.structure.source, 0, 24);
        _expectUtf16Span(structural.structure.visibleSource, 15, 24);
        _expectUtf16Span(structural.projection.projectedSource, 15, 24);
        expect(
          structural.inlineFacts,
          isNull,
          reason: 'source revision 4 requires a fresh sidecar demand',
        );

        final beforeSecondPresentation =
            revisionFourStatus.inlinePresentationGeneration;
        final beforeSecondOutcome =
            revisionFourStatus.inlineAttemptOutcomeGeneration;
        final secondOutcome = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > beforeSecondPresentation ||
              status.inlineAttemptOutcomeGeneration > beforeSecondOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(22, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        late final FlarkV3DocumentRuntimeStatus secondOutcomeStatus;
        try {
          secondOutcomeStatus = await secondOutcome;
        } catch (error) {
          fail(
            'revision-4 demand produced no terminal status ($error): '
            '${_statusTrace(observedStatuses)}',
          );
        }
        expect(
          secondOutcomeStatus.state,
          FlarkV3DocumentRuntimeState.open,
          reason:
              'revision-4 Unicode demand must not fault: '
              '${_statusTrace(observedStatuses)}',
        );
        expect(
          secondOutcomeStatus.inlineAttemptOutcomeGeneration,
          beforeSecondOutcome + 1,
        );
        expect(
          secondOutcomeStatus.inlinePresentationGeneration,
          beforeSecondPresentation + 1,
          reason:
              'the second outcome must commit rather than abort: '
              '${_statusTrace(observedStatuses)}',
        );

        final schema8 = lease.queryAtUtf16(22);
        expect(schema8, isA<FlarkV3DocumentStructuralQuery>());
        final refined = schema8 as FlarkV3DocumentStructuralQuery;
        final inline = refined.inlineFacts;
        expect(inline, isNotNull);
        expect(
          inline!.disposition,
          FlarkV3InlineFactsDisposition.authoritative,
        );
        expect(inline.facts, hasLength(1));
        final strong = inline.facts.single;
        expect(strong.kind, FlarkV3InlineFactKind.strong);
        _expectUtf16Span(strong.source, 15, 24);
        _expectUtf16Span(strong.opener, 15, 17);
        _expectUtf16Span(strong.content, 17, 22);
        _expectUtf16Span(strong.closer, 22, 24);
        expect(runtime.exportMarkdown(), finalMarkdown);
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await statusSubscription.cancel();
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime refines coalesced Unicode edits without a prior sidecar',
    () async {
      const initialMarkdown = '[ref]: /target\n**x**';
      const finalMarkdown = '[ref]: /target\n**y日本🌍**';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        initialMarkdown,
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        final exactRevisionFour = _awaitStatus(
          runtime,
          (status) =>
              status.sourceRevision == 4 &&
                  status.sourceCurrent &&
                  status.structureCurrent ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: 1,
            operation: const FlarkV3SourceEdit(
              startUtf16: 17,
              endUtf16: 18,
              replacement: 'y',
            ),
          ),
        );
        runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: 2,
            operation: const FlarkV3SourceEdit(
              startUtf16: 18,
              endUtf16: 18,
              replacement: '日本',
            ),
          ),
        );
        runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: 3,
            operation: const FlarkV3SourceEdit(
              startUtf16: 20,
              endUtf16: 20,
              replacement: '🌍',
            ),
          ),
        );
        final revisionFourStatus = await exactRevisionFour;
        expect(revisionFourStatus.state, FlarkV3DocumentRuntimeState.open);
        final query = runtime.queryAtUtf16(22);
        expect(query, isA<FlarkV3DocumentStructuralQuery>());
        final structural = query as FlarkV3DocumentStructuralQuery;
        expect(structural.inlineFacts, isNull);
        _expectUtf16Span(structural.structure.visibleSource, 15, 24);

        final initialPresentation =
            revisionFourStatus.inlinePresentationGeneration;
        final initialOutcome =
            revisionFourStatus.inlineAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(22, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final settledStatus = await settled;
        expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
        expect(
          settledStatus.inlineAttemptOutcomeGeneration,
          initialOutcome + 1,
        );
        expect(
          settledStatus.inlinePresentationGeneration,
          initialPresentation + 1,
        );
        final refined = lease.queryAtUtf16(22);
        expect(refined, isA<FlarkV3DocumentStructuralQuery>());
        final inline = (refined as FlarkV3DocumentStructuralQuery).inlineFacts;
        expect(inline, isNotNull);
        expect(
          inline!.disposition,
          FlarkV3InlineFactsDisposition.authoritative,
        );
        expect(inline.facts.single.kind, FlarkV3InlineFactKind.strong);
        _expectUtf16Span(inline.facts.single.content, 17, 22);
        expect(runtime.exportMarkdown(), finalMarkdown);
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real native runtime refines the same Unicode source from open',
    () async {
      const markdown = '[ref]: /target\n**y日本🌍**';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      FlarkV3DocumentRuntimeAdapterLease? lease;
      Object? firstError;
      StackTrace? firstStackTrace;
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
        lease = FlarkV3DocumentRuntimeAdapter.borrow(
          runtime,
          inlineDemandOwner: true,
        );
        final query = runtime.queryAtUtf16(22);
        expect(query, isA<FlarkV3DocumentStructuralQuery>());
        final structural = query as FlarkV3DocumentStructuralQuery;
        expect(structural.inlineFacts, isNull);
        _expectUtf16Span(structural.structure.visibleSource, 15, 24);
        final initialPresentation = runtime.status.inlinePresentationGeneration;
        final initialOutcome = runtime.status.inlineAttemptOutcomeGeneration;
        final settled = _awaitStatus(
          runtime,
          (status) =>
              status.inlinePresentationGeneration > initialPresentation ||
              status.inlineAttemptOutcomeGeneration > initialOutcome ||
              status.state == FlarkV3DocumentRuntimeState.faulted,
        );
        expect(
          lease.ensureInlineAtUtf16(22, structuralQuery: structural),
          FlarkV3InlineDemandDisposition.scheduled,
        );
        final settledStatus = await settled;
        expect(settledStatus.state, FlarkV3DocumentRuntimeState.open);
        expect(
          settledStatus.inlineAttemptOutcomeGeneration,
          initialOutcome + 1,
        );
        expect(
          settledStatus.inlinePresentationGeneration,
          initialPresentation + 1,
        );
        final refined = lease.queryAtUtf16(22);
        expect(refined, isA<FlarkV3DocumentStructuralQuery>());
        final inline = (refined as FlarkV3DocumentStructuralQuery).inlineFacts;
        expect(inline, isNotNull);
        expect(
          inline!.disposition,
          FlarkV3InlineFactsDisposition.authoritative,
        );
        _expectUtf16Span(inline.facts.single.content, 17, 22);
        expect(runtime.exportMarkdown(), markdown);
      } catch (error, stackTrace) {
        firstError = error;
        firstStackTrace = stackTrace;
      }

      lease?.release();
      try {
        await runtime.close().timeout(_closeTimeout);
      } catch (error, stackTrace) {
        firstError ??= error;
        firstStackTrace ??= stackTrace;
      }
      if (firstError != null) {
        Error.throwWithStackTrace(firstError, firstStackTrace!);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );
}

Future<FlarkV3DocumentRuntimeStatus> _awaitStatus(
  FlarkV3DocumentRuntime runtime,
  bool Function(FlarkV3DocumentRuntimeStatus status) predicate,
) {
  final current = runtime.status;
  if (predicate(current)) {
    return Future<FlarkV3DocumentRuntimeStatus>.value(current);
  }
  return runtime.statuses.firstWhere(predicate).timeout(_functionalTimeout);
}

void _expectUtf16Span(FlarkV3SourceSpan span, int start, int end) {
  expect(span.startUtf16, start);
  expect(span.endUtf16, end);
}

String _statusTrace(Iterable<FlarkV3DocumentRuntimeStatus> statuses) => statuses
    .map(
      (status) =>
          '${status.state.name}:'
          'source=${status.sourceRevision}/${status.sourceCurrent}:'
          'structure=${status.structureRevision}/'
          '${status.structureCurrent}:'
          'inline=${status.inlinePresentationGeneration}/'
          '${status.inlineAttemptOutcomeGeneration}',
    )
    .join(' -> ');
