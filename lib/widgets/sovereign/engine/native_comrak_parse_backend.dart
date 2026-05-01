import 'dart:convert';

import 'package:flutter/services.dart';

import '../logic/sovereign_style_scanner.dart';
import '../models/block_node.dart';
import '../models/sovereign_style.dart';
import 'commonmark_parse_backend.dart';
import 'native_comrak_bridge_factory.dart';
import 'native_comrak_ffi.dart';
import 'syntax_engine.dart';
import 'syntax_snapshot.dart';
import 'syntax_types.dart';
import 'utf8_utf16_offset_mapper.dart';

/// Native comrak backend scaffold.
///
/// This file intentionally keeps mapping logic local so the eventual FFI parser
/// swap can reuse the same normalization + UTF-16 conversion path.
class ComrakCommonMarkParseBackend implements CommonMarkParseBackend {
  final NativeComrakBridge bridge;

  const ComrakCommonMarkParseBackend({required this.bridge});

  factory ComrakCommonMarkParseBackend.withNativeBridge({
    String? overrideLibraryPath,
  }) {
    return ComrakCommonMarkParseBackend(
      bridge: createNativeComrakBridge(
        overrideLibraryPath: overrideLibraryPath,
      ),
    );
  }

  @override
  String get backendId => 'comrak_native_v1';

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    final text = request.text;
    if (text.isEmpty) {
      return SyntaxSnapshot.empty(revision: request.revision, textLength: 0);
    }

    final result = await bridge.parse(
      NativeComrakParseInput(
        revision: request.revision,
        profile: _mapProfile(request.profile),
        utf8Text: Uint8List.fromList(utf8.encode(text)),
      ),
    );

    final mapper = Utf8Utf16OffsetMapper.fromText(text);
    final diagnostics = <SyntaxDiagnostic>[
      if (result.revision != request.revision)
        const SyntaxDiagnostic(
          start: 0,
          end: 0,
          message: 'Native parse revision mismatch.',
          code: 'COMRAK_REVISION_MISMATCH',
          isError: true,
        ),
    ];

    final blocks = <BlockSpan>[];
    for (final block in result.blocks) {
      final blockType = _mapBlockType(block.type);
      if (blockType == null) {
        diagnostics.add(
          SyntaxDiagnostic(
            start: 0,
            end: 0,
            message: 'Unknown comrak block type: ${block.type}',
            code: 'COMRAK_UNKNOWN_BLOCK',
            isError: true,
          ),
        );
        continue;
      }
      final range = _mapRange(mapper, block.range);
      if (range.end <= range.start) continue;
      blocks.add(
        BlockSpan(
          type: blockType,
          start: range.start,
          end: range.end,
          payload: block.payload,
        ),
      );
    }

    final nativeInlineTokens = <InlineSpanToken>[];
    for (final token in result.inlineTokens) {
      final style = _mapInlineStyle(token.styles);
      if (style == null) continue;
      final range = _mapRange(mapper, token.range);
      if (range.end <= range.start) continue;
      nativeInlineTokens.add(
        InlineSpanToken(style: style, start: range.start, end: range.end),
      );
    }

    final exclusionRanges = _normalizeRanges(
      result.exclusionRanges.map((range) => _mapRange(mapper, range)),
      text.length,
    );
    final supplementalInlineRuns = _collectSupplementalLinkAndImageRuns(
      text,
      exclusionRanges,
    );
    final inlineTokens = _mergeInlineTokens(
      nativeInlineTokens,
      supplementalInlineRuns,
    );
    final supplementalMarkerRanges = SovereignStyleScanner.extractHiddenRanges(
      text,
      supplementalInlineRuns,
    );
    final markerRanges = _normalizeRanges(
      [
        ...result.markerRanges.map((range) => _mapRange(mapper, range)),
        ...supplementalMarkerRanges,
      ],
      text.length,
      mergeAdjacent: false,
    );

    for (final diagnostic in result.diagnostics) {
      final range = _mapRange(mapper, diagnostic.range);
      diagnostics.add(
        SyntaxDiagnostic(
          start: range.start,
          end: range.end,
          message: diagnostic.message,
          code: diagnostic.code,
          isError: diagnostic.isError,
        ),
      );
    }

