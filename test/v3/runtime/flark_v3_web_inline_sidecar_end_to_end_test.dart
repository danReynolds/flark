@TestOn('browser')
library;

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3InlineDemandDisposition,
        FlarkV3InlineFactKind,
        FlarkV3InlineFactsDisposition,
        FlarkV3InlineLinkKind,
        FlarkV3InlineLinkTargetRecipe,
        FlarkV3InlineMarkerPolicy,
        FlarkV3InlineProjection;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'real Web runtime promotes exact styles and autolinks to schema 8',
    () async {
      const markdown =
          '**bold** ~~gone~~ <https://example.test/a> <me@example.test>';
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(_functionalTimeout);

        final schema1 = lease.queryAtUtf16(3);
        expect(schema1, isA<FlarkV3DocumentStructuralQuery>());
        final structural = schema1 as FlarkV3DocumentStructuralQuery;
        expect(
          structural.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(
          structural.inlineFacts,
          isNull,
          reason: 'viewport schema 1 must not imply inline authority',
        );
        expect(runtime.exportMarkdown(), markdown);

        final initialGeneration = runtime.status.inlinePresentationGeneration;
        final committed = _awaitStatus(
          runtime,
          (status) => status.inlinePresentationGeneration > initialGeneration,
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
          committedStatus.inlinePresentationGeneration,
          initialGeneration + 1,
          reason: 'the generation advances at host commit',
        );

        final schema8 = lease.queryAtUtf16(3);
        expect(schema8, isA<FlarkV3DocumentStructuralQuery>());
        final refined = schema8 as FlarkV3DocumentStructuralQuery;
        final inline = refined.inlineFacts;
        expect(
          inline,
          isNotNull,
          reason: 'viewport schema 8 joins the committed sidecar facts',
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
          reason: 'presentation facts never rewrite exact source truth',
        );
      } finally {
        lease.release();
        await runtime.close().timeout(_closeTimeout);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real Web runtime carries direct links and images through marker-free projection',
    () async {
      const markdown =
          '[*link*](&bsol;* "link&amp;title") '
          '![alt](&bsol;* "image&amp;title")';
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(_functionalTimeout);

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
      } finally {
        lease.release();
        await runtime.close().timeout(_closeTimeout);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'real Web runtime carries strict GFM bare autolinks markerlessly',
    () async {
      const scheme = 'https://example.test/a';
      const www = 'www.example.test/b';
      const email = 'me@example.test';
      const markdown = 'before $scheme $www $email after';
      final runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(
        runtime,
        inlineDemandOwner: true,
      );
      try {
        await runtime.initialReady.timeout(_functionalTimeout);
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
      } finally {
        lease.release();
        await runtime.close().timeout(_closeTimeout);
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
