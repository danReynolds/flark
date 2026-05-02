import 'package:flutter/widgets.dart';

import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';

part 'sovereign_style_scanner_link_image_helpers.dart';
part 'sovereign_style_scanner_models.dart';

/// A strictly budgeted, single-pass scanner for Markdown inline styles.
///
/// Patterns supported (V1):
/// - Bold: **text**
/// - Italic: _text_
/// - Code: `text`
///
/// Invariants:
/// 1. **Prefix Stability**: If budget exhausted, rollback to last safe boundary.
/// 2. **Excluded Ranges**: Respects provided ranges (e.g. Code Blocks) by skipping them.
class SovereignStyleScanner {
  // Budget Constants
  // Budget Constants
  static const int kTimeBudgetMicros =
      1500; // 1.5ms (User Requested for p99 safety)
  static const int kSpanBudget = 250; // User Requested Limit

  /// Scans the [text] and returns a list of styled runs.
  ///
  /// [excludedRanges] must be sorted and non-overlapping.
  static ScannerResult scan(
    String text, {
    List<TextRange> excludedRanges = const [],
    int timeBudgetMicros = kTimeBudgetMicros,
    int spanBudget = kSpanBudget,
    int? charLimit,
  }) {
    final stopwatch = Stopwatch()..start();
    final runs = <StyleRun>[];

    // Scan State
    int currentOffset = 0;
    int lastSafeOffset = 0; // The end of the last successfully closed run

    // Markers
    int? boldStart;
    int? italicStart;
    int? codeStart;

    // Optimizations
    final len = text.length;
    int exclusionIndex = 0;

    ScannerResult finalize(int validTo, bool complete) {
      final supplementalRuns = _mergeSupplementalRuns(
        text: text,
        baseRuns: runs,
        excludedRanges: excludedRanges,
        validTo: validTo,
        spanBudget: spanBudget,
      );
      return ScannerResult(
        runs: supplementalRuns,
        validTo: validTo,
        complete: complete,
      );
    }

    try {
      int checkCounter = 0;
      while (currentOffset < len) {
        // 1. Budget Checks
        // Span Budget: Strict Check (Cheap)
        if (runs.length >= spanBudget) {
          return finalize(lastSafeOffset, false);
        }

        // Time Budget: Amortized (Expensive)
        if (checkCounter++ > 64) {
          checkCounter = 0;
          if (stopwatch.elapsedMicroseconds > timeBudgetMicros) {
            // Budget exhausted.
            return finalize(lastSafeOffset, false);
          }
        }

        // Explicit Char Limit Check (Strict)
        if (charLimit != null && currentOffset >= charLimit) {
          return finalize(lastSafeOffset, false);
        }

        // 2. Excluded Range Skip
        if (exclusionIndex < excludedRanges.length) {
          final range = excludedRanges[exclusionIndex];
          // If we reached the start of an exclusion
          if (currentOffset == range.start) {
            // Close any open styles? In V1/CommonMark, block boundaries (like code fences)
            // typically break emphasis.
            // Safety: Reset local state.
            boldStart = null;
            italicStart = null;
            codeStart = null;

            // Jump to end of exclusion
            currentOffset = range.end;
            lastSafeOffset = currentOffset; // Exclusion boundary is safe
            continue;
          } else if (currentOffset > range.start && currentOffset < range.end) {
            // Should not happen if we jump, but safety check:
            currentOffset = range.end;
            lastSafeOffset = currentOffset;
            continue;
          } else if (currentOffset >= range.end) {
            exclusionIndex++;
          }
        }

        final char = text.codeUnitAt(currentOffset);
        final escapedInlineDelimiter = codeStart == null &&
            (char == 96 || char == 42 || char == 95) &&
            _ScannerLinkImageParsers._isEscapedAt(text, currentOffset);
        if (escapedInlineDelimiter) {
          currentOffset++;
          if (boldStart == null && italicStart == null) {
            lastSafeOffset = currentOffset;
          }
          continue;
        }

        // Inline Code (`...`)
        if (char == 96) {
          // `
          if (codeStart == null) {
            // Start Code
            codeStart = currentOffset;
          } else {
            // End Code?

            // NOTE: Markdown allows `` inside ` if backtick counts differ.
            // V1 Simplification: First backtick closes it.
            // If text is `foo`, we match.
            // If text is ``foo``, we match empty ``, then foo, then empty ``?
            // Correct V1: Match first closing backtick.

            // Create Run
            // Code runs usually override others?
            // V1: Linear scan.
            // V1: Linear scan.
            // Fix: Ignore empty code spans (``) to support fence typing (```).
            // Length must be > 2 to have content.
            if ((currentOffset + 1) - codeStart > 2) {
              _addRun(runs, codeStart, currentOffset + 1, SovereignStyle.code);
            }
            codeStart = null;
            lastSafeOffset = currentOffset + 1;
          }
        }
        // Bold (**)
        else if (codeStart == null && char == 42) {
          // *
          // Check next char for **
          if (currentOffset + 1 < len &&
              text.codeUnitAt(currentOffset + 1) == 42) {
            // It is **
            if (boldStart == null) {
              boldStart = currentOffset;
              currentOffset++; // Skip second *
            } else {
              // Close Bold (Always match **)
              _addRun(runs, boldStart, currentOffset + 2, SovereignStyle.bold);
              boldStart = null;
              currentOffset++; // Skip second *
              lastSafeOffset = currentOffset + 1;
            }
          }
          // Single * (Italic)
          else {
            if (italicStart == null) {
              italicStart = currentOffset;
            } else {
              // Check opener: Only close if opener was * (42)
              if (text.codeUnitAt(italicStart) == 42) {
                _addRun(
                  runs,
                  italicStart,
                  currentOffset + 1,
                  SovereignStyle.italic,
                );
                italicStart = null;
                lastSafeOffset = currentOffset + 1;
              } else {
                // Opener was _ (95), so * is just text inside _italic_
                // e.g. _foo * bar_
              }
            }
          }
        }
        // Italic (_)
        else if (codeStart == null && char == 95) {
          // _
          if (italicStart == null) {
            italicStart = currentOffset;
          } else {
            // Check opener: Only close if opener was _ (95)
            if (text.codeUnitAt(italicStart) == 95) {
              _addRun(
                runs,
                italicStart,
                currentOffset + 1,
                SovereignStyle.italic,
              );
              italicStart = null;
              lastSafeOffset = currentOffset + 1;
            }
          }
        }

        currentOffset++;

        // If no styles are open, this is a safe boundary
        if (codeStart == null && boldStart == null && italicStart == null) {
          lastSafeOffset = currentOffset;
        }
      }
    } catch (e) {
      // Escape Hatch: On any crash, return what we have so far (Prefix Stable)
      return finalize(lastSafeOffset, false);
    }

    // Finished
    return finalize(len, true);
  }

