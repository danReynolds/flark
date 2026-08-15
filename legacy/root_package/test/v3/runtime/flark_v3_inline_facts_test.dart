import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3InlineFactsDecoder', () {
    test('decodes the Rust 20-byte little-endian record layout', () {
      const source = '***x***';
      final decoded = _decode(
        source,
        records: [
          Uint8List.fromList(const [
            1, 0, 0, 0, // kind, flags, reserved
            0, 0, 0, 0, // relative start
            7, 0, 0, 0, // relative length
            1, 0, 0, 0, // content start
            5, 0, 0, 0, // content length
          ]),
        ],
      );

      expect(decoded.facts.single.kind, FlarkV3InlineFactKind.emphasis);
      _expectUtf8(decoded.facts.single.source, 0, 7);
      _expectUtf8(decoded.facts.single.content, 1, 6);
    });

    test('decodes nested ***x*** facts in parser preorder', () {
      const source = '***x***';
      final decoded = _decode(
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

      expect(decoded.sourceRevision, 0);
      expect(decoded.profilePartition, 3);
      expect(decoded.disposition, FlarkV3InlineFactsDisposition.authoritative);
      expect(decoded.facts, hasLength(2));
      expect(decoded.facts[0].kind, FlarkV3InlineFactKind.emphasis);
      _expectUtf8(decoded.facts[0].source, 0, 7);
      _expectUtf8(decoded.facts[0].opener, 0, 1);
      _expectUtf8(decoded.facts[0].content, 1, 6);
      _expectUtf8(decoded.facts[0].closer, 6, 7);
      expect(decoded.facts[1].kind, FlarkV3InlineFactKind.strong);
      _expectUtf8(decoded.facts[1].source, 1, 6);
      _expectUtf8(decoded.facts[1].opener, 1, 3);
      _expectUtf8(decoded.facts[1].content, 3, 4);
      _expectUtf8(decoded.facts[1].closer, 4, 6);
    });

    test('accepts adjacent non-overlapping facts', () {
      const source = '*a***b**';
      final decoded = _decode(
        source,
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

      expect(decoded.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.strong,
      ]);
      _expectUtf8(decoded.facts[0].source, 0, 3);
      _expectUtf8(decoded.facts[1].source, 3, 8);
    });

    test('decodes only the two canonical code flags', () {
      const source = '` a\n `';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 3,
            flags: 3,
            start: 0,
            length: 6,
            contentStart: 1,
            contentLength: 4,
          ),
        ],
      );

      final fact = decoded.facts.single;
      expect(fact.kind, FlarkV3InlineFactKind.code);
      expect(fact.normalizesCodeLineEndings, isTrue);
      expect(fact.trimsOneCodeEdgeSpace, isTrue);
      _expectUtf8(fact.content, 1, 5);
    });

    test('decodes canonical one- and two-tilde strikethrough facts', () {
      const source = '~a~ ~~b~~';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 4,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
          _record(
            kind: 4,
            start: 4,
            length: 5,
            contentStart: 6,
            contentLength: 1,
          ),
        ],
      );

      expect(decoded.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.strikethrough,
        FlarkV3InlineFactKind.strikethrough,
      ]);
      _expectUtf8(decoded.facts[0].opener, 0, 1);
      _expectUtf8(decoded.facts[0].closer, 2, 3);
      _expectUtf8(decoded.facts[1].opener, 4, 6);
      _expectUtf8(decoded.facts[1].closer, 7, 9);
    });

    test('decodes parser-owned URI and email autolink targets exactly', () {
      const uri = r'https://example.test/\[/%"?a=1&b=2';
      const email = 'me@example.test';
      const source = '<$uri> <$email>';
      final emailStart = uri.length + 3;
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 5,
            start: 0,
            length: uri.length + 2,
            contentStart: 1,
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

      final uriFact = decoded.facts[0];
      expect(uriFact.kind, FlarkV3InlineFactKind.autolinkUri);
      expect(uriFact.normalizesCodeLineEndings, isFalse);
      expect(uriFact.trimsOneCodeEdgeSpace, isFalse);
      expect(uriFact.linkAnnotation?.kind, FlarkV3InlineLinkKind.uri);
      expect(
        uriFact.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.exactContent,
      );
      expect(uriFact.linkAnnotation?.destination, uri);
      expect(uriFact.linkAnnotation?.source, same(uriFact.source));
      expect(uriFact.linkAnnotation?.content, same(uriFact.content));
      _expectUtf8(uriFact.opener, 0, 1);
      _expectUtf8(uriFact.closer, uri.length + 1, uri.length + 2);

      final emailFact = decoded.facts[1];
      expect(emailFact.kind, FlarkV3InlineFactKind.autolinkEmail);
      expect(emailFact.linkAnnotation?.kind, FlarkV3InlineLinkKind.email);
      expect(
        emailFact.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
      );
      expect(emailFact.linkAnnotation?.destination, 'mailto:$email');
      expect(emailFact.linkAnnotation?.source, same(emailFact.source));
      expect(emailFact.linkAnnotation?.content, same(emailFact.content));
    });

    test('decodes markerless GFM URI, www, and email autolinks exactly', () {
      const scheme = 'https://example.test/a';
      const www = 'www.example.test/b';
      const email = 'me@example.test';
      const source = 'before $scheme $www $email after';
      final schemeStart = source.indexOf(scheme);
      final wwwStart = source.indexOf(www);
      final emailStart = source.indexOf(email);
      final decoded = _decode(
        source,
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

      final schemeFact = decoded.facts[0];
      expect(schemeFact.linkAnnotation?.destination, scheme);
      expect(
        schemeFact.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.exactContent,
      );
      _expectUtf8(schemeFact.source, schemeStart, schemeStart + scheme.length);
      _expectUtf8(schemeFact.content, schemeStart, schemeStart + scheme.length);
      _expectUtf8(schemeFact.opener, schemeStart, schemeStart);
      _expectUtf8(
        schemeFact.closer,
        schemeStart + scheme.length,
        schemeStart + scheme.length,
      );

      final wwwFact = decoded.facts[1];
      expect(wwwFact.linkAnnotation?.destination, 'http://$www');
      expect(
        wwwFact.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent,
      );
      _expectUtf8(wwwFact.source, wwwStart, wwwStart + www.length);
      _expectUtf8(wwwFact.content, wwwStart, wwwStart + www.length);
      _expectUtf8(wwwFact.opener, wwwStart, wwwStart);
      _expectUtf8(wwwFact.closer, wwwStart + www.length, wwwStart + www.length);

      final emailFact = decoded.facts[2];
      expect(emailFact.linkAnnotation?.destination, 'mailto:$email');
      expect(
        emailFact.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
      );
      _expectUtf8(emailFact.source, emailStart, emailStart + email.length);
      _expectUtf8(emailFact.content, emailStart, emailStart + email.length);
      _expectUtf8(emailFact.opener, emailStart, emailStart);
      _expectUtf8(
        emailFact.closer,
        emailStart + email.length,
        emailStart + email.length,
      );
    });

    test('rejects noncanonical markerless-autolink flags and geometry', () {
      for (final fixture in <({String source, Uint8List record})>[
        (
          source: '<www.example.test>',
          record: _record(
            kind: 5,
            flags: 1,
            start: 0,
            length: 18,
            contentStart: 1,
            contentLength: 16,
          ),
        ),
        (
          source: 'me@example.test',
          record: _record(
            kind: 6,
            flags: 1,
            start: 0,
            length: 15,
            contentStart: 0,
            contentLength: 15,
          ),
        ),
        (
          source: 'www.example.test',
          record: _record(
            kind: 5,
            flags: 2,
            start: 0,
            length: 16,
            contentStart: 0,
            contentLength: 16,
          ),
        ),
        (
          source: 'www.example.test>',
          record: _record(
            kind: 5,
            start: 0,
            length: 17,
            contentStart: 0,
            contentLength: 16,
          ),
        ),
        (
          source: '<www.example.test',
          record: _record(
            kind: 5,
            start: 0,
            length: 17,
            contentStart: 1,
            contentLength: 16,
          ),
        ),
      ]) {
        expect(
          () => _decode(fixture.source, records: [fixture.record]),
          _throwsDecode,
        );
      }
    });

    test('rejects character-reference children under a markerless URI', () {
      const source = 'https://e.test/?q=&amp;';
      final entityStart = source.indexOf('&');
      expect(
        () => _decode(
          source,
          records: [
            _record(
              kind: 5,
              start: 0,
              length: source.length,
              contentStart: 0,
              contentLength: source.length,
            ),
            _characterReferenceRecord(
              start: entityStart,
              length: 5,
              first: 0x26,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test(
      'projects certified character-reference children into a URI target',
      () {
        const source = '<http://example.test/?x=&amp;&y=&ngE;>';
        final entityStart = source.indexOf('&');
        final secondEntityStart = source.indexOf('&ngE;');
        final decoded = _decode(
          source,
          records: [
            _record(
              kind: 5,
              start: 0,
              length: source.length,
              contentStart: 1,
              contentLength: source.length - 2,
            ),
            _characterReferenceRecord(
              start: entityStart,
              length: 5,
              first: 0x26,
            ),
            _characterReferenceRecord(
              start: secondEntityStart,
              length: 5,
              first: 0x2267,
              second: 0x0338,
            ),
          ],
        );

        expect(decoded.facts.map((fact) => fact.kind), [
          FlarkV3InlineFactKind.autolinkUri,
          FlarkV3InlineFactKind.characterReference,
          FlarkV3InlineFactKind.characterReference,
        ]);
        final annotation = decoded.facts.first.linkAnnotation!;
        expect(annotation.kind, FlarkV3InlineLinkKind.uri);
        expect(
          annotation.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.characterReferenceProjectedContent,
        );
        expect(annotation.destination, 'http://example.test/?x=&&y=≧\u{338}');
        expect(
          decoded.facts.skip(1).map((fact) => fact.characterReferenceValue),
          ['&', '≧\u{338}'],
          reason:
              'the target and visible label share one parser-authored cooked '
              'value',
        );
      },
    );

    test('rejects a character-reference child under an email autolink', () {
      const source = '<a&copy;@b>';
      final entityStart = source.indexOf('&');
      expect(
        () => _decode(
          source,
          records: [
            _record(
              kind: 6,
              start: 0,
              length: source.length,
              contentStart: 1,
              contentLength: source.length - 2,
            ),
            _characterReferenceRecord(
              start: entityStart,
              length: 6,
              first: 0xA9,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test(
      'joins direct link and image values by raw ordinal with title presence',
      () {
        const source = '[label](<> "") ![alt]()';
        final decoded = _decode(
          source,
          records: [
            _record(
              kind: 10,
              start: 0,
              length: 14,
              contentStart: 1,
              contentLength: 5,
            ),
            _record(
              kind: 11,
              start: 15,
              length: 8,
              contentStart: 17,
              contentLength: 3,
            ),
          ],
          inlineValues: _inlineValuesPayload(
            source,
            entries: const [
              _ValueEntry(
                parentFactOrdinal: 0,
                destinationStart: 9,
                destinationLength: 0,
                titleStart: 11,
                titleLength: 2,
                cookedDestination: '',
                cookedTitle: '',
              ),
              _ValueEntry(
                parentFactOrdinal: 1,
                destinationStart: 22,
                destinationLength: 0,
                cookedDestination: '',
              ),
            ],
          ),
        );

        final link = decoded.facts[0];
        expect(link.kind, FlarkV3InlineFactKind.directLink);
        expect(link.imageAnnotation, isNull);
        expect(link.linkAnnotation?.kind, FlarkV3InlineLinkKind.direct);
        expect(
          link.linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        );
        expect(link.linkAnnotation?.destination, '');
        expect(link.linkAnnotation?.title, '');
        _expectUtf8(link.linkAnnotation!.destinationSource, 9, 9);
        _expectUtf8(link.linkAnnotation!.titleSource!, 11, 13);

        final image = decoded.facts[1];
        expect(image.kind, FlarkV3InlineFactKind.directImage);
        expect(image.linkAnnotation, isNull);
        expect(image.imageAnnotation?.destination, '');
        expect(image.imageAnnotation?.title, isNull);
        expect(image.imageAnnotation?.titleSource, isNull);
        _expectUtf8(image.imageAnnotation!.destinationSource, 22, 22);
      },
    );

    test(
      'joins reference link and image values using document-absolute cuts',
      () {
        const source =
            '[label] ![alt]\n\n[label]: destination "title"\n[alt]: image.png';
        final authority = _authority(source);
        final inlineEnd = source.indexOf('\n');
        final leaf = FlarkV3SourceSpan(
          startUtf8: 0,
          endUtf8: inlineEnd,
          startUtf16: 0,
          endUtf16: inlineEnd,
        );
        final destinationStart = source.indexOf('destination');
        final titleStart = source.indexOf('"title"');
        final imageStart = source.indexOf('image.png');
        final decoded = FlarkV3InlineFactsDecoder.decode(
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
              kind: 12,
              start: 0,
              length: 7,
              contentStart: 1,
              contentLength: 5,
            ),
            ..._record(
              kind: 13,
              start: 8,
              length: 6,
              contentStart: 10,
              contentLength: 3,
            ),
          ]),
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
              _ValueEntry(
                parentFactOrdinal: 1,
                destinationStart: imageStart,
                destinationLength: 'image.png'.length,
                cookedDestination: 'image.png',
              ),
            ]),
          ),
        );

        final link = decoded.facts[0];
        expect(link.kind, FlarkV3InlineFactKind.referenceLink);
        expect(link.linkAnnotation?.kind, FlarkV3InlineLinkKind.reference);
        expect(
          link.linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        );
        expect(link.linkAnnotation?.destination, 'destination');
        expect(link.linkAnnotation?.title, 'title');
        _expectUtf8(
          link.linkAnnotation!.destinationSource,
          destinationStart,
          destinationStart + 'destination'.length,
        );
        _expectUtf8(
          link.linkAnnotation!.titleSource!,
          titleStart,
          titleStart + '"title"'.length,
        );
        expect(
          link.linkAnnotation!.destinationSource.startUtf8,
          greaterThan(leaf.endUtf8),
          reason:
              'reference values name their winning definition, not a leaf cut',
        );

        final image = decoded.facts[1];
        expect(image.kind, FlarkV3InlineFactKind.referenceImage);
        expect(image.linkAnnotation, isNull);
        expect(image.imageAnnotation?.destination, 'image.png');
        _expectUtf8(
          image.imageAnnotation!.destinationSource,
          imageStart,
          imageStart + 'image.png'.length,
        );
      },
    );

    test('keeps reference value cuts absolute for a nonzero leaf', () {
      const source = 'prefix\n[x]\n\n[x]: destination';
      final authority = _authority(source);
      const leaf = FlarkV3SourceSpan(
        startUtf8: 7,
        endUtf8: 10,
        startUtf16: 7,
        endUtf16: 10,
      );
      final destinationStart = source.indexOf('destination');
      final decoded = FlarkV3InlineFactsDecoder.decode(
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
          length: 3,
          contentStart: 1,
          contentLength: 1,
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
              cookedDestination: 'destination',
            ),
          ]),
        ),
      );

      _expectUtf8(
        decoded.facts.single.linkAnnotation!.destinationSource,
        destinationStart,
        destinationStart + 'destination'.length,
      );
    });

    test('rejects reference value cuts outside exact document source', () {
      const source = '[x]\n\n[x]: dest';
      final authority = _authority(source);
      const leaf = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: 3,
        startUtf16: 0,
        endUtf16: 3,
      );
      final encodedValues = _encodeInlineValues([
        _ValueEntry(
          parentFactOrdinal: 0,
          destinationStart: utf8.encode(source).length,
          destinationLength: 1,
          cookedDestination: 'forged',
        ),
      ]);

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
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
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
          inlineValues: FlarkV3InlineValuesPayload(
            sourceVersion: authority.version,
            profilePartition: 3,
            source: leaf,
            encodedBytes: encodedValues,
          ),
        ),
        _throwsDecode,
      );
    });

    test('trusts parser geometry and values without recognizing Markdown', () {
      const source = 'abcdefghij';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 10,
            start: 0,
            length: 10,
            contentStart: 1,
            contentLength: 4,
          ),
        ],
        inlineValues: _inlineValuesPayload(
          source,
          entries: const [
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 6,
              destinationLength: 2,
              titleStart: 8,
              titleLength: 1,
              cookedDestination: 'parser-owned💡',
              cookedTitle: 'parser-title',
            ),
          ],
        ),
      );

      expect(decoded.facts.single.kind, FlarkV3InlineFactKind.directLink);
      expect(
        decoded.facts.single.linkAnnotation?.destination,
        'parser-owned💡',
      );
      expect(decoded.facts.single.linkAnnotation?.title, 'parser-title');
      _expectUtf8(decoded.facts.single.linkAnnotation!.destinationSource, 6, 8);
    });

    test('maps companion source cuts through exact UTF-8/UTF-16 authority', () {
      const source = '😀[x](é "t")';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 10,
            start: 4,
            length: 11,
            contentStart: 5,
            contentLength: 1,
          ),
        ],
        inlineValues: _inlineValuesPayload(
          source,
          entries: const [
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 8,
              destinationLength: 2,
              titleStart: 11,
              titleLength: 3,
              cookedDestination: 'é',
              cookedTitle: 't',
            ),
          ],
        ),
      );

      final annotation = decoded.facts.single.linkAnnotation!;
      _expectUtf8(annotation.destinationSource, 8, 10);
      expect(
        (
          annotation.destinationSource.startUtf16,
          annotation.destinationSource.endUtf16,
        ),
        (6, 7),
      );
      _expectUtf8(annotation.titleSource!, 11, 14);
      expect(
        (annotation.titleSource!.startUtf16, annotation.titleSource!.endUtf16),
        (8, 11),
      );
    });

    test('keeps direct value cuts leaf-relative for a nonzero leaf', () {
      const source = 'prefix\n[x](d)';
      final authority = _authority(source);
      const leaf = FlarkV3SourceSpan(
        startUtf8: 7,
        endUtf8: 13,
        startUtf16: 7,
        endUtf16: 13,
      );
      final decoded = FlarkV3InlineFactsDecoder.decode(
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
          kind: 10,
          start: 0,
          length: 6,
          contentStart: 1,
          contentLength: 1,
        ),
        inlineValues: FlarkV3InlineValuesPayload(
          sourceVersion: authority.version,
          profilePartition: 3,
          source: leaf,
          encodedBytes: _encodeInlineValues(const [
            _ValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 4,
              destinationLength: 1,
              cookedDestination: 'd',
            ),
          ]),
        ),
      );

      _expectUtf8(
        decoded.facts.single.linkAnnotation!.destinationSource,
        11,
        12,
      );
    });

    test('accepts a canonical zero-entry companion without direct facts', () {
      const source = '*x*';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
        ],
        inlineValues: _inlineValuesPayload(source, entries: const []),
      );

      expect(decoded.facts.single.kind, FlarkV3InlineFactKind.emphasis);
    });

    test('rejects an actionable link nested inside a direct link label', () {
      const source = '[[x](u)](v)';
      expect(
        () => _decode(
          source,
          records: [
            _record(
              kind: 10,
              start: 0,
              length: 11,
              contentStart: 1,
              contentLength: 6,
            ),
            _record(
              kind: 10,
              start: 1,
              length: 6,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
          inlineValues: _inlineValuesPayload(
            source,
            entries: const [
              _ValueEntry(
                parentFactOrdinal: 0,
                destinationStart: 9,
                destinationLength: 1,
                cookedDestination: 'v',
              ),
              _ValueEntry(
                parentFactOrdinal: 1,
                destinationStart: 5,
                destinationLength: 1,
                cookedDestination: 'u',
              ),
            ],
          ),
        ),
        _throwsDecode,
      );
    });

    test('rejects an actionable link nested inside a reference link label', () {
      expect(
        () => _decode(
          '[[x]]',
          records: [
            _record(
              kind: 12,
              start: 0,
              length: 5,
              contentStart: 1,
              contentLength: 3,
            ),
            _record(
              kind: 12,
              start: 1,
              length: 3,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test(
      'rejects missing duplicate orphan and authority-mismatched companions',
      () {
        const directSource = '[x](u)';
        final directRecord = _record(
          kind: 10,
          start: 0,
          length: 6,
          contentStart: 1,
          contentLength: 1,
        );
        expect(
          () => _decode(directSource, records: [directRecord]),
          _throwsDecode,
        );
        expect(
          () => _decode(
            directSource,
            records: [
              _record(
                kind: 12,
                start: 0,
                length: 3,
                contentStart: 1,
                contentLength: 1,
              ),
            ],
          ),
          _throwsDecode,
          reason: 'reference link missing its companion',
        );

        const twoSource = '[x](u)![y](v)';
        final twoRecords = [
          directRecord,
          _record(
            kind: 11,
            start: 6,
            length: 7,
            contentStart: 8,
            contentLength: 1,
          ),
        ];
        expect(
          () => _decode(
            twoSource,
            records: twoRecords,
            inlineValues: _inlineValuesPayload(
              twoSource,
              entries: const [
                _ValueEntry(
                  parentFactOrdinal: 0,
                  destinationStart: 4,
                  destinationLength: 1,
                  cookedDestination: 'u',
                ),
                _ValueEntry(
                  parentFactOrdinal: 0,
                  destinationStart: 11,
                  destinationLength: 1,
                  cookedDestination: 'v',
                ),
              ],
            ),
          ),
          _throwsDecode,
          reason: 'duplicate parent ordinal',
        );

        expect(
          () => _decode(
            '*x*',
            records: [
              _record(
                kind: 1,
                start: 0,
                length: 3,
                contentStart: 1,
                contentLength: 1,
              ),
            ],
            inlineValues: _inlineValuesPayload(
              '*x*',
              entries: const [
                _ValueEntry(
                  parentFactOrdinal: 0,
                  destinationStart: 1,
                  destinationLength: 0,
                  cookedDestination: 'orphan',
                ),
              ],
            ),
          ),
          _throwsDecode,
          reason: 'orphan value entry',
        );

        final targetAuthority = _authority(directSource);
        final wrongAuthority = _authority('[y](u)');
        expect(
          () => FlarkV3InlineFactsDecoder.decode(
            sourceDocument: targetAuthority.document,
            expectedSource: targetAuthority.version,
            factSource: targetAuthority.version,
            expectedProfilePartition: 3,
            profilePartition: 3,
            expectedLeaf: _sourceSpan(directSource),
            factLeaf: _sourceSpan(directSource),
            disposition: FlarkV3InlineFactsDisposition.authoritative,
            factCount: 1,
            encodedFacts: directRecord,
            inlineValues: FlarkV3InlineValuesPayload(
              sourceVersion: wrongAuthority.version,
              profilePartition: 3,
              source: _sourceSpan(directSource),
              encodedBytes: _encodeInlineValues(const [
                _ValueEntry(
                  parentFactOrdinal: 0,
                  destinationStart: 4,
                  destinationLength: 1,
                  cookedDestination: 'u',
                ),
              ]),
            ),
          ),
          _throwsDecode,
          reason: 'companion source authority mismatch',
        );
      },
    );

    test('rejects malformed FLKIV001 headers, metadata, and cooked UTF-8', () {
      const source = '[x](u)';
      final record = _record(
        kind: 10,
        start: 0,
        length: 6,
        contentStart: 1,
        contentLength: 1,
      );
      final validBytes = _encodeInlineValues(const [
        _ValueEntry(
          parentFactOrdinal: 0,
          destinationStart: 4,
          destinationLength: 1,
          cookedDestination: 'u',
        ),
      ]);
      FlarkV3InlineValuesPayload payload(Uint8List bytes) {
        final authority = _authority(source);
        return FlarkV3InlineValuesPayload(
          sourceVersion: authority.version,
          profilePartition: 3,
          source: _sourceSpan(source),
          encodedBytes: bytes,
        );
      }

      final badMagic = Uint8List.fromList(validBytes)..[0] = 0;
      final invalidUtf8 = Uint8List.fromList(validBytes)
        ..[validBytes.length - 1] = 0xFF;
      final absentTitleWithSource = _encodeInlineValues(const [
        _ValueEntry(
          parentFactOrdinal: 0,
          destinationStart: 4,
          destinationLength: 1,
          titleStart: 2,
          titleLength: 1,
          cookedDestination: 'u',
        ),
      ]);
      final outOfCloser = _encodeInlineValues(const [
        _ValueEntry(
          parentFactOrdinal: 0,
          destinationStart: 1,
          destinationLength: 1,
          cookedDestination: 'u',
        ),
      ]);
      final oversized = Uint8List(
        FlarkV3InlineFactsDecoder.maximumEncodedValueBytes + 1,
      );
      oversized.setRange(0, 8, ascii.encode('FLKIV001'));
      ByteData.sublistView(oversized)
        ..setUint32(8, 1, Endian.little)
        ..setUint32(12, 1, Endian.little);
      final tooManyEntries = Uint8List(16);
      tooManyEntries.setRange(0, 8, ascii.encode('FLKIV001'));
      ByteData.sublistView(tooManyEntries)
        ..setUint32(8, 1, Endian.little)
        ..setUint32(
          12,
          FlarkV3InlineFactsDecoder.maximumValueEntryCount + 1,
          Endian.little,
        );

      for (final bytes in [
        badMagic,
        invalidUtf8,
        absentTitleWithSource,
        outOfCloser,
        Uint8List.fromList([...validBytes, 0]),
        oversized,
        tooManyEntries,
      ]) {
        expect(
          () =>
              _decode(source, records: [record], inlineValues: payload(bytes)),
          _throwsDecode,
        );
      }
    });

    test(
      'decodes canonical escaped punctuation as collapsed-closer metadata',
      () {
        const source = r'\*';
        final decoded = _decode(
          source,
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

        final fact = decoded.facts.single;
        expect(fact.kind, FlarkV3InlineFactKind.escapedPunctuation);
        expect(fact.linkAnnotation, isNull);
        expect(fact.normalizesCodeLineEndings, isFalse);
        expect(fact.trimsOneCodeEdgeSpace, isFalse);
        _expectUtf8(fact.source, 0, 2);
        _expectUtf8(fact.opener, 0, 1);
        _expectUtf8(fact.content, 1, 2);
        _expectUtf8(fact.closer, 2, 2);
        expect(fact.closer.startUtf16, fact.closer.endUtf16);
      },
    );

    test('allows a certified escape as a non-container style child', () {
      const source = r'*\**';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 4,
            contentStart: 1,
            contentLength: 2,
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

      expect(decoded.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.escapedPunctuation,
      ]);
      _expectUtf8(decoded.facts[1].closer, 3, 3);
    });

    test('decodes canonical hard line breaks across physical endings', () {
      for (final fixture in <({String source, int markerBytes})>[
        (source: '  \n', markerBytes: 2),
        (source: '\\\n', markerBytes: 1),
        (source: '   \r', markerBytes: 3),
        (source: '\\\r', markerBytes: 1),
        (source: '  \r\n', markerBytes: 2),
        (source: '\\\r\n', markerBytes: 1),
      ]) {
        final sourceBytes = utf8.encode(fixture.source).length;
        final decoded = _decode(
          fixture.source,
          records: [
            _record(
              kind: 8,
              start: 0,
              length: sourceBytes,
              contentStart: fixture.markerBytes,
              contentLength: sourceBytes - fixture.markerBytes,
            ),
          ],
        );

        final fact = decoded.facts.single;
        expect(fact.kind, FlarkV3InlineFactKind.hardLineBreak);
        expect(fact.linkAnnotation, isNull);
        expect(fact.normalizesCodeLineEndings, isFalse);
        expect(fact.trimsOneCodeEdgeSpace, isFalse);
        _expectUtf8(fact.source, 0, sourceBytes);
        _expectUtf8(fact.opener, 0, fixture.markerBytes);
        _expectUtf8(fact.content, fixture.markerBytes, sourceBytes);
        _expectUtf8(fact.closer, sourceBytes, sourceBytes);
      }
    });

    test('allows a certified hard line break as a style child', () {
      const source = '*a  \nb*';
      final decoded = _decode(
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
            kind: 8,
            start: 2,
            length: 3,
            contentStart: 4,
            contentLength: 1,
          ),
        ],
      );

      expect(decoded.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.hardLineBreak,
      ]);
      _expectUtf8(decoded.facts[1].opener, 2, 4);
      _expectUtf8(decoded.facts[1].content, 4, 5);
      _expectUtf8(decoded.facts[1].closer, 5, 5);
    });

    test('decodes one- and two-scalar character-reference payloads', () {
      const source = '&amp; &acE; &#x1F600;';
      final decoded = _decode(
        source,
        records: [
          _characterReferenceRecord(start: 0, length: 5, first: 0x26),
          _characterReferenceRecord(
            start: 6,
            length: 5,
            first: 0x223E,
            second: 0x0333,
          ),
          _characterReferenceRecord(start: 12, length: 9, first: 0x1F600),
        ],
      );

      expect(
        decoded.facts.map((fact) => fact.kind),
        everyElement(FlarkV3InlineFactKind.characterReference),
      );
      expect(decoded.facts.map((fact) => fact.characterReferenceValue), [
        '&',
        '\u223E\u0333',
        '\u{1F600}',
      ]);
      for (final fact in decoded.facts) {
        expect(fact.linkAnnotation, isNull);
        expect(fact.normalizesCodeLineEndings, isFalse);
        expect(fact.trimsOneCodeEdgeSpace, isFalse);
        expect(
          (
            fact.content.startUtf8,
            fact.content.endUtf8,
            fact.content.startUtf16,
            fact.content.endUtf16,
          ),
          (
            fact.source.startUtf8,
            fact.source.endUtf8,
            fact.source.startUtf16,
            fact.source.endUtf16,
          ),
        );
        expect(fact.opener.startUtf16, fact.opener.endUtf16);
        expect(fact.opener.startUtf16, fact.source.startUtf16);
        expect(fact.closer.startUtf16, fact.closer.endUtf16);
        expect(fact.closer.startUtf16, fact.source.endUtf16);
      }
    });

    test('allows a character reference as a style child', () {
      const source = '*&amp;*';
      final decoded = _decode(
        source,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _characterReferenceRecord(start: 1, length: 5, first: 0x26),
        ],
      );

      expect(decoded.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.emphasis,
        FlarkV3InlineFactKind.characterReference,
      ]);
      expect(decoded.facts[1].characterReferenceValue, '&');
    });

    test('rejects malformed character-reference scalar payloads', () {
      for (final record in [
        _characterReferenceRecord(
          start: 0,
          length: 5,
          first: 0x26,
          scalarCount: 0,
        ),
        _characterReferenceRecord(
          start: 0,
          length: 5,
          first: 0x26,
          scalarCount: 3,
        ),
        _characterReferenceRecord(start: 0, length: 5, first: 0xD800),
        _characterReferenceRecord(start: 0, length: 5, first: 0x110000),
        _characterReferenceRecord(
          start: 0,
          length: 5,
          first: 0x26,
          second: 0x41,
          scalarCount: 1,
        ),
        _characterReferenceRecord(
          start: 0,
          length: 5,
          first: 0x26,
          second: 0,
          scalarCount: 2,
        ),
        _characterReferenceRecord(start: 0, length: 3, first: 0x26),
      ]) {
        expect(() => _decode('&amp;', records: [record]), _throwsDecode);
      }
      expect(
        () => _decode(
          'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
          records: [
            _characterReferenceRecord(start: 0, length: 34, first: 0x26),
          ],
        ),
        _throwsDecode,
      );
    });

    test('rejects a character reference used as a container', () {
      expect(
        () => _decode(
          '&amp;',
          records: [
            _characterReferenceRecord(start: 0, length: 5, first: 0x26),
            _record(
              kind: 1,
              start: 1,
              length: 3,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test('maps exact UTF-8 fact boundaries into UTF-16', () {
      const source = '*😀*';
      final decoded = _decode(
        source,
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

      final fact = decoded.facts.single;
      expect(
        (
          fact.source.startUtf8,
          fact.source.endUtf8,
          fact.source.startUtf16,
          fact.source.endUtf16,
        ),
        (0, 6, 0, 4),
      );
      expect(
        (
          fact.content.startUtf8,
          fact.content.endUtf8,
          fact.content.startUtf16,
          fact.content.endUtf16,
        ),
        (1, 5, 1, 3),
      );
    });

    test('translates leaf-relative records into exact document spans', () {
      const document = 'head ***x*** tail';
      final authority = _authority(document);
      const leaf = FlarkV3SourceSpan(
        startUtf8: 5,
        endUtf8: 12,
        startUtf16: 5,
        endUtf16: 12,
      );
      final decoded = FlarkV3InlineFactsDecoder.decode(
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
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          ..._record(
            kind: 2,
            start: 1,
            length: 5,
            contentStart: 3,
            contentLength: 1,
          ),
        ]),
      );

      expect(decoded.sourceVersion, authority.version);
      _expectUtf8(decoded.source, 5, 12);
      _expectUtf8(decoded.facts[0].source, 5, 12);
      _expectUtf8(decoded.facts[1].source, 6, 11);
    });

    test('rejects rebinding facts to another equal-length leaf', () {
      const document = '*a* xx *b*';
      final authority = _authority(document);
      const requestedLeaf = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: 3,
        startUtf16: 0,
        endUtf16: 3,
      );
      const foreignLeaf = FlarkV3SourceSpan(
        startUtf8: 7,
        endUtf8: 10,
        startUtf16: 7,
        endUtf16: 10,
      );

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: requestedLeaf,
          factLeaf: foreignLeaf,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 1,
          encodedFacts: _record(
            kind: 1,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
        ),
        _throwsDecode,
      );
    });

    test('whole-leaf unsupported accepts no plausible partial facts', () {
      final empty = _decode(
        '[label](url)',
        disposition: FlarkV3InlineFactsDisposition.unsupported,
      );
      expect(empty.facts, isEmpty);

      expect(
        () => _decode(
          '*a*',
          disposition: FlarkV3InlineFactsDisposition.unsupported,
          records: [
            _record(
              kind: 1,
              start: 0,
              length: 3,
              contentStart: 1,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test('rejects crossing and marker-overlapping ranges', () {
      expect(
        () => _decode(
          '*******',
          records: [
            _record(
              kind: 1,
              start: 0,
              length: 5,
              contentStart: 1,
              contentLength: 3,
            ),
            _record(
              kind: 2,
              start: 2,
              length: 5,
              contentStart: 4,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );

      expect(
        () => _decode(
          '*******',
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
              start: 0,
              length: 5,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );

      expect(
        () => _decode(
          '<*x*>',
          records: [
            _record(
              kind: 5,
              start: 0,
              length: 5,
              contentStart: 1,
              contentLength: 3,
            ),
            _record(
              kind: 1,
              start: 1,
              length: 3,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
        reason: 'angle autolinks cannot contain nested inline facts',
      );
    });

    test('rejects invalid ranges and noncanonical marker widths', () {
      for (final record in [
        _record(
          kind: 1,
          start: 0,
          length: 8,
          contentStart: 1,
          contentLength: 6,
        ),
        _record(
          kind: 1,
          start: 0,
          length: 4,
          contentStart: 2,
          contentLength: 1,
        ),
        _record(
          kind: 3,
          start: 0,
          length: 5,
          contentStart: 1,
          contentLength: 2,
        ),
        _record(
          kind: 4,
          start: 0,
          length: 4,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 5,
          start: 0,
          length: 4,
          contentStart: 2,
          contentLength: 1,
        ),
      ]) {
        expect(() => _decode('*******', records: [record]), _throwsDecode);
      }
    });

    test('rejects noncanonical escaped-punctuation geometry', () {
      for (final ({String source, Uint8List record}) fixture in [
        (
          source: r'\\*',
          record: _record(
            kind: 7,
            start: 0,
            length: 3,
            contentStart: 2,
            contentLength: 1,
          ),
        ),
        (
          source: r'\**',
          record: _record(
            kind: 7,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 1,
          ),
        ),
        (
          source: r'\**',
          record: _record(
            kind: 7,
            start: 0,
            length: 3,
            contentStart: 1,
            contentLength: 2,
          ),
        ),
        (
          source: r'\😀',
          record: _record(
            kind: 7,
            start: 0,
            length: 5,
            contentStart: 1,
            contentLength: 4,
          ),
        ),
        (
          source: '*a',
          record: _record(
            kind: 1,
            start: 0,
            length: 2,
            contentStart: 1,
            contentLength: 1,
          ),
        ),
      ]) {
        expect(
          () => _decode(fixture.source, records: [fixture.record]),
          _throwsDecode,
        );
      }
    });

    test('rejects noncanonical hard-line-break geometry', () {
      for (final ({String source, Uint8List record}) fixture in [
        (
          source: '\n',
          record: _record(
            kind: 8,
            start: 0,
            length: 1,
            contentStart: 0,
            contentLength: 1,
          ),
        ),
        (
          source: r'\',
          record: _record(
            kind: 8,
            start: 0,
            length: 1,
            contentStart: 1,
            contentLength: 0,
          ),
        ),
        (
          source: '  abc',
          record: _record(
            kind: 8,
            start: 0,
            length: 5,
            contentStart: 2,
            contentLength: 3,
          ),
        ),
        (
          source: '  \nx',
          record: _record(
            kind: 8,
            start: 0,
            length: 4,
            contentStart: 2,
            contentLength: 1,
          ),
        ),
        (
          source: '  \n',
          record: _record(
            kind: 8,
            flags: 1,
            start: 0,
            length: 3,
            contentStart: 2,
            contentLength: 1,
          ),
        ),
      ]) {
        expect(
          () => _decode(fixture.source, records: [fixture.record]),
          _throwsDecode,
        );
      }
    });

    test('rejects a hard line break used as a container', () {
      expect(
        () => _decode(
          r'  \*',
          records: [
            _record(
              kind: 8,
              start: 0,
              length: 4,
              contentStart: 2,
              contentLength: 2,
            ),
            _record(
              kind: 7,
              start: 2,
              length: 2,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
        ),
        _throwsDecode,
      );
    });

    test('rejects unknown kind, reserved bits, and invalid flags', () {
      for (final record in [
        _record(
          kind: 255,
          start: 0,
          length: 3,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 1,
          reserved: 1,
          start: 0,
          length: 3,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 1,
          flags: 1,
          start: 0,
          length: 3,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 3,
          flags: 4,
          start: 0,
          length: 3,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 6,
          flags: 1,
          start: 0,
          length: 3,
          contentStart: 1,
          contentLength: 1,
        ),
        _record(
          kind: 7,
          flags: 1,
          start: 0,
          length: 2,
          contentStart: 1,
          contentLength: 1,
        ),
      ]) {
        expect(() => _decode('*a*', records: [record]), _throwsDecode);
      }
    });

    test('rejects mismatched and over-cap fact counts', () {
      final authority = _authority('*a*');
      final sourceSpan = _sourceSpan('*a*');
      final record = _record(
        kind: 1,
        start: 0,
        length: 3,
        contentStart: 1,
        contentLength: 1,
      );

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: sourceSpan,
          factLeaf: sourceSpan,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 2,
          encodedFacts: record,
        ),
        _throwsDecode,
      );
      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: sourceSpan,
          factLeaf: sourceSpan,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: FlarkV3InlineFactsDecoder.maximumFactCount + 1,
          encodedFacts: Uint8List(0),
        ),
        _throwsDecode,
      );
    });

    test('rejects foreign exact-source authority and profile', () {
      const source = '*a*';
      final authority = _authority(source);
      final expected = authority.version;
      final foreignSameRevision = FlarkV3SourceVersion(
        documentSession: expected.documentSession,
        revision: expected.revision,
        metric: expected.metric,
        contentHash: const FlarkV3ContentHash128(20, 21, 22, 23),
      );
      final record = _record(
        kind: 1,
        start: 0,
        length: 3,
        contentStart: 1,
        contentLength: 1,
      );

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: expected,
          factSource: foreignSameRevision,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: _sourceSpan(source),
          factLeaf: _sourceSpan(source),
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 1,
          encodedFacts: record,
        ),
        _throwsDecode,
        reason: 'equal revision/metrics cannot authorize foreign source bytes',
      );
      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: expected,
          factSource: expected,
          expectedProfilePartition: 3,
          profilePartition: 4,
          expectedLeaf: _sourceSpan(source),
          factLeaf: _sourceSpan(source),
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 1,
          encodedFacts: record,
        ),
        _throwsDecode,
      );
    });

    test('rejects coordinate authority from another exact source', () {
      const source = '*😀*';
      final authority = _authority(source);
      final foreignDocument = FlarkV3SourceDocument.fromString('*😁*');

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: foreignDocument,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: _sourceSpan(source),
          factLeaf: _sourceSpan(source),
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 0,
          encodedFacts: Uint8List(0),
        ),
        _throwsDecode,
      );
    });

    test('rejects an impossible empty paragraph leaf', () {
      final authority = _authority('x');
      const emptyLeaf = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: 0,
        startUtf16: 0,
        endUtf16: 0,
      );

      expect(
        () => FlarkV3InlineFactsDecoder.decode(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          factSource: authority.version,
          expectedProfilePartition: 3,
          profilePartition: 3,
          expectedLeaf: emptyLeaf,
          factLeaf: emptyLeaf,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: 0,
          encodedFacts: Uint8List(0),
        ),
        _throwsDecode,
      );
    });
  });
}

final Matcher _throwsDecode = throwsA(isA<FlarkV3InlineFactsDecodeException>());

FlarkV3InlineFacts _decode(
  String source, {
  FlarkV3InlineFactsDisposition disposition =
      FlarkV3InlineFactsDisposition.authoritative,
  List<Uint8List> records = const [],
  FlarkV3InlineValuesPayload? inlineValues,
}) {
  final authority = _authority(source);
  final leaf = _sourceSpan(source);
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
  int reserved = 0,
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
    ..setUint16(2, reserved, Endian.little)
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
  int second = 0,
  int? scalarCount,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 9)
    ..setUint8(1, scalarCount ?? (second == 0 ? 1 : 2))
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, first, Endian.little)
    ..setUint32(16, second, Endian.little);
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
    source: _sourceSpan(source),
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

FlarkV3SourceSpan _sourceSpan(String source) => FlarkV3SourceSpan(
  startUtf8: 0,
  endUtf8: utf8.encode(source).length,
  startUtf16: 0,
  endUtf16: source.length,
);

void _expectUtf8(FlarkV3SourceSpan span, int start, int end) {
  expect((span.startUtf8, span.endUtf8), (start, end));
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
