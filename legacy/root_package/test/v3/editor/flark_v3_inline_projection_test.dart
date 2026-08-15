import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/editor/flark_v3_inline_projection.dart';
import 'package:flark/src/v3/editor/flark_v3_source_projection.dart';
import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3InlineProjection', () {
    test('nested ***x*** composes styles and certified marker chains', () {
      final projection = _projection(
        '***x***',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 2,
            start: 1,
            length: 5,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
      );

      expect(projection.displayText, 'x');
      expect(projection.runs, hasLength(1));
      final run = projection.runs.single;
      expect(run.text, 'x');
      expect((run.sourceStartUtf16, run.sourceEndUtf16), (3, 4));
      expect((run.displayStartUtf16, run.displayEndUtf16), (0, 1));
      expect(run.semanticStyles, [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.strong,
      ]);

      expect(projection.sourceToDisplayOffset(0), 0);
      expect(projection.sourceToDisplayOffset(2), 0);
      expect(projection.sourceToDisplayOffset(3), 0);
      expect(projection.sourceToDisplayOffset(4), 1);
      expect(projection.sourceToDisplayOffset(7), 1);
    });

    test('projects link annotations separately from style and topology', () {
      const uri = 'https://e.test';
      const email = 'me@e.test';
      const source = '*<$uri>* <$email>';
      final emailStart = uri.length + 5;
      final projection = _projection(
        source,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: uri.length + 4,
            contentStart: 1,
            contentLength: uri.length + 2,
          ),
          _record(
            kind: 5,
            start: 1,
            length: uri.length + 2,
            contentStart: 2,
            contentLength: uri.length,
          ),
          _record(
            kind: 6,
            start: emailStart,
            length: email.length + 2,
            contentStart: emailStart + 1,
            contentLength: email.length,
          ),
        ],
      );

      expect(projection.displayText, '$uri $email');
      expect(projection.runs.map((run) => run.text), [uri, ' ', email]);

      final uriRun = projection.runs[0];
      expect(uriRun.semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.autolinkUri,
      ]);
      expect(uriRun.semanticStyles, [FlarkV3InlineFactKind.emphasis]);
      expect(uriRun.linkAnnotation?.kind, FlarkV3InlineLinkKind.uri);
      expect(uriRun.linkAnnotation?.destination, uri);

      final emailRun = projection.runs[2];
      expect(emailRun.semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.autolinkEmail,
      ]);
      expect(emailRun.semanticStyles, isEmpty);
      expect(emailRun.linkAnnotation?.kind, FlarkV3InlineLinkKind.email);
      expect(emailRun.linkAnnotation?.destination, 'mailto:$email');

      expect(projection.sourceToDisplayOffset(0), 0);
      expect(projection.sourceToDisplayOffset(2), 0);
      expect(projection.sourceToDisplayOffset(uri.length + 2), uri.length);
      expect(projection.sourceToDisplayOffset(uri.length + 4), uri.length);
      expect(projection.sourceToDisplayOffset(emailStart), uri.length + 1);
      expect(
        projection.sourceToDisplayOffset(source.length),
        projection.displayLengthUtf16,
      );

      final topology = projection.delimiterTopology;
      expect(topology.pairs, hasLength(1));
      expect(topology.pairs.single.kind, FlarkV3InlineFactKind.emphasis);
      expect(topology.planOrphanCleanup(), isEmpty);
    });

    test(
      'markerless autolinks preserve identity and bound adjacent link runs',
      () {
        const scheme = 'https://e.test';
        const www = 'www.e.test';
        const email = 'me@e.test';
        const source = 'a $scheme b $www c $email d';
        final schemeStart = source.indexOf(scheme);
        final wwwStart = source.indexOf(www);
        final emailStart = source.indexOf(email);
        final projection = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 5,
              start: schemeStart,
              length: scheme.length,
              contentStart: schemeStart,
              contentLength: scheme.length,
            ),
            _record(
              kind: 5,
              flags: 1,
              start: wwwStart,
              length: www.length,
              contentStart: wwwStart,
              contentLength: www.length,
            ),
            _record(
              kind: 6,
              start: emailStart,
              length: email.length,
              contentStart: emailStart,
              contentLength: email.length,
            ),
          ],
        );

        expect(projection.displayText, source);
        expect(projection.runs.map((run) => run.text), [
          'a ',
          scheme,
          ' b ',
          www,
          ' c ',
          email,
          ' d',
        ]);
        expect(projection.runs.map((run) => run.linkAnnotation?.destination), [
          null,
          scheme,
          null,
          'http://$www',
          null,
          'mailto:$email',
          null,
        ]);
        expect(
          projection.runs[3].linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent,
        );
        expect(projection.delimiterTopology.isEmpty, isTrue);
        expect(
          projection.sourceProjection.pieces,
          everyElement(
            isA<FlarkV3SourceProjectionPiece>().having(
              (piece) => piece.isCopied,
              'isCopied',
              isTrue,
            ),
          ),
          reason:
              'markerless link boundaries split semantics without '
              'manufacturing hidden source pieces',
        );
        for (var offset = 0; offset <= source.length; offset += 1) {
          expect(projection.sourceToDisplayOffset(offset), offset);
          expect(
            projection.displayToSourceOffset(
              offset,
              affinity: FlarkV3InlineProjectionAffinity.downstream,
            ),
            offset,
          );
        }
      },
    );

    test(
      'URI autolinks retain link authority across cooked character references',
      () {
        const entity = '&amp;';
        const targetSource = 'https://e.test/?q=&amp;';
        const cookedTarget = 'https://e.test/?q=&';
        const source = '<https://e.test/?q=&amp;>';
        final entityStart = source.indexOf(entity);
        final records = [
          _record(
            kind: 5,
            start: 0,
            length: source.length,
            contentStart: 1,
            contentLength: targetSource.length,
          ),
          _characterReferenceRecord(
            start: entityStart,
            length: entity.length,
            first: 0x26,
          ),
        ];

        final hidden = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: records,
        );
        expect(hidden.displayText, cookedTarget);
        expect(hidden.runs.map((run) => run.text), ['https://e.test/?q=', '&']);
        expect(
          hidden.runs.map((run) => run.linkAnnotation?.destination),
          everyElement(cookedTarget),
        );
        expect(hidden.runs.last.semanticFacts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.autolinkUri,
          FlarkV3InlineFactKind.characterReference,
        ]);
        expect(hidden.runs.last.semanticStyles, isEmpty);
        expect(hidden.delimiterTopology.isEmpty, isTrue);
        expect(
          hidden.sourceProjection.pieces
              .where((piece) => piece.isReplaced)
              .single
              .displayText,
          '&',
        );

        final visible = _projection(source, records: records);
        expect(visible.displayText, source);
        expect(visible.sourceProjection.displayText, visible.sourceText);
        final visibleEntityRun = visible.runs.singleWhere(
          (run) => run.text == entity,
        );
        expect(visibleEntityRun.linkAnnotation?.destination, cookedTarget);
        expect(visibleEntityRun.semanticStyles, isEmpty);
      },
    );

    test(
      'direct links hide their full tail while retaining nested label styles',
      () {
        const source = '[*label*](<dest> "title")';
        final projection = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 10,
              start: 0,
              length: source.length,
              contentStart: 1,
              contentLength: 7,
            ),
            _record(
              kind: 1,
              start: 1,
              length: 7,
              contentStart: 2,
              contentLength: 5,
            ),
          ],
          inlineValues: _inlineValuesPayload(
            source,
            entries: const [
              _ValueEntry(
                parentFactOrdinal: 0,
                destinationStart: 11,
                destinationLength: 4,
                titleStart: 17,
                titleLength: 7,
                cookedDestination: 'dest',
                cookedTitle: 'title',
              ),
            ],
          ),
        );

        expect(projection.displayText, 'label');
        expect(projection.runs, hasLength(1));
        final run = projection.runs.single;
        expect(run.semanticFacts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.directLink,
          FlarkV3InlineFactKind.emphasis,
        ]);
        expect(run.semanticStyles, [FlarkV3InlineFactKind.emphasis]);
        expect(run.linkAnnotation?.kind, FlarkV3InlineLinkKind.direct);
        expect(run.linkAnnotation?.destination, 'dest');
        expect(run.linkAnnotation?.title, 'title');
        expect(run.imageAnnotation, isNull);
        expect(projection.delimiterTopology.pairs, hasLength(1));
        expect(
          projection.delimiterTopology.pairs.single.kind,
          FlarkV3InlineFactKind.emphasis,
          reason: 'the complex link tail is not generic delimiter topology',
        );
        expect(projection.sourceProjection.pieces.map((piece) => piece.kind), [
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.copy,
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.hide,
        ]);
      },
    );

    test(
      'reference links hide only their use-site markers and retain document-absolute values',
      () {
        const source = '[label]\n\n[label]: destination "title"';
        final authority = _authority(source);
        final inlineEnd = source.indexOf('\n');
        final destinationStart = source.indexOf('destination');
        final titleStart = source.indexOf('"title"');
        final leaf = FlarkV3SourceSpan(
          startUtf8: 0,
          endUtf8: inlineEnd,
          startUtf16: 0,
          endUtf16: inlineEnd,
        );
        final facts = FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: leaf,
          factLeaf: leaf,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 1,
          encodedFacts: _record(
            kind: 12,
            start: 0,
            length: inlineEnd,
            contentStart: 1,
            contentLength: 5,
          ),
          inlineValues: FlarkV3InlineValuesPayload(
            sourceVersion: authority.version,
            profilePartition: 3,
            source: leaf,
            encodedBytes: _encodeInlineValues([
              _ValueEntry(
                parentFactOrdinal: 0,
                destinationStart: destinationStart,
                destinationLength: 'destination'.length,
                titleStart: titleStart,
                titleLength: '"title"'.length,
                cookedDestination: 'destination',
                cookedTitle: 'title',
              ),
            ]),
          ),
        );

        final projection = FlarkV3InlineProjection.fromValidatedFacts(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          facts: facts,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        );

        expect(projection.displayText, 'label');
        expect(projection.runs, hasLength(1));
        final annotation = projection.runs.single.linkAnnotation;
        expect(annotation?.kind, FlarkV3InlineLinkKind.reference);
        expect(annotation?.destination, 'destination');
        expect(annotation?.title, 'title');
        expect(annotation?.destinationSource.startUtf8, destinationStart);
        expect(annotation?.destinationSource.startUtf8, greaterThan(inlineEnd));
      },
    );

    test(
      'links nested in image alt retain facts but cannot become actions',
      () {
        const source = '![[x](u)](img)';
        final projection = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 11,
              start: 0,
              length: source.length,
              contentStart: 2,
              contentLength: 6,
            ),
            _record(
              kind: 10,
              start: 2,
              length: 6,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
          inlineValues: _inlineValuesPayload(
            source,
            entries: const [
              _ValueEntry(
                parentFactOrdinal: 0,
                destinationStart: 10,
                destinationLength: 3,
                cookedDestination: 'img',
              ),
              _ValueEntry(
                parentFactOrdinal: 1,
                destinationStart: 6,
                destinationLength: 1,
                cookedDestination: 'u',
              ),
            ],
          ),
        );

        expect(projection.displayText, 'x');
        final run = projection.runs.single;
        expect(run.semanticFacts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.directImage,
          FlarkV3InlineFactKind.directLink,
        ]);
        expect(run.linkAnnotation, isNull);
        expect(run.imageAnnotation?.destination, 'img');
        expect(projection.delimiterTopology.isEmpty, isTrue);
      },
    );

    test('links nested in reference-image alt cannot become actions', () {
      const source = '![[x](u)][img]\n\n[img]: image.png';
      final authority = _authority(source);
      const inlineEnd = 14;
      final imageDestinationStart = source.indexOf('image.png');
      const leaf = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: inlineEnd,
        startUtf16: 0,
        endUtf16: inlineEnd,
      );
      final facts = FlarkV3InlineFactsDecoder.decode(
        sourceDocument: authority.document,
        expectedSource: authority.version,
        factSource: authority.version,
        expectedProfilePartition: 3,
        profilePartition: 3,
        expectedLeaf: leaf,
        factLeaf: leaf,
        disposition: FlarkV3InlineFactsDisposition.authoritative,
        factCount: 2,
        encodedFacts: Uint8List.fromList([
          ..._record(
            kind: 13,
            start: 0,
            length: inlineEnd,
            contentStart: 2,
            contentLength: 6,
          ),
          ..._record(
            kind: 10,
            start: 2,
            length: 6,
            contentStart: 3,
            contentLength: 1,
          ),
        ]),
        inlineValues: FlarkV3InlineValuesPayload(
          sourceVersion: authority.version,
          profilePartition: 3,
          source: leaf,
          encodedBytes: _encodeInlineValues([
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: imageDestinationStart,
              destinationLength: 'image.png'.length,
              cookedDestination: 'image.png',
            ),
            const _ValueEntry(
              parentFactOrdinal: 1,
              destinationStart: 6,
              destinationLength: 1,
              cookedDestination: 'u',
            ),
          ]),
        ),
      );
      final projection = FlarkV3InlineProjection.fromValidatedFacts(
        sourceDocument: authority.document,
        expectedSource: authority.version,
        facts: facts,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
      );

      expect(projection.displayText, 'x');
      final run = projection.runs.single;
      expect(run.semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.referenceImage,
        FlarkV3InlineFactKind.directLink,
      ]);
      expect(run.linkAnnotation, isNull);
      expect(run.imageAnnotation?.destination, 'image.png');
    });

    test('empty image alt retains its image annotation without a text run', () {
      const source = '![]()';
      final projection = _projection(
        source,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 11,
            start: 0,
            length: 5,
            contentStart: 2,
            contentLength: 0,
          ),
        ],
        inlineValues: _inlineValuesPayload(
          source,
          entries: const [
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 4,
              destinationLength: 0,
              cookedDestination: '',
            ),
          ],
        ),
      );

      expect(projection.displayText, isEmpty);
      expect(projection.runs, isEmpty);
      expect(projection.imageAnnotations, hasLength(1));
      expect(projection.imageAnnotations.single.destination, isEmpty);
      expect(projection.imageAnnotations.single.content.startUtf16, 2);
      expect(projection.imageAnnotations.single.content.endUtf16, 2);
    });

    test('an image nested in a link preserves the surrounding link action', () {
      const source = '[![x](img)](outer)';
      final projection = _projection(
        source,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 10,
            start: 0,
            length: source.length,
            contentStart: 1,
            contentLength: 9,
          ),
          _record(
            kind: 11,
            start: 1,
            length: 9,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
        inlineValues: _inlineValuesPayload(
          source,
          entries: const [
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 12,
              destinationLength: 5,
              cookedDestination: 'outer',
            ),
            _ValueEntry(
              parentFactOrdinal: 1,
              destinationStart: 6,
              destinationLength: 3,
              cookedDestination: 'img',
            ),
          ],
        ),
      );

      expect(projection.displayText, 'x');
      final run = projection.runs.single;
      expect(run.linkAnnotation?.destination, 'outer');
      expect(run.imageAnnotation?.destination, 'img');
      expect(run.semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.directLink,
        FlarkV3InlineFactKind.directImage,
      ]);
    });

    test(
      'authoritative character references replace complete tokens only when hidden',
      () {
        const entity = '&NotEqualTilde;';
        const source = '*&NotEqualTilde;*';
        final records = [
          _record(
            kind: 1,
            start: 0,
            length: source.length,
            contentStart: 1,
            contentLength: entity.length,
          ),
          _characterReferenceRecord(
            start: 1,
            length: entity.length,
            first: 0x2242,
            second: 0x0338,
          ),
        ];

        final hidden = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: records,
        );
        expect(hidden.sourceText, source);
        expect(hidden.displayText, '\u2242\u0338');
        expect(hidden.runs, hasLength(1));
        expect(hidden.runs.single.text, '\u2242\u0338');
        expect(hidden.runs.single.semanticStyles, [
          FlarkV3InlineFactKind.emphasis,
        ]);
        expect(hidden.runs.single.semanticFacts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.characterReference,
        ]);
        expect(hidden.sourceProjection.pieces.map((piece) => piece.kind), [
          FlarkV3SourceProjectionPieceKind.hide,
          FlarkV3SourceProjectionPieceKind.replace,
          FlarkV3SourceProjectionPieceKind.hide,
        ]);
        final replacement = hidden.sourceProjection.pieces[1];
        expect(
          (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
          (1, 1 + entity.length),
        );
        expect(replacement.displayText, '\u2242\u0338');
        expect(hidden.delimiterTopology.pairs, hasLength(1));
        expect(
          hidden.delimiterTopology.pairs.single.kind,
          FlarkV3InlineFactKind.emphasis,
          reason: 'a replacement atom is not a delimiter pair',
        );

        final visible = _projection(source, records: records);
        expect(visible.displayText, source);
        expect(visible.sourceProjection.displayText, visible.sourceText);
        expect(
          visible.sourceProjection.pieces,
          everyElement(
            isA<FlarkV3SourceProjectionPiece>().having(
              (piece) => piece.isCopied,
              'isCopied',
              isTrue,
            ),
          ),
        );
      },
    );

    test(
      'escaped punctuation is marker-free non-style atomic edit metadata',
      () {
        final projection = _projection(
          r'\*',
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 7,
              start: 0,
              length: 2,
              contentStart: 1,
              contentLength: 1,
            ),
          ],
        );

        expect(projection.sourceText, r'\*');
        expect(projection.displayText, '*');
        expect(projection.runs, hasLength(1));
        expect(projection.runs.single.text, '*');
        expect(
          (
            projection.runs.single.sourceStartUtf16,
            projection.runs.single.sourceEndUtf16,
          ),
          (1, 2),
        );
        expect(
          projection.runs.single.semanticFacts.single.kind,
          FlarkV3InlineFactKind.escapedPunctuation,
        );
        expect(projection.runs.single.semanticStyles, isEmpty);

        final topology = projection.delimiterTopology;
        expect(topology.pairs, hasLength(1));
        expect(
          topology.pairs.single.kind,
          FlarkV3InlineFactKind.escapedPunctuation,
        );
        expect(topology.pairs.single.closer.isCollapsed, isTrue);

        expect(projection.sourceToDisplayOffset(0), 0);
        expect(projection.sourceToDisplayOffset(1), 0);
        expect(projection.sourceToDisplayOffset(2), 1);
        final mappedStart = projection.displayToSourceOffset(
          0,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        );
        final mappedEnd = projection.displayToSourceOffset(
          1,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        );
        expect((mappedStart, mappedEnd), (1, 2));

        final replacement = topology.planEdit(
          FlarkV3SourceEdit(
            startUtf16: mappedStart,
            endUtf16: mappedEnd,
            replacement: 'x',
          ),
          cleanupOrphanedPairs: false,
        );
        expect(
          (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
          (0, 2),
        );
        expect(replacement.replacement, 'x');
        expect(replacement.removedEscapedPunctuationFactIds, [0]);
        expect(replacement.removedPairedDelimiterFactIds, isEmpty);
        expect(
          replacement.authorizesEscapedPunctuationBoundaryInsertion,
          isFalse,
        );

        final deletion = topology.planEdit(
          FlarkV3SourceEdit(
            startUtf16: mappedStart,
            endUtf16: mappedEnd,
            replacement: '',
          ),
          cleanupOrphanedPairs: false,
        );
        expect((deletion.sourceStartUtf16, deletion.sourceEndUtf16), (0, 2));
        expect(deletion.removesEscapedPunctuationAtoms, isTrue);
        expect(deletion.removesPairedDelimiters, isFalse);

        final insertBefore = topology.planEdit(
          FlarkV3SourceEdit(
            startUtf16: mappedStart,
            endUtf16: mappedStart,
            replacement: 'x',
          ),
          cleanupOrphanedPairs: false,
        );
        expect(
          (insertBefore.sourceStartUtf16, insertBefore.sourceEndUtf16),
          (0, 0),
        );
        expect(
          insertBefore.authorizesEscapedPunctuationBoundaryInsertion,
          isTrue,
        );
        final insertAfterSource = projection.displayToSourceOffset(
          1,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        );
        final insertAfter = topology.planEdit(
          FlarkV3SourceEdit(
            startUtf16: insertAfterSource,
            endUtf16: insertAfterSource,
            replacement: 'x',
          ),
          cleanupOrphanedPairs: false,
        );
        expect(
          (insertAfter.sourceStartUtf16, insertAfter.sourceEndUtf16),
          (2, 2),
        );
        expect(
          insertAfter.authorizesEscapedPunctuationBoundaryInsertion,
          isTrue,
        );

        expect(
          () => topology.afterSourceEdit(
            FlarkV3SourceEdit(
              startUtf16: mappedStart,
              endUtf16: mappedEnd,
              replacement: 'x',
            ),
          ),
          throwsA(isA<FlarkV3InlineProjectionException>()),
        );
        final afterReplacement = topology.afterSourceEdit(
          replacement.sourceEdit,
        );
        expect(afterReplacement.pairs, isEmpty);
        expect(afterReplacement.sourceEndUtf16, 1);
      },
    );

    test('escape metadata ends at its byte and preserves parent style', () {
      final projection = _projection(
        r'*\*b*',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 5,
            contentStart: 1,
            contentLength: 3,
          ),
          _record(
            kind: 7,
            start: 1,
            length: 2,
            contentStart: 2,
            contentLength: 1,
          ),
        ],
      );

      expect(projection.displayText, '*b');
      expect(projection.runs.map((run) => run.text), ['*', 'b']);
      expect(projection.runs[0].semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.escapedPunctuation,
      ]);
      expect(projection.runs[0].semanticStyles, [
        FlarkV3InlineFactKind.emphasis,
      ]);
      expect(projection.runs[1].semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
      ]);

      final replacement = projection.delimiterTopology.planEdit(
        const FlarkV3SourceEdit(startUtf16: 2, endUtf16: 3, replacement: 'x'),
        cleanupOrphanedPairs: false,
      );
      expect(
        (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
        (1, 3),
      );
      expect(replacement.removedEscapedPunctuationFactIds, [1]);
      expect(replacement.removedPairedDelimiterFactIds, isEmpty);
    });

    test(
      'hard breaks hide either marker and normalize LF CR and CRLF atomically',
      () {
        for (final fixture in <({String marker, String lineEnding})>[
          (marker: '  ', lineEnding: '\n'),
          (marker: '\\', lineEnding: '\n'),
          (marker: '   ', lineEnding: '\r'),
          (marker: '\\', lineEnding: '\r'),
          (marker: '  ', lineEnding: '\r\n'),
          (marker: '\\', lineEnding: '\r\n'),
        ]) {
          final source = 'a${fixture.marker}${fixture.lineEnding}b';
          final markerStart = 1;
          final contentStart = markerStart + fixture.marker.length;
          final atomEnd = contentStart + fixture.lineEnding.length;
          final projection = _projection(
            source,
            markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
            records: [
              _record(
                kind: 8,
                start: markerStart,
                length: atomEnd - markerStart,
                contentStart: contentStart,
                contentLength: fixture.lineEnding.length,
              ),
            ],
          );

          expect(projection.sourceText, source);
          expect(projection.displayText, 'a\nb');
          expect(projection.sourceProjection.sourceText, source);
          expect(projection.sourceProjection.displayText, 'a\nb');
          expect(
            projection.sourceProjection.pieces.map((piece) => piece.kind),
            [
              FlarkV3SourceProjectionPieceKind.copy,
              FlarkV3SourceProjectionPieceKind.hide,
              FlarkV3SourceProjectionPieceKind.replace,
              FlarkV3SourceProjectionPieceKind.copy,
            ],
          );
          expect(projection.sourceProjection.pieces[2].displayText, '\n');

          final hardBreakRun = projection.runs.singleWhere(
            (run) => run.text == '\n',
          );
          expect(hardBreakRun.semanticStyles, isEmpty);
          expect(
            hardBreakRun.semanticFacts.single.kind,
            FlarkV3InlineFactKind.hardLineBreak,
          );

          expect(projection.sourceToDisplayOffset(markerStart), 1);
          expect(projection.sourceToDisplayOffset(contentStart), 1);
          if (fixture.lineEnding.length == 2) {
            expect(projection.sourceToDisplayOffset(contentStart + 1), 1);
          }
          expect(projection.sourceToDisplayOffset(atomEnd), 2);
          final mappedStart = projection.displayToSourceOffset(
            1,
            affinity: FlarkV3InlineProjectionAffinity.downstream,
          );
          final mappedEnd = projection.displayToSourceOffset(
            2,
            affinity: FlarkV3InlineProjectionAffinity.upstream,
          );
          expect((mappedStart, mappedEnd), (contentStart, atomEnd));

          final replacement = projection.delimiterTopology.planEdit(
            FlarkV3SourceEdit(
              startUtf16: mappedStart,
              endUtf16: mappedEnd,
              replacement: 'x',
            ),
            cleanupOrphanedPairs: false,
          );
          expect(
            (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
            (markerStart, atomEnd),
          );
          expect(replacement.removedAtomicFactIds, [0]);
          expect(replacement.removedHardLineBreakFactIds, [0]);
          expect(replacement.removedEscapedPunctuationFactIds, isEmpty);
          expect(replacement.removesHardLineBreakAtoms, isTrue);
          expect(replacement.removesAtomicInlineAtoms, isTrue);

          final insertBefore = projection.delimiterTopology.planEdit(
            FlarkV3SourceEdit(
              startUtf16: mappedStart,
              endUtf16: mappedStart,
              replacement: 'x',
            ),
            cleanupOrphanedPairs: false,
          );
          expect(
            (insertBefore.sourceStartUtf16, insertBefore.sourceEndUtf16),
            (markerStart, markerStart),
          );
          expect(insertBefore.authorizesAtomicBoundaryInsertion, isTrue);
          expect(insertBefore.authorizesHardLineBreakBoundaryInsertion, isTrue);
          expect(
            insertBefore.authorizesEscapedPunctuationBoundaryInsertion,
            isFalse,
          );

          expect(
            () => projection.delimiterTopology.afterSourceEdit(
              FlarkV3SourceEdit(
                startUtf16: mappedStart,
                endUtf16: mappedEnd,
                replacement: 'x',
              ),
            ),
            throwsA(isA<FlarkV3InlineProjectionException>()),
          );
          final afterReplacement = projection.delimiterTopology.afterSourceEdit(
            replacement.sourceEdit,
          );
          expect(afterReplacement.pairs, isEmpty);
        }
      },
    );

    test('hard break remains a non-style leaf inside emphasis', () {
      final projection = _projection(
        '*a  \nb*',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 8,
            start: 2,
            length: 3,
            contentStart: 4,
            contentLength: 1,
          ),
        ],
      );

      expect(projection.displayText, 'a\nb');
      expect(projection.runs.map((run) => run.text), ['a', '\n', 'b']);
      expect(projection.runs.map((run) => run.semanticStyles), [
        [FlarkV3InlineFactKind.emphasis],
        [FlarkV3InlineFactKind.emphasis],
        [FlarkV3InlineFactKind.emphasis],
      ]);
      expect(projection.runs[1].semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.hardLineBreak,
      ]);
    });

    test('soft breaks and marker-visible hard breaks remain source exact', () {
      const softBreak = 'a \r\n b';
      final softProjection = _projection(
        softBreak,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
      );
      expect(softProjection.displayText, softBreak);
      expect(
        softProjection.sourceProjection.pieces.single.kind,
        FlarkV3SourceProjectionPieceKind.copy,
      );

      const hardBreak = 'a  \r\nb';
      final markerVisible = _projection(
        hardBreak,
        records: [
          _record(
            kind: 8,
            start: 1,
            length: 4,
            contentStart: 3,
            contentLength: 2,
          ),
        ],
      );
      expect(markerVisible.displayText, hardBreak);
      expect(
        markerVisible.sourceProjection.pieces.every(
          (piece) => piece.kind == FlarkV3SourceProjectionPieceKind.copy,
        ),
        isTrue,
      );
    });

    test(
      'general edit planning leaves paired replacement behavior unchanged',
      () {
        final projection = _projection(
          '*a*',
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 1,
              start: 0,
              length: 3,
              contentStart: 1,
              contentLength: 1,
            ),
          ],
        );
        final topology = projection.delimiterTopology;

        final replacement = topology.planEdit(
          const FlarkV3SourceEdit(startUtf16: 1, endUtf16: 2, replacement: 'x'),
        );
        expect(
          (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
          (1, 2),
        );
        expect(replacement.removesCertifiedConstructs, isFalse);

        final deferredDeletion = topology.planEdit(
          const FlarkV3SourceEdit(startUtf16: 1, endUtf16: 2, replacement: ''),
          cleanupOrphanedPairs: false,
        );
        expect(
          (deferredDeletion.sourceStartUtf16, deferredDeletion.sourceEndUtf16),
          (1, 2),
        );
        expect(deferredDeletion.removesCertifiedConstructs, isFalse);

        final cleanupDeletion = topology.planEdit(
          const FlarkV3SourceEdit(startUtf16: 1, endUtf16: 2, replacement: ''),
        );
        expect(
          (cleanupDeletion.sourceStartUtf16, cleanupDeletion.sourceEndUtf16),
          (0, 3),
        );
        expect(cleanupDeletion.removedEscapedPunctuationFactIds, isEmpty);
        expect(cleanupDeletion.removedPairedDelimiterFactIds, [0]);
      },
    );

    test(
      'delimiter topology expands nested orphan deletion without parsing',
      () {
        final projection = _projection(
          '***x***',
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 1,
              start: 0,
              length: 7,
              contentStart: 1,
              contentLength: 5,
            ),
            _record(
              kind: 2,
              start: 1,
              length: 5,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
        );

        final topology = projection.delimiterTopology;
        expect(topology.pairs, hasLength(2));
        expect(topology.pairs[0].parentId, isNull);
        expect(topology.pairs[1].parentId, 0);

        final deletion = topology.planDeletion(3, 4);
        expect((deletion.sourceStartUtf16, deletion.sourceEndUtf16), (0, 7));
        expect(deletion.removedPairIds, [0, 1]);
        final empty = topology.afterSourceEdit(
          FlarkV3SourceEdit(
            startUtf16: deletion.sourceStartUtf16,
            endUtf16: deletion.sourceEndUtf16,
            replacement: '',
          ),
        );
        expect(empty.pairs, isEmpty);
        expect(empty.sourceEndUtf16, 0);
      },
    );

    test(
      'provisional topology preserves partial content then removes orphan',
      () {
        final projection = _projection(
          '**fo**',
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 2,
              start: 0,
              length: 6,
              contentStart: 2,
              contentLength: 2,
            ),
          ],
        );

        final first = projection.delimiterTopology.planDeletion(3, 4);
        expect((first.sourceStartUtf16, first.sourceEndUtf16), (3, 4));
        expect(first.removedPairIds, isEmpty);
        final afterFirst = projection.delimiterTopology.afterSourceEdit(
          const FlarkV3SourceEdit(startUtf16: 3, endUtf16: 4, replacement: ''),
        );
        expect(
          (
            afterFirst.pairs.single.content.startUtf16,
            afterFirst.pairs.single.content.endUtf16,
          ),
          (2, 3),
        );

        final second = afterFirst.planDeletion(2, 3);
        expect((second.sourceStartUtf16, second.sourceEndUtf16), (0, 5));
        expect(second.removedPairIds, [0]);
      },
    );

    test('deferred nested cleanup reaches a fixed point mechanically', () {
      final projection = _projection(
        '***x***',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 2,
            start: 1,
            length: 5,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
      );

      var topology = projection.delimiterTopology.afterSourceEdit(
        const FlarkV3SourceEdit(startUtf16: 3, endUtf16: 4, replacement: ''),
      );
      var cleanup = topology.planOrphanCleanup();
      expect(cleanup, hasLength(1));
      expect(
        (cleanup.single.sourceStartUtf16, cleanup.single.sourceEndUtf16),
        (1, 5),
      );
      topology = topology.afterSourceEdit(
        FlarkV3SourceEdit(
          startUtf16: cleanup.single.sourceStartUtf16,
          endUtf16: cleanup.single.sourceEndUtf16,
          replacement: '',
        ),
      );
      cleanup = topology.planOrphanCleanup();
      expect(cleanup, hasLength(1));
      expect(
        (cleanup.single.sourceStartUtf16, cleanup.single.sourceEndUtf16),
        (0, 2),
      );
      topology = topology.afterSourceEdit(
        FlarkV3SourceEdit(
          startUtf16: cleanup.single.sourceStartUtf16,
          endUtf16: cleanup.single.sourceEndUtf16,
          replacement: '',
        ),
      );
      expect(topology.pairs, isEmpty);
      expect(topology.planOrphanCleanup(), isEmpty);
    });

    test('all-markers-visible is an exact identity projection', () {
      const source = '***x***';
      final projection = _projection(
        source,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 2,
            start: 1,
            length: 5,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
      );

      expect(projection.displayText, source);
      expect(projection.runs.map((run) => run.text).join(), source);
      for (var offset = 0; offset <= source.length; offset += 1) {
        expect(projection.sourceToDisplayOffset(offset), offset);
        expect(
          projection.displayToSourceOffset(
            offset,
            affinity: FlarkV3InlineProjectionAffinity.upstream,
          ),
          offset,
        );
        expect(
          projection.displayToSourceOffset(
            offset,
            affinity: FlarkV3InlineProjectionAffinity.downstream,
          ),
          offset,
        );
      }
      expect(
        projection.runs.singleWhere((run) => run.text == 'x').semanticStyles,
        [FlarkV3InlineFactKind.emphasis, FlarkV3InlineFactKind.strong],
      );
    });

    test(
      'adjacent styled spans do not leak styles across their marker chain',
      () {
        final projection = _projection(
          '*a***b**',
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: [
            _record(
              kind: 1,
              start: 0,
              length: 3,
              contentStart: 1,
              contentLength: 1,
            ),
            _record(
              kind: 2,
              start: 3,
              length: 5,
              contentStart: 5,
              contentLength: 1,
            ),
          ],
        );

        expect(projection.displayText, 'ab');
        expect(projection.runs.map((run) => run.text), ['a', 'b']);
        expect(projection.runs[0].semanticStyles, [
          FlarkV3InlineFactKind.emphasis,
        ]);
        expect(projection.runs[1].semanticStyles, [
          FlarkV3InlineFactKind.strong,
        ]);
        expect(projection.sourceToDisplayOffset(2), 1);
        expect(projection.sourceToDisplayOffset(4), 1);
        expect(projection.sourceToDisplayOffset(5), 1);
      },
    );

    test('hidden-marker boundary affinity spans adjacent marker clusters', () {
      final projection = _projection(
        '*a***b**',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
          _record(
            kind: 2,
            start: 3,
            length: 5,
            contentStart: 5,
            contentLength: 1,
          ),
        ],
      );

      expect(
        projection.displayToSourceOffset(
          0,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        0,
      );
      expect(
        projection.displayToSourceOffset(
          0,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        1,
      );
      expect(
        projection.displayToSourceOffset(
          1,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        2,
      );
      expect(
        projection.displayToSourceOffset(
          1,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        5,
      );
      expect(
        projection.displayToSourceOffset(
          2,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        6,
      );
      expect(
        projection.displayToSourceOffset(
          2,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        8,
      );
    });

    test('UTF-16 mapping remains exact through visible Unicode content', () {
      final projection = _projection(
        '*😀*',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 6,
            contentStart: 1,
            contentLength: 4,
          ),
        ],
      );

      expect(projection.displayText, '😀');
      expect(projection.displayLengthUtf16, 2);
      expect(
        (
          projection.runs.single.sourceStartUtf16,
          projection.runs.single.sourceEndUtf16,
        ),
        (1, 3),
      );
      expect(projection.sourceToDisplayOffset(1), 0);
      expect(projection.sourceToDisplayOffset(2), 1);
      expect(projection.sourceToDisplayOffset(3), 2);
      expect(
        projection.displayToSourceOffset(
          1,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        2,
      );
      expect(
        projection.displayToSourceOffset(
          2,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        3,
      );
      expect(
        projection.displayToSourceOffset(
          2,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        4,
      );
    });

    test('adjacent code is a semantic style and never rewrites source', () {
      final projection = _projection(
        '*a*` b\n `',
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
          _record(
            kind: 3,
            flags: 3,
            start: 3,
            length: 6,
            contentStart: 4,
            contentLength: 4,
          ),
        ],
      );

      expect(projection.displayText, 'a b\n ');
      expect(projection.runs.map((run) => run.text), ['a', ' b\n ']);
      expect(projection.runs[0].semanticStyles, [
        FlarkV3InlineFactKind.emphasis,
      ]);
      expect(projection.runs[1].semanticStyles, [FlarkV3InlineFactKind.code]);
      final certifiedCode = projection.runs[1].semanticFacts.single;
      expect(certifiedCode.normalizesCodeLineEndings, isTrue);
      expect(certifiedCode.trimsOneCodeEdgeSpace, isTrue);
    });

    test('unsupported leaf ignores a request to hide markers', () {
      const source = '*literal*';
      final projection = _projection(
        source,
        disposition: FlarkV3InlineFactsDisposition.unsupported,
        markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
      );

      expect(projection.displayText, source);
      expect(projection.runs.single.text, source);
      expect(projection.runs.single.semanticStyles, isEmpty);
      expect(projection.sourceToDisplayOffset(source.length), source.length);
    });

    test('valid nested overlaps use stable outer-to-inner parser order', () {
      final projection = _projection(
        '***x***',
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 2,
            start: 1,
            length: 5,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
      );

      expect(projection.runs.map((run) => run.semanticStyles).toList(), [
        <FlarkV3InlineFactKind>[],
        [FlarkV3InlineFactKind.emphasis],
        [FlarkV3InlineFactKind.emphasis, FlarkV3InlineFactKind.strong],
        [FlarkV3InlineFactKind.emphasis],
        <FlarkV3InlineFactKind>[],
      ]);
    });

    test('requires the exact source authority that validated the facts', () {
      const source = '*x*';
      final authority = _authority(source);
      final facts = _decode(
        authority,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
        ],
      );
      final foreignVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(9, 8, 7, 6),
        document: authority.document,
      );

      expect(
        () => FlarkV3InlineProjection.fromValidatedFacts(
          sourceDocument: authority.document,
          expectedSource: foreignVersion,
          facts: facts,
        ),
        throwsA(isA<FlarkV3InlineProjectionException>()),
      );
    });

    test(
      'dense 8 KiB leaf uses a linear semantic sweep and one source read',
      () {
        const factCount = 2048;
        final source = List<String>.filled(factCount, '*a* ').join();
        final records = <Uint8List>[
          for (var index = 0; index < factCount; index += 1)
            _record(
              kind: 1,
              start: index * 4,
              length: 3,
              contentStart: index * 4 + 1,
              contentLength: 1,
            ),
        ];

        final projection = _projection(
          source,
          markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          records: records,
        );

        expect(source.length, FlarkV3InlineFactsDecoder.maximumLeafBytes);
        expect(
          projection.displayText,
          List<String>.filled(factCount, 'a ').join(),
        );
        expect(projection.work.factStartEventsApplied, factCount);
        expect(projection.work.factEndEventsApplied, factCount);
        expect(projection.work.sourceLeafReads, 1);
        expect(projection.work.sourceSlices, projection.runs.length);
        expect(projection.work.semanticStackNodesAllocated, factCount);
        expect(
          projection.work.runStackReferencesStored,
          projection.runs.length,
        );
        expect(projection.work.logicalSemanticDepthSum, factCount);
        expect(
          projection.work.styleBoundaryComparisons,
          lessThanOrEqualTo(
            2 *
                (projection.work.boundaryPointsVisited +
                    projection.work.factStartEventsApplied),
          ),
        );
        expect(
          projection.work.styleBoundaryComparisons,
          lessThan(factCount * 10),
        );
      },
    );

    test('maximally nested leaf shares stacks instead of copying depth', () {
      const depth = (FlarkV3InlineFactsDecoder.maximumLeafBytes - 1) ~/ 2;
      final markers = List<String>.filled(depth, '*').join();
      final source = '${markers}x$markers';
      final records = <Uint8List>[
        for (var level = 0; level < depth; level += 1)
          _record(
            kind: 1,
            start: level,
            length: source.length - level * 2,
            contentStart: level + 1,
            contentLength: source.length - level * 2 - 2,
          ),
      ];

      final projection = _projection(source, records: records);

      expect(source.length, 8191);
      expect(projection.displayText, source);
      expect(projection.work.factStartEventsApplied, depth);
      expect(projection.work.factEndEventsApplied, depth);
      expect(projection.work.semanticStackNodesAllocated, depth);
      expect(projection.work.runStackReferencesStored, projection.runs.length);
      expect(projection.runs, hasLength(source.length));
      // A copied-list design would materialize this many fact references.
      // The persistent design records the logical depth in O(1) per run.
      expect(projection.work.logicalSemanticDepthSum, depth * depth);
      expect(projection.work.sourceLeafReads, 1);
      expect(
        projection.work.styleBoundaryComparisons,
        lessThanOrEqualTo(
          2 *
              (projection.work.boundaryPointsVisited +
                  projection.work.factStartEventsApplied),
        ),
      );
    });
  });
}

