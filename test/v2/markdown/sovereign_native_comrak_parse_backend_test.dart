import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

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
  group('SovereignNativeComrakParseBackend', () {
    test('reports v2 parser capabilities', () {
      final backend = SovereignNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          const NativeComrakParseResult(revision: 0),
        ),
      );

      expect(backend.capabilities.parserName, 'comrak_native_v2_adapter');
      expect(
        backend.capabilities.schemaVersion,
        SovereignMarkdownParseProtocol.currentSchemaVersion,
      );
      expect(
        backend.capabilities.supports(SovereignMarkdownProfile.commonMarkGfm),
        isTrue,
      );
    });

    test('supports no-throw native backend probing for platform fallbacks', () {
      final missingPath =
          '${Directory.systemTemp.path}/sovereign_missing_comrak_bridge.dylib';

      final preflight = SovereignNativeComrakParseBackend.preflight(
        overrideLibraryPath: missingPath,
      );

      expect(preflight.isAvailable, isFalse);
      expect(
        preflight.error!.kind,
        NativeComrakBridgeLoadFailureKind.libraryNotFound,
      );
      expect(
        SovereignNativeComrakParseBackend.tryLoad(
          overrideLibraryPath: missingPath,
        ),
        isNull,
      );
    });

    test('marshals text and profile to the native bridge', () async {
      final bridge = _FakeNativeComrakBridge(
        const NativeComrakParseResult(revision: 9),
      );
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 9,
          markdown: '## hello',
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(bridge.lastInput, isNotNull);
      expect(bridge.lastInput!.revision, 9);
      expect(bridge.lastInput!.profile, NativeComrakProfile.commonMarkGfm);
      expect(utf8.decode(bridge.lastInput!.utf8Text), '## hello');
    });

    test('maps utf8 native ranges into v2 parse results', () async {
      const text = '🎨\n# **T**\n';
      final mapper = SovereignUtf8Utf16Mapper(text);
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 12,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(
        result.schemaVersion,
        SovereignMarkdownParseProtocol.currentSchemaVersion,
      );
      expect(result.revision, 12);
      expect(result.sourceTextLength, text.length);
      expect(result.blocks.single.kind, SovereignMarkdownBlockKind.heading);
      expect(result.blocks.single.type, 'heading');
      expect(
        result.blocks.single.sourceRange,
        SovereignSourceRange(3, text.length),
      );
      expect(result.blocks.single.attributes['level'], 1);
      expect(
        result.inlineTokens.single.kind,
        SovereignMarkdownInlineKind.strong,
      );
      expect(
        result.inlineTokens.single.sourceRange,
        const SovereignSourceRange(6, 7),
      );
      expect(result.hiddenRanges.map((range) => range.sourceRange), const [
        SovereignSourceRange(3, 4),
        SovereignSourceRange(4, 6),
        SovereignSourceRange(7, 9),
      ]);
      expect(result.extensions['nativeParser'], 'comrak');
      expect(result.extensions['nativeExclusionRanges'], [
        {'start': 3, 'end': text.length},
      ]);
    });

    test('keeps partial strong delimiter intent literal', () async {
      const text = '**wow*';
      final mapper = SovereignUtf8Utf16Mapper(text);
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 13,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(result.inlineTokens, isEmpty);
      expect(result.hiddenRanges, isEmpty);
      expect(
        SovereignProjection.fromParseResult(result).projectText(text),
        text,
      );
    });

    test(
      'maps native replacement ranges and filters hidden overlaps',
      () async {
        const text = 'A &amp; [x](/a&amp;b)';
        final mapper = SovereignUtf8Utf16Mapper(text);
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
        final backend = SovereignNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkCore,
          ),
        );

        expect(result.replacementRanges, hasLength(1));
        expect(
          result.replacementRanges.single.kind,
          SovereignMarkdownReplacementRangeKind.htmlEntity,
        );
        expect(
          result.replacementRanges.single.sourceRange,
          SovereignSourceRange(firstEntityStart, firstEntityStart + 5),
        );
        expect(result.replacementRanges.single.replacementText, '&');
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
          'A & x',
        );
      },
    );

    test('keeps marker-only native blockquotes source-visible', () async {
      const markerOnlyText = '>';
      final markerOnlyMapper = SovereignUtf8Utf16Mapper(markerOnlyText);
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
      final markerOnlyBackend = SovereignNativeComrakParseBackend(
        bridge: markerOnlyBridge,
      );

      final markerOnly = await markerOnlyBackend.parse(
        const SovereignMarkdownParseRequest(
          revision: 16,
          markdown: markerOnlyText,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(
        markerOnly.blocks.single.kind,
        SovereignMarkdownBlockKind.paragraph,
      );
      expect(markerOnly.hiddenRanges, isEmpty);
      expect(
        SovereignProjection.fromParseResult(
          markerOnly,
        ).projectText(markerOnlyText),
        markerOnlyText,
      );

      const emptyQuoteText = '> ';
      final emptyQuoteMapper = SovereignUtf8Utf16Mapper(emptyQuoteText);
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
      final emptyQuoteBackend = SovereignNativeComrakParseBackend(
        bridge: emptyQuoteBridge,
      );

      final emptyQuote = await emptyQuoteBackend.parse(
        const SovereignMarkdownParseRequest(
          revision: 17,
          markdown: emptyQuoteText,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(
        emptyQuote.blocks.single.kind,
        SovereignMarkdownBlockKind.blockquote,
      );
      expect(emptyQuote.hiddenRanges, isNotEmpty);
      expect(
        SovereignProjection.fromParseResult(
          emptyQuote,
        ).projectText(emptyQuoteText),
        isEmpty,
      );

      const quoteText = '> quote';
      final quoteMapper = SovereignUtf8Utf16Mapper(quoteText);
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
      final quoteBackend = SovereignNativeComrakParseBackend(
        bridge: quoteBridge,
      );

      final quote = await quoteBackend.parse(
        const SovereignMarkdownParseRequest(
          revision: 18,
          markdown: quoteText,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(quote.blocks.single.kind, SovereignMarkdownBlockKind.blockquote);
      expect(
        SovereignProjection.fromParseResult(quote).projectText(quoteText),
        'quote',
      );
    });

    test('keeps multiline native blockquotes as one semantic block', () async {
      const text = '> first\n> second\ncontinued';
      final mapper = SovereignUtf8Utf16Mapper(text);
      final secondMarkerStart = text.indexOf('> second');
      final backend = SovereignNativeComrakParseBackend(
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
        const SovereignMarkdownParseRequest(
          revision: 19,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      final quotes = result.blocks
          .where((block) => block.kind == SovereignMarkdownBlockKind.blockquote)
          .toList(growable: false);
      expect(quotes, hasLength(1));
      expect(quotes.single.sourceRange, const SovereignSourceRange(0, 26));
      expect(
        SovereignProjection.fromParseResult(result).projectText(text),
        'first\nsecond\ncontinued',
      );
    });

    test('keeps marker-only native list items source-visible', () async {
      for (final markerOnlyText in const ['*', '-', '+', '1.']) {
        final markerOnlyMapper = SovereignUtf8Utf16Mapper(markerOnlyText);
        final markerOnlyBackend = SovereignNativeComrakParseBackend(
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
          SovereignMarkdownParseRequest(
            revision: 19,
            markdown: markerOnlyText,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          markerOnly.blocks.single.kind,
          SovereignMarkdownBlockKind.paragraph,
          reason: markerOnlyText,
        );
        expect(markerOnly.hiddenRanges, isEmpty, reason: markerOnlyText);
        expect(
          SovereignProjection.fromParseResult(
            markerOnly,
          ).projectText(markerOnlyText),
          markerOnlyText,
          reason: markerOnlyText,
        );
      }
    });

    test('keeps complete empty native list markers rendered', () async {
      const completeMarkerText = '* ';
      final completeMarkerMapper = SovereignUtf8Utf16Mapper(completeMarkerText);
      final completeMarkerBackend = SovereignNativeComrakParseBackend(
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
        const SovereignMarkdownParseRequest(
          revision: 20,
          markdown: completeMarkerText,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        completeMarker.blocks.single.kind,
        SovereignMarkdownBlockKind.listItem,
      );
      expect(
        completeMarker.hiddenRanges.single.sourceRange,
        const SovereignSourceRange(0, 2),
      );
      expect(
        SovereignProjection.fromParseResult(
          completeMarker,
        ).projectText(completeMarkerText),
        isEmpty,
      );
    });

    test('keeps fenced code delimiters out of editable code content', () async {
      const text = '```dart\ncode\n```';
      final mapper = SovereignUtf8Utf16Mapper(text);
      final openingMarkerEnd = text.indexOf('\n');
      final closingLineBreak = text.lastIndexOf('\n');
      final closingMarkerStart = closingLineBreak + 1;
      final backend = SovereignNativeComrakParseBackend(
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
        const SovereignMarkdownParseRequest(
          revision: 21,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(result.hiddenRanges.map((range) => range.sourceRange), [
        const SovereignSourceRange(0, 3),
        SovereignSourceRange(3, openingMarkerEnd),
        SovereignSourceRange(openingMarkerEnd, openingMarkerEnd + 1),
        SovereignSourceRange(closingLineBreak, text.length),
      ]);
      expect(
        SovereignProjection.fromParseResult(result).projectText(text),
        'code',
      );
    });

    test(
      'hides fenced code info strings when native markers omit them',
      () async {
        const text = '```rust\ncode\n```';
        final mapper = SovereignUtf8Utf16Mapper(text);
        final openingMarkerEnd = text.indexOf('rust');
        final openingLineBreak = text.indexOf('\n');
        final closingLineBreak = text.lastIndexOf('\n');
        final closingMarkerStart = closingLineBreak + 1;
        final backend = SovereignNativeComrakParseBackend(
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
          const SovereignMarkdownParseRequest(
            revision: 23,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.hiddenRanges.map((range) => range.sourceRange), [
          SovereignSourceRange(0, openingMarkerEnd),
          SovereignSourceRange(openingMarkerEnd, openingLineBreak),
          SovereignSourceRange(openingLineBreak, openingLineBreak + 1),
          SovereignSourceRange(closingLineBreak, text.length),
        ]);
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
          'code',
        );
      },
    );

    test(
      'does not overlap hidden ranges for an empty closed code fence',
      () async {
        const text = '```dart\n```';
        final mapper = SovereignUtf8Utf16Mapper(text);
        final openingMarkerEnd = text.indexOf('\n');
        final closingMarkerStart = openingMarkerEnd + 1;
        final backend = SovereignNativeComrakParseBackend(
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
          const SovereignMarkdownParseRequest(
            revision: 22,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.hiddenRanges.map((range) => range.sourceRange), [
          const SovereignSourceRange(0, 3),
          SovereignSourceRange(3, openingMarkerEnd),
          SovereignSourceRange(openingMarkerEnd, openingMarkerEnd + 1),
          SovereignSourceRange(closingMarkerStart, text.length),
        ]);
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
          isEmpty,
        );
      },
    );

    test('extends unclosed fenced code ranges to the end of source', () async {
      const text = '```\nopen fence\n  code';
      final mapper = SovereignUtf8Utf16Mapper(text);
      final openingMarkerEnd = text.indexOf('\n');
      final truncatedNativeEnd = text.indexOf('\n  code');
      final backend = SovereignNativeComrakParseBackend(
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
        const SovereignMarkdownParseRequest(
          revision: 22,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.blocks.single.sourceRange,
        const SovereignSourceRange(0, text.length),
      );
      expect(
        SovereignProjection.fromParseResult(result).projectText(text),
        'open fence\n  code',
      );
    });

    test(
      'synthesizes editable list items when native output omits them',
      () async {
        const markerOnlyText = '- ';
        final markerOnlyMapper = SovereignUtf8Utf16Mapper(markerOnlyText);
        final markerOnlyBackend = SovereignNativeComrakParseBackend(
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
          const SovereignMarkdownParseRequest(
            revision: 41,
            markdown: markerOnlyText,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          markerOnly.blocks.single.kind,
          SovereignMarkdownBlockKind.listItem,
        );
        expect(markerOnly.blocks.single.attributes['listKind'], 'unordered');
        expect(
          markerOnly.hiddenRanges.single.sourceRange,
          const SovereignSourceRange(0, 2),
        );
        expect(
          SovereignProjection.fromParseResult(
            markerOnly,
          ).projectText(markerOnlyText),
          '',
        );

        const itemText = '3. ordered';
        final itemMapper = SovereignUtf8Utf16Mapper(itemText);
        final itemBackend = SovereignNativeComrakParseBackend(
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
          const SovereignMarkdownParseRequest(
            revision: 42,
            markdown: itemText,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(item.blocks.single.kind, SovereignMarkdownBlockKind.listItem);
        expect(item.blocks.single.attributes['listKind'], 'ordered');
        expect(
          SovereignProjection.fromParseResult(item).projectText(itemText),
          'ordered',
        );
      },
    );

    test(
      'maps native link and image metadata into inline attributes',
      () async {
        const text = '[OpenAI](https://openai.com) ![Logo](asset://logo.png)';
        final mapper = SovereignUtf8Utf16Mapper(text);
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
        final backend = SovereignNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 13,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        final link = result.inlineTokens.first;
        final image = result.inlineTokens.last;
        expect(link.kind, SovereignMarkdownInlineKind.link);
        expect(link.attributes['destination'], 'https://openai.com');
        expect(link.attributes['title'], 'OpenAI');
        expect(link.attributes['label'], 'OpenAI');
        expect(image.kind, SovereignMarkdownInlineKind.image);
        expect(image.attributes['src'], 'asset://logo.png');
        expect(image.attributes['alt'], 'Logo');
      },
    );

    test(
      'adds native link hidden ranges for projected render labels',
      () async {
        const text =
            '[OpenAI](https://openai.com) and ![Logo](asset://logo.png)';
        final mapper = SovereignUtf8Utf16Mapper(text);
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
        final backend = SovereignNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 14,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.hiddenRanges.map((range) => range.kind),
          containsAll(const [
            SovereignMarkdownHiddenRangeKind.inlineMarker,
            SovereignMarkdownHiddenRangeKind.linkDestination,
          ]),
        );
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
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
        final mapper = SovereignUtf8Utf16Mapper(text);
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
        final backend = SovereignNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(result.blocks.map((block) => block.kind), const [
          SovereignMarkdownBlockKind.listItem,
          SovereignMarkdownBlockKind.listItem,
          SovereignMarkdownBlockKind.blockquote,
          SovereignMarkdownBlockKind.table,
        ]);
        expect(result.blocks[1].attributes['checked'], isTrue);
        final table = result.blocks.singleWhere(
          (block) => block.kind == SovereignMarkdownBlockKind.table,
        );
        expect(table.children.map((block) => block.kind), const [
          SovereignMarkdownBlockKind.tableRow,
        ]);
        expect(
          table.children.single.children.map((block) => block.kind),
          const [SovereignMarkdownBlockKind.tableCell],
        );
      },
    );

    test('adds reference-definition and raw-html hidden ranges', () async {
      const text =
          '[id]: https://example.com\n\n<div>raw</div>\n\ntext <span>x</span>';
      final mapper = SovereignUtf8Utf16Mapper(text);
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 14,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.hiddenRanges.map((range) => range.kind),
        containsAll(const [
          SovereignMarkdownHiddenRangeKind.referenceDefinition,
          SovereignMarkdownHiddenRangeKind.rawHtml,
        ]),
      );
      expect(
        result.hiddenRanges
            .where(
              (range) =>
                  range.kind ==
                  SovereignMarkdownHiddenRangeKind.referenceDefinition,
            )
            .single
            .sourceRange,
        const SovereignSourceRange(0, 26),
      );
      expect(
        result.inlineTokens.single.kind,
        SovereignMarkdownInlineKind.htmlInline,
      );
    });

    test(
      'keeps invalid reference-definition-looking paragraphs visible',
      () async {
        const text = '[foo]: <bar>(baz)\n\n[foo]\n';
        final mapper = SovereignUtf8Utf16Mapper(text);
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
        final backend = SovereignNativeComrakParseBackend(bridge: bridge);

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 15,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.hiddenRanges.where(
            (range) =>
                range.kind ==
                SovereignMarkdownHiddenRangeKind.referenceDefinition,
          ),
          isEmpty,
        );
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
          text,
        );
      },
    );

    test('keeps unsupported GitHub footnote syntax source-visible', () async {
      const text = 'Text[^1]\n\n[^1]: Footnote\n';
      final mapper = SovereignUtf8Utf16Mapper(text);
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 16,
          markdown: text,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(
        result.inlineTokens.where(
          (token) => token.kind == SovereignMarkdownInlineKind.link,
        ),
        isEmpty,
      );
      expect(
        result.hiddenRanges.where(
          (range) =>
              range.kind ==
              SovereignMarkdownHiddenRangeKind.referenceDefinition,
        ),
        isEmpty,
      );
      expect(
        SovereignProjection.fromParseResult(result).projectText(text),
        text,
      );
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 3,
          markdown: 'test',
          profile: SovereignMarkdownProfile.commonMarkCore,
        ),
      );

      expect(result.blocks.single.kind, SovereignMarkdownBlockKind.unknown);
      expect(result.blocks.single.attributes['nativeType'], 'admonition');
      expect(
        result.inlineTokens.single.kind,
        SovereignMarkdownInlineKind.unknown,
      );
      expect(result.inlineTokens.single.attributes['nativeStyle'], 'wikilink');
      expect(result.diagnostics.single.code, 'NATIVE_WARNING');
    });

    test('records a diagnostic when native revisions do not match', () async {
      final backend = SovereignNativeComrakParseBackend(
        bridge: _FakeNativeComrakBridge(
          const NativeComrakParseResult(revision: 1),
        ),
      );

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 2,
          markdown: 'text',
          profile: SovereignMarkdownProfile.commonMarkCore,
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
      final backend = SovereignNativeComrakParseBackend(bridge: bridge);

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 4,
          markdown: '',
          profile: SovereignMarkdownProfile.commonMarkCore,
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

      final backend = SovereignNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      );

      final result = await backend.parse(
        const SovereignMarkdownParseRequest(
          revision: 21,
          markdown: '# Title\n\n| a |\n| - |\n| b |\n\n**bold**\n',
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );

      expect(result.revision, 21);
      expect(
        result.blocks.map((block) => block.kind),
        containsAll(const [
          SovereignMarkdownBlockKind.heading,
          SovereignMarkdownBlockKind.table,
        ]),
      );
      expect(
        result.inlineTokens.map((token) => token.kind),
        contains(SovereignMarkdownInlineKind.strong),
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

        final backend = SovereignNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 22,
            markdown:
                '[OpenAI](https://openai.com "AI")\n\n![Logo](asset://logo.png)\n\n- [x] done\n\n| A | B |\n| :- | -: |\n| 1 | 2 |\n\n[id]: https://example.com\n\n<div>raw</div>\n',
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        final link = result.inlineTokens.firstWhere(
          (token) => token.kind == SovereignMarkdownInlineKind.link,
        );
        final image = result.inlineTokens.firstWhere(
          (token) => token.kind == SovereignMarkdownInlineKind.image,
        );
        expect(link.attributes['destination'], 'https://openai.com');
        expect(link.attributes['title'], 'AI');
        expect(link.attributes['label'], 'OpenAI');
        expect(image.attributes['src'], 'asset://logo.png');
        expect(image.attributes['alt'], 'Logo');
        expect(
          result.blocks.map((block) => block.kind),
          containsAll(const [
            SovereignMarkdownBlockKind.listItem,
            SovereignMarkdownBlockKind.table,
          ]),
        );
        final table = result.blocks.singleWhere(
          (block) => block.kind == SovereignMarkdownBlockKind.table,
        );
        expect(table.children, isNotEmpty);
        expect(table.children.first.kind, SovereignMarkdownBlockKind.tableRow);
        expect(
          table.children.first.children.map((block) => block.kind),
          contains(SovereignMarkdownBlockKind.tableCell),
        );
        expect(
          result.blocks
              .where(
                (block) => block.kind == SovereignMarkdownBlockKind.listItem,
              )
              .any((block) => block.attributes['checked'] == true),
          isTrue,
        );
        expect(
          result.hiddenRanges.map((range) => range.kind),
          containsAll(const [
            SovereignMarkdownHiddenRangeKind.referenceDefinition,
            SovereignMarkdownHiddenRangeKind.rawHtml,
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

        final backend = SovereignNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final markerOnly = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 23,
            markdown: '*',
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          markerOnly.blocks.single.kind,
          SovereignMarkdownBlockKind.paragraph,
        );
        expect(markerOnly.hiddenRanges, isEmpty);
        expect(
          SovereignProjection.fromParseResult(markerOnly).projectText('*'),
          '*',
        );

        final completeMarker = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 24,
            markdown: '* ',
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          completeMarker.blocks.single.kind,
          SovereignMarkdownBlockKind.listItem,
        );
        expect(
          SovereignProjection.fromParseResult(completeMarker).projectText('* '),
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
        final backend = SovereignNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 25,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        expect(
          result.blocks.map((block) => block.kind),
          containsAll(const [
            SovereignMarkdownBlockKind.codeBlock,
            SovereignMarkdownBlockKind.blockquote,
          ]),
        );
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
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
        final backend = SovereignNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );

        final result = await backend.parse(
          const SovereignMarkdownParseRequest(
            revision: 26,
            markdown: text,
            profile: SovereignMarkdownProfile.commonMarkGfm,
          ),
        );

        final codeBlock = result.blocks.singleWhere(
          (block) => block.kind == SovereignMarkdownBlockKind.codeBlock,
        );
        expect(
          codeBlock.sourceRange,
          const SovereignSourceRange(0, text.length),
        );
        expect(
          SovereignProjection.fromParseResult(result).projectText(text),
          'open fence\n  code',
        );
      },
    );
  });
}
