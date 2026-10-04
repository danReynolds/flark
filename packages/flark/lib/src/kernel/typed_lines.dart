part of 'editor.dart';

/// One spelling of typed text: the source, the caret, the edits that made it
/// from the current source (start, end and the length of what replaced it,
/// sorted), and where the typed text starts in it.
typedef _Spelling = ({
  String source,
  int caret,
  List<(int, int, int)> edits,
  int typedAt,
});

/// Typing where the text changes the block structure of its line: an empty
/// row, a rule, a bare or hidden marker, a lazy line, an empty line of
/// indented code, a table's delimiter row shown as its source, or leading
/// whitespace. The text is committed in the first spelling the parser reads
/// with every other row as it was, the typed text in the containers its row
/// shows, and nothing hidden painted. Ordinary text in a row never comes
/// here.
extension _TypedLines on FlarkEditor {
  /// Commits [plain], text typed or pasted at [at] on [row] (the caret's
  /// row) as `[inserted]` over [removed] characters with the caret
  /// [plain]'s, or another spelling of it; null when [row] is ordinary text,
  /// where [plain] is committed as it is. Only a lazy line and leading
  /// whitespace take a selection. [wraps] checks that a pending style's
  /// delimiters around the text pair. [fence] and [underline] are the
  /// typed-construct completions [_commit] makes of the spelling that is
  /// [plain].
  bool? _typeOnLine(
    ProjectedRow row,
    int at,
    String inserted,
    ({String text, int caret, PendingStyle? pending}) plain,
    String typed, {
    required bool typing,
    int removed = 0,
    bool fence = false,
    String? underline,
    bool Function(FlarkDocument next, int typedAt)? wraps,
  }) {
    if (typed.contains('\n') || typed.contains('\r')) return null;
    final m = _doc.model, line = m.lineOfUtf16(at);
    final i = line - row.firstLine;
    if (i < 0 ||
        i >= row.contentStarts.length ||
        row.contentStarts[i] < 0 ||
        m.lineOfUtf16(at + removed) != line) {
      return null;
    }
    // Ordinary text in a row returns here, before anything is computed.
    final delimiterRow = row.kind == RowKind.tableCell && row.tableRowBlock < 0;
    final lazy =
        row.kind == RowKind.paragraph &&
        i > 0 &&
        row.shells.isNotEmpty &&
        row.prefixStarts[i] == row.contentStarts[i];
    final leading =
        (row.kind == RowKind.paragraph || row.kind == RowKind.heading) &&
        at == row.contentStarts[i] &&
        typed.trim().isEmpty;
    if (!lazy &&
        !leading &&
        !delimiterRow &&
        (removed > 0 ||
            row.kind != RowKind.blank &&
                row.kind != RowKind.thematicBreak &&
                !_isBarePrefixRow(row) &&
                (row.kind != RowKind.codeBlock || row.fenced))) {
      return null;
    }
    final lineStart = _lineStart(source, m, line);
    // Where the caret sits in [inserted].
    final offset = plain.caret - at;
    final nl = _lineBreakAt(at);
    final hasNext = line + 1 < m.lineCount;
    final spellings = <_Spelling>[];
    var keepRow = false, sameKind = false, quick = false;
    // [inserted] replaces [lineStart]..[at] with [before], [lead] and [after]
    // around it; plain when it only goes at [at].
    _Spelling around(String before, String lead, String after) {
      final text = '$before$lead$inserted$after';
      final typedAt = lineStart + before.length + lead.length;
      return (
        source: source.replaceRange(lineStart, at, text),
        caret: typedAt + offset,
        edits: [(lineStart, at, text.length)],
        typedAt: typedAt,
      );
    }

    final grown = plain.text.length - source.length;
    final plainSpelling = (
      source: plain.text,
      caret: plain.caret,
      edits: [(at, at + removed, removed + grown)],
      typedAt: at,
    );
    // The continuation prefix of [row]'s line (a rule's or a bare marker's,
    // whose block starts on it) for the lines after it.
    String continued() => _continuationPrefix(
      source,
      m,
      line,
      m.blockStart(row.block),
      row.block,
    );
    if (removed == 0 && row.kind == RowKind.blank) {
      // An empty row: the typed text starts a block on its line, in the
      // containers the row shows. The line gets the prefix of the innermost
      // container when it lacks it (an empty line in an item or footnote may
      // have no indentation), a hidden empty item's marker a separating
      // space, and an empty line before or after keeps the blocks around it
      // apart where Markdown would read the text into them.
      final existing = source.substring(lineStart, at);
      final inner = row.shells.isEmpty ? null : row.shells.last;
      final opens = inner != null && m.blockFirstLine(inner.block) == line;
      var lead = existing, sep = '';
      if (inner != null) {
        if (inner.kind == ShellKind.blockQuote) {
          sep = existing.trimRight();
        } else {
          final prefix = _innerPrefix(inner.block);
          sep = prefix.trimRight();
          if (!opens && prefix != existing && prefix.startsWith(existing)) {
            lead = prefix;
          }
        }
      }
      // An item marker the projection hides, with no space after it: text
      // run into it would make the line a paragraph that shows the marker.
      final spaced =
          opens &&
          inner.kind == ShellKind.item &&
          at > 0 &&
          (at == m.itemMarkerEnd(inner.block) || at == inner.checkboxEnd) &&
          !FlarkEditor._isSpace(source, at - 1);
      if (spaced) lead = '$existing ';
      if (lead == existing) {
        spellings.add(plainSpelling);
      } else {
        spellings.add(around('', lead, ''));
        if (!spaced) spellings.add(plainSpelling);
      }
      if (hasNext) spellings.add(around('', lead, '$nl$sep'));
      if (line > 0) {
        spellings.add(around('$sep$nl', lead, ''));
        if (hasNext) spellings.add(around('$sep$nl', lead, '$nl$sep'));
      }
    } else if (removed == 0 && row.kind == RowKind.thematicBreak) {
      // A rule stays a rule: the text starts a block on the line after it.
      final lead = continued();
      _Spelling after(String rest) {
        final text = '$nl$lead$inserted$rest';
        return (
          source: source.replaceRange(at, at, text),
          caret: at + nl.length + lead.length + offset,
          edits: [(at, at, text.length)],
          typedAt: at + nl.length + lead.length,
        );
      }

      spellings.add(after(''));
      if (hasNext) spellings.add(after('$nl${lead.trimRight()}'));
      keepRow = true;
    } else if (removed == 0 && _isBarePrefixRow(row) || delimiterRow) {
      // A bare marker shown as text, which the typed text joins as paragraph
      // text, or a table's delimiter row shown as its source, which the text
      // edits; the block after either keeps its reading.
      spellings.add(plainSpelling);
      final end = projection.lineContentEnd(line);
      if (hasNext && row.kind != RowKind.tableCell && end >= at) {
        final sep = '$nl${continued().trimRight()}';
        spellings.add((
          source: plain.text.replaceRange(end + grown, end + grown, sep),
          caret: plain.caret,
          edits: [(at, at, grown), (end, end, sep.length)],
          typedAt: at,
        ));
      }
    } else if (removed == 0 && row.kind == RowKind.codeBlock && !row.fenced) {
      // An empty line of indented code needs the code's indentation, as a
      // code edit gives new lines the edited line's prefix.
      var first = 0;
      while (first < row.contentStarts.length &&
          (row.contentStarts[first] < 0 ||
              row.contentStarts[first] == row.contentEnds[first])) {
        first++;
      }
      if (first == row.contentStarts.length ||
          row.contentStarts[i] != row.contentEnds[i]) {
        return null;
      }
      final prefix = _continuationPrefix(
        source,
        m,
        row.firstLine + first,
        row.contentStarts[first],
        row.block,
      );
      final existing = source.substring(lineStart, at);
      if (prefix == existing || !prefix.startsWith(existing)) return null;
      spellings
        ..add(around('', prefix, ''))
        ..add(plainSpelling);
      sameKind = true;
    } else if (lazy) {
      // A lazy line shows inside its containers without their prefix, so
      // block syntax typed on it would open the block outside them. Its
      // ordinary text commits as it is; text that would move a block gets
      // the prefix of the paragraph's first line.
      final prefix = _continuationPrefix(
        source,
        m,
        row.firstLine,
        m.blockStart(row.block),
        row.block,
      );
      spellings
        ..add(plainSpelling)
        ..add((
          source: plain.text.replaceRange(lineStart, lineStart, prefix),
          caret: plain.caret + prefix.length,
          edits: [
            (lineStart, lineStart, prefix.length),
            (at, at + removed, removed + grown),
          ],
          typedAt: at + prefix.length,
        ));
      quick = true;
    } else if (leading) {
      // Whitespace typed where a line's content starts is indentation or
      // marker padding Markdown does not show; it may not move a block.
      spellings.add(plainSpelling);
      sameKind = true;
    } else {
      return null;
    }
    // A longer spelling must not cost the edit its admission: past a limit
    // it is passed over, as the typed underline's blank line is.
    FlarkRejection? rejected;
    var limited = false;
    for (final s in spellings) {
      final isPlain = identical(s, plainSpelling);
      var read = false;
      _lastRejection = null;
      if (_commit(
        s.source,
        FlarkSelection.collapsed(s.caret),
        typing: typing,
        pending: plain.pending,
        completeTypedFence: isPlain && fence,
        typedUnderline: isPlain ? underline : null,
        accept: (next) {
          read = true;
          return (wraps == null || wraps(next, s.typedAt)) &&
              (quick &&
                      identical(s, spellings.first) &&
                      _sameRow(next, row, s) ||
                  _keepsTyped(
                    next,
                    row,
                    s.edits,
                    s.caret,
                    s.typedAt,
                    keepRow: keepRow,
                    sameKind: sameKind,
                  ));
        },
      )) {
        return true;
      }
      if (isPlain) {
        rejected = _lastRejection;
      } else if (!read) {
        // Past the source limit, or past the live tier into source mode.
        limited = true;
      }
    }
    _lastRejection = rejected;
    // A typed underline the parser reads as one where no blank line fits,
    // at a limit, is typed as it is, as the profile has it.
    return rejected == null &&
        limited &&
        underline != null &&
        spellings.contains(plainSpelling) &&
        _commit(
          plain.text,
          FlarkSelection.collapsed(plain.caret),
          typing: typing,
          pending: plain.pending,
        );
  }