  /// Returns the link under a collapsed caret, if any.
  ///
  /// The caret matches the visible (display) link text range, including the
  /// start/end boundaries, so a caret at the end of the label still counts.
  static SovereignLinkMatch? linkAtCaret(String text, int caret) {
    if (text.isEmpty) return null;
    if (caret < 0 || caret > text.length) return null;

    var offset = 0;
    while (offset < text.length) {
      final match = _ScannerLinkImageParsers._matchLinkDetailedAt(text, offset);
      if (match == null) {
        offset++;
        continue;
      }
      if (match.containsCaret(caret)) {
        return match.toPublic();
      }
      offset = match.nextOffset > offset ? match.nextOffset : offset + 1;
    }
    return null;
  }

  /// Returns the markdown image under a collapsed caret, if any.
  ///
  /// The caret must be within the visible alt-text range.
  static SovereignImageMatch? imageAtCaret(String text, int caret) {
    if (text.isEmpty) return null;
    if (caret < 0 || caret > text.length) return null;

    var offset = 0;
    while (offset < text.length) {
      final match = _ScannerLinkImageParsers._matchMarkdownImageDetailed(
        text,
        offset,
      );
      if (match == null) {
        offset++;
        continue;
      }
      if (match.containsCaret(caret)) {
        return match.toPublic();
      }
      offset = match.nextOffset > offset ? match.nextOffset : offset + 1;
    }
    return null;
  }

  static List<StyleRun> _mergeSupplementalRuns({
    required String text,
    required List<StyleRun> baseRuns,
    required List<TextRange> excludedRanges,
    required int validTo,
    required int spanBudget,
  }) {
    if (text.isEmpty || validTo <= 0) return baseRuns;
    if (baseRuns.length >= spanBudget) return baseRuns;
    if (!_mayHaveSupplementalLinkOrImage(text)) return baseRuns;

    final limit = validTo.clamp(0, text.length);
    final supplementalRuns = <StyleRun>[];
    var cursor = 0;
    var exclusionIndex = 0;

    while (cursor < limit) {
      if (exclusionIndex < excludedRanges.length) {
        final exclusion = excludedRanges[exclusionIndex];
        if (cursor >= exclusion.end) {
          exclusionIndex++;
          continue;
        }
        if (cursor >= exclusion.start && cursor < exclusion.end) {
          cursor = exclusion.end.clamp(0, limit);
          continue;
        }
      }

      final imageMatch = _ScannerLinkImageParsers._matchMarkdownImage(
        text,
        cursor,
      );
      if (imageMatch != null && imageMatch.nextOffset <= limit) {
        final overlapsBase = _overlapsAny(
          baseRuns,
          imageMatch.start,
          imageMatch.end,
        );
        final overlapsSupplemental = _overlapsAny(
          supplementalRuns,
          imageMatch.start,
          imageMatch.end,
        );
        if (!overlapsBase && !overlapsSupplemental) {
          supplementalRuns.add(
            StyleRun(imageMatch.start, imageMatch.end, SovereignStyle.image),
          );
          if (baseRuns.length + supplementalRuns.length >= spanBudget) break;
        }
        cursor = imageMatch.nextOffset <= cursor
            ? cursor + 1
            : imageMatch.nextOffset;
        continue;
      }

      final linkMatch = _ScannerLinkImageParsers._matchLinkAt(text, cursor);
      if (linkMatch == null || linkMatch.nextOffset > limit) {
        cursor++;
        continue;
      }

      final overlapsBase = _overlapsAny(
        baseRuns,
        linkMatch.start,
        linkMatch.end,
      );
      final overlapsLinks = _overlapsAny(
        supplementalRuns,
        linkMatch.start,
        linkMatch.end,
      );
      if (!overlapsBase && !overlapsLinks) {
        supplementalRuns.add(
          StyleRun(linkMatch.start, linkMatch.end, SovereignStyle.link),
        );
        if (baseRuns.length + supplementalRuns.length >= spanBudget) break;
      }

      cursor =
          linkMatch.nextOffset <= cursor ? cursor + 1 : linkMatch.nextOffset;
    }

    if (supplementalRuns.isEmpty) return baseRuns;

    final merged = <StyleRun>[...baseRuns, ...supplementalRuns]..sort((a, b) {
        final byStart = a.start.compareTo(b.start);
        if (byStart != 0) return byStart;
        return a.end.compareTo(b.end);
      });
    return merged;
  }

