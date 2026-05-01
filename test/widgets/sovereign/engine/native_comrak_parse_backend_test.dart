import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_ffi.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/utf8_utf16_offset_mapper.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';
import 'support/bootstrap_commonmark_parse_backend.dart';
import 'support/test_paths.dart';

class _FakeNativeComrakBridge implements NativeComrakBridge {
  NativeComrakParseInput? lastInput;
  NativeComrakParseResult result;
  int parseCount = 0;

  _FakeNativeComrakBridge({required this.result});

  @override
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input) async {
    parseCount++;
    lastInput = input;
    return result;
  }
}

void main() {
  group('ComrakCommonMarkParseBackend', () {
    test(
      'supports deterministic empty parse via explicit fake bridge',
      () async {
        final backend = ComrakCommonMarkParseBackend(
          bridge: _FakeNativeComrakBridge(
            result: const NativeComrakParseResult(revision: 1),
          ),
        );
        const request = SyntaxParseRequest(revision: 1, text: 'title');

        final snapshot = await backend.parse(request);

        expect(backend.backendId, 'comrak_native_v1');
        expect(snapshot.revision, 1);
        expect(snapshot.blocks, isEmpty);
        expect(snapshot.markerRanges, isEmpty);
        expect(snapshot.exclusionRanges, isEmpty);
        expect(snapshot.diagnostics, isEmpty);
      },
    );

    test('marshals text/profile into bridge input', () async {
      final bridge = _FakeNativeComrakBridge(
        result: const NativeComrakParseResult(revision: 9),
      );
      final backend = ComrakCommonMarkParseBackend(bridge: bridge);
      const request = SyntaxParseRequest(
        revision: 9,
        text: '## hello',
        profile: MarkdownSyntaxProfile.commonMarkGfm,
      );

      final snapshot = await backend.parse(request);

      expect(snapshot.revision, 9);
      expect(bridge.parseCount, 1);
      expect(bridge.lastInput, isNotNull);
      expect(bridge.lastInput!.revision, 9);
      expect(bridge.lastInput!.profile, NativeComrakProfile.commonMarkGfm);
      expect(utf8.decode(bridge.lastInput!.utf8Text), request.text);
    });

    test('maps utf8 source spans to utf16 snapshot ranges', () async {
      const text = '🎨\n# T\n';
      final mapper = Utf8Utf16OffsetMapper.fromText(text);
      final bridge = _FakeNativeComrakBridge(
        result: NativeComrakParseResult(
          revision: 5,
          blocks: [
            NativeComrakBlockSpan(
              type: 'header',
              range: NativeComrakRange(
                startByte: mapper.utf16ToUtf8(3),
                endByte: mapper.utf16ToUtf8(text.length),
              ),
              payload: const {'level': 1},
            ),
          ],
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf16ToUtf8(5),
                endByte: mapper.utf16ToUtf8(6),
              ),
              styles: const {'bold'},
            ),
          ],
          markerRanges: [
            NativeComrakRange(
              startByte: mapper.utf16ToUtf8(4),
              endByte: mapper.utf16ToUtf8(6),
            ),
            NativeComrakRange(
              startByte: mapper.utf16ToUtf8(3),
              endByte: mapper.utf16ToUtf8(4),
            ),
          ],
          exclusionRanges: [
            NativeComrakRange(
              startByte: mapper.utf16ToUtf8(3),
              endByte: mapper.utf16ToUtf8(text.length),
            ),
          ],
        ),
      );
      final backend = ComrakCommonMarkParseBackend(bridge: bridge);

      final snapshot = await backend.parse(
        const SyntaxParseRequest(revision: 5, text: text),
      );

      expect(snapshot.blocks, hasLength(1));
      expect(snapshot.blocks.first.type, BlockType.header);
      expect(snapshot.blocks.first.start, 3);
      expect(snapshot.blocks.first.end, text.length);
      expect(snapshot.blocks.first.payload['level'], 1);
      expect(snapshot.inlineTokens, hasLength(1));
      expect(snapshot.inlineTokens.first.start, 5);
      expect(snapshot.inlineTokens.first.end, 6);
      expect(
        snapshot.inlineTokens.first.style,
        const SovereignStyle({SovereignStyleType.bold}),
      );
      expect(snapshot.markerRanges, const [
        TextRange(start: 3, end: 4),
        TextRange(start: 4, end: 6),
      ]);
      expect(snapshot.exclusionRanges, [TextRange(start: 3, end: text.length)]);
      expect(snapshot.cursorMask.snapToSafeOffset(4), 4);
      expect(snapshot.diagnostics, isEmpty);
    });

    test('drops unknown block types and records diagnostics', () async {
      final bridge = _FakeNativeComrakBridge(
        result: const NativeComrakParseResult(
          revision: 7,
          blocks: [
            NativeComrakBlockSpan(
              type: 'mystery',
              range: NativeComrakRange(startByte: 0, endByte: 3),
            ),
          ],
        ),
      );
      final backend = ComrakCommonMarkParseBackend(bridge: bridge);

      final snapshot = await backend.parse(
        const SyntaxParseRequest(revision: 7, text: 'abc'),
      );

      expect(snapshot.blocks, isEmpty);
      expect(
        snapshot.diagnostics.any((d) => d.code == 'COMRAK_UNKNOWN_BLOCK'),
        isTrue,
      );
    });

    test(
      'does not synthesize inline or marker ranges when payload omits them',
      () async {
        final bridge = _FakeNativeComrakBridge(
          result: const NativeComrakParseResult(revision: 8),
        );
        final backend = ComrakCommonMarkParseBackend(bridge: bridge);
        const request = SyntaxParseRequest(revision: 8, text: '**bold**');

        final snapshot = await backend.parse(request);

        expect(snapshot.inlineTokens, isEmpty);
        expect(snapshot.markerRanges, isEmpty);
        expect(snapshot.diagnostics, isEmpty);
      },
    );

    test('maps native link inline style into sovereign link token', () async {
      const text = '[OpenAI](https://openai.com)';
      final mapper = Utf8Utf16OffsetMapper.fromText(text);
      final bridge = _FakeNativeComrakBridge(
        result: NativeComrakParseResult(
          revision: 31,
          inlineTokens: [
            NativeComrakInlineToken(
              range: NativeComrakRange(
                startByte: mapper.utf16ToUtf8(1),
                endByte: mapper.utf16ToUtf8(7),
              ),
              styles: const {'link'},
            ),
          ],
        ),
      );
      final backend = ComrakCommonMarkParseBackend(bridge: bridge);

      final snapshot = await backend.parse(
        const SyntaxParseRequest(revision: 31, text: text),
      );

      expect(snapshot.inlineTokens, hasLength(1));
      expect(snapshot.inlineTokens.first.start, 1);
      expect(snapshot.inlineTokens.first.end, 7);
      expect(
        snapshot.inlineTokens.first.style,
        const SovereignStyle({SovereignStyleType.link}),
      );
    });

    test(
      'supplements link/image inline markers and avoids duplicate overlapping image tokens',
      () async {
        const text = '[link](/uri "title") ![alt](u)';
        final mapper = Utf8Utf16OffsetMapper.fromText(text);
        final bridge = _FakeNativeComrakBridge(
          result: NativeComrakParseResult(
            revision: 31,
            inlineTokens: [
              // Deliberately broader than the markdown image alt range to
              // mimic native sourcepos behavior and guard against duplicate
              // supplemental image tokens.
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf16ToUtf8(text.indexOf('![')),
                  endByte: mapper.utf16ToUtf8(text.length),
                ),
                styles: const {'image'},
              ),
            ],
          ),
        );
        final backend = ComrakCommonMarkParseBackend(bridge: bridge);

        final snapshot = await backend.parse(
          const SyntaxParseRequest(revision: 31, text: text),
        );

        final styleNames = snapshot.inlineTokens
            .map(
              (t) => t.style.types.map((s) => s.name).toList(growable: false)
                ..sort(),
            )
            .toList(growable: false);
        expect(
          styleNames.any((names) => names.length == 1 && names.first == 'link'),
          isTrue,
        );
        expect(
          snapshot.inlineTokens
              .where((t) => t.style.types.contains(SovereignStyleType.image))
              .length,
          1,
        );
        expect(
          _visibleTextAfterHiddenRanges(text, snapshot.markerRanges),
          equals('link alt'),
        );
      },
    );

    test(
      'maps new native block and inline types (table/thematic/image)',
      () async {
        const text = '---\n\n| a |\n| - |\n| b |\n\n![alt](u)';
        final mapper = Utf8Utf16OffsetMapper.fromText(text);
        final bridge = _FakeNativeComrakBridge(
          result: NativeComrakParseResult(
            revision: 32,
            blocks: [
              NativeComrakBlockSpan(
                type: 'thematic_break',
                range: NativeComrakRange(
                  startByte: mapper.utf16ToUtf8(0),
                  endByte: mapper.utf16ToUtf8(4),
                ),
              ),
              NativeComrakBlockSpan(
                type: 'table',
                range: NativeComrakRange(
                  startByte: mapper.utf16ToUtf8(5),
                  endByte: mapper.utf16ToUtf8(24),
                ),
                payload: const {'columns': 1, 'rows': 2},
              ),
            ],
            inlineTokens: [
              NativeComrakInlineToken(
                range: NativeComrakRange(
                  startByte: mapper.utf16ToUtf8(26),
                  endByte: mapper.utf16ToUtf8(text.length),
                ),
                styles: const {'image'},
              ),
            ],
          ),
        );
        final backend = ComrakCommonMarkParseBackend(bridge: bridge);

        final snapshot = await backend.parse(
          const SyntaxParseRequest(revision: 32, text: text),
        );

        expect(
          snapshot.blocks.map((b) => b.type),
          containsAll(const [BlockType.thematicBreak, BlockType.table]),
        );
        final imageToken = snapshot.inlineTokens.singleWhere(
          (t) => t.style.types.contains(SovereignStyleType.image),
        );
        expect(
          imageToken.style,
          const SovereignStyle({SovereignStyleType.image}),
        );
      },
    );

    test('withNativeBridge throws when library is missing', () {
      expect(
        () => ComrakCommonMarkParseBackend.withNativeBridge(
          overrideLibraryPath:
              '/definitely/missing/libsovereign_comrak_bridge.so',
        ),
        throwsA(isA<Object>()),
      );
    });

    test('withNativeBridge maps real native parse output', () async {
      final libPath = sovereignNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        return;
      }

      final backend = ComrakCommonMarkParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      );
      const request = SyntaxParseRequest(
        revision: 16,
        text: '# Header\n\n```\ncode\n```\n',
      );

      final snapshot = await backend.parse(request);

      expect(snapshot.revision, 16);
      expect(
        snapshot.blocks.any((block) => block.type == BlockType.header),
        isTrue,
      );
      expect(
        snapshot.blocks.any((block) => block.type == BlockType.fencedCode),
        isTrue,
      );
      expect(snapshot.exclusionRanges, isNotEmpty);
      expect(snapshot.diagnostics.where((d) => d.isError), isEmpty);
    });

    test(
      'real native parse surfaces table/thematic/image nodes in GFM mode',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final backend = ComrakCommonMarkParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );
        const request = SyntaxParseRequest(
          revision: 17,
          profile: MarkdownSyntaxProfile.commonMarkGfm,
          text:
              '---\n\n| a |\n| - |\n| b |\n\n![alt](https://img.example/x.png)\n',
        );

        final snapshot = await backend.parse(request);

        expect(
          snapshot.blocks.any((block) => block.type == BlockType.thematicBreak),
          isTrue,
        );
        expect(
          snapshot.blocks.any((block) => block.type == BlockType.table),
          isTrue,
        );
        expect(
          snapshot.inlineTokens.any(
            (token) =>
                token.style == const SovereignStyle({SovereignStyleType.image}),
          ),
          isTrue,
        );
      },
    );

    test(
      'native backend matches bootstrap block/exclusion shape for sample text',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final nativeBackend = ComrakCommonMarkParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );
        const bootstrapBackend = BootstrapCommonMarkParseBackend();
        const request = SyntaxParseRequest(
          revision: 23,
          text:
              '# Header\n\n> Quote\n\n- item\n\ninline **bold** and `code`\n\n```dart\nfinal x = 1;\n```\n',
        );

        final native = await nativeBackend.parse(request);
        final bootstrap = await bootstrapBackend.parse(request);

        final nativeTypes =
            native.blocks.map((block) => block.type).toList(growable: false);
        final bootstrapTypes =
            bootstrap.blocks.map((block) => block.type).toList(growable: false);
        expect(nativeTypes, contains(BlockType.header));
        expect(nativeTypes, contains(BlockType.blockquote));
        expect(nativeTypes, contains(BlockType.unorderedList));
        expect(nativeTypes, contains(BlockType.fencedCode));
        expect(
          bootstrapTypes,
          containsAll(const [
            BlockType.header,
            BlockType.blockquote,
            BlockType.unorderedList,
            BlockType.fencedCode,
          ]),
        );
        expect(native.exclusionRanges, bootstrap.exclusionRanges);
        expect(native.markerRanges, isNotEmpty);
        expect(
          native.inlineTokens.any(
            (token) =>
                token.style == const SovereignStyle({SovereignStyleType.bold}),
          ),
          isTrue,
        );
      },
    );

    test(
      'real native parse emits utf16 scalar-boundary-safe ranges for mixed unicode',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final backend = ComrakCommonMarkParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        );
        const text = '# 🎨 e\u0301 אב\n\n`code` and **bold**\n';
        const request = SyntaxParseRequest(revision: 24, text: text);

        final snapshot = await backend.parse(request);
        final mapper = Utf8Utf16OffsetMapper.fromText(text);

        void expectBoundary(int offset) {
          expect(
            mapper.isUtf16ScalarBoundary(offset),
            isTrue,
            reason: 'Offset $offset must map to a UTF-16 scalar boundary.',
          );
        }

        for (final block in snapshot.blocks) {
          expectBoundary(block.start);
          expectBoundary(block.end);
        }
        for (final token in snapshot.inlineTokens) {
          expectBoundary(token.start);
          expectBoundary(token.end);
        }
        for (final range in snapshot.markerRanges) {
          expectBoundary(range.start);
          expectBoundary(range.end);
        }
        for (final range in snapshot.exclusionRanges) {
          expectBoundary(range.start);
          expectBoundary(range.end);
        }
      },
    );
  });
}

String _visibleTextAfterHiddenRanges(
  String text,
  List<TextRange> hiddenRanges,
) {
  if (hiddenRanges.isEmpty) return text;
  final buffer = StringBuffer();
  var cursor = 0;
  for (final range in hiddenRanges) {
    final start = range.start.clamp(0, text.length);
    final end = range.end.clamp(0, text.length);
    if (start > cursor) buffer.write(text.substring(cursor, start));
    if (end > cursor) cursor = end;
  }
  if (cursor < text.length) buffer.write(text.substring(cursor));
  return buffer.toString();
}
