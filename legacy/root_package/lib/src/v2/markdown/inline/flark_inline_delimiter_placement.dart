import '../../core/transaction/flark_source_range.dart';
import 'flark_inline_flanking.dart';
import 'flark_inline_run_scanner.dart';

/// One canonical source edit computed by the placement rules: replace [range]
/// with [replacement] and collapse the caret to [caretAfter].
///
/// When [continuationMarker] is non-null, the edit committed whitespace
/// outside that delimiter cluster to keep the source valid; the caller keeps
/// the cluster's styles armed so the next styled keystroke re-enters the run
/// (pulling the whitespace back inside) instead of starting a sibling run.
final class FlarkInlinePlacementEdit {
  const FlarkInlinePlacementEdit({
    required this.range,
    required this.replacement,
    required this.caretAfter,
    this.continuationMarker,
    this.authoredMarkers = const [],
  });

  final FlarkSourceRange range;
  final String replacement;
  final int caretAfter;
  final String? continuationMarker;

  /// The delimiter clusters this edit itself writes, in post-edit source
  /// coordinates.
  ///
  /// The controller folds these into the *predicted* projection so
  /// just-authored markers are hidden on the very frame they are written —
  /// before the authoritative parse confirms them. Without this, the editable
  /// briefly resyncs raw delimiters to the platform, which flashes markers
  /// and cancels any active IME composition. Only the typing-path placements
  /// report markers; selection-path repairs may leave it empty (composition
  /// is never active across a selection edit).
  final List<FlarkAuthoredMarker> authoredMarkers;
}

/// One delimiter cluster written by a placement edit: its post-edit source
/// range and whether it opens (or closes) its run.
final class FlarkAuthoredMarker {
  const FlarkAuthoredMarker({required this.range, required this.opens});

  final FlarkSourceRange range;
  final bool opens;
}

/// Input text split into the edge whitespace CommonMark refuses to style and the
/// core that can sit against delimiters.
final class FlarkInlineEdgeWhitespace {
  const FlarkInlineEdgeWhitespace({
    required this.leading,
    required this.core,
    required this.trailing,
  });

  final String leading;
  final String core;
  final String trailing;
}

/// Canonical delimiter placement for every editor-generated inline-style
/// write.
///
/// CommonMark refuses an opening delimiter followed by whitespace and a
/// closing delimiter preceded by whitespace, so `**hello **` is literal text,
/// not a bold run. Flark's invariant is that editor-generated edits never
/// commit such source: whitespace a user types (or exposes by deleting) at a
/// run's edge lives *outside* the delimiters, and the editing intent — "the
/// next character is still bold" — is carried as armed controller state
/// (see [FlarkInlinePlacementEdit.continuationMarker]), never as invalid
/// markdown. Text the user hand-types is never rewritten: these rules only
/// place markers the editor itself is writing, or relocate delimiters of runs
/// that were flanking-valid before the edit.
abstract final class FlarkInlineDelimiterPlacement {
  /// Splits [text] at its edges: `leading + core + trailing`, where leading
  /// and trailing are Unicode whitespace. A whitespace-only [text] comes back
  /// entirely in `leading` with an empty core.
  static FlarkInlineEdgeWhitespace splitEdgeWhitespace(String text) {
    var start = 0;
    while (start < text.length &&
        FlarkInlineFlanking.isUnicodeWhitespace(text.codeUnitAt(start))) {
      start += 1;
    }
    if (start == text.length) {
      return FlarkInlineEdgeWhitespace(leading: text, core: '', trailing: '');
    }
    var end = text.length;
    while (end > start &&
        FlarkInlineFlanking.isUnicodeWhitespace(text.codeUnitAt(end - 1))) {
      end -= 1;
    }
    return FlarkInlineEdgeWhitespace(
      leading: text.substring(0, start),
      core: text.substring(start, end),
      trailing: text.substring(end),
    );
  }

  /// Places an armed insertion wrap for [text] typed at a collapsed [caret]:
  /// the canonical form of "the user typed [text] with [open]/[close] styles
  /// armed".
  ///
  /// - Whitespace-only text commits unwrapped and keeps the styles armed.
  /// - Text typed in a re-entry gap (`**hello** |`) whose armed cluster
  ///   matches the run extends the run instead of opening a sibling.
  /// - Otherwise the wrap hugs the text's core; edge whitespace stays outside.
  ///
  /// [edgeSensitive] is false when the innermost armed style is an inline code
  /// span, whose backticks may legally sit against whitespace — the wrap is
  /// then applied verbatim.
  static FlarkInlinePlacementEdit armedWrap({
    required String source,
    required int caret,
    required String text,
    required String open,
    required String close,
    bool edgeSensitive = true,
    List<FlarkInlineRunScan>? runs,
  }) {
    if (!edgeSensitive) {
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(caret, caret),
        replacement: '$open$text$close',
        caretAfter: caret + open.length + text.length,
        authoredMarkers: [
          FlarkAuthoredMarker(
            range: FlarkSourceRange(caret, caret + open.length),
            opens: true,
          ),
          FlarkAuthoredMarker(
            range: FlarkSourceRange(
              caret + open.length + text.length,
              caret + open.length + text.length + close.length,
            ),
            opens: false,
          ),
        ],
      );
    }

