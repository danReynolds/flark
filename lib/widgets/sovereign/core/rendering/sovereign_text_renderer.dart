import 'dart:collection';

import 'package:flutter/widgets.dart';

import 'package:sovereign_editor/helpers/logger.dart';
import 'package:sovereign_editor/theme/dune_markdown_theme.dart';

import '../../logic/fenced_code_scanner.dart';
import '../../logic/markdown_marker_grammar.dart';
import '../../logic/sovereign_code_highlighter.dart';
import '../../logic/sovereign_markdown_markers.dart';
import '../../logic/sovereign_style_scanner.dart';
import '../../models/block_node.dart';
import '../../models/block_tree.dart';
import '../../models/decoration_model.dart';
import '../../models/sovereign_style.dart';
import '../../theme/sovereign_editor_theme.dart';
import 'editor_heading_style_policy.dart';
import '../structure/markdown_line_helpers.dart';
import '../structure/models/list_marker_context.dart' as structure;
import '../structure/models/task_marker_info.dart' as structure;

part 'sovereign_text_renderer_block_runs_builder.dart';
part 'sovereign_text_renderer_code_highlight_builder.dart';
part 'sovereign_text_renderer_inline_runs.dart';
part 'sovereign_text_renderer_markers.dart';
part 'sovereign_text_renderer_span_builder.dart';

class SovereignTextRenderer {
  String? _cachedKey;
  List<StyleRun>? _cachedRuns;
  List<_CodeStyleRun>? _cachedCodeRuns;
  List<_BlockStyleRun>? _cachedBlockRuns;
  List<TextRange>? _cachedRenderHiddenRanges;
  final _SovereignCodeHighlightRunsBuilder _codeHighlightRunsBuilder =
      _SovereignCodeHighlightRunsBuilder();
  static const _SovereignBlockStyleRunsBuilder _blockStyleRunsBuilder =
      _SovereignBlockStyleRunsBuilder();

  int _renderCallCount = 0;
  int _renderLastMicros = 0;
  int _renderMaxMicros = 0;

  int get renderCallCount => _renderCallCount;
  int get renderLastMicros => _renderLastMicros;
  int get renderMaxMicros => _renderMaxMicros;

  void resetRenderTelemetry() {
    _renderCallCount = 0;
    _renderLastMicros = 0;
    _renderMaxMicros = 0;
  }

