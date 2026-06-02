import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/sovereign_test_paths.dart';

class _FakeNativeComrakBridge implements NativeComrakBridge {
  _FakeNativeComrakBridge(this.result);

  NativeComrakParseInput? lastInput;
  NativeComrakParseResult result;

  @override
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input) async {
    lastInput = input;
    return result;
  }
}

void main() {
  group('FlarkNativeComrakParseBackend', () {
    test('reports v2 parser capabilities', () {
      final backend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          const NativeComrakParseResult(revision: 0),
        ),
      );

      expect(backend.capabilities.parserName, 'comrak_native_v2_adapter');
      expect(
        backend.capabilities.schemaVersion,
        FlarkMarkdownParseProtocol.currentSchemaVersion,
      );
      expect(
        backend.capabilities.supports(FlarkMarkdownProfile.commonMarkGfm),
        isTrue,
      );
    });

    test('supports no-throw native backend probing for platform fallbacks', () {
      final missingPath =
          '${Directory.systemTemp.path}/sovereign_missing_comrak_bridge.dylib';

      final preflight = FlarkNativeComrakParseBackend.preflight(
        overrideLibraryPath: missingPath,
      );

      expect(preflight.isAvailable, isFalse);
      expect(
        preflight.error!.kind,
        NativeComrakBridgeLoadFailureKind.libraryNotFound,
      );
      expect(
        FlarkNativeComrakParseBackend.tryLoad(overrideLibraryPath: missingPath),
        isNull,
      );
    });

    test('marshals text and profile to the native bridge', () async {
      final bridge = _FakeNativeComrakBridge(
        const NativeComrakParseResult(revision: 9),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 9,
          markdown: '## hello',
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(bridge.lastInput, isNotNull);
      expect(bridge.lastInput!.revision, 9);
      expect(bridge.lastInput!.profile, NativeComrakProfile.commonMarkGfm);
      expect(utf8.decode(bridge.lastInput!.utf8Text), '## hello');
    });

    test('maps utf8 native ranges into v2 parse results', () async {
      const text = '🎨\n# **T**\n';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final bridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 12,
          blocks: [
            NativeComrakBlockSpan(
              type: 'header',
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(3),
                endByte: mapper.utf8OffsetForUtf16Offset(text.length),
              ),
              payload: const {'level': 1},
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(6),
                endByte: mapper.utf8OffsetForUtf16Offset(7),
              ),
              styles: const {'bold'},
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(3),
              endByte: mapper.utf8OffsetForUtf16Offset(4),
            ),
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(4),
              endByte: mapper.utf8OffsetForUtf16Offset(6),
            ),
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(7),
              endByte: mapper.utf8OffsetForUtf16Offset(9),
            ),
          ],
          exclusionRanges: [
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(3),
              endByte: mapper.utf8OffsetForUtf16Offset(text.length),
            ),
          ],
        ),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 12,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(
        result.schemaVersion,
        FlarkMarkdownParseProtocol.currentSchemaVersion,
      );
      expect(result.revision, 12);
      expect(result.sourceTextLength, text.length);
      expect(result.blocks.single.kind, FlarkMarkdownBlockKind.heading);
      expect(result.blocks.single.type, 'heading');
      expect(
        result.blocks.single.sourceRange,
        FlarkSourceRange(3, text.length),
      );
      expect(result.blocks.single.attributes['level'], 1);
      expect(result.inlineTokens.single.kind, FlarkMarkdownInlineKind.strong);
      expect(
        result.inlineTokens.single.sourceRange,
        const FlarkSourceRange(6, 7),
      );
      expect(result.hiddenRanges.map((range) => range.sourceRange), const [
        FlarkSourceRange(3, 4),
        FlarkSourceRange(4, 6),
        FlarkSourceRange(7, 9),
      ]);
      expect(result.extensions['nativeParser'], 'comrak');
      expect(result.extensions['nativeExclusionRanges'], [
        {'start': 3, 'end': text.length},
      ]);
    });

    test('keeps partial strong delimiter intent literal', () async {
      const text = '**wow*';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final bridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 13,
          blocks: [
            NativeComrakBlockSpan(
              type: 'paragraph',
              range: NativeComrakRange(
                startByte: 0,
                endByte: mapper.utf8OffsetForUtf16Offset(text.length),
              ),
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(1),
                endByte: mapper.utf8OffsetForUtf16Offset(text.length),
              ),
              styles: const {'italic'},
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(1),
              endByte: mapper.utf8OffsetForUtf16Offset(2),
            ),
            NativeComrakRange(
              startByte: mapper.utf8OffsetForUtf16Offset(5),
              endByte: mapper.utf8OffsetForUtf16Offset(6),
            ),
          ],
        ),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 13,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(result.inlineTokens, isEmpty);
      expect(result.hiddenRanges, isEmpty);
      expect(FlarkProjection.fromParseResult(result).projectText(text), text);
    });

    test(
      'maps native replacement ranges and filters hidden overlaps',
      () async {
        const text = 'A &amp; [x](/a&amp;b)';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final firstEntityStart = text.indexOf('&amp;');
        final secondEntityStart = text.lastIndexOf('&amp;');
        final linkStart = text.indexOf('[x]');
        final bridge = _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 15,
            blocks: [
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
            ],
            inlineTokens: [
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(linkStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
                styles: const {'link'},
                payload: const {'destination': '/a&amp;b', 'label': 'x'},
              ),
            ],
            replacementRanges: [
              NativeComrakReplacementRange(
                type: 'htmlEntity',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(firstEntityStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(
                    firstEntityStart + '&amp;'.length,
                  ),
                ),
                text: '&',
              ),
              NativeComrakReplacementRange(
                type: 'htmlEntity',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(secondEntityStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(
                    secondEntityStart + '&amp;'.length,
                  ),
                ),
                text: '&',
              ),
            ],
          ),
        );
        final backend = FlarkNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkCore,
          ),
        );

        expect(result.replacementRanges, hasLength(1));
        expect(
          result.replacementRanges.single.kind,
          FlarkMarkdownReplacementRangeKind.htmlEntity,
        );
        expect(
          result.replacementRanges.single.sourceRange,
          FlarkSourceRange(firstEntityStart, firstEntityStart + 5),
        );
        expect(result.replacementRanges.single.replacementText, '&');
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          'A & x',
        );
      },
    );

    test('keeps marker-only native blockquotes source-visible', () async {
      const markerOnlyText = '>';
      final markerOnlyMapper = FlarkUtf8Utf16Mapper(markerOnlyText);
      final markerOnlyBridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 16,
          blocks: [
            NativeComrakBlockSpan(
              type: 'blockquote',
              range: NativeComrakRange(
                startByte: 0,
                endByte: markerOnlyMapper.utf8OffsetForUtf16Offset(
                  markerOnlyText.length,
                ),
              ),
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: 0,
              endByte: markerOnlyMapper.utf8OffsetForUtf16Offset(
                markerOnlyText.length,
              ),
            ),
          ],
        ),
      );
      final markerOnlyBackend = FlarkNativeComrakParseBackend(
        bridge: markerOnlyBridge,
      );

      final markerOnly = await markerOnlyBackend.parse(
        const FlarkMarkdownParseRequest(
          revision: 16,
          markdown: markerOnlyText,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(markerOnly.blocks.single.kind, FlarkMarkdownBlockKind.paragraph);
      expect(markerOnly.hiddenRanges, isEmpty);
      expect(
        FlarkProjection.fromParseResult(markerOnly).projectText(markerOnlyText),
        markerOnlyText,
      );

      const emptyQuoteText = '> ';
      final emptyQuoteMapper = FlarkUtf8Utf16Mapper(emptyQuoteText);
      final emptyQuoteBridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 17,
          blocks: [
            NativeComrakBlockSpan(
              type: 'blockquote',
              range: NativeComrakRange(
                startByte: 0,
                endByte: emptyQuoteMapper.utf8OffsetForUtf16Offset(1),
              ),
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: 0,
              endByte: emptyQuoteMapper.utf8OffsetForUtf16Offset(
                emptyQuoteText.length,
              ),
            ),
          ],
        ),
      );
      final emptyQuoteBackend = FlarkNativeComrakParseBackend(
        bridge: emptyQuoteBridge,
      );

      final emptyQuote = await emptyQuoteBackend.parse(
        const FlarkMarkdownParseRequest(
          revision: 17,
          markdown: emptyQuoteText,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(emptyQuote.blocks.single.kind, FlarkMarkdownBlockKind.blockquote);
      expect(emptyQuote.hiddenRanges, isNotEmpty);
      expect(
        FlarkProjection.fromParseResult(emptyQuote).projectText(emptyQuoteText),
        isEmpty,
      );

      const quoteText = '> quote';
      final quoteMapper = FlarkUtf8Utf16Mapper(quoteText);
      final quoteBridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 18,
          blocks: [
            NativeComrakBlockSpan(
              type: 'blockquote',
              range: NativeComrakRange(
                startByte: 0,
                endByte: quoteMapper.utf8OffsetForUtf16Offset(quoteText.length),
              ),
            ),
            NativeComrakBlockSpan(
              type: 'paragraph',
              range: NativeComrakRange(
                startByte: 0,
                endByte: quoteMapper.utf8OffsetForUtf16Offset(quoteText.length),
              ),
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: 0,
              endByte: quoteMapper.utf8OffsetForUtf16Offset(2),
            ),
          ],
        ),
      );
      final quoteBackend = FlarkNativeComrakParseBackend(bridge: quoteBridge);

      final quote = await quoteBackend.parse(
        const FlarkMarkdownParseRequest(
          revision: 18,
          markdown: quoteText,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(quote.blocks.single.kind, FlarkMarkdownBlockKind.blockquote);
      expect(
        FlarkProjection.fromParseResult(quote).projectText(quoteText),
        'quote',
      );
    });

    test('keeps multiline native blockquotes as one semantic block', () async {
      const text = '> first\n> second\ncontinued';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final secondMarkerStart = text.indexOf('> second');
      final backend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 19,
            blocks: [
              NativeComrakBlockSpan(
                type: 'blockquote',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
            ],
            markerRanges: [
              NativeComrakRange(
                startByte: 0,
                endByte: mapper.utf8OffsetForUtf16Offset(2),
              ),
              NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(secondMarkerStart),
                endByte: mapper.utf8OffsetForUtf16Offset(secondMarkerStart + 2),
              ),
            ],
          ),
        ),
      );

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 19,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      final quotes = result.blocks
          .where((block) => block.kind == FlarkMarkdownBlockKind.blockquote)
          .toList(growable: false);
      expect(quotes, hasLength(1));
      expect(quotes.single.sourceRange, const FlarkSourceRange(0, 26));
      expect(
        FlarkProjection.fromParseResult(result).projectText(text),
        'first\nsecond\ncontinued',
      );
    });

    test('keeps marker-only native list items source-visible', () async {
      for (final markerOnlyText in const ['*', '-', '+', '1.']) {
        final markerOnlyMapper = FlarkUtf8Utf16Mapper(markerOnlyText);
        final markerOnlyBackend = FlarkNativeComrakParseBackend(
          bridge: _FakeNativeComrakBridge(
            NativeComrakParseResult(
              revision: 19,
              blocks: [
                NativeComrakBlockSpan(
                  type: 'list_item',
                  range: NativeComrakRange(
                    startByte: 0,
                    endByte: markerOnlyMapper.utf8OffsetForUtf16Offset(
                      markerOnlyText.length,
                    ),
                  ),
                ),
              ],
              markerRanges: [
                NativeComrakRange(
                  startByte: 0,
                  endByte: markerOnlyMapper.utf8OffsetForUtf16Offset(
                    markerOnlyText.length,
                  ),
                ),
              ],
            ),
          ),
        );

        final markerOnly = await markerOnlyBackend.parse(
          FlarkMarkdownParseRequest(
            revision: 19,
            markdown: markerOnlyText,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          markerOnly.blocks.single.kind,
          FlarkMarkdownBlockKind.paragraph,
          reason: markerOnlyText,
        );
        expect(markerOnly.hiddenRanges, isEmpty, reason: markerOnlyText);
        expect(
          FlarkProjection.fromParseResult(
            markerOnly,
          ).projectText(markerOnlyText),
          markerOnlyText,
          reason: markerOnlyText,
        );
      }
    });

    test('keeps complete empty native list markers rendered', () async {
      const completeMarkerText = '* ';
      final completeMarkerMapper = FlarkUtf8Utf16Mapper(completeMarkerText);
      final completeMarkerBackend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 20,
            blocks: [
              NativeComrakBlockSpan(
                type: 'list_item',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: completeMarkerMapper.utf8OffsetForUtf16Offset(1),
                ),
                payload: const {'listKind': 'unordered'},
              ),
            ],
            markerRanges: [
              NativeComrakRange(
                startByte: 0,
                endByte: completeMarkerMapper.utf8OffsetForUtf16Offset(
                  completeMarkerText.length,
                ),
              ),
            ],
          ),
        ),
      );

      final completeMarker = await completeMarkerBackend.parse(
        const FlarkMarkdownParseRequest(
          revision: 20,
          markdown: completeMarkerText,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        completeMarker.blocks.single.kind,
        FlarkMarkdownBlockKind.listItem,
      );
      expect(
        completeMarker.hiddenRanges.single.sourceRange,
        const FlarkSourceRange(0, 2),
      );
      expect(
        FlarkProjection.fromParseResult(
          completeMarker,
        ).projectText(completeMarkerText),
        isEmpty,
      );
    });

    test('keeps fenced code delimiters out of editable code content', () async {
      const text = '```dart\ncode\n```';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final openingMarkerEnd = text.indexOf('\n');
      final closingLineBreak = text.lastIndexOf('\n');
      final closingMarkerStart = closingLineBreak + 1;
      final backend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 21,
            blocks: [
              NativeComrakBlockSpan(
                type: 'fenced_code',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
            ],
            markerRanges: [
              NativeComrakRange(
                startByte: 0,
                endByte: mapper.utf8OffsetForUtf16Offset(openingMarkerEnd),
              ),
              NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(closingMarkerStart),
                endByte: mapper.utf8OffsetForUtf16Offset(text.length),
              ),
            ],
          ),
        ),
      );

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 21,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(result.hiddenRanges.map((range) => range.sourceRange), [
        const FlarkSourceRange(0, 3),
        FlarkSourceRange(3, openingMarkerEnd),
        FlarkSourceRange(openingMarkerEnd, openingMarkerEnd + 1),
        FlarkSourceRange(closingLineBreak, text.length),
      ]);
      expect(FlarkProjection.fromParseResult(result).projectText(text), 'code');
    });

    test(
      'hides fenced code info strings when native markers omit them',
      () async {
        const text = '```rust\ncode\n```';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final openingMarkerEnd = text.indexOf('rust');
        final openingLineBreak = text.indexOf('\n');
        final closingLineBreak = text.lastIndexOf('\n');
        final closingMarkerStart = closingLineBreak + 1;
        final backend = FlarkNativeComrakParseBackend(
          bridge: _FakeNativeComrakBridge(
            NativeComrakParseResult(
              revision: 23,
              blocks: [
                NativeComrakBlockSpan(
                  type: 'fenced_code',
                  range: NativeComrakRange(
                    startByte: 0,
                    endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                  ),
                ),
              ],
              markerRanges: [
                NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(openingMarkerEnd),
                ),
                NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(
                    closingMarkerStart,
                  ),
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ],
            ),
          ),
        );

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 23,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.hiddenRanges.map((range) => range.sourceRange), [
          FlarkSourceRange(0, openingMarkerEnd),
          FlarkSourceRange(openingMarkerEnd, openingLineBreak),
          FlarkSourceRange(openingLineBreak, openingLineBreak + 1),
          FlarkSourceRange(closingLineBreak, text.length),
        ]);
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          'code',
        );
      },
    );

    test(
      'does not overlap hidden ranges for an empty closed code fence',
      () async {
        const text = '```dart\n```';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final openingMarkerEnd = text.indexOf('\n');
        final closingMarkerStart = openingMarkerEnd + 1;
        final backend = FlarkNativeComrakParseBackend(
          bridge: _FakeNativeComrakBridge(
            NativeComrakParseResult(
              revision: 22,
              blocks: [
                NativeComrakBlockSpan(
                  type: 'fenced_code',
                  range: NativeComrakRange(
                    startByte: 0,
                    endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                  ),
                ),
              ],
              markerRanges: [
                NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(openingMarkerEnd),
                ),
                NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(
                    closingMarkerStart,
                  ),
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ],
            ),
          ),
        );

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 22,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.hiddenRanges.map((range) => range.sourceRange), [
          const FlarkSourceRange(0, 3),
          FlarkSourceRange(3, openingMarkerEnd),
          FlarkSourceRange(openingMarkerEnd, openingMarkerEnd + 1),
          FlarkSourceRange(closingMarkerStart, text.length),
        ]);
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          isEmpty,
        );
      },
    );

    test('extends unclosed fenced code ranges to the end of source', () async {
      const text = '```\nopen fence\n  code';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final openingMarkerEnd = text.indexOf('\n');
      final truncatedNativeEnd = text.indexOf('\n  code');
      final backend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 22,
            blocks: [
              NativeComrakBlockSpan(
                type: 'fenced_code',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(truncatedNativeEnd),
                ),
              ),
            ],
            markerRanges: [
              NativeComrakRange(
                startByte: 0,
                endByte: mapper.utf8OffsetForUtf16Offset(openingMarkerEnd),
              ),
            ],
          ),
        ),
      );

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 22,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.blocks.single.sourceRange,
        const FlarkSourceRange(0, text.length),
      );
      expect(
        FlarkProjection.fromParseResult(result).projectText(text),
        'open fence\n  code',
      );
    });

    test(
      'synthesizes editable list items when native output omits them',
      () async {
        const markerOnlyText = '- ';
        final markerOnlyMapper = FlarkUtf8Utf16Mapper(markerOnlyText);
        final markerOnlyBackend = FlarkNativeComrakParseBackend(
          bridge: _FakeNativeComrakBridge(
            NativeComrakParseResult(
              revision: 41,
              blocks: [
                NativeComrakBlockSpan(
                  type: 'paragraph',
                  range: NativeComrakRange(
                    startByte: 0,
                    endByte: markerOnlyMapper.utf8OffsetForUtf16Offset(
                      markerOnlyText.length,
                    ),
                  ),
                ),
              ],
            ),
          ),
        );

        final markerOnly = await markerOnlyBackend.parse(
          const FlarkMarkdownParseRequest(
            revision: 41,
            markdown: markerOnlyText,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(markerOnly.blocks.single.kind, FlarkMarkdownBlockKind.listItem);
        expect(markerOnly.blocks.single.attributes['listKind'], 'unordered');
        expect(
          markerOnly.hiddenRanges.single.sourceRange,
          const FlarkSourceRange(0, 2),
        );
        expect(
          FlarkProjection.fromParseResult(
            markerOnly,
          ).projectText(markerOnlyText),
          '',
        );

        const itemText = '3. ordered';
        final itemMapper = FlarkUtf8Utf16Mapper(itemText);
        final itemBackend = FlarkNativeComrakParseBackend(
          bridge: _FakeNativeComrakBridge(
            NativeComrakParseResult(
              revision: 42,
              blocks: [
                NativeComrakBlockSpan(
                  type: 'paragraph',
                  range: NativeComrakRange(
                    startByte: 0,
                    endByte: itemMapper.utf8OffsetForUtf16Offset(
                      itemText.length,
                    ),
                  ),
                ),
              ],
            ),
          ),
        );

        final item = await itemBackend.parse(
          const FlarkMarkdownParseRequest(
            revision: 42,
            markdown: itemText,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(item.blocks.single.kind, FlarkMarkdownBlockKind.listItem);
        expect(item.blocks.single.attributes['listKind'], 'ordered');
        expect(
          FlarkProjection.fromParseResult(item).projectText(itemText),
          'ordered',
        );
      },
    );

    test(
      'maps native link and image metadata into inline attributes',
      () async {
        const text = '[OpenAI](https://openai.com) ![Logo](asset://logo.png)';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final bridge = _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 13,
            inlineTokens: [
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(1),
                  endByte: mapper.utf8OffsetForUtf16Offset(7),
                ),
                styles: const {'link'},
                payload: const {
                  'destination': 'https://openai.com',
                  'title': 'OpenAI',
                  'label': 'OpenAI',
                },
              ),
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(31),
                  endByte: mapper.utf8OffsetForUtf16Offset(35),
                ),
                styles: const {'image'},
                payload: const {'src': 'asset://logo.png', 'alt': 'Logo'},
              ),
            ],
          ),
        );
        final backend = FlarkNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 13,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        final link = result.inlineTokens.first;
        final image = result.inlineTokens.last;
        expect(link.kind, FlarkMarkdownInlineKind.link);
        expect(link.attributes['destination'], 'https://openai.com');
        expect(link.attributes['title'], 'OpenAI');
        expect(link.attributes['label'], 'OpenAI');
        expect(image.kind, FlarkMarkdownInlineKind.image);
        expect(image.attributes['src'], 'asset://logo.png');
        expect(image.attributes['alt'], 'Logo');
      },
    );

    test(
      'adds native link hidden ranges for projected render labels',
      () async {
        const text =
            '[OpenAI](https://openai.com) and ![Logo](asset://logo.png)';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final imageStart = text.indexOf('![');
        final bridge = _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 14,
            blocks: [
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
            ],
            inlineTokens: [
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(
                    '[OpenAI](https://openai.com)'.length,
                  ),
                ),
                styles: const {'link'},
                payload: const {
                  'destination': 'https://openai.com',
                  'label': 'OpenAI',
                },
              ),
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(imageStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
                styles: const {'image'},
                payload: const {'src': 'asset://logo.png', 'alt': 'Logo'},
              ),
            ],
          ),
        );
        final backend = FlarkNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 14,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.hiddenRanges.map((range) => range.kind),
          containsAll(const [
            FlarkMarkdownHiddenRangeKind.inlineMarker,
            FlarkMarkdownHiddenRangeKind.linkDestination,
          ]),
        );
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          'OpenAI and Logo',
        );
      },
    );

    test(
      'normalizes flat native structural blocks for render surfaces',
      () async {
        const text =
            '- First bullet\n'
            '- [x] Complete task\n'
            '\n'
            '> A quoted note\n'
            '\n'
            '| Area | Status |\n'
            '| --- | --- |\n'
            '| Inline | Good |';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final firstItemEnd = text.indexOf('\n');
        final taskStart = text.indexOf('- [x]');
        final taskEnd = text.indexOf('\n\n');
        final quoteStart = text.indexOf('>');
        final quoteEnd = text.indexOf('\n\n', quoteStart);
        final tableStart = text.indexOf('| Area');
        final tableEnd = text.length;
        final bridge = _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 15,
            blocks: [
              NativeComrakBlockSpan(
                type: 'unordered_list',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(taskEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'list_item',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(firstItemEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(firstItemEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'list_item',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(taskStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(taskEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'list_item',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(taskStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(taskEnd),
                ),
                payload: const {'checked': true},
              ),
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(taskStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(taskEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'blockquote',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(quoteStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(quoteEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(quoteStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(quoteEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'table',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(tableStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(tableEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'table_row',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(tableStart),
                  endByte: mapper.utf8OffsetForUtf16Offset(tableEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'table_cell',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(tableStart + 2),
                  endByte: mapper.utf8OffsetForUtf16Offset(tableStart + 6),
                ),
              ),
            ],
          ),
        );
        final backend = FlarkNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.blocks.map((block) => block.kind), const [
          FlarkMarkdownBlockKind.listItem,
          FlarkMarkdownBlockKind.listItem,
          FlarkMarkdownBlockKind.blockquote,
          FlarkMarkdownBlockKind.table,
        ]);
        expect(result.blocks[1].attributes['checked'], isTrue);
        final table = result.blocks.singleWhere(
          (block) => block.kind == FlarkMarkdownBlockKind.table,
        );
        expect(table.children.map((block) => block.kind), const [
          FlarkMarkdownBlockKind.tableRow,
        ]);
        expect(
          table.children.single.children.map((block) => block.kind),
          const [FlarkMarkdownBlockKind.tableCell],
        );
      },
    );

    test('adds reference-definition and raw-html hidden ranges', () async {
      const text =
          '[id]: https://example.com\n\n<div>raw</div>\n\ntext <span>x</span>';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final htmlBlockStart = text.indexOf('<div>');
      final htmlBlockEnd = htmlBlockStart + '<div>raw</div>'.length;
      final htmlInlineStart = text.indexOf('<span>');
      final htmlInlineEnd = htmlInlineStart + '<span>'.length;
      final bridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 14,
          blocks: [
            NativeComrakBlockSpan(
              type: 'html_block',
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(htmlBlockStart),
                endByte: mapper.utf8OffsetForUtf16Offset(htmlBlockEnd),
              ),
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(htmlInlineStart),
                endByte: mapper.utf8OffsetForUtf16Offset(htmlInlineEnd),
              ),
              styles: const {'htmlInline'},
            ),
          ],
        ),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 14,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.hiddenRanges.map((range) => range.kind),
        containsAll(const [
          FlarkMarkdownHiddenRangeKind.referenceDefinition,
          FlarkMarkdownHiddenRangeKind.rawHtml,
        ]),
      );
      expect(
        result.hiddenRanges
            .where(
              (range) =>
                  range.kind ==
                  FlarkMarkdownHiddenRangeKind.referenceDefinition,
            )
            .single
            .sourceRange,
        const FlarkSourceRange(0, 26),
      );
      expect(
        result.inlineTokens.single.kind,
        FlarkMarkdownInlineKind.htmlInline,
      );
    });

    test(
      'keeps invalid reference-definition-looking paragraphs visible',
      () async {
        const text = '[foo]: <bar>(baz)\n\n[foo]\n';
        final mapper = FlarkUtf8Utf16Mapper(text);
        final firstParagraphEnd = text.indexOf('\n');
        final secondParagraphStart = text.lastIndexOf('[foo]');
        final bridge = _FakeNativeComrakBridge(
          NativeComrakParseResult(
            revision: 15,
            blocks: [
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: 0,
                  endByte: mapper.utf8OffsetForUtf16Offset(firstParagraphEnd),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'paragraph',
                range: NativeComrakRange(
                  startByte: mapper.utf8OffsetForUtf16Offset(
                    secondParagraphStart,
                  ),
                  endByte: mapper.utf8OffsetForUtf16Offset(text.length),
                ),
              ),
            ],
          ),
        );
        final backend = FlarkNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.hiddenRanges.where(
            (range) =>
                range.kind == FlarkMarkdownHiddenRangeKind.referenceDefinition,
          ),
          isEmpty,
        );
        expect(FlarkProjection.fromParseResult(result).projectText(text), text);
      },
    );

    test('keeps unsupported GitHub footnote syntax source-visible', () async {
      const text = 'Text[^1]\n\n[^1]: Footnote\n';
      final mapper = FlarkUtf8Utf16Mapper(text);
      final bridge = _FakeNativeComrakBridge(
        NativeComrakParseResult(
          revision: 16,
          blocks: [
            NativeComrakBlockSpan(
              type: 'paragraph',
              range: NativeComrakRange(
                startByte: 0,
                endByte: mapper.utf8OffsetForUtf16Offset(text.length),
              ),
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf8OffsetForUtf16Offset(4),
                endByte: mapper.utf8OffsetForUtf16Offset(8),
              ),
              styles: const {'link'},
              payload: const {'destination': 'Footnote'},
            ),
          ],
        ),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 16,
          markdown: text,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.inlineTokens.where(
          (token) => token.kind == FlarkMarkdownInlineKind.link,
        ),
        isEmpty,
      );
      expect(
        result.hiddenRanges.where(
          (range) =>
              range.kind == FlarkMarkdownHiddenRangeKind.referenceDefinition,
        ),
        isEmpty,
      );
      expect(FlarkProjection.fromParseResult(result).projectText(text), text);
    });

    test('preserves unknown native variants without crashing', () async {
      final bridge = _FakeNativeComrakBridge(
        const NativeComrakParseResult(
          revision: 3,
          blocks: [
            NativeComrakBlockSpan(
              type: 'admonition',
              range: NativeComrakRange(startByte: 0, endByte: 4),
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(startByte: 0, endByte: 4),
              styles: {'wikilink'},
            ),
          ],
          diagnostics: [
            NativeComrakDiagnostic(
              range: NativeComrakRange(startByte: 0, endByte: 0),
              message: 'warning',
              code: 'NATIVE_WARNING',
            ),
          ],
        ),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 3,
          markdown: 'test',
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(result.blocks.single.kind, FlarkMarkdownBlockKind.unknown);
      expect(result.blocks.single.attributes['nativeType'], 'admonition');
      expect(result.inlineTokens.single.kind, FlarkMarkdownInlineKind.unknown);
      expect(result.inlineTokens.single.attributes['nativeStyle'], 'wikilink');
      expect(result.diagnostics.single.code, 'NATIVE_WARNING');
    });

    test('records a diagnostic when native revisions do not match', () async {
      final backend = FlarkNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          const NativeComrakParseResult(revision: 1),
        ),
      );

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 2,
          markdown: 'text',
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(
        result.diagnostics.map((diagnostic) => diagnostic.code),
        contains('COMRAK_REVISION_MISMATCH'),
      );
    });

    test('returns an empty v2 result without calling native parser', () async {
      final bridge = _FakeNativeComrakBridge(
        const NativeComrakParseResult(revision: 0),
      );
      final backend = FlarkNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 4,
          markdown: '',
          profile: FlarkMarkdownProfile.commonMarkCore,
        ),
      );

      expect(result.revision, 4);
      expect(result.sourceTextLength, 0);
      expect(result.blocks, isEmpty);
      expect(bridge.lastInput, isNull);
    });

    test('maps real native comrak output into the v2 contract', () async {
      final libPath = sovereignNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        return;
      }

      final backend = FlarkNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      );

      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 21,
          markdown: '# Title\n\n| a |\n| - |\n| b |\n\n**bold**\n',
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(result.revision, 21);
      expect(
        result.blocks.map((block) => block.kind),
        containsAll(const [
          FlarkMarkdownBlockKind.heading,
          FlarkMarkdownBlockKind.table,
        ]),
      );
      expect(
        result.inlineTokens.map((token) => token.kind),
        contains(FlarkMarkdownInlineKind.strong),
      );
      expect(result.hiddenRanges, isNotEmpty);
      expect(
        result.diagnostics.where((diagnostic) {
          return diagnostic.extensions['isError'] == true;
        }),
        isEmpty,
      );
    });

    test(
      'real native output carries v2 action and table/task metadata',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final backend = FlarkNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 22,
            markdown:
                '[OpenAI](https://openai.com "AI")\n\n![Logo](asset://logo.png)\n\n- [x] done\n\n| A | B |\n| :- | -: |\n| 1 | 2 |\n\n[id]: https://example.com\n\n<div>raw</div>\n',
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        final link = result.inlineTokens.firstWhere(
          (token) => token.kind == FlarkMarkdownInlineKind.link,
        );
        final image = result.inlineTokens.firstWhere(
          (token) => token.kind == FlarkMarkdownInlineKind.image,
        );
        expect(link.attributes['destination'], 'https://openai.com');
        expect(link.attributes['title'], 'AI');
        expect(link.attributes['label'], 'OpenAI');
        expect(image.attributes['src'], 'asset://logo.png');
        expect(image.attributes['alt'], 'Logo');
        expect(
          result.blocks.map((block) => block.kind),
          containsAll(const [
            FlarkMarkdownBlockKind.listItem,
            FlarkMarkdownBlockKind.table,
          ]),
        );
        final table = result.blocks.singleWhere(
          (block) => block.kind == FlarkMarkdownBlockKind.table,
        );
        expect(table.children, isNotEmpty);
        expect(table.children.first.kind, FlarkMarkdownBlockKind.tableRow);
        expect(
          table.children.first.children.map((block) => block.kind),
          contains(FlarkMarkdownBlockKind.tableCell),
        );
        expect(
          result.blocks
              .where((block) => block.kind == FlarkMarkdownBlockKind.listItem)
              .any((block) => block.attributes['checked'] == true),
          isTrue,
        );
        expect(
          result.hiddenRanges.map((range) => range.kind),
          containsAll(const [
            FlarkMarkdownHiddenRangeKind.referenceDefinition,
            FlarkMarkdownHiddenRangeKind.rawHtml,
          ]),
        );
      },
    );

    test(
      'real native keeps marker-only unordered list input source-visible',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final backend = FlarkNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final markerOnly = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 23,
            markdown: '*',
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(markerOnly.blocks.single.kind, FlarkMarkdownBlockKind.paragraph);
        expect(markerOnly.hiddenRanges, isEmpty);
        expect(
          FlarkProjection.fromParseResult(markerOnly).projectText('*'),
          '*',
        );

        final completeMarker = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 24,
            markdown: '* ',
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          completeMarker.blocks.single.kind,
          FlarkMarkdownBlockKind.listItem,
        );
        expect(
          FlarkProjection.fromParseResult(completeMarker).projectText('* '),
          isEmpty,
        );
      },
    );

    test(
      'real native keeps fenced code bounded before following blocks',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        const text = '```dart\ncode\n```\n\n> quote';
        final backend = FlarkNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 25,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.blocks.map((block) => block.kind),
          containsAll(const [
            FlarkMarkdownBlockKind.codeBlock,
            FlarkMarkdownBlockKind.blockquote,
          ]),
        );
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          'code\n\nquote',
        );
      },
    );

    test(
      'real native keeps unclosed fenced code editable through EOF',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        const text = '```\nopen fence\n  code';
        final backend = FlarkNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const FlarkMarkdownParseRequest(
            revision: 26,
            markdown: text,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );

        final codeBlock = result.blocks.singleWhere(
          (block) => block.kind == FlarkMarkdownBlockKind.codeBlock,
        );
        expect(codeBlock.sourceRange, const FlarkSourceRange(0, text.length));
        expect(
          FlarkProjection.fromParseResult(result).projectText(text),
          'open fence\n  code',
        );
      },
    );
  });
}