    return SyntaxSnapshot(
      revision: request.revision,
      blocks: blocks,
      inlineTokens: inlineTokens,
      markerRanges: markerRanges,
      exclusionRanges: exclusionRanges,
      ambiguityZones: const [],
      cursorMask: HiddenRangeCursorValidationMask(
        textLength: text.length,
        hiddenRanges: markerRanges,
      ),
      diagnostics: diagnostics,
    );
  }

  static NativeComrakProfile _mapProfile(MarkdownSyntaxProfile profile) {
    return switch (profile) {
      MarkdownSyntaxProfile.commonMarkCore =>
        NativeComrakProfile.commonMarkCore,
      MarkdownSyntaxProfile.commonMarkGfm => NativeComrakProfile.commonMarkGfm,
    };
  }

  static BlockType? _mapBlockType(String type) {
    return switch (type) {
      'paragraph' => BlockType.paragraph,
      'header' => BlockType.header,
      'thematic_break' => BlockType.thematicBreak,
      'fenced_code' => BlockType.fencedCode,
      'blockquote' => BlockType.blockquote,
      'unordered_list' => BlockType.unorderedList,
      'ordered_list' => BlockType.orderedList,
      'table' => BlockType.table,
      _ => null,
    };
  }

  static SovereignStyle? _mapInlineStyle(Set<String> styles) {
    if (styles.isEmpty) return null;

    final mapped = <SovereignStyleType>{};
    for (final style in styles) {
      switch (style) {
        case 'bold':
          mapped.add(SovereignStyleType.bold);
          break;
        case 'italic':
          mapped.add(SovereignStyleType.italic);
          break;
        case 'code':
          mapped.add(SovereignStyleType.code);
          break;
        case 'link':
          mapped.add(SovereignStyleType.link);
          break;
        case 'image':
          mapped.add(SovereignStyleType.image);
          break;
        default:
          break;
      }
    }
    if (mapped.isEmpty) return null;
    return SovereignStyle(mapped);
  }

  static TextRange _mapRange(
    Utf8Utf16OffsetMapper mapper,
    NativeComrakRange range,
  ) {
    final start = mapper.utf8ToUtf16(range.startByte);
    final end = mapper.utf8ToUtf16(range.endByte);
    if (end < start) return TextRange(start: start, end: start);
    return TextRange(start: start, end: end);
  }

  static List<StyleRun> _collectSupplementalLinkAndImageRuns(
    String text,
    List<TextRange> exclusionRanges,
  ) {
    final scan = SovereignStyleScanner.scan(
      text,
      excludedRanges: exclusionRanges,
      // Authoritative snapshots should not rely on tiny predictive budgets.
      timeBudgetMicros: 50000,
      spanBudget: 4096,
      charLimit: text.length,
    );
    return scan.runs
        .where(
          (run) =>
              run.style.types.contains(SovereignStyleType.link) ||
              run.style.types.contains(SovereignStyleType.image),
        )
        .toList(growable: false);
  }

  static List<InlineSpanToken> _mergeInlineTokens(
    List<InlineSpanToken> nativeInlineTokens,
    List<StyleRun> supplementalRuns,
  ) {
    if (supplementalRuns.isEmpty) return nativeInlineTokens;

    final seen = <String>{
      for (final token in nativeInlineTokens)
        '${token.start}:${token.end}:${_inlineStyleKey(token.style)}',
    };
    final merged = <InlineSpanToken>[...nativeInlineTokens];
    for (final run in supplementalRuns) {
      final styleKey = _inlineStyleKey(run.style);
      final key = '${run.start}:${run.end}:$styleKey';
      if (!seen.add(key)) continue;
      final overlapsSameStyle = merged.any(
        (token) =>
            _inlineStyleKey(token.style) == styleKey &&
            token.start < run.end &&
            run.start < token.end,
      );
      if (overlapsSameStyle) continue;
      merged.add(
        InlineSpanToken(style: run.style, start: run.start, end: run.end),
      );
    }
    merged.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });
    return merged;
  }

  static String _inlineStyleKey(SovereignStyle style) {
    final names = style.types.map((t) => t.name).toList(growable: false)
      ..sort();
    return names.join('+');
  }

  static List<TextRange> _normalizeRanges(
    Iterable<TextRange> ranges,
    int textLength, {
    bool mergeAdjacent = true,
  }) {
    final sorted = ranges
        .map(
          (range) => TextRange(
            start: range.start.clamp(0, textLength).toInt(),
            end: range.end.clamp(0, textLength).toInt(),
          ),
        )
        .where((range) => range.end > range.start)
        .toList(growable: true)
      ..sort((a, b) {
        final byStart = a.start.compareTo(b.start);
        if (byStart != 0) return byStart;
        return a.end.compareTo(b.end);
      });

    if (sorted.isEmpty) return const [];

    final merged = <TextRange>[];
    for (final range in sorted) {
      if (merged.isEmpty) {
        merged.add(range);
        continue;
      }
      final last = merged.last;
      final shouldMerge =
          mergeAdjacent ? range.start <= last.end : range.start < last.end;
      if (shouldMerge) {
        merged[merged.length - 1] = TextRange(
          start: last.start,
          end: range.end > last.end ? range.end : last.end,
        );
      } else {
        merged.add(range);
      }
    }
    return merged;
  }
}