  TextSpan render({
    required BuildContext context,
    required String text,
    TextStyle? style,
    required int revision,
    required DecorationModel latestDecoration,
    required List<TextRange> projectedExclusionRanges,
    required int authoritativeInlineRunsRevision,
    required List<StyleRun>? authoritativeInlineRuns,
  }) {
    final sw = Stopwatch()..start();
    final editorTheme = SovereignEditorThemeScope.maybeOf(context);
    final markdownTheme = editorTheme?.resolveMarkdownTheme(context) ??
        DuneMarkdownTheme.of(context);

    final exclusionsRevision =
        latestDecoration.originRevision == revision ? revision : -1;
    // Theme identity participates in cache invalidation so syntax-highlight
    // styles refresh immediately when the host theme changes.
    final themeCacheIdentity = identityHashCode(markdownTheme);
    final cacheKey = '$revision:$exclusionsRevision:$themeCacheIdentity';

    if (_cachedKey != cacheKey) {
      final excludedRanges =
          _SovereignTextRendererInlineRuns.inlineExcludedRangesForBuildTextSpan(
        text,
        latestDecoration: latestDecoration,
        revision: revision,
        projectedExclusionRanges: projectedExclusionRanges,
      );
      final inlineStyleExcludedRanges =
          _SovereignTextRendererInlineRuns.inlineStyleScanExcludedRanges(
        text: text,
        baseExcludedRanges: excludedRanges,
      );
      List<TextRange>? supplementalInlineHiddenRanges;

      final canUseAuthoritativeInlineRuns =
          latestDecoration.originRevision == revision &&
              authoritativeInlineRunsRevision == revision &&
              authoritativeInlineRuns != null;

      if (canUseAuthoritativeInlineRuns) {
        _cachedRuns = _SovereignTextRendererInlineRuns
            .mergeAuthoritativeRunsWithLocalSupplementalInline(
          text: text,
          authoritativeRuns: authoritativeInlineRuns,
          excludedRanges: excludedRanges,
        );
        supplementalInlineHiddenRanges = _SovereignTextRendererInlineRuns
            .localSupplementalInlineHiddenRanges(
          text: text,
          excludedRanges: excludedRanges,
        );
      } else {
        try {
          final result = SovereignStyleScanner.scan(
            text,
            excludedRanges: inlineStyleExcludedRanges,
          );
          _cachedRuns = result.runs;
        } catch (e) {
          _cachedRuns = [];
          _SovereignRendererUtils.logger.log(
            'SovereignStyleScanner failed: $e',
          );
        }
      }

      _cachedCodeRuns = _codeHighlightRunsBuilder.buildRuns(
        text,
        markdownTheme,
      );
      _cachedBlockRuns = _blockStyleRunsBuilder.buildRuns(
        text,
        latestDecoration.tree,
      );
      _cachedRenderHiddenRanges =
          _SovereignTextRendererInlineRuns.buildRenderHiddenRangesForTextSpan(
        text: text,
        authoritativeHiddenRanges: latestDecoration.hiddenRanges,
        cachedRuns: _cachedRuns ?? const <StyleRun>[],
        includeInlineHiddenFromCachedRuns: !canUseAuthoritativeInlineRuns,
        supplementalInlineHiddenRanges: supplementalInlineHiddenRanges,
      );

      _cachedKey = cacheKey;
    }

    final children = <InlineSpan>[];
    var currentOffset = 0;

    final plainStyle = style ?? const TextStyle();
    final inlineOverrides = editorTheme?.inlineText;
    final boldStyle = plainStyle
        .copyWith(fontWeight: FontWeight.bold)
        .merge(inlineOverrides?.bold);
    final italicStyle = plainStyle
        .copyWith(fontStyle: FontStyle.italic)
        .merge(inlineOverrides?.italic);
    final codeStyle = markdownTheme
        .inlineCodeStyleFor(plainStyle)
        .merge(inlineOverrides?.inlineCode);
    final linkStyle =
        markdownTheme.linkStyleFor(plainStyle).merge(inlineOverrides?.link);
    final imageStyle = markdownTheme
        .linkStyleFor(plainStyle)
        .copyWith(decoration: TextDecoration.none, fontStyle: FontStyle.normal)
        .merge(inlineOverrides?.image);

    final hiddenRanges =
        _cachedRenderHiddenRanges ?? latestDecoration.hiddenRanges;
    final taskCheckboxTheme = editorTheme?.taskCheckbox;
    final headingTheme = editorTheme?.headings;
    var hiddenIndex = 0;
    final codeRuns = _cachedCodeRuns ?? const <_CodeStyleRun>[];
    var codeIndex = 0;
    final blockRuns = _cachedBlockRuns ?? const <_BlockStyleRun>[];
    var blockIndex = 0;

    if (_cachedRuns != null) {
      for (final run in _cachedRuns!) {
        if (run.start > currentOffset) {
          final idx = _SovereignTextRendererSpanBuilder.appendSpan(
            children,
            text,
            currentOffset,
            run.start,
            plainStyle,
            hiddenRanges,
            hiddenIndex,
            codeRuns,
            codeIndex,
            blockRuns,
            blockIndex,
            markdownTheme,
            headingTheme,
            taskCheckboxTheme,
          );
          hiddenIndex = idx.hiddenIndex;
          codeIndex = idx.codeIndex;
          blockIndex = idx.blockIndex;
          currentOffset = run.start;
        }

        var runTextStyle = plainStyle;
        if (run.style.types.contains(SovereignStyleType.link)) {
          runTextStyle = runTextStyle.merge(linkStyle);
        }
        if (run.style.types.contains(SovereignStyleType.image)) {
          runTextStyle = runTextStyle.merge(imageStyle);
        }
        if (run.style.types.contains(SovereignStyleType.bold)) {
          runTextStyle = runTextStyle.merge(boldStyle);
        }
        if (run.style.types.contains(SovereignStyleType.italic)) {
          runTextStyle = runTextStyle.merge(italicStyle);
        }
        if (run.style.types.contains(SovereignStyleType.code)) {
          runTextStyle = runTextStyle.merge(codeStyle);
        }

        final idx = _SovereignTextRendererSpanBuilder.appendSpan(
          children,
          text,
          run.start,
          run.end,
          runTextStyle,
          hiddenRanges,
          hiddenIndex,
          codeRuns,
          codeIndex,
          blockRuns,
          blockIndex,
          markdownTheme,
          headingTheme,
          taskCheckboxTheme,
        );
        hiddenIndex = idx.hiddenIndex;
        codeIndex = idx.codeIndex;
        blockIndex = idx.blockIndex;
        currentOffset = run.end;
      }
    }

    if (currentOffset < text.length) {
      _SovereignTextRendererSpanBuilder.appendSpan(
        children,
        text,
        currentOffset,
        text.length,
        plainStyle,
        hiddenRanges,
        hiddenIndex,
        codeRuns,
        codeIndex,
        blockRuns,
        blockIndex,
        markdownTheme,
        headingTheme,
        taskCheckboxTheme,
      );
    }

    final span = TextSpan(children: children, style: style);
    sw.stop();
    _renderCallCount++;
    _renderLastMicros = sw.elapsedMicroseconds;
    if (_renderLastMicros > _renderMaxMicros) {
      _renderMaxMicros = _renderLastMicros;
    }
    return span;
  }
}