    final split = splitEdgeWhitespace(text);
    if (split.core.isEmpty) {
      // Whitespace-only text commits unwrapped and keeps the styles armed.
      // When the caret sits on an existing run's edge, the whitespace still
      // goes through the content repair so it never lands against that run's
      // delimiters (armed emphasis + a space at `~~|f~~` must not produce
      // `~~ f~~`); the repaired run's styles join the armed continuation.
      final repair = _repair(source, caret, caret, text, text.length, runs);
      if (repair != null) {
        return FlarkInlinePlacementEdit(
          range: repair.range,
          replacement: repair.replacement,
          caretAfter: repair.caretAfter,
          continuationMarker: '$open${repair.continuationMarker ?? ''}',
          authoredMarkers: repair.authoredMarkers,
        );
      }
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(caret, caret),
        replacement: text,
        caretAfter: caret + text.length,
        continuationMarker: open,
      );
    }

    final gap = runs == null
        ? FlarkInlineRunScanner.reentryGapAt(source, caret)
        : _reentryGapFromRuns(source, caret, runs);
    if (gap != null && open == close && gap.run.marker == open) {
      final inside = '${gap.whitespace}${split.leading}${split.core}';
      final replacement = '$inside${gap.run.marker}${split.trailing}';
      final insideCaret = gap.run.closeStart + inside.length;
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(gap.run.closeStart, caret),
        replacement: replacement,
        caretAfter: split.trailing.isEmpty
            ? insideCaret
            : gap.run.closeStart + replacement.length,
        continuationMarker: split.trailing.isEmpty ? null : open,
        authoredMarkers: [
          // The relocated close: its old position dies inside the replaced
          // range, so the prediction must hide the new one.
          FlarkAuthoredMarker(
            range: FlarkSourceRange(
              insideCaret,
              insideCaret + gap.run.marker.length,
            ),
            opens: false,
          ),
        ],
      );
    }

    final replacement =
        '${split.leading}$open${split.core}$close${split.trailing}';
    final openStart = caret + split.leading.length;
    final closeStart = openStart + open.length + split.core.length;
    return FlarkInlinePlacementEdit(
      range: FlarkSourceRange(caret, caret),
      replacement: replacement,
      caretAfter: split.trailing.isEmpty
          ? caret + split.leading.length + open.length + split.core.length
          : caret + replacement.length,
      continuationMarker: split.trailing.isEmpty ? null : open,
      authoredMarkers: [
        FlarkAuthoredMarker(
          range: FlarkSourceRange(openStart, openStart + open.length),
          opens: true,
        ),
        FlarkAuthoredMarker(
          range: FlarkSourceRange(closeStart, closeStart + close.length),
          opens: false,
        ),
      ],
    );
  }

  /// Canonical repair for replacing `[start, end)` — an edit strictly inside a
  /// flanking-valid run's content — with [text], or null when the plain edit
  /// already leaves the run canonical.
  ///
  /// This one rule covers insertions at a run's edges (`**hello**` + typed
  /// `' '` → `**hello** `), deletions that expose edge whitespace
  /// (`**foo x**` minus `x` → `**foo** `), and selection replacements. The
  /// run's delimiters are relocated to hug the surviving core; content reduced
  /// to blank dissolves the delimiters entirely. Edge whitespace bubbles out
  /// through every *flush* enclosing delimiter (`*~~f~~*` + a space at the
  /// inner trailing edge → `*~~f~~* `, never `*~~f ~~*` or `*~~f~~ *`), and a
  /// dissolve cascades outward while it leaves the enclosing content blank.
  /// Literal text is never touched: the rule fires only for runs that are
  /// flanking-valid before the edit.
  /// When [runs] is provided (the parser's own runs, paired from a
  /// projection's hidden ranges), containment is resolved against it instead
  /// of the textual scanner — the scanner approximates CommonMark's
  /// delimiter-pairing and can disagree with the parser on adversarial
  /// shapes, and delimiters must only ever be relocated for runs the parser
  /// actually recognizes.
  static FlarkInlinePlacementEdit? contentEditRepair({
    required String source,
    required int start,
    required int end,
    required String text,
    List<FlarkInlineRunScan>? runs,
  }) {
    return _repair(source, start, end, text, text.length, runs);
  }

  static FlarkInlinePlacementEdit? _repair(
    String source,
    int start,
    int end,
    String text,
    int caretInText,
    List<FlarkInlineRunScan>? runs,
  ) {
    final run = _innermostRunContainingContentRange(source, start, end, runs);
    if (run == null) return null;

    final pre = source.substring(run.contentStart, start);
    final post = source.substring(end, run.closeStart);
    final content = '$pre$text$post';
    final caretInContent = pre.length + caretInText;
    final split = splitEdgeWhitespace(content);

    if (split.core.isEmpty) {
      // Blank content dissolves this run's delimiters. The blank text is then
      // itself an edit inside whatever encloses the run, so recurse: the
      // enclosing run dissolves too (`*~~f~~*` minus `f` → ``), relocates its
      // delimiters away from the exposed whitespace (`*~~f~~ tail*` minus `f`
      // → ` *tail*`), or is untouched.
      final outer = _repair(
        source,
        run.openStart,
        run.closeEnd,
        content,
        caretInContent,
        runs,
      );
      if (outer != null) {
        return FlarkInlinePlacementEdit(
          range: outer.range,
          replacement: outer.replacement,
          caretAfter: outer.caretAfter,
          continuationMarker: '${run.marker}${outer.continuationMarker ?? ''}',
          authoredMarkers: outer.authoredMarkers,
        );
      }
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(run.openStart, run.closeEnd),
        replacement: content,
        caretAfter: run.openStart + caretInContent,
        continuationMarker: run.marker,
      );
    }
    if (split.leading.isEmpty && split.trailing.isEmpty) return null;

    return _relocateEdges(source, run, split, caretInContent, runs);
  }

  /// Maps a caret at [caretInContent] — an offset into the run's rebuilt
  /// content `leading + core + trailing` — to its absolute position in the
  /// document. [leadingBase], [coreBase], and [trailingBase] are the absolute
  /// offsets at which each zone begins in the rebuilt text; the caret rides
  /// whichever zone it falls in.
  static int _caretAfterThroughSplit(
    int caretInContent,
    FlarkInlineEdgeWhitespace split, {
    required int leadingBase,
    required int coreBase,
    required int trailingBase,
  }) {
    final coreEnd = split.leading.length + split.core.length;
    if (caretInContent <= split.leading.length) {
      return leadingBase + caretInContent;
    }
    if (caretInContent <= coreEnd) {
      return coreBase + (caretInContent - split.leading.length);
    }
    return trailingBase + (caretInContent - coreEnd);
  }

  /// Rebuilds the nesting around [run] with its new content [split] so edge
  /// whitespace sits outside every delimiter it would otherwise touch.
  ///
  /// The containment chain around the run is flattened into
  /// `open_n head_n … open_0 core close_0 … tail_n close_n`; leading
  /// whitespace escapes outward while the heads between levels are empty
  /// (flush opens), trailing whitespace while the tails are empty (flush
  /// closes). Non-flush levels park the whitespace as legal interior content.
  static FlarkInlinePlacementEdit _relocateEdges(
    String source,
    FlarkInlineRunScan run,
    FlarkInlineEdgeWhitespace split,
    int caretInContent,
    List<FlarkInlineRunScan>? runs,
  ) {
    final markers = <String>[run.marker];
    final heads = <String>[];
    final tails = <String>[];
    var innerOpenStart = run.openStart;
    var innerCloseEnd = run.closeEnd;
    while (true) {
      final outer = _innermostRunProperlyContaining(
        source,
        innerOpenStart,
        innerCloseEnd,
        runs,
      );
      if (outer == null) break;
      markers.add(outer.marker);
      heads.add(source.substring(outer.contentStart, innerOpenStart));
      tails.add(source.substring(innerCloseEnd, outer.closeStart));
      innerOpenStart = outer.openStart;
      innerCloseEnd = outer.closeEnd;
    }

    var leadingDepth = 0;
    if (split.leading.isNotEmpty) {
      while (leadingDepth < heads.length && heads[leadingDepth].isEmpty) {
        leadingDepth += 1;
      }
    }
    var trailingDepth = 0;
    if (split.trailing.isNotEmpty) {
      while (trailingDepth < tails.length && tails[trailingDepth].isEmpty) {
        trailingDepth += 1;
      }
    }

    // The whole nesting is rewritten, so every delimiter in the replacement
    // is authored — recorded (post-edit absolute) as the buffer assembles.
    final authoredMarkers = <FlarkAuthoredMarker>[];
    void recordMarker(
      StringBuffer buffer,
      String marker, {
      required bool opens,
    }) {
      authoredMarkers.add(
        FlarkAuthoredMarker(
          range: FlarkSourceRange(
            innerOpenStart + buffer.length,
            innerOpenStart + buffer.length + marker.length,
          ),
          opens: opens,
        ),
      );
      buffer.write(marker);
    }

    final buffer = StringBuffer();
    for (var level = markers.length - 1; level > leadingDepth; level -= 1) {
      recordMarker(buffer, markers[level], opens: true);
      buffer.write(heads[level - 1]);
    }
    buffer.write(split.leading);
    final leadingLength = buffer.length;
    for (var level = leadingDepth; level > 0; level -= 1) {
      recordMarker(buffer, markers[level], opens: true);
    }
    recordMarker(buffer, markers[0], opens: true);
    final coreStart = buffer.length;
    buffer.write(split.core);
    final closeCluster = StringBuffer();
    for (var level = 0; level <= trailingDepth; level += 1) {
      recordMarker(buffer, markers[level], opens: false);
      closeCluster.write(markers[level]);
    }
    final trailingStart = buffer.length;
    buffer.write(split.trailing);
    for (
      var level = trailingDepth + 1;
      level <= markers.length - 1;
      level += 1
    ) {
      buffer.write(tails[level - 1]);
      recordMarker(buffer, markers[level], opens: false);
    }

    final coreEnd = split.leading.length + split.core.length;
    final caretAfter = _caretAfterThroughSplit(
      caretInContent,
      split,
      // Leading whitespace sits [split.leading.length] before [leadingLength]
      // in the rebuilt text; core and trailing at their recorded buffer
      // offsets.
      leadingBase: innerOpenStart + leadingLength - split.leading.length,
      coreBase: innerOpenStart + coreStart,
      trailingBase: innerOpenStart + trailingStart,
    );
    return FlarkInlinePlacementEdit(
      range: FlarkSourceRange(innerOpenStart, innerCloseEnd),
      replacement: buffer.toString(),
      caretAfter: caretAfter,
      continuationMarker: caretInContent >= coreEnd && split.trailing.isNotEmpty
          ? closeCluster.toString()
          : null,
      authoredMarkers: authoredMarkers,
    );
  }

  /// Repairs a deletion/replacement whose range covers exactly one delimiter
  /// cluster of a recognized run — the hidden close of a run the edit starts
  /// inside, the hidden open of a run it ends inside, or both (one run's
  /// close and a following run's open) — so the surviving counterpart is
  /// never orphaned as literal text. Returns null when the edit does not
  /// cross a marker that way (fully-inside-content edits belong to
  /// [contentEditRepair]; edits covering a whole pair fall through to the
  /// caller's expansion/plain handling), or when the shape is one this repair
  /// does not guarantee a valid result for (it then leaves the plain edit
  /// alone rather than guessing).
  ///
  /// Policy:
  ///
  /// - **Covered close of run A** — typed text joins A (the selection started
  ///   inside it, the Docs/Word convention): the close relocates to hug the
  ///   surviving core, edge whitespace stays outside the delimiters.
  /// - **Covered open of run B** — typed text stays outside (the selection
  ///   started outside): B's open relocates past the text (and past any
  ///   whitespace now leading B's surviving content).
  /// - **Both covered, same marker** — the two runs merge into one run
  ///   absorbing the typed text.
  /// - **Both covered, different markers** — both pairs are rebalanced
  ///   (`T` joins A, B resumes over its survivors); an adjacency that would
  ///   fuse identical marker characters rewrites B with its equivalent
  ///   alternate delimiter character (`*` ↔ `_`).
  /// - **Code spans** participate (an orphaned backtick swallows the rest of
  ///   the document), but with no whitespace splitting — code whitespace is
  ///   content, so typed text is absorbed verbatim and the relocated
  ///   backtick sits directly against it.
  ///
  /// Every produced edit is verified against the flanking rules on the
  /// resulting text (with the alternate-character rewrite as a fallback for
  /// emphasis-family markers); anything unverifiable returns null.
  ///
  /// [runs] must be the parser's own runs; pass the projection's scans with
  /// `includeCodeSpans: true`.
  static FlarkInlinePlacementEdit? markerCrossingRepair({
    required String source,
    required int start,
    required int end,
    required String text,
    required List<FlarkInlineRunScan> runs,
  }) {
    if (start < 0 || end > source.length || start >= end) return null;
    // Replacement text carrying delimiter characters is the user speaking
    // markdown; relocating markers around it could pair with them in ways
    // this repair cannot predict. Leave the plain edit alone.
    for (var index = 0; index < text.length; index += 1) {
      if (_isDelimiterChar(text.codeUnitAt(index))) return null;
    }

    FlarkInlineRunScan? closeRun;
    FlarkInlineRunScan? openRun;
    for (final run in runs) {
      // An edit boundary inside a delimiter cluster is a shape the display
      // mapping never produces; refuse rather than split a marker.
      if (_splitsCluster(start, run) || _splitsCluster(end, run)) return null;
      final openCovered = start <= run.openStart && run.contentStart <= end;
      final closeCovered = start <= run.closeStart && run.closeEnd <= end;
      if (openCovered == closeCovered) continue;
      if (closeCovered) {
        // The close is covered but the open is not: the edit must start
        // inside the run's content (or at its inside-end). A covered close
        // reachable only through an adjacent stacked cluster is unhandled.
        if (run.contentStart <= start && start <= run.closeStart) {
          if (closeRun != null) return null; // Stacked crossing: bail.
          closeRun = run;
        } else {
          return null;
        }
      } else {
        if (run.contentStart <= end && end <= run.closeStart) {
          if (openRun != null) return null; // Stacked crossing: bail.
          openRun = run;
        } else {
          return null;
        }
      }
    }
    if (closeRun == null && openRun == null) return null;
    if (closeRun != null && openRun != null) {
      if (closeRun.closeEnd > openRun.openStart) return null;
      return _bothCoveredRepair(
        source,
        start,
        end,
        text,
        closeRun,
        openRun,
        runs,
      );
    }
    if (closeRun != null) {
      return _coveredCloseRepair(source, start, end, text, closeRun, runs);
    }
    return _coveredOpenRepair(source, start, end, text, openRun!, runs);
  }

  /// Merges two sibling runs whose delimiter clusters a deletion has left
  /// textually adjacent — `**a** **b**` minus the space would otherwise
  /// commit the fused literal `**a****b**`. Returns the merge edit
  /// (`**ab**`), an alternate-marker rewrite when the neighbors carry
  /// different same-character markers (`**a** *b*` → `**a**_b_`; the fused
  /// `***` would leak), or null when the plain deletion already yields valid
  /// adjacency (different marker characters never fuse).
  ///
  /// Fires only for pure deletions that consume the entire inter-run gap.
  /// Stacked (nested) neighbors merge when their cluster chains pair
  /// cleanly (`***a*** ***b***` → `***ab***`).
  static FlarkInlinePlacementEdit? joiningDeletionRepair({
    required String source,
    required int start,
    required int end,
    required String text,
    required List<FlarkInlineRunScan> runs,
  }) {
    if (text.isNotEmpty) return null;
    if (start < 0 || end > source.length || start >= end) return null;

    final left = _closeChainEndingAt(runs, start);
    final right = _openChainStartingAt(runs, end);
    if (left == null || right == null) return null;
    final leftInner = left.last;
    final rightInner = right.last;

    if (_chainsMerge(source, left, right)) {
      final seamBefore = source.codeUnitAt(leftInner.closeStart - 1);
      final seamAfter = source.codeUnitAt(rightInner.contentStart);
      // Content characters meeting at the seam must not be delimiter
      // characters that could pair into fresh markers (`**x~** **~y**`
      // would produce an unpredictable `~~` inside the merged run).
      if (_isDelimiterChar(seamBefore) || _isDelimiterChar(seamAfter)) {
        return null;
      }
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(leftInner.closeStart, rightInner.contentStart),
        replacement: '',
        caretAfter: leftInner.closeStart,
      );
    }

    // Different markers: only a same-character adjacency fuses (`**` + `*`
    // becomes a `***` delimiter run); different characters stay two valid
    // runs and the plain deletion needs no help.
    if (source.codeUnitAt(start - 1) != source.codeUnitAt(end)) return null;
    return _altRewriteRun(
          source,
          right.first,
          deleteRange: FlarkSourceRange(start, end),
          caretAfter: start,
        ) ??
        _altRewriteRun(
          source,
          left.first,
          deleteRange: FlarkSourceRange(start, end),
          caretAtRewriteEnd: true,
        );
  }

  // ---------------------------------------------------------------------
  // Marker-crossing internals
  // ---------------------------------------------------------------------

  static bool _splitsCluster(int offset, FlarkInlineRunScan run) {
    return (run.openStart < offset && offset < run.contentStart) ||
        (run.closeStart < offset && offset < run.closeEnd);
  }

  static bool _isDelimiterChar(int codeUnit) {
    return codeUnit == 0x2A ||
        codeUnit == 0x5F ||
        codeUnit == 0x7E ||
        codeUnit == 0x60;
  }

  static bool _isCodeMarker(String marker) {
    return marker.isNotEmpty && marker.codeUnitAt(0) == 0x60;
  }

  /// The equivalent alternate delimiter cluster (`*` ↔ `_` families), or
  /// null when the marker has no same-style alternate (`~~`, backticks).
  static String? _alternateMarker(String marker) {
    if (marker.isEmpty) return null;
    return switch (marker.codeUnitAt(0)) {
      0x2A => '_' * marker.length,
      0x5F => '*' * marker.length,
      _ => null,
    };
  }

  /// Whether some other run in [runs] properly contains [run]'s whole span
  /// within its content — edge whitespace produced there would need to
  /// bubble through the enclosing delimiters, which the crossing repair
  /// does not attempt.
  static bool _hasEnclosingRun(
    List<FlarkInlineRunScan> runs,
    FlarkInlineRunScan run,
  ) {
    for (final candidate in runs) {
      if (identical(candidate, run)) continue;
      if (candidate.contentStart <= run.openStart &&
          run.closeEnd <= candidate.closeStart) {
        return true;
      }
    }
    return false;
  }

  /// Verifies [edit] leaves every rebuilt delimiter cluster recognizable:
  /// [clusters] lists (offset within the replacement, cluster text, whether
  /// it opens) triples. Emphasis-family clusters must be exact, unescaped,
  /// and flanking-valid in the resulting text; backtick clusters must not
  /// touch another backtick. Returns [edit] or null.
  static FlarkInlinePlacementEdit? _verified(
    String source,
    FlarkInlinePlacementEdit edit,
    List<(int, String, bool)> clusters,
  ) {
    final candidate = source.replaceRange(
      edit.range.start,
      edit.range.end,
      edit.replacement,
    );
    for (final (offset, marker, opens) in clusters) {
      final clusterStart = edit.range.start + offset;
      final clusterEnd = clusterStart + marker.length;
      final markerChar = marker.codeUnitAt(0);
      if (clusterStart > 0 &&
          candidate.codeUnitAt(clusterStart - 1) == markerChar) {
        return null;
      }
      if (clusterEnd < candidate.length &&
          candidate.codeUnitAt(clusterEnd) == markerChar) {
        return null;
      }
      if (FlarkInlineFlanking.isEscaped(candidate, clusterStart)) return null;
      if (_isCodeMarker(marker)) continue;
      final valid = opens
          ? FlarkInlineFlanking.canOpen(candidate, clusterStart, clusterEnd)
          : FlarkInlineFlanking.canClose(candidate, clusterStart, clusterEnd);
      if (!valid) return null;
    }
    return edit;
  }

  /// The edit range covers run A's closing cluster but not its opening one:
  /// [text] joins A, A's close relocates to hug the surviving core.
  static FlarkInlinePlacementEdit? _coveredCloseRepair(
    String source,
    int start,
    int end,
    String text,
    FlarkInlineRunScan run,
    List<FlarkInlineRunScan> runs,
  ) {
    final pre = source.substring(run.contentStart, start);
    final enclosed = _hasEnclosingRun(runs, run);

    if (_isCodeMarker(run.marker)) {
      if (text.contains('`') || pre.contains('`')) return null;
      if (pre.isEmpty && text.isEmpty) {
        // Nothing of the span survives: dissolve it instead of keeping an
        // empty (unparseable) backtick pair.
        if (enclosed) return null;
        return FlarkInlinePlacementEdit(
          range: FlarkSourceRange(run.openStart, end),
          replacement: '',
          caretAfter: run.openStart,
        );
      }
      // Code whitespace is content: no edge splitting, the close hugs the
      // typed text verbatim.
      return _verified(
        source,
        FlarkInlinePlacementEdit(
          range: FlarkSourceRange(start, end),
          replacement: '$text${run.marker}',
          caretAfter: start + text.length,
        ),
        [(text.length, run.marker, false)],
      );
    }

    final inner = pre + text;
    final split = splitEdgeWhitespace(inner);
    if (split.core.isEmpty) {
      // Nothing stylable survives: the run dissolves; its whitespace stays
      // as plain text and the style is kept armed for the next keystroke.
      if (enclosed) return null;
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(run.openStart, end),
        replacement: inner,
        caretAfter: run.openStart + inner.length,
        continuationMarker: run.marker,
      );
    }
    if (enclosed && (split.leading.isNotEmpty || split.trailing.isNotEmpty)) {
      return null;
    }
    return _rebuiltRun(
          source,
          run.marker,
          split,
          rangeStart: run.openStart,
          rangeEnd: end,
          caretInContent: inner.length,
        ) ??
        _rebuiltRun(
          source,
          _alternateMarker(run.marker),
          split,
          rangeStart: run.openStart,
          rangeEnd: end,
          caretInContent: inner.length,
          isAlternate: true,
        );
  }

  /// The edit range covers run B's opening cluster but not its closing one:
  /// [text] stays outside, B's open relocates past it (and past whitespace
  /// now leading B's surviving content).
  static FlarkInlinePlacementEdit? _coveredOpenRepair(
    String source,
    int start,
    int end,
    String text,
    FlarkInlineRunScan run,
    List<FlarkInlineRunScan> runs,
  ) {
    final survivors = source.substring(end, run.closeStart);
    final enclosed = _hasEnclosingRun(runs, run);

    if (_isCodeMarker(run.marker)) {
      if (text.contains('`')) return null;
      if (survivors.isEmpty) {
        if (enclosed) return null;
        return FlarkInlinePlacementEdit(
          range: FlarkSourceRange(start, run.closeEnd),
          replacement: text,
          caretAfter: start + text.length,
        );
      }
      return _verified(
        source,
        FlarkInlinePlacementEdit(
          range: FlarkSourceRange(start, end),
          replacement: '$text${run.marker}',
          caretAfter: start + text.length,
        ),
        [(text.length, run.marker, true)],
      );
    }

    final split = splitEdgeWhitespace(survivors);
    if (split.core.isEmpty) {
      // B's whole content was covered: the run dissolves around the typed
      // text; the orphanable close goes too.
      if (enclosed) return null;
      return FlarkInlinePlacementEdit(
        range: FlarkSourceRange(start, run.closeEnd),
        replacement: text + survivors,
        caretAfter: start + text.length,
      );
    }
    if (enclosed && split.leading.isNotEmpty) return null;

    final replacement = '$text${split.leading}${run.marker}';
    final direct = _verified(
      source,
      FlarkInlinePlacementEdit(
        range: FlarkSourceRange(start, end + split.leading.length),
        replacement: replacement,
        caretAfter: start + text.length,
      ),
      [(text.length + split.leading.length, run.marker, true)],
    );
    if (direct != null) return direct;

    // The relocated open would fuse with or misflank against its new
    // surroundings: rewrite the run with its alternate marker character.
    final alternate = _alternateMarker(run.marker);
    if (alternate == null) return null;
    final altChar = alternate.substring(0, 1);
    if (split.core.contains(altChar)) return null;
    final rebuilt =
        '$text${split.leading}$alternate${split.core}$alternate'
        '${split.trailing}';
    return _verified(
      source,
      FlarkInlinePlacementEdit(
        range: FlarkSourceRange(start, run.closeEnd),
        replacement: rebuilt,
        caretAfter: start + text.length,
      ),
      [
        (text.length + split.leading.length, alternate, true),
        (
          text.length +
              split.leading.length +
              alternate.length +
              split.core.length,
          alternate,
          false,
        ),
      ],
    );
  }

  /// The edit covers A's close and B's open: same markers merge into one
  /// run absorbing [text]; different markers rebalance both pairs.
  static FlarkInlinePlacementEdit? _bothCoveredRepair(
    String source,
    int start,
    int end,
    String text,
    FlarkInlineRunScan left,
    FlarkInlineRunScan right,
    List<FlarkInlineRunScan> runs,
  ) {
    final leftCode = _isCodeMarker(left.marker);
    final rightCode = _isCodeMarker(right.marker);
    final pre = source.substring(left.contentStart, start);
    final survivors = source.substring(end, right.closeStart);
    final enclosed =
        _hasEnclosingRun(runs, left) || _hasEnclosingRun(runs, right);

    if (left.marker == right.marker) {
      if (leftCode) {
        if (text.contains('`') ||
            pre.contains('`') ||
            survivors.contains('`')) {
          return null;
        }
        if (pre.isEmpty && text.isEmpty && survivors.isEmpty) {
          if (enclosed) return null;
          return FlarkInlinePlacementEdit(
            range: FlarkSourceRange(left.openStart, right.closeEnd),
            replacement: '',
            caretAfter: left.openStart,
          );
        }
        return FlarkInlinePlacementEdit(
          range: FlarkSourceRange(start, end),
          replacement: text,
          caretAfter: start + text.length,
        );
      }
      final content = pre + text + survivors;
      final split = splitEdgeWhitespace(content);
      if (split.core.isEmpty) {
        if (enclosed) return null;
        return FlarkInlinePlacementEdit(
          range: FlarkSourceRange(left.openStart, right.closeEnd),
          replacement: content,
          caretAfter: left.openStart + pre.length + text.length,
          continuationMarker: left.marker,
        );
      }
      if (enclosed && (split.leading.isNotEmpty || split.trailing.isNotEmpty)) {
        return null;
      }
      final caretInContent = pre.length + text.length;
      return _rebuiltRun(
            source,
            left.marker,
            split,
            rangeStart: left.openStart,
            rangeEnd: right.closeEnd,
            caretInContent: caretInContent,
          ) ??
          _rebuiltRun(
            source,
            _alternateMarker(left.marker),
            split,
            rangeStart: left.openStart,
            rangeEnd: right.closeEnd,
            caretInContent: caretInContent,
            isAlternate: true,
          );
    }

    // Different markers: keep both runs, T joins A, B resumes over its
    // survivors. Mixed code/emphasis crossings are left alone.
    if (leftCode || rightCode) return null;
    final innerLeft = pre + text;
    final splitLeft = splitEdgeWhitespace(innerLeft);
    final splitRight = splitEdgeWhitespace(survivors);
    if (splitLeft.core.isEmpty || splitRight.core.isEmpty) return null;
    if (enclosed &&
        (splitLeft.leading.isNotEmpty ||
            splitLeft.trailing.isNotEmpty ||
            splitRight.leading.isNotEmpty)) {
      return null;
    }

    final leftPart =
        '${splitLeft.leading}${left.marker}${splitLeft.core}${left.marker}'
        '${splitLeft.trailing}';
    final caretAfter = splitLeft.trailing.isEmpty
        ? left.openStart +
              splitLeft.leading.length +
              left.marker.length +
              splitLeft.core.length
        : left.openStart + leftPart.length;
    final continuation = splitLeft.trailing.isEmpty ? null : left.marker;
    final leftClusters = <(int, String, bool)>[
      (splitLeft.leading.length, left.marker, true),
      (
        splitLeft.leading.length + left.marker.length + splitLeft.core.length,
        left.marker,
        false,
      ),
    ];

    final direct = _verified(
      source,
      FlarkInlinePlacementEdit(
        range: FlarkSourceRange(
          left.openStart,
          end + splitRight.leading.length,
        ),
        replacement: '$leftPart${splitRight.leading}${right.marker}',
        caretAfter: caretAfter,
        continuationMarker: continuation,
      ),
      [
        ...leftClusters,
        (leftPart.length + splitRight.leading.length, right.marker, true),
      ],
    );
    if (direct != null) return direct;

    // The rebalanced markers would fuse (`**bo{T}***in*`) or misflank:
    // rewrite B with its alternate marker character.
    final alternate = _alternateMarker(right.marker);
    if (alternate == null) return null;
    final altChar = alternate.substring(0, 1);
    if (splitRight.core.contains(altChar)) return null;
    final rightPart =
        '${splitRight.leading}$alternate${splitRight.core}$alternate'
        '${splitRight.trailing}';
    return _verified(
      source,
      FlarkInlinePlacementEdit(
        range: FlarkSourceRange(left.openStart, right.closeEnd),
        replacement: '$leftPart$rightPart',
        caretAfter: caretAfter,
        continuationMarker: continuation,
      ),
      [
        ...leftClusters,
        (leftPart.length + splitRight.leading.length, alternate, true),
        (
          leftPart.length +
              splitRight.leading.length +
              alternate.length +
              splitRight.core.length,
          alternate,
          false,
        ),
      ],
    );
  }

  /// One emphasis-family run rebuilt over `[rangeStart, rangeEnd)` from its
  /// new content [split], verified; [caretInContent] is the caret position
  /// within the content (leading + core + trailing). Null [marker] (no
  /// alternate available) returns null.
  static FlarkInlinePlacementEdit? _rebuiltRun(
    String source,
    String? marker,
    FlarkInlineEdgeWhitespace split, {
    required int rangeStart,
    required int rangeEnd,
    required int caretInContent,
    bool isAlternate = false,
  }) {
    if (marker == null) return null;
    // An alternate-character rewrite must not introduce a marker character
    // the content already carries (a literal `_` inside `*a _b*` would pair
    // with the rewritten `_` delimiters); the original marker character in
    // content is fine — it was already there.
    if (isAlternate && split.core.contains(marker.substring(0, 1))) {
      return null;
    }
    final replacement =
        '${split.leading}$marker${split.core}$marker${split.trailing}';
    final coreEnd = split.leading.length + split.core.length;
    final caretAfter = _caretAfterThroughSplit(
      caretInContent,
      split,
      leadingBase: rangeStart,
      coreBase: rangeStart + split.leading.length + marker.length,
      trailingBase:
          rangeStart +
          split.leading.length +
          marker.length +
          split.core.length +
          marker.length,
    );
    // A caret in the trailing zone lands past the reopened close, so keep the
    // style armed for the next keystroke.
    final continuation = caretInContent > coreEnd ? marker : null;
    return _verified(
      source,
      FlarkInlinePlacementEdit(
        range: FlarkSourceRange(rangeStart, rangeEnd),
        replacement: replacement,
        caretAfter: caretAfter,
        continuationMarker: continuation,
      ),
      [
        (split.leading.length, marker, true),
        (
          split.leading.length + marker.length + split.core.length,
          marker,
          false,
        ),
      ],
    );
  }

  /// Rewrites [run] with its alternate marker character, deleting
  /// [deleteRange] (the consumed inter-run gap) in the same edit. The range
  /// covers gap + run when the run follows the gap, or run + gap when it
  /// precedes it.
  static FlarkInlinePlacementEdit? _altRewriteRun(
    String source,
    FlarkInlineRunScan run, {
    required FlarkSourceRange deleteRange,
    int? caretAfter,
    bool caretAtRewriteEnd = false,
  }) {
    final alternate = _alternateMarker(run.marker);
    if (alternate == null) return null;
    final content = source.substring(run.contentStart, run.closeStart);
    if (content.contains(alternate.substring(0, 1))) return null;
    final rebuilt = '$alternate$content$alternate';
    // Gap-then-run rewrites B in place of gap + B; run-then-gap rewrites A
    // in place of A + gap. Either way the rewritten run starts the
    // replacement, so the cluster offsets are the same.
    final follows = run.openStart >= deleteRange.end;
    final range = follows
        ? FlarkSourceRange(deleteRange.start, run.closeEnd)
        : FlarkSourceRange(run.openStart, deleteRange.end);
    return _verified(
      source,
      FlarkInlinePlacementEdit(
        range: range,
        replacement: rebuilt,
        caretAfter: caretAtRewriteEnd
            ? range.start + rebuilt.length
            : (caretAfter ?? range.start),
      ),
      [
        (0, alternate, true),
        (alternate.length + content.length, alternate, false),
      ],
    );
  }

  /// The contiguous chain of runs whose closing clusters end exactly at
  /// [offset], outermost first (`***a***` closing at 7 yields the `**` run
  /// then the `*` run), or null when no run closes there.
  static List<FlarkInlineRunScan>? _closeChainEndingAt(
    List<FlarkInlineRunScan> runs,
    int offset,
  ) {
    return _runChain(
      runs,
      (run) => run.closeEnd == offset,
      (run, inner) =>
          run.closeEnd == inner.closeStart && run.openStart >= inner.openStart,
    );
  }

  /// The contiguous chain of runs whose opening clusters start exactly at
  /// [offset], outermost first, or null when no run opens there.
  static List<FlarkInlineRunScan>? _openChainStartingAt(
    List<FlarkInlineRunScan> runs,
    int offset,
  ) {
    return _runChain(
      runs,
      (run) => run.openStart == offset,
      (run, inner) =>
          run.openStart == inner.contentStart && run.closeEnd <= inner.closeEnd,
    );
  }

  /// Builds a run chain outermost-first: the first run matching [isOutermost]
  /// seeds it, then each run [extendsInward] accepts against the current
  /// innermost is appended. Returns null when nothing matches [isOutermost].
  /// Shared skeleton for the mirror-image [_closeChainEndingAt] (walks in
  /// through adjacent closes) and [_openChainStartingAt] (adjacent opens).
  static List<FlarkInlineRunScan>? _runChain(
    List<FlarkInlineRunScan> runs,
    bool Function(FlarkInlineRunScan run) isOutermost,
    bool Function(FlarkInlineRunScan candidate, FlarkInlineRunScan inner)
    extendsInward,
  ) {
    FlarkInlineRunScan? outer;
    for (final run in runs) {
      if (isOutermost(run)) {
        outer = run;
        break;
      }
    }
    if (outer == null) return null;
    final chain = [outer];
    var found = true;
    while (found) {
      found = false;
      for (final run in runs) {
        if (extendsInward(run, chain.last)) {
          chain.add(run);
          found = true;
          break;
        }
      }
    }
    return chain;
  }

  /// Whether dropping [left]'s close cluster and [right]'s open cluster
  /// yields one validly nested run stack: the chains' marker sequences must
  /// pair (outermost-first equality), or both fused cluster texts must be
  /// one repeated character of equal total length (`***` + `***`), whose
  /// reparse is canonical regardless of internal pairing order.
  static bool _chainsMerge(
    String source,
    List<FlarkInlineRunScan> left,
    List<FlarkInlineRunScan> right,
  ) {
    if (left.length == right.length) {
      var sequencesMatch = true;
      for (var level = 0; level < left.length; level += 1) {
        if (left[level].marker != right[level].marker) {
          sequencesMatch = false;
          break;
        }
      }
      if (sequencesMatch) return true;
    }
    final openText = source.substring(
      left.first.openStart,
      left.last.contentStart,
    );
    final closeText = source.substring(
      right.last.closeStart,
      right.first.closeEnd,
    );
    if (openText.length != closeText.length || openText.isEmpty) return false;
    final char = openText.codeUnitAt(0);
    for (var index = 0; index < openText.length; index += 1) {
      if (openText.codeUnitAt(index) != char ||
          closeText.codeUnitAt(index) != char) {
        return false;
      }
    }
    return true;
  }

  /// Splits a run's content around a caret for a muted-exit middle split:
  /// whitespace straddling the split point moves between the closing and
  /// reopening delimiters so both halves stay flanking-valid.
  ///
  /// For `**foo bar**` split after `foo ` with plain text `x`, produces the
  /// edit yielding `**foo** x**bar**`.
  static FlarkInlinePlacementEdit runSplit({
    required String source,
    required FlarkSourceRange contentRange,
    required int caret,
    required String marker,
    required String text,
  }) {
    var whitespaceStart = caret;
    while (whitespaceStart > contentRange.start &&
        FlarkInlineFlanking.isUnicodeWhitespace(
          source.codeUnitAt(whitespaceStart - 1),
        )) {
      whitespaceStart -= 1;
    }
    var whitespaceEnd = caret;
    while (whitespaceEnd < contentRange.end &&
        FlarkInlineFlanking.isUnicodeWhitespace(
          source.codeUnitAt(whitespaceEnd),
        )) {
      whitespaceEnd += 1;
    }
    final leftWhitespace = source.substring(whitespaceStart, caret);
    final rightWhitespace = source.substring(caret, whitespaceEnd);
    final replacement = '$marker$leftWhitespace$text$rightWhitespace$marker';
    final reopenStart = whitespaceStart + replacement.length - marker.length;
    return FlarkInlinePlacementEdit(
      range: FlarkSourceRange(whitespaceStart, whitespaceEnd),
      replacement: replacement,
      caretAfter:
          whitespaceStart + marker.length + leftWhitespace.length + text.length,
      authoredMarkers: [
        // The split writes a close for the left half and a reopen for the
        // right half.
        FlarkAuthoredMarker(
          range: FlarkSourceRange(
            whitespaceStart,
            whitespaceStart + marker.length,
          ),
          opens: false,
        ),
        FlarkAuthoredMarker(
          range: FlarkSourceRange(reopenStart, reopenStart + marker.length),
          opens: true,
        ),
      ],
    );
  }

  /// The re-entry gap ending at [caret], resolved against the parser's own
  /// [runs].
  ///
  /// A nested stack's closes are adjacent (`***h***` closes as `*` inside
  /// `**`), so the gap walks inward from the run closing at the whitespace
  /// start, collecting the whole contiguous close cluster — the armed-wrap
  /// extension then matches `***` against the stack, not just its outermost
  /// delimiter.
  static FlarkInlineReentryGap? _reentryGapFromRuns(
    String source,
    int caret,
    List<FlarkInlineRunScan> runs,
  ) {
    var whitespaceStart = caret;
    while (whitespaceStart > 0) {
      final codeUnit = source.codeUnitAt(whitespaceStart - 1);
      if (codeUnit != 0x20 && codeUnit != 0x09) break;
      whitespaceStart -= 1;
    }
    if (whitespaceStart == caret) return null;
    // The gap's run spans the whole close cluster ending at the whitespace:
    // the outermost run closing there down through its innermost stacked
    // close.
    final chain = _closeChainEndingAt(runs, whitespaceStart);
    if (chain == null) return null;
    final outermost = chain.first;
    final innermost = chain.last;
    return FlarkInlineReentryGap(
      run: FlarkInlineRunScan(
        openStart: outermost.openStart,
        contentStart: innermost.contentStart,
        closeStart: innermost.closeStart,
        closeEnd: whitespaceStart,
        marker: source.substring(innermost.closeStart, whitespaceStart),
      ),
      whitespace: source.substring(whitespaceStart, caret),
    );
  }

  /// The innermost run whose content contains `[start, end)` — from [runs]
  /// when provided, else from the textual scanner.
  static FlarkInlineRunScan? _innermostRunContainingContentRange(
    String source,
    int start,
    int end, [
    List<FlarkInlineRunScan>? runs,
  ]) {
    return _innermostRunWhere(
      source,
      start,
      runs,
      (run) => run.contentStart <= start && end <= run.closeStart,
    );
  }

  /// The innermost run whose content contains the whole span
  /// `[openStart, closeEnd)` — the next level out in a nesting chain.
  static FlarkInlineRunScan? _innermostRunProperlyContaining(
    String source,
    int openStart,
    int closeEnd, [
    List<FlarkInlineRunScan>? runs,
  ]) {
    // The proper-containment clause (`openStart`/`closeEnd` strictly wider)
    // only filters the [runs] branch; a scanner-found run always encloses the
    // probe, so `validEnclosingRun` never returns the probed run itself.
    return _innermostRunWhere(
      source,
      openStart,
      runs,
      (run) =>
          run.contentStart <= openStart &&
          closeEnd <= run.closeStart &&
          (run.openStart < openStart || run.closeEnd > closeEnd),
    );
  }

  /// The innermost run (largest [FlarkInlineRunScan.contentStart]) satisfying
  /// [contains]: from [runs] when provided, else discovered from the textual
  /// scanner by probing an enclosing run at [probeOffset] for every marker.
  static FlarkInlineRunScan? _innermostRunWhere(
    String source,
    int probeOffset,
    List<FlarkInlineRunScan>? runs,
    bool Function(FlarkInlineRunScan run) contains,
  ) {
    FlarkInlineRunScan? innermost;
    if (runs != null) {
      for (final run in runs) {
        if (contains(run) &&
            (innermost == null || run.contentStart > innermost.contentStart)) {
          innermost = run;
        }
      }
      return innermost;
    }
    for (final marker in FlarkInlineRunScanner.allMarkers) {
      final run = FlarkInlineRunScanner.validEnclosingRun(
        source,
        probeOffset,
        marker,
      );
      if (run != null &&
          contains(run) &&
          (innermost == null || run.contentStart > innermost.contentStart)) {
        innermost = run;
      }
    }
    return innermost;
  }
}