  /// [plain], typed at [at] in [row], completed block markup that hides the
  /// caret's own line, which then holds no caret, so the next characters
  /// would go elsewhere. A table's delimiter row would make the lines after
  /// it the table's rows: a line break after it keeps them apart and leaves
  /// the row, shown as its source, holding the caret, as a typed setext
  /// underline gets a line of its own. A fence marker that makes a line
  /// with text after it an opening fence would hide that text in its info
  /// string and turn what follows into code: the marker is escaped instead,
  /// as a pipe typed in a cell is. Null when neither keeps the caret on its
  /// line in [row]'s containers.
  bool? _unhideLine(
    ProjectedRow row,
    int at,
    ({String text, int caret, PendingStyle? pending}) plain,
    String typed,
  ) {
    final text = plain.text, caret = plain.caret;
    final grown = text.length - source.length;
    var end = text.indexOf('\n', caret);
    if (end > 0 && text.codeUnitAt(end - 1) == 0x0D) end--;
    final nl = _lineBreakAt(at);
    for (final (spelled, moved, edits) in [
      if (end > 0)
        (
          text.replaceRange(end, end, nl),
          caret,
          [(at, at, grown), (end - grown, end - grown, nl.length)],
        ),
      if (typed == '~' || typed == '`')
        (text.replaceRange(at, at, r'\'), caret + 1, [(at, at, grown + 1)]),
    ]) {
      if (_commit(
        spelled,
        FlarkSelection.collapsed(moved),
        pending: plain.pending,
        typing: true,
        acceptSourceMode: true,
        accept: (next) =>
            next.selection.extent == moved &&
            _keepsTyped(next, row, edits, moved, at),
      )) {
        return true;
      }
      if (_lastRejection != null) return false;
    }
    return null;
  }

  /// A deletion in a table's delimiter row shown as its source edits that
  /// source and must keep the table: one that would dissolve it, painting
  /// the header's pipes, is refused, as table restructuring uses source mode.
  /// Null outside such a row.
  bool? _deleteInDelimiterRow({required bool backward, required bool word}) {
    final sel = selection, row = _doc.rowAt(sel.extent);
    if (row.kind != RowKind.tableCell || row.tableRowBlock >= 0) return null;
    if (_doc.rowAt(sel.base).index != row.index) return false;
    var start = sel.start, end = sel.end;
    if (sel.isCollapsed) {
      final text = row.text, d = row.displayForSource(sel.extent).$1;
      if (backward ? d == 0 : d >= text.length) return false;
      final from = backward
          ? (word
                ? FlarkEditor._wordStart(text, d)
                : d - text.substring(0, d).characters.last.length)
          : d;
      final to = backward
          ? d
          : (word
                ? FlarkEditor._wordEnd(text, d)
                : d + text.substring(d).characters.first.length);
      start = row.sourceForDisplay(from);
      end = row.sourceForDisplay(to);
    }
    return start < end &&
        _commit(
          source.replaceRange(start, end, ''),
          FlarkSelection.collapsed(start),
          typing: sel.isCollapsed && !word,
          acceptSourceMode: true,
          accept: (next) =>
              _keepsTyped(next, row, [(start, end, 0)], start, start),
        );
  }

  /// Whether the caret of [s] is still in [row]'s paragraph in [next]: the
  /// row starts where it did and keeps its kind and containers, so typing on
  /// its lazy line opened no block.
  bool _sameRow(FlarkDocument next, ProjectedRow row, _Spelling s) {
    final now = next.rowAt(s.caret);
    return now.kind == row.kind &&
        now.sourceStart == row.sourceStart &&
        _sameContainers(now, row);
  }

  static bool _sameContainers(ProjectedRow a, ProjectedRow b) {
    if (a.shells.length != b.shells.length) return false;
    for (var k = 0; k < a.shells.length; k++) {
      if (a.shells[k].kind != b.shells[k].kind) return false;
    }
    return true;
  }

  /// Whether [next], typed on [row] by [edits] with the caret at [caret] and
  /// the typed text from [typedAt], keeps what the user sees elsewhere: every
  /// other row that shows something keeps its kind and the kinds of its
  /// containers (a paragraph may take the typed text as a line of its own),
  /// the caret's row is in [row]'s containers, and nothing the current
  /// projection hides is painted. The caret's row may be in containers
  /// inside those when the typed text, or the bare marker it completes,
  /// opened them, or when its line carries their prefix (the indentation
  /// Return gave a footnote's next line); not when it reads on lazily.
  /// [keepRow] checks [row] too; [sameKind] keeps the caret's row of
  /// [row]'s kind.
  bool _keepsTyped(
    FlarkDocument next,
    ProjectedRow row,
    List<(int, int, int)> edits,
    int caret,
    int typedAt, {
    bool keepRow = false,
    bool sameKind = false,
  }) {
    int forward(int offset) {
      var shift = 0;
      for (final (start, end, length) in edits) {
        if (offset < start) break;
        if (offset < end) return -1;
        shift += length - (end - start);
      }
      return offset + shift;
    }

    int back(int offset) {
      var shift = 0;
      for (final (start, end, length) in edits) {
        if (offset < start + shift) break;
        if (offset < start + shift + length) return -1;
        shift += length - (end - start);
      }
      return offset - shift;
    }

    // An empty item the text fills: the blocks after it that are indented
    // for it join it again, as they were before it was emptied.
    final inner = row.shells.isEmpty ? null : row.shells.last;
    final filled =
        row.kind == RowKind.blank &&
            inner != null &&
            inner.kind == ShellKind.item &&
            _doc.model.blockFirstLine(inner.block) == row.firstLine
        ? forward(_doc.model.blockStart(inner.block))
        : -1;
    final now = next.projection.rows;
    var j = 0;
    for (final old in projection.rows) {
      if (old.kind == RowKind.blank || old.index == row.index && !keepRow) {
        continue;
      }
      final at = forward(old.sourceStart);
      if (at < 0) continue;
      while (j + 1 < now.length && now[j + 1].sourceStart <= at) {
        j++;
      }
      final holder = at > now[j].sourceEnd && j + 1 < now.length
          ? now[j + 1]
          : now[j];
      // A bare marker shows as text only while it starts its own list, and
      // as an empty item once another item follows it: a presentation the
      // profile lets flip.
      if (_isBarePrefixRow(old) &&
          holder.kind == RowKind.blank &&
          holder.shells.isNotEmpty &&
          holder.shells.last.kind == ShellKind.item &&
          next.model.blockStart(holder.shells.last.block) == at) {
        continue;
      }
      // A delimiter row shown as its source hides once its table has a body
      // row, which the text typed under it starts.
      if (old.kind == RowKind.tableCell &&
          old.tableRowBlock < 0 &&
          holder.kind == RowKind.tableCell &&
          _sameContainers(holder, old)) {
        continue;
      }
      // The row keeps what it shows: a paragraph the typed line joins is
      // that line's start or end, and its last `\` or spaces stay text
      // rather than becoming a hard break.
      if (holder.kind != old.kind ||
          !_sameContainers(holder, old) &&
              !_joins(next, holder, old, at, filled) ||
          !(holder.sourceStart == at
              ? holder.text.startsWith(old.text)
              : holder.text.endsWith(old.text))) {
        return false;
      }
    }
    final typedRow = next.rowAt(caret);
    // Whitespace typed over all of a row's text may leave it empty.
    if (sameKind &&
        typedRow.kind != row.kind &&
        typedRow.kind != RowKind.blank) {
      return false;
    }
    final shells = typedRow.shells;
    if (shells.length < row.shells.length) return false;
    final from = _isBarePrefixRow(row) ? row.sourceStart : typedAt;
    // Only a paragraph's line reads on lazily: any other row in a container
    // carries its prefix.
    final k = next.model.lineOfUtf16(caret) - typedRow.firstLine;
    final prefixed =
        typedRow.kind != RowKind.paragraph ||
        k >= 0 &&
            k < typedRow.prefixStarts.length &&
            typedRow.prefixStarts[k] >= 0 &&
            typedRow.prefixStarts[k] < typedRow.contentStarts[k];
    for (var s = 0; s < shells.length; s++) {
      if (s < row.shells.length
          ? shells[s].kind != row.shells[s].kind
          : !prefixed &&
                shells[s].kind != ShellKind.list &&
                next.model.blockStart(shells[s].block) < from) {
        return false;
      }
    }
    return !_revealsHiddenText(next, back);
  }

  /// Whether [holder], [old]'s row in [next] holding its start [at], is in
  /// [old]'s containers and then only in the item starting at [filled] (and
  /// its list), with that item's indentation on its line: a paragraph that
  /// reads on lazily does not count.
  static bool _joins(
    FlarkDocument next,
    ProjectedRow holder,
    ProjectedRow old,
    int at,
    int filled,
  ) {
    if (filled < 0 || holder.shells.length <= old.shells.length) return false;
    final k = next.model.lineOfUtf16(at) - holder.firstLine;
    if (holder.kind == RowKind.paragraph &&
        (k < 0 ||
            k >= holder.prefixStarts.length ||
            holder.prefixStarts[k] < 0 ||
            holder.prefixStarts[k] >= holder.contentStarts[k])) {
      return false;
    }
    for (var s = 0; s < holder.shells.length; s++) {
      final shell = holder.shells[s];
      if (s < old.shells.length
          ? shell.kind != old.shells[s].kind
          : shell.kind != ShellKind.list &&
                next.model.blockStart(shell.block) != filled) {
        return false;
      }
    }
    return true;
  }

  /// The prefix that holds a later line of container block [c] (a list item
  /// or footnote definition) inside it: the prefix of its first line before
  /// it, with the markers of items and definitions opening there turned into
  /// their indentation, and its own marker turned into its content's column,
  /// or a footnote's four columns. Every range comes from the parser.
  String _innerPrefix(int c) {
    final m = _doc.model, line = m.blockFirstLine(c);
    final outer = _continuationPrefix(source, m, line, m.blockStart(c), c);
    if (m.blockKind(c) == BlockKind.footnoteDefinition) {
      final top = m.blockKind(m.blockParent(c)) == BlockKind.document;
      return '${top ? '' : outer}$_footnoteIndent';
    }
    final end = m.itemMarkerEnd(c);
    final marker = source
        .substring(m.blockStart(c), end)
        .replaceAll(_notTab, ' ');
    // A marker with no padding in the source starts an item with a blank
    // line, whose content is one column past it.
    return FlarkEditor._isSpace(source, end - 1)
        ? '$outer$marker'
        : '$outer$marker ';
  }

  /// Whether [row] shows a bare empty heading or item marker as text.
  bool _isBarePrefixRow(ProjectedRow row) =>
      row.kind == RowKind.paragraph &&
      row.block >= 0 &&
      (_doc.model.blockKind(row.block) == BlockKind.heading ||
          _doc.model.blockKind(row.block) == BlockKind.item);

  /// Whether [next] shows the [length] characters typed after a pending
  /// style's opening delimiter at [from] in that style, with the delimiters
  /// around them hidden. Where they cannot pair (after a backslash, inside
  /// an autolink, beside another delimiter run) they would be painted.
  static bool _wrapShows(
    FlarkDocument next,
    int from,
    PendingStyle style,
    int length,
  ) {
    final start = from + style.open.length, end = start + length;
    final close = end + style.close.length;
    var shown = 0;
    for (final s in next.rowAt(start).segments) {
      if (s.lineBreak || s.sourceEnd <= s.sourceStart) continue;
      bool over(int a, int b) => s.sourceStart < b && s.sourceEnd > a;
      if (over(from, start) || over(end, close)) return false;
      if (over(start, end)) {
        if (s.styles & style.styles != style.styles) return false;
        shown +=
            (s.sourceEnd < end ? s.sourceEnd : end) -
            (s.sourceStart > start ? s.sourceStart : start);
      }
    }
    return shown == length;
  }
}
