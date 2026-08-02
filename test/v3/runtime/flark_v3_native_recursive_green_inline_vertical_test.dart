@TestOn('vm')
library;

import 'package:flark/flark_adapter.dart'
    show
        FlarkV3AuthoritativeInlineIslandPresentation,
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3InlineIslandPresentation,
        FlarkV3SourcePaintInlineIslandPresentation;
import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

import 'support/flark_v3_public_runtime_test_platform.dart';

const _functionalTimeout = Duration(seconds: 20);
const _closeTimeout = Duration(seconds: 10);

void main() {
  test(
    'native recursive Green Paragraph joins marker-free sidecar authority',
    () async {
      const markdown = '- item\n  > **bold** and _em_\n';
      final runtime = await openFlarkV3PublicRuntimeForTest(
        markdown,
      ).timeout(_functionalTimeout);
      final lease = FlarkV3DocumentRuntimeAdapter.borrow(runtime);
      addTearDown(() async {
        lease.release();
        await runtime.close().timeout(_closeTimeout);
      });
      await runtime.initialReady.timeout(_functionalTimeout);

      final boldPoint = markdown.indexOf('bold') + 1;
      final initial = runtime.queryAtUtf16(boldPoint);
      expect(initial, isA<FlarkV3RecursiveGreenPointQuery>());
      final green = initial as FlarkV3RecursiveGreenPointQuery;
      expect(green.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
      expect(green.inlineFacts, isNull);
      expect(green.ancestry.map((ancestor) => ancestor.kind), const [
        FlarkV3RecursiveGreenKind.document,
        FlarkV3RecursiveGreenKind.list,
        FlarkV3RecursiveGreenKind.item,
        FlarkV3RecursiveGreenKind.blockQuote,
        FlarkV3RecursiveGreenKind.paragraph,
      ]);

      final refined =
          await runtime
                  .queryInlineAtUtf16(boldPoint)
                  .timeout(_functionalTimeout)
              as FlarkV3RecursiveGreenPointQuery;
      expect(refined.inlineFacts, isNotNull);
      expect(refined.paragraphSource, isNotNull);
      expect(refined.inlineSource, isNotNull);
      final presentation =
          FlarkV3InlineIslandPresentation.resolveRecursiveGreenParagraph(
                sourceDocument: lease.document.source,
                expectedSource: lease.document.sourceVersion,
                recursiveQuery: refined,
              )
              as FlarkV3AuthoritativeInlineIslandPresentation;
      expect(presentation.projection.sourceText, '**bold** and _em_');
      expect(presentation.projection.displayText, 'bold and em');
      expect(runtime.exportMarkdown(), markdown);

      final contentEnd = markdown.lastIndexOf('\n');
      final boundary =
          await runtime
                  .queryInlineAtUtf16(
                    contentEnd,
                    affinity: FlarkV3DocumentQueryAffinity.downstream,
                  )
                  .timeout(_functionalTimeout)
              as FlarkV3RecursiveGreenPointQuery;
      expect(boundary.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
      expect(boundary.inlineFacts, isNotNull);
      expect(
        FlarkV3InlineIslandPresentation.resolveRecursiveGreenParagraph(
          sourceDocument: lease.document.source,
          expectedSource: lease.document.sourceVersion,
          recursiveQuery: boundary,
        ),
        isA<FlarkV3AuthoritativeInlineIslandPresentation>(),
        reason:
            'the Paragraph terminal caret must reuse its exact inline island',
      );

      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: boldPoint,
            endUtf16: boldPoint,
            replacement: '!',
          ),
        ),
      );
      expect(
        FlarkV3InlineIslandPresentation.resolveRecursiveGreenParagraph(
          sourceDocument: lease.document.source,
          expectedSource: lease.document.sourceVersion,
          recursiveQuery: refined,
        ),
        isA<FlarkV3SourcePaintInlineIslandPresentation>(),
        reason: 'an edit must invalidate the old sidecar before repaint',
      );
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );
}