FlarkV3InlineProjection _projection(
  String source, {
  FlarkV3InlineFactsDisposition disposition =
      FlarkV3InlineFactsDisposition.authoritative,
  FlarkV3InlineMarkerPolicy markerPolicy =
      FlarkV3InlineMarkerPolicy.allMarkersVisible,
  List<Uint8List> records = const [],
  FlarkV3InlineValuesPayload? inlineValues,
}) {
  final authority = _authority(source);
  final facts = _decode(
    authority,
    disposition: disposition,
    records: records,
    inlineValues: inlineValues,
  );
  return FlarkV3InlineProjection.fromValidatedFacts(
    sourceDocument: authority.document,
    expectedSource: authority.version,
    facts: facts,
    markerPolicy: markerPolicy,
  );
}

FlarkV3InlineFacts _decode(
  ({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) authority, {
  FlarkV3InlineFactsDisposition disposition =
      FlarkV3InlineFactsDisposition.authoritative,
  List<Uint8List> records = const [],
  FlarkV3InlineValuesPayload? inlineValues,
}) {
  final leaf = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: authority.document.utf8Length,
    startUtf16: 0,
    endUtf16: authority.document.utf16Length,
  );
  return FlarkV3InlineFactsDecoder.decode(
    sourceDocument: authority.document,
    expectedSource: authority.version,
    factSource: authority.version,
    expectedProfilePartition: 3,
    profilePartition: 3,
    expectedLeaf: leaf,
    factLeaf: leaf,
    disposition: disposition,
    factCount: records.length,
    encodedFacts: Uint8List.fromList([for (final record in records) ...record]),
    inlineValues: inlineValues,
  );
}

