import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';
import 'support/bootstrap_commonmark_parse_backend.dart';

class _FakeCommonMarkParseBackend implements CommonMarkParseBackend {
  SyntaxParseRequest? lastRequest;
  int parseCount = 0;

  @override
  String get backendId => 'fake_backend';

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    parseCount++;
    lastRequest = request;
    return SyntaxSnapshot.empty(revision: 999, textLength: request.text.length);
  }
}

void main() {
  group('CommonMarkSyntaxEngineAdapter', () {
    test('supports bootstrap backend for fixture conformance checks', () {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      expect(adapter.parseBackend.backendId, 'bootstrap_commonmark_v2');
    });

    test('parse delegates to injected backend', () async {
      final backend = _FakeCommonMarkParseBackend();
      final adapter = CommonMarkSyntaxEngineAdapter(parseBackend: backend);
      const request = SyntaxParseRequest(
        revision: 1,
        text: '# title',
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );

      final snapshot = await adapter.parse(request);

      expect(backend.parseCount, 1);
      expect(backend.lastRequest, isNotNull);
      expect(backend.lastRequest!.revision, request.revision);
      expect(backend.lastRequest!.profile, request.profile);
      expect(snapshot.revision, 999);
    });

    test('parse accepts commonMarkCore profile', () async {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      const request = SyntaxParseRequest(
        revision: 3,
        text: '  # Title\n  > quote\n  - item\n',
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );

      final snapshot = await adapter.parse(request);
      final headers = snapshot.blocks
          .where((block) => block.type == BlockType.header)
          .toList(growable: false);
      final blockTypes =
          snapshot.blocks.map((block) => block.type).toList(growable: false);

      expect(snapshot.revision, 3);
      expect(headers, isNotEmpty);
      expect(headers.first.payload['level'], 1);
      expect(blockTypes, contains(BlockType.blockquote));
      expect(blockTypes, contains(BlockType.unorderedList));
      expect(snapshot.cursorMask.snapToSafeOffset(999), request.text.length);
    });

    test('parse accepts commonMarkGfm profile', () async {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      const request = SyntaxParseRequest(
        revision: 4,
        text: '- [x] done\n- [ ] todo\n',
        profile: MarkdownSyntaxProfile.commonMarkGfm,
      );

      final snapshot = await adapter.parse(request);

      expect(snapshot.revision, 4);
      expect(snapshot.blocks, isNotEmpty);
      expect(snapshot.blocks.first.type, BlockType.unorderedList);
      expect(snapshot.cursorMask.snapToSafeOffset(999), request.text.length);
    });

    test(
      'parse maps indented fenced code ranges and language payload',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 7,
          text: '  ```dart\n  final x = 1;\n  ```\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);

        expect(snapshot.blocks.length, 1);
        expect(snapshot.blocks.first.type, BlockType.fencedCode);
        expect(snapshot.blocks.first.payload['language'], 'dart');
        expect(snapshot.exclusionRanges, hasLength(1));
        expect(
          snapshot.exclusionRanges.first,
          TextRange(start: 0, end: request.text.length),
        );
        expect(snapshot.markerRanges, contains(TextRange(start: 2, end: 5)));
      },
    );

    test(
      'parse keeps UTF-16 block offsets correct across emoji prefix',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 8,
          text: '🎨\n  # hi\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);

        expect(snapshot.blocks.length, 2);
        expect(snapshot.blocks[0].type, BlockType.paragraph);
        expect(snapshot.blocks[1].type, BlockType.header);
        // Emoji is two UTF-16 code units, plus newline => heading starts at 3.
        expect(snapshot.blocks[1].start, 3);
      },
    );

    test('commonmark parse includes quoted list marker ranges', () async {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      const request = SyntaxParseRequest(
        revision: 10,
        text: '> - alpha\n> 2. beta\n',
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );

      final snapshot = await adapter.parse(request);

      expect(
        snapshot.markerRanges,
        contains(const TextRange(start: 2, end: 4)),
      );
      expect(
        snapshot.markerRanges,
        contains(const TextRange(start: 12, end: 15)),
      );
    });

    test(
      'parse emits link inline tokens for markdown links and autolinks',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const text =
            '[OpenAI](https://openai.com) and https://example.com and <https://dune.ai>';
        const request = SyntaxParseRequest(
          revision: 12,
          text: text,
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);
        final linkTokens = snapshot.inlineTokens
            .where(
              (token) => token.style.types.contains(SovereignStyleType.link),
            )
            .toList(growable: false);

        expect(linkTokens.length, 3);
        expect(
          text.substring(linkTokens[0].start, linkTokens[0].end),
          'OpenAI',
        );
        expect(
          text.substring(linkTokens[1].start, linkTokens[1].end),
          'https://example.com',
        );
        expect(
          text.substring(linkTokens[2].start, linkTokens[2].end),
          'https://dune.ai',
        );
      },
    );

    test(
      'parse keeps escaped delimiters and entities marker-free',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const text =
            r'\*not emphasized\* \_not emphasized\_ \`not code\` &amp;';
        const request = SyntaxParseRequest(
          revision: 13,
          text: text,
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);
        final escapedStar = text.indexOf('*');
        final entity = text.indexOf('&');

        expect(snapshot.inlineTokens, isEmpty);
        expect(snapshot.markerRanges, isEmpty);
        expect(snapshot.cursorMask.snapToSafeOffset(escapedStar), escapedStar);
        expect(snapshot.cursorMask.snapToSafeOffset(entity), entity);
      },
    );

    test(
      'GFM profile emits task checkbox marker range while core does not',
      () async {
        const coreAdapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const gfmAdapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const text = '- [x] done\n';

        const coreRequest = SyntaxParseRequest(
          revision: 14,
          text: text,
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );
        const gfmRequest = SyntaxParseRequest(
          revision: 15,
          text: text,
          profile: MarkdownSyntaxProfile.commonMarkGfm,
        );

        final coreSnapshot = await coreAdapter.parse(coreRequest);
        final gfmSnapshot = await gfmAdapter.parse(gfmRequest);

        const checkboxRange = TextRange(start: 2, end: 6);
        expect(coreSnapshot.markerRanges, isNot(contains(checkboxRange)));
        expect(gfmSnapshot.markerRanges, contains(checkboxRange));
      },
    );

    test(
      'parse recognizes setext headings and hides underline marker',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 11,
          text: 'Heading\n---\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);

        expect(snapshot.blocks, hasLength(1));
        expect(snapshot.blocks.first.type, BlockType.header);
        expect(snapshot.blocks.first.payload['level'], 2);
        expect(
          snapshot.markerRanges,
          contains(const TextRange(start: 8, end: 11)),
        );
      },
    );

    test(
      'parse recognizes setext level-1 heading and hides equals underline',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 15,
          text: 'Heading\n===\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);

        expect(snapshot.blocks, hasLength(1));
        expect(snapshot.blocks.first.type, BlockType.header);
        expect(snapshot.blocks.first.payload['level'], 1);
        expect(
          snapshot.markerRanges,
          contains(const TextRange(start: 8, end: 11)),
        );
      },
    );

    test(
      'parse classifies isolated dashes as thematic break, not setext heading',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 17,
          text: 'Heading\n\n---\n\nBody\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);
        final blockTypes =
            snapshot.blocks.map((block) => block.type).toList(growable: false);

        expect(blockTypes, contains(BlockType.thematicBreak));
        expect(blockTypes, isNot(contains(BlockType.header)));
        final thematicBreak = snapshot.blocks.firstWhere(
          (block) => block.type == BlockType.thematicBreak,
        );
        expect(thematicBreak.start, 9);
        expect(thematicBreak.end, 13);
      },
    );

    test('parse treats indented code block as exclusion range', () async {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      const request = SyntaxParseRequest(
        revision: 12,
        text: '    code\ntext\n',
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );

      final snapshot = await adapter.parse(request);

      expect(snapshot.blocks.map((b) => b.type).toList(growable: false), [
        BlockType.fencedCode,
        BlockType.paragraph,
      ]);
      expect(
        snapshot.exclusionRanges,
        contains(const TextRange(start: 0, end: 9)),
      );
    });

    test(
      'parse does not hide unknown fence info strings as marker ranges',
      () async {
        const adapter = CommonMarkSyntaxEngineAdapter(
          parseBackend: BootstrapCommonMarkParseBackend(),
        );
        const request = SyntaxParseRequest(
          revision: 16,
          text: '```notalanguage\nx\n```\n',
          profile: MarkdownSyntaxProfile.commonMarkCore,
        );

        final snapshot = await adapter.parse(request);

        expect(snapshot.blocks, hasLength(1));
        expect(snapshot.blocks.first.type, BlockType.fencedCode);
        expect(
          snapshot.markerRanges,
          contains(const TextRange(start: 0, end: 3)),
        );
        expect(
          snapshot.markerRanges,
          isNot(contains(const TextRange(start: 3, end: 15))),
        );
      },
    );

    test('predict accepts profile and preserves revision', () {
      const adapter = CommonMarkSyntaxEngineAdapter(
        parseBackend: BootstrapCommonMarkParseBackend(),
      );
      const request = SyntaxPredictRequest(
        revision: 9,
        text: '`code`',
        profile: MarkdownSyntaxProfile.commonMarkGfm,
      );

      final prediction = adapter.predict(request);

      expect(prediction.revision, 9);
      expect(prediction.cursorMask.snapToSafeOffset(999), request.text.length);
    });
  });
}
