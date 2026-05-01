import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'support/test_paths.dart';

class _FixtureCase {
  const _FixtureCase({required this.id, required this.markdown});

  final String id;
  final String markdown;

  factory _FixtureCase.fromJson(Map<String, dynamic> json) {
    return _FixtureCase(
      id: (json['id'] as String?) ?? 'unknown',
      markdown: (json['markdown'] as String?) ?? '',
    );
  }
}

List<_FixtureCase> _loadFixtureCases(String fixturePath) {
  final raw = File(fixturePath).readAsStringSync();
  final decoded = jsonDecode(raw);
  if (decoded is! List) {
    throw StateError('Fixture file must decode to a JSON array: $fixturePath');
  }

  return decoded
      .whereType<Map<String, dynamic>>()
      .map(_FixtureCase.fromJson)
      .where((fixture) => fixture.markdown.isNotEmpty)
      .toList(growable: false);
}

bool _hasNativeFallbackDiagnostics(SyntaxSnapshot snapshot) {
  return snapshot.diagnostics.any(
    (diagnostic) =>
        diagnostic.code == 'COMRAK_INLINE_FALLBACK_USED' ||
        diagnostic.code == 'COMRAK_MARKER_FALLBACK_USED',
  );
}

bool _areRangesValid(List<TextRange> ranges, int textLength) {
  for (final range in ranges) {
    if (range.start < 0 || range.end > textLength || range.start >= range.end) {
      return false;
    }
  }
  return true;
}

bool _areBlocksValid(List<BlockSpan> blocks, int textLength) {
  for (final block in blocks) {
    if (block.start < 0 || block.end > textLength || block.start >= block.end) {
      return false;
    }
  }
  return true;
}

bool _areInlineTokensNormalized(List<InlineSpanToken> tokens, int textLength) {
  for (final token in tokens) {
    if (token.start < 0 || token.end > textLength || token.start >= token.end) {
      return false;
    }
  }
  return true;
}

List<String> _blockSignature(SyntaxSnapshot snapshot) {
  return snapshot.blocks
      .map((block) => '${block.type.name}:${block.start}:${block.end}')
      .toList(growable: false);
}

List<String> _inlineSignature(SyntaxSnapshot snapshot) {
  String styleKey(InlineSpanToken token) {
    final styleNames = token.style.types
        .map((type) => type.name)
        .toList(growable: false)
      ..sort();
    return styleNames.join('+');
  }

  return snapshot.inlineTokens
      .map((token) => '${styleKey(token)}:${token.start}:${token.end}')
      .toList(growable: false);
}

void main() {
  final coreFixturePath = sovereignFixturePath('commonmark/core_cases.json');
  final gfmFixturePath = sovereignFixturePath('commonmark/gfm_cases.json');
  final coreCases = _loadFixtureCases(coreFixturePath);
  final gfmCases = _loadFixtureCases(gfmFixturePath);

  group('Native CommonMark fixture contracts', () {
    final libPath = sovereignNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; fixture contract suite skipped', () {
        expect(true, isTrue);
      });
      return;
    }

    final nativeBackend = ComrakCommonMarkParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    Future<void> expectFixtureContracts({
      required _FixtureCase fixture,
      required MarkdownSyntaxProfile profile,
      required int revision,
    }) async {
      final request = SyntaxParseRequest(
        revision: revision,
        text: fixture.markdown,
        profile: profile,
      );
      final first = await nativeBackend.parse(request);
      final second = await nativeBackend.parse(request);
      final textLength = fixture.markdown.length;

      expect(
        first.diagnostics.where((diagnostic) => diagnostic.isError),
        isEmpty,
        reason: 'Native backend reported an error for fixture ${fixture.id}',
      );
      expect(
        _hasNativeFallbackDiagnostics(first),
        isFalse,
        reason:
            'Native fallback scanner unexpectedly used for fixture ${fixture.id}',
      );
      expect(
        _areRangesValid(first.markerRanges, textLength),
        isTrue,
        reason: 'Marker ranges invalid for fixture ${fixture.id}',
      );
      expect(
        _areRangesValid(first.exclusionRanges, textLength),
        isTrue,
        reason: 'Exclusion ranges invalid for fixture ${fixture.id}',
      );
      expect(
        _areBlocksValid(first.blocks, textLength),
        isTrue,
        reason: 'Block spans invalid for fixture ${fixture.id}',
      );
      expect(
        _areInlineTokensNormalized(first.inlineTokens, textLength),
        isTrue,
        reason: 'Inline tokens invalid for fixture ${fixture.id}',
      );

      expect(
        first.markerRanges,
        second.markerRanges,
        reason: 'Marker ranges are not deterministic for fixture ${fixture.id}',
      );
      expect(
        first.exclusionRanges,
        second.exclusionRanges,
        reason:
            'Exclusion ranges are not deterministic for fixture ${fixture.id}',
      );
      expect(
        _blockSignature(first),
        _blockSignature(second),
        reason:
            'Block signatures are not deterministic for fixture ${fixture.id}',
      );
      expect(
        _inlineSignature(first),
        _inlineSignature(second),
        reason:
            'Inline signatures are not deterministic for fixture ${fixture.id}',
      );

      expect(
        first.cursorMask.snapToSafeOffset(-100),
        inInclusiveRange(0, textLength),
      );
      expect(
        first.cursorMask.snapToSafeOffset(textLength + 100),
        inInclusiveRange(0, textLength),
      );
    }

    for (var i = 0; i < coreCases.length; i++) {
      final fixture = coreCases[i];
      test('core fixture contracts: ${fixture.id}', () async {
        await expectFixtureContracts(
          fixture: fixture,
          profile: MarkdownSyntaxProfile.commonMarkCore,
          revision: i + 1000,
        );
      });
    }

    for (var i = 0; i < gfmCases.length; i++) {
      final fixture = gfmCases[i];
      test('gfm fixture contracts: ${fixture.id}', () async {
        await expectFixtureContracts(
          fixture: fixture,
          profile: MarkdownSyntaxProfile.commonMarkGfm,
          revision: i + 2000,
        );
      });
    }
  });
}
