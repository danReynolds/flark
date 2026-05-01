part of 'sovereign_text_renderer.dart';

class _SovereignCodeHighlightRunsBuilder {
  int? _codeHighlightThemeIdentity;
  final LinkedHashMap<String, List<_RelativeCodeStyleRun>> _codeHighlightCache =
      LinkedHashMap<String, List<_RelativeCodeStyleRun>>();
  static const Set<String> _plainFenceTags = <String>{
    'plain',
    'text',
    'plaintext',
    'none',
  };

  List<_CodeStyleRun> buildRuns(
    String fullText,
    DuneMarkdownTheme markdownTheme,
  ) {
    if (fullText.isEmpty) return const [];

    final themeIdentity = identityHashCode(markdownTheme);
    if (_codeHighlightThemeIdentity != themeIdentity) {
      _codeHighlightCache.clear();
      _codeHighlightThemeIdentity = themeIdentity;
    }

    final blocks = FencedCodeScanner.scan(fullText);
    if (blocks.isEmpty) return const [];

    final runs = <_CodeStyleRun>[];
    final activeSignatures = <String>{};
    var totalChars = 0;

    for (final block in blocks) {
      final openLineEnd = FencedCodeScanner.endOfLine(fullText, block.start);
      final openLineContentEnd =
          (openLineEnd > 0 && fullText.codeUnitAt(openLineEnd - 1) == 10)
              ? openLineEnd - 1
              : openLineEnd;

      String? fenceTag;
      final infoStart = (block.start + 3).clamp(0, fullText.length);
      if (infoStart < openLineContentEnd) {
        final info = fullText.substring(infoStart, openLineContentEnd).trim();
        if (info.isNotEmpty) {
          fenceTag = info.split(RegExp(r'\s+')).first;
        }
      }

      final normalizedFenceTag = fenceTag?.trim().toLowerCase();
      final hasExplicitTag =
          normalizedFenceTag != null && normalizedFenceTag.isNotEmpty;
      final language = SovereignCodeHighlighter.normalizeFenceTag(fenceTag);
      // Explicit "plain/text" tags intentionally opt out of syntax colors.
      if (hasExplicitTag &&
          language == null &&
          _plainFenceTags.contains(normalizedFenceTag)) {
        continue;
      }

      final contentStart = openLineEnd.clamp(0, fullText.length);
      if (contentStart >= fullText.length) continue;

      final candidateCloseLineStart =
          _SovereignRendererUtils.lineStartForOffset(fullText, block.end - 1);
      final hasClosingFence = candidateCloseLineStart != block.start &&
          candidateCloseLineStart + 3 <= fullText.length &&
          fullText.startsWith('```', candidateCloseLineStart);
      final contentEnd = hasClosingFence
          ? candidateCloseLineStart
          : block.end.clamp(0, fullText.length);
      if (contentEnd <= contentStart) continue;

      final code = fullText.substring(contentStart, contentEnd);
      if (code.isEmpty) continue;

      if (code.length > _SovereignRendererUtils.kMaxHighlightedCodeBlockChars) {
        continue;
      }
      if (totalChars + code.length >
          _SovereignRendererUtils.kMaxTotalHighlightedCodeChars) {
        break;
      }
      totalChars += code.length;

      final signature = _codeHighlightSignature(language, code);
      activeSignatures.add(signature);
      final relativeRuns = _codeHighlightCache.putIfAbsent(
        signature,
        () => _computeRelativeHighlightRuns(
          code: code,
          language: language,
          markdownTheme: markdownTheme,
        ),
      );

      for (final rel in relativeRuns) {
        final s = (contentStart + rel.start).clamp(0, fullText.length);
        final e = (contentStart + rel.end).clamp(0, fullText.length);
        if (e <= s) continue;
        if (runs.isNotEmpty &&
            runs.last.end == s &&
            runs.last.style == rel.style) {
          runs[runs.length - 1] = _CodeStyleRun(
            start: runs.last.start,
            end: e,
            style: rel.style,
          );
        } else {
          runs.add(_CodeStyleRun(start: s, end: e, style: rel.style));
        }
      }
    }

    _trimCodeHighlightCache(activeSignatures);
    return runs;
  }

  static TextStyle? _styleForHighlightClass(
    String className,
    DuneMarkdownTheme markdownTheme,
  ) {
    return markdownTheme.syntaxStyleForClass(className);
  }

  static String _codeHighlightSignature(String? language, String code) {
    final lang = language ?? 'auto';
    final headTail = _headTailFingerprint(code);
    return '$lang:${code.length}:${code.hashCode}:$headTail';
  }

  static int _headTailFingerprint(String text) {
    if (text.length <= 128) return text.hashCode;
    final head = text.substring(0, 64);
    final tail = text.substring(text.length - 64);
    return Object.hash(head, tail);
  }

  void _trimCodeHighlightCache(Set<String> activeSignatures) {
    if (_codeHighlightCache.length <=
        _SovereignRendererUtils.kMaxCodeHighlightCacheEntries) {
      return;
    }
    final staleKeys = <String>[
      for (final key in _codeHighlightCache.keys)
        if (!activeSignatures.contains(key)) key,
    ];
    for (final key in staleKeys) {
      _codeHighlightCache.remove(key);
      if (_codeHighlightCache.length <=
          _SovereignRendererUtils.kMaxCodeHighlightCacheEntries) {
        return;
      }
    }
    while (_codeHighlightCache.length >
        _SovereignRendererUtils.kMaxCodeHighlightCacheEntries) {
      _codeHighlightCache.remove(_codeHighlightCache.keys.first);
    }
  }

  List<_RelativeCodeStyleRun> _computeRelativeHighlightRuns({
    required String code,
    required String? language,
    required DuneMarkdownTheme markdownTheme,
  }) {
    List<CodeHighlightRun> hlRuns;
    try {
      hlRuns = language != null
          ? SovereignCodeHighlighter.instance.highlight(
              code,
              language: language,
            )
          : SovereignCodeHighlighter.instance.highlightAuto(code);
    } catch (_) {
      return const [];
    }

    final relative = <_RelativeCodeStyleRun>[];
    for (final r in hlRuns) {
      final style = _styleForHighlightClass(r.className, markdownTheme);
      if (style == null) continue;

      final s = r.start.clamp(0, code.length).toInt();
      final e = r.end.clamp(0, code.length).toInt();
      if (e <= s) continue;

      if (relative.isNotEmpty &&
          relative.last.end == s &&
          relative.last.style == style) {
        relative[relative.length - 1] = _RelativeCodeStyleRun(
          start: relative.last.start,
          end: e,
          style: style,
        );
      } else {
        relative.add(_RelativeCodeStyleRun(start: s, end: e, style: style));
      }
    }
    return relative;
  }
}