  static bool _mayHaveSupplementalLinkOrImage(String text) {
    return text.contains('[') ||
        text.contains('<') ||
        text.contains('http://') ||
        text.contains('https://') ||
        text.contains('![');
  }

  static bool _overlapsAny(List<StyleRun> runs, int start, int end) {
    for (final run in runs) {
      if (run.start < end && start < run.end) return true;
    }
    return false;
  }

  static void _addRun(
    List<StyleRun> runs,
    int start,
    int end,
    SovereignStyle style,
  ) {
    if (runs.isNotEmpty) {
      final last = runs.last;
      if (last.end == start && last.style == style) {
        // Coalesce: Replace last run with extended run
        runs[runs.length - 1] = StyleRun(last.start, end, style);
        return;
      }
    }
    runs.add(StyleRun(start, end, style));
  }

  /// Phase 5 post-process step for extracting syntax markers.
  static List<TextRange> extractHiddenRanges(String text, List<StyleRun> runs) {
    final hidden = <TextRange>[];
    for (final run in runs) {
      if (run.style.types.contains(SovereignStyleType.bold)) {
        // **...** (Markers: 4 chars)
        // Require content: Length > 4
        if (run.end - run.start > 4) {
          hidden.add(TextRange(start: run.start, end: run.start + 2));
          hidden.add(TextRange(start: run.end - 2, end: run.end));
        }
      } else if (run.style.types.contains(SovereignStyleType.italic)) {
        // *...* or _..._ (Markers: 2 chars)
        // Require content: Length > 2
        if (run.end - run.start > 2) {
          hidden.add(TextRange(start: run.start, end: run.start + 1));
          hidden.add(TextRange(start: run.end - 1, end: run.end));
        }
      } else if (run.style.types.contains(SovereignStyleType.code)) {
        // `...` (Markers: 2 chars)
        // Require content: Length > 2
        // Fixes ` ``` ` typing flow where ` `` ` would otherwise snap hide.
        if (run.end - run.start > 2) {
          hidden.add(TextRange(start: run.start, end: run.start + 1));
          hidden.add(TextRange(start: run.end - 1, end: run.end));
        }
      } else if (run.style.types.contains(SovereignStyleType.image)) {
        final ranges = _ScannerLinkImageParsers._extractImageHiddenRanges(
          text,
          run.start,
          run.end,
        );
        hidden.addAll(ranges);
      } else if (run.style.types.contains(SovereignStyleType.link)) {
        hidden.addAll(
          _ScannerLinkImageParsers._extractLinkHiddenRanges(
            text,
            run.start,
            run.end,
          ),
        );
      }
    }
    return hidden;
  }

  static String? resolveReferenceLinkUrl(String text, SovereignLinkMatch link) {
    if (link.kind != SovereignLinkMatchKind.reference) return null;
    final rawLabel = link.referenceLabelText(text);
    if (rawLabel == null || rawLabel.isEmpty) return null;
    return _ScannerLinkImageParsers._lookupReferenceDefinitionUrl(
      text,
      rawLabel,
    );
  }

  static SovereignReferenceDefinitionMatch? referenceDefinitionForLink(
    String text,
    SovereignLinkMatch link,
  ) {
    if (link.kind != SovereignLinkMatchKind.reference) return null;
    final rawLabel = link.referenceLabelText(text);
    if (rawLabel == null || rawLabel.isEmpty) return null;
    return _ScannerLinkImageParsers._lookupReferenceDefinition(text, rawLabel);
  }
}

class _ReferenceDefinitionLineMatch {
  final String label;
  final String url;
  final int labelStart;
  final int labelEnd;
  final int urlStart;
  final int urlEnd;

  const _ReferenceDefinitionLineMatch({
    required this.label,
    required this.url,
    required this.labelStart,
    required this.labelEnd,
    required this.urlStart,
    required this.urlEnd,
  });
}