Uint8List _record({
  required int kind,
  int flags = 0,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, kind)
    ..setUint8(1, flags)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

Uint8List _characterReferenceRecord({
  required int start,
  required int length,
  required int first,
  int? second,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 9)
    ..setUint8(1, second == null ? 1 : 2)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, first, Endian.little)
    ..setUint32(16, second ?? 0, Endian.little);
  return bytes;
}

FlarkV3InlineValuesPayload _inlineValuesPayload(
  String source, {
  required List<_ValueEntry> entries,
}) {
  final authority = _authority(source);
  return FlarkV3InlineValuesPayload(
    sourceVersion: authority.version,
    profilePartition: 3,
    source: FlarkV3SourceSpan(
      startUtf8: 0,
      endUtf8: authority.document.utf8Length,
      startUtf16: 0,
      endUtf16: authority.document.utf16Length,
    ),
    encodedBytes: _encodeInlineValues(entries),
  );
}

Uint8List _encodeInlineValues(List<_ValueEntry> entries) {
  final cookedEntries = <({Uint8List destination, Uint8List title})>[
    for (final entry in entries)
      (
        destination: Uint8List.fromList(utf8.encode(entry.cookedDestination)),
        title: Uint8List.fromList(utf8.encode(entry.cookedTitle ?? '')),
      ),
  ];
  final encoded = Uint8List(
    16 +
        entries.length * 32 +
        cookedEntries.fold(
          0,
          (sum, value) => sum + value.destination.length + value.title.length,
        ),
  );
  encoded.setRange(0, 8, ascii.encode('FLKIV001'));
  final data = ByteData.sublistView(encoded);
  data
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, entries.length, Endian.little);
  var offset = 16;
  for (var index = 0; index < entries.length; index += 1) {
    final entry = entries[index];
    final cooked = cookedEntries[index];
    data
      ..setUint32(offset, entry.parentFactOrdinal, Endian.little)
      ..setUint32(offset + 4, entry.cookedTitle == null ? 0 : 1, Endian.little)
      ..setUint32(offset + 8, entry.destinationStart, Endian.little)
      ..setUint32(offset + 12, entry.destinationLength, Endian.little)
      ..setUint32(offset + 16, entry.titleStart, Endian.little)
      ..setUint32(offset + 20, entry.titleLength, Endian.little)
      ..setUint32(offset + 24, cooked.destination.length, Endian.little)
      ..setUint32(offset + 28, cooked.title.length, Endian.little);
    offset += 32;
    encoded.setRange(
      offset,
      offset + cooked.destination.length,
      cooked.destination,
    );
    offset += cooked.destination.length;
    encoded.setRange(offset, offset + cooked.title.length, cooked.title);
    offset += cooked.title.length;
  }
  return encoded;
}

final class _ValueEntry {
  const _ValueEntry({
    required this.parentFactOrdinal,
    required this.destinationStart,
    required this.destinationLength,
    this.titleStart = 0,
    this.titleLength = 0,
    required this.cookedDestination,
    this.cookedTitle,
  });

  final int parentFactOrdinal;
  final int destinationStart;
  final int destinationLength;
  final int titleStart;
  final int titleLength;
  final String cookedDestination;
  final String? cookedTitle;
}

({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) _authority(
  String source,
) {
  final document = FlarkV3SourceDocument.fromString(source);
  return (
    document: document,
    version: FlarkV3SourceVersion.fromDocument(
      documentSession: _documentSession,
      document: document,
    ),
  );
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