class _CodeStyleRun {
  final int start;
  final int end;
  final TextStyle style;

  const _CodeStyleRun({
    required this.start,
    required this.end,
    required this.style,
  });
}

class _RelativeCodeStyleRun {
  final int start;
  final int end;
  final TextStyle style;

  const _RelativeCodeStyleRun({
    required this.start,
    required this.end,
    required this.style,
  });
}

class _AppendSpanResult {
  final int hiddenIndex;
  final int codeIndex;
  final int blockIndex;

  const _AppendSpanResult({
    required this.hiddenIndex,
    required this.codeIndex,
    required this.blockIndex,
  });
}

enum _BlockStyleKind { header, blockquote, thematicBreak, table, taskChecked }

class _BlockStyleRun {
  final int start;
  final int end;
  final _BlockStyleKind kind;
  final int? headerLevel;

  const _BlockStyleRun({
    required this.start,
    required this.end,
    required this.kind,
    this.headerLevel,
  });
}

class _BlockCodeAppendResult {
  final int codeIndex;
  final int blockIndex;

  const _BlockCodeAppendResult({
    required this.codeIndex,
    required this.blockIndex,
  });
}

class _SovereignRendererUtils {
  const _SovereignRendererUtils._();

  static final Logger logger = Logger('SovereignTextRenderer');

  static const int kMaxHighlightedCodeBlockChars = 8000;
  static const int kMaxTotalHighlightedCodeChars = 20000;
  static const int kMaxCodeHighlightCacheEntries = 128;

  static int lineStartForOffset(String text, int offset) =>
      MarkdownLineHelpers.lineStartForOffset(text, offset);

  static int unorderedListMarkerLength(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      MarkdownLineHelpers.unorderedListMarkerLength(text, lineStart, lineEnd);

  static structure.ListMarkerContext? listMarkerForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
        text,
        lineStart,
        lineEnd,
      );

  static structure.TaskMarkerInfo? taskMarkerInfo(
    String text,
    int start,
    int lineEnd,
  ) =>
      MarkdownLineHelpers.taskMarkerInfo(text, start, lineEnd);

  static List<TextRange> normalizeHiddenRanges(
    Iterable<TextRange> ranges,
    int textLength,
  ) {
    final sanitized = <TextRange>[];
    for (final range in ranges) {
      final start = range.start.clamp(0, textLength).toInt();
      final end = range.end.clamp(0, textLength).toInt();
      if (end <= start) continue;
      sanitized.add(TextRange(start: start, end: end));
    }
    sanitized.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });

    final normalized = <TextRange>[];
    for (final range in sanitized) {
      if (normalized.isEmpty) {
        normalized.add(range);
        continue;
      }
      final last = normalized.last;
      if (range.start < last.end) continue;
      if (range.start == last.start && range.end == last.end) continue;
      normalized.add(range);
    }
    return normalized;
  }
}
