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
  /// where [plain] is committed as it is. Only a lazy line, leading
  /// whitespace and a table's delimiter row shown as its source take a
  /// selection, and only that row lines of text. [wraps] checks that a
  /// pending style's delimiters around the text pair. [fence], a typed
  /// fence run, is completed in whichever spelling commits, the completed
  /// text checked as the spelling is; [underline], a typed setext underline
  /// run, is typed as it is at a limit where no blank line before it fits.
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
    final m = _doc.model, line = m.lineOfUtf16(at);
    final i = line - row.firstLine;
    if (i < 0 || i >= row.contentStarts.length || row.contentStarts[i] < 0) {
      return null;
    }
    // Ordinary text in a row returns here, before anything is computed.
    final delimiterRow = row.delimiterSource;
    // Lines pasted into a delimiter row shown as its source must keep the
    // table too; elsewhere they keep Markdown's literal meaning.
    final lines = typed.contains('\n') || typed.contains('\r');
    if (lines && !delimiterRow) return null;
    final lazy =
        row.kind == RowKind.paragraph &&
        i > 0 &&
        row.shells.isNotEmpty &&
        row.prefixStarts[i] == row.contentStarts[i];
    // Whitespace typed where a line's content shows, where its first
    // character's hidden syntax starts it too, and over a selection from
    // there, even one reaching the row's later lines.
    final leading =
        (row.kind == RowKind.paragraph || row.kind == RowKind.heading) &&
        typed.trim().isEmpty &&
        (at == row.contentStarts[i] ||
            row.displayForSource(at).$1 ==
                row.displayForSource(row.contentStarts[i]).$1) &&
        m.lineOfUtf16(at + removed) < row.firstLine + row.lineCount;
    if (!leading && m.lineOfUtf16(at + removed) != line) return null;
    // Text that starts with indentation and puts it where a line's content
    // starts: a paste of indented text there, or text over the whole of a
    // span that starts the line, whose leading whitespace moves out before
    // the span's delimiters. Markdown does not show the indentation, but
    // after an item's marker it moves the item's content column, and with
    // it the blocks indented under the item, and before a paragraph's or a
    // setext heading's first line it can make the text code.
    final cs = row.contentStarts[i];
    final indented =
        !leading &&
        (row.kind == RowKind.paragraph || row.kind == RowKind.heading) &&
        _leadingIndentation(typed) > 0 &&
        _leadingIndentation(plain.text, cs) > _leadingIndentation(source, cs);
    if (!lazy &&
        !leading &&
        !indented &&
        !delimiterRow &&
        (removed > 0 ||
            row.kind != RowKind.blank &&
                row.kind != RowKind.thematicBreak &&
                !projection.isBarePrefix(row) &&
                (row.kind != RowKind.codeBlock || row.fenced))) {
      return null;
    }
    final lineStart = lineStartPastMark(source, m, line);
    // Where the caret sits in [inserted].
    final offset = plain.caret - at;
    final nl = _lineBreakAt(at);
    final hasNext = line + 1 < m.lineCount;
    final spellings = <_Spelling>[];
    var sameKind = false, ordinary = false, apart = false;
    // Whether only the plain spelling must keep the row's kind.
    var plainKind = false;
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
      // Whitespace typed in the hidden syntax that starts a line, or leading
      // text put over a span there, moves out before it, so the edit is
      // read from the text.
      edits: [
        leading || indented
            ? _changeTo(plain.text)
            : (at, at + removed, removed + grown),
      ],
      typedAt: at,
    );
    // The continuation prefix of [row]'s line (a rule's or a bare marker's,
    // whose block starts on it) for the lines after it.
    String continued() =>
        continuationPrefix(source, m, line, m.blockStart(row.block), row.block);
    if (removed == 0 && row.kind == RowKind.blank) {
      // An empty row: the typed text starts a block on its line, in the
      // containers the row shows. The line gets the prefix of the innermost
      // container when it lacks it (an empty line in an item or footnote may
      // have no indentation), a hidden empty item's marker a separating
      // space, and an empty line before or after keeps the blocks around it
      // apart where Markdown would read the text into them. Whitespace the
      // row does not show after its containers' prefix stays unshown: where
      // it would show (indentation that makes the text code, or that joins
      // it to code or literal HTML), the text goes where the prefix ends,
      // an empty item's marker padded with one space. Typing never shortens
      // the source: the whitespace becomes spaces before the text, which a
      // paragraph does not show, or the blank line after or before it.
      final existing = source.substring(lineStart, at);
      final inner = row.shells.isEmpty ? null : row.shells.last;
      final opens = inner != null && m.blockFirstLine(inner.block) == line;
      var lead = existing, sep = '', bare = '';
      if (inner != null) {
        if (inner.kind == ShellKind.blockQuote) {
          sep = existing.trimRight();
          bare = sep.length < existing.length ? '$sep ' : sep;
        } else {
          final prefix = _innerPrefix(inner.block);
          sep = prefix.trimRight();
          if (!opens && prefix != existing && prefix.startsWith(existing)) {
            lead = prefix;
          }
          bare = !opens
              ? prefix
              : inner.kind == ShellKind.item
              ? _emptyItemLead(inner, lineStart)
              : existing;
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
      final moved = bare != lead && bare != existing;
      final item = opens && inner.kind == ShellKind.item;
      // The blank line that keeps the blocks apart from text at [bare] is
      // the line's own whitespace (after an empty item's marker) where the
      // containers' prefix alone would leave the line shorter than it was.
      final room = bare.length + nl.length + sep.length >= existing.length;
      final blank = room
          ? sep
          : item
          ? existing.substring(bare.length - 1)
          : existing;
      // Under a fence with no body and no closing fence the line would be
      // its first line of code: the fence closes first, so the text starts
      // a block after it, where the row shows it.
      final above = row.index > 0 ? projection.rows[row.index - 1] : null;
      if (above != null &&
          above.bodyless &&
          m.blockFlags(above.block) & 2 == 0 &&
          above.firstLine + above.lineCount == line) {
        final fenceAt = m.blockStart(above.block);
        final closer =
            '${continuationPrefix(source, m, above.firstLine, fenceAt, above.block)}'
            '${source.substring(fenceAt, fenceAt + m.blockAttr(above.block))}';
        spellings.add(around('$closer$nl', lead, ''));
      }
      if (lead == existing) {
        spellings.add(plainSpelling);
      } else {
        spellings.add(around('', lead, ''));
        if (!spaced) spellings.add(plainSpelling);
      }
      if (moved) {
        final pad = existing.length - bare.length;
        final padded = '$bare${' ' * (pad < 0 ? 0 : pad)}';
        if (padded != existing) spellings.add(around('', padded, ''));
      }
      final after = moved ? around('', bare, '$nl$blank') : null;
      if (hasNext) {
        spellings.add(around('', lead, '$nl$sep'));
        if (after != null) spellings.add(after);
      }
      if (line > 0) {
        final before = moved && !item ? '$blank$nl' : null;
        spellings.add(around('$sep$nl', lead, ''));
        if (before != null) spellings.add(around(before, bare, ''));
        if (hasNext) {
          spellings.add(around('$sep$nl', lead, '$nl$sep'));
          if (before != null) spellings.add(around(before, bare, '$nl$sep'));
        }
      }
      // On the last line a blank line after the text would trail the
      // document, so one before it comes first.
      if (!hasNext && after != null) spellings.add(after);
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
    } else if (removed == 0 && projection.isBarePrefix(row) || delimiterRow) {
      // A bare marker shown as text, which the typed text joins as paragraph
      // text, or a table's delimiter row shown as its source, which the text
      // edits; the block after either keeps its reading, a paragraph after
      // the marker too, which would otherwise read on as part of the text.
      spellings.add(plainSpelling);
      apart = !delimiterRow;
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
      final prefix = continuationPrefix(
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
      // ordinary text commits as it is, as on the paragraph's other lines,
      // even where it changes a span (a backslash before a closing `~~`):
      // as typed, it need only leave the caret's row the paragraph it was,
      // in its containers. Text that would move a block gets the prefix of
      // the paragraph's first line.
      // The paragraph's first line is its block's, which can be earlier
      // than its row's when the block opens with link reference definitions
      // its row leaves out.
      final prefix = continuationPrefix(
        source,
        m,
        m.blockFirstLine(row.block),
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
      ordinary = true;
    } else if (leading) {
      // Whitespace typed where a line's content starts is indentation or
      // marker padding Markdown does not show; it may not move a block.
      spellings.add(plainSpelling);
      sameKind = true;
    } else if (indented) {
      // The text goes in as it is where its indentation changes neither its
      // row's kind nor another block, else without the indentation it puts
      // where the line's content starts, which shows the same: indentation
      // in pasted text does not make the row code.
      spellings.add(plainSpelling);
      plainKind = true;
      final lead =
          _leadingIndentation(plain.text, cs) - _leadingIndentation(source, cs);
      final stripped = plain.text.replaceRange(cs, cs + lead, '');
      spellings.add((
        source: stripped,
        caret: plain.caret - lead,
        edits: [_changeTo(stripped)],
        // Where the text would start with its indentation, so that a pending
        // style's delimiters are found after it.
        typedAt: at - lead,
      ));
    } else {
      return null;
    }
    // A longer spelling must not cost the edit its admission: past a limit
    // it is passed over.
    FlarkRejection? rejected;
    var limited = false;
    // The first spelling passed over because it would leave the live tier,
    // where no parse checks it: when no spelling qualifies, it enters source
    // mode, as ordinary text past the tier does.
    _Spelling? beyond;
    // Text typed on an empty row starts its line and shows no whitespace the
    // row hid.
    final strict = row.kind == RowKind.blank;
    for (final s in spellings) {
      final isPlain = identical(s, plainSpelling);
      var read = false;
      // Whether [next], [s] made by [edits] with the caret at [caret] (a
      // typed fence completed in it moves both), qualifies.
      bool keeps(FlarkDocument next, List<(int, int, int)> edits, int caret) {
        read = true;
        return (wraps == null || wraps(next, s.typedAt)) &&
            (ordinary &&
                    identical(s, spellings.first) &&
                    _sameRow(next, row, caret) ||
                _keepsTyped(
                  next,
                  row,
                  edits,
                  caret,
                  s.typedAt,
                  sameKind: sameKind || plainKind && isPlain,
                  strict: strict,
                  apart: apart,
                ));
      }

      _lastRejection = null;
      if (_commit(
        s.source,
        FlarkSelection.collapsed(s.caret),
        coalesce: typing,
        pending: plain.pending,
        completeTypedFence: fence,
        accept: (next) => keeps(next, s.edits, s.caret),
        acceptCompleted: (next, at, length) => keeps(
          next,
          _withInsertion(s.edits, at, length),
          next.selection.extent,
        ),
      )) {
        return true;
      }
      if (isPlain) {
        rejected = _lastRejection;
      } else if (!read) {
        // Past the source limit, or past the live tier into source mode.
        limited = true;
      }
      if (!read && _lastRejection == null && s.source != source) {
        beyond ??= s;
      }
    }
    _lastRejection = rejected;
    if (rejected != null) return false;
    // A typed underline the parser reads as one where no blank line fits,
    // at a limit, is typed as it is, as the profile has it.
    if (limited && underline != null && spellings.contains(plainSpelling)) {
      return _commit(
        plain.text,
        FlarkSelection.collapsed(plain.caret),
        coalesce: typing,
        pending: plain.pending,
      );
    }
    final over = beyond;
    return over != null &&
        _commit(
          over.source,
          FlarkSelection.collapsed(over.caret),
          coalesce: typing,
          pending: plain.pending,
          completeTypedFence: fence,
          acceptSourceMode: true,
          accept: (_) => false,
          acceptCompleted: (_, _, _) => false,
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
      // Past the live tier the parser cannot check the respelling: it is
      // passed over, and the text is typed as it is.
      if (_commit(
        spelled,
        FlarkSelection.collapsed(moved),
        pending: plain.pending,
        coalesce: true,
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
    if (!row.delimiterSource) return null;
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
        _cutDelimiterRow(row, start, end, typing: sel.isCollapsed && !word);
  }

  /// Deletes [start]..[end] of [row], a delimiter row shown as its source,
  /// keeping its table (see [_deleteInDelimiterRow]).
  bool _cutDelimiterRow(
    ProjectedRow row,
    int start,
    int end, {
    required bool typing,
  }) => _commit(
    source.replaceRange(start, end, ''),
    FlarkSelection.collapsed(start),
    coalesce: typing,
    acceptSourceMode: true,
    accept: (next) => _keepsTyped(next, row, [(start, end, 0)], start, start),
  );

  /// Whether [caret] is still in [row]'s paragraph in [next]: the row starts
  /// where it did and keeps its kind and containers, so typing on its lazy
  /// line opened no block.
  bool _sameRow(FlarkDocument next, ProjectedRow row, int caret) {
    final now = next.rowAt(caret);
    return now.kind == row.kind &&
        now.sourceStart == row.sourceStart &&
        now.sameContainerKinds(row);
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
  /// An empty item stays one, rather than becoming the setext underline of
  /// the typed text, and a fence that shows no body keeps none. Text typed
  /// on an empty row joins no table or literal HTML above it, which an
  /// empty line keeps apart, but may start the first body row of a table
  /// that has none. [row] takes the typed text and is not compared; a rule
  /// takes it on the line after, which cannot change the rule. [sameKind]
  /// keeps the caret's row of [row]'s kind; [apart] keeps each row after
  /// [row] a row of its own, so no paragraph there reads on as part of the
  /// text.
  /// [strict], for text typed on an empty row, has it start its line with
  /// no whitespace shown that the current projection hides.
  bool _keepsTyped(
    FlarkDocument next,
    ProjectedRow row,
    List<(int, int, int)> edits,
    int caret,
    int typedAt, {
    bool sameKind = false,
    bool strict = false,
    bool apart = false,
  }) {
    int forward(int offset) => FlarkEditor._afterEdits(edits, offset);
    int back(int offset) => FlarkEditor._beforeEdits(edits, offset);

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
      final emptyItem =
          old.kind == RowKind.blank &&
          old.shells.isNotEmpty &&
          old.shells.last.kind == ShellKind.item &&
          _doc.model.blockFirstLine(old.shells.last.block) == old.firstLine;
      if (old.kind == RowKind.blank && !emptyItem || old.index == row.index) {
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
      // An empty item stays one. Shown as a bare marker instead, it would
      // paint the marker it hides, which the hidden-text check refuses.
      if (emptyItem) {
        if (holder.kind == RowKind.blank && holder.sameContainerKinds(old)) {
          continue;
        }
        return false;
      }
      if (old.bodyless && !holder.bodyless) {
        return false;
      }
      // A bare marker shows as text only while it starts its own list, and
      // as an empty item once another item follows it: a presentation the
      // profile lets flip.
      if (projection.isBarePrefix(old) &&
          holder.kind == RowKind.blank &&
          holder.shells.isNotEmpty &&
          holder.shells.last.kind == ShellKind.item &&
          next.model.blockStart(holder.shells.last.block) == at) {
        continue;
      }
      // A delimiter row shown as its source hides once its table has a body
      // row, which the text typed under it starts.
      if (old.delimiterSource &&
          holder.kind == RowKind.tableCell &&
          holder.sameContainerKinds(old)) {
        continue;
      }
      // The row keeps what it shows: a paragraph the typed line joins is
      // that line's start or end, and its last `\` or spaces stay text
      // rather than becoming a hard break.
      if (holder.kind != old.kind ||
          !holder.sameContainerKinds(old) &&
              !_joins(next, holder, old, at, filled) ||
          !(holder.sourceStart == at
              ? holder.text.startsWith(old.text)
              : !(apart && old.index > row.index) &&
                    holder.text.endsWith(old.text))) {
        return false;
      }
    }
    final typedRow = next.rowAt(caret);
    // Text typed on an empty row is not read as the next row of a table
    // above it, or as more of literal HTML above it: an empty line keeps
    // them apart. A table with no body row, whose delimiter row shows as
    // its source, takes the text as its first body row (above).
    if (row.kind == RowKind.blank && row.index > 0) {
      final above = projection.rows[row.index - 1];
      final html =
          above.kind == RowKind.htmlBlock && typedRow.kind == RowKind.htmlBlock;
      final table =
          above.kind == RowKind.tableCell &&
          !above.delimiterSource &&
          typedRow.kind == RowKind.tableCell;
      final at = html || table ? forward(above.sourceStart) : -1;
      if (at >= 0 &&
          (html
              ? next.rowAt(at).block == typedRow.block
              : next.rowAt(at).tableBlock == typedRow.tableBlock)) {
        return false;
      }
    }
    // Whitespace typed over all of a row's text may leave it empty.
    if (sameKind &&
        typedRow.kind != row.kind &&
        typedRow.kind != RowKind.blank) {
      return false;
    }
    // An empty row shows nothing, so the text typed on it starts its line:
    // no whitespace or tab column shows before it.
    if (strict && !_startsLine(next, typedAt)) return false;
    // The typed row keeps the row's containers; any it gains are checked
    // below.
    if (!row.withinContainerKindsOf(typedRow)) return false;
    final shells = typedRow.shells;
    final from = projection.isBarePrefix(row) ? row.sourceStart : typedAt;
    // Only a paragraph's line reads on lazily: any other row in a container
    // carries its prefix.
    final k = typedRow.lineIndexOf(next.model, caret);
    final prefixed =
        typedRow.kind != RowKind.paragraph ||
        k >= 0 &&
            k < typedRow.prefixStarts.length &&
            typedRow.prefixStarts[k] >= 0 &&
            typedRow.prefixStarts[k] < typedRow.contentStarts[k];
    for (var s = row.shells.length; s < shells.length; s++) {
      if (!prefixed &&
          shells[s].kind != ShellKind.list &&
          next.model.blockStart(shells[s].block) < from) {
        return false;
      }
    }
    // Nor does text typed on an empty row show whitespace that row or the
    // blocks around it hid, as a line it joins to code or HTML would.
    return !_revealsHiddenText(next, back, whitespace: strict);
  }

  /// The spaces and tabs [text] starts with, or has from [from].
  static int _leadingIndentation(String text, [int from = 0]) {
    var n = from;
    while (n < text.length &&
        (text.codeUnitAt(n) == 0x20 || text.codeUnitAt(n) == 0x09)) {
      n++;
    }
    return n - from;
  }

  /// [edits], the sorted edits of the current source that made a spelling,
  /// with [length] characters inserted at [at] of that spelling too: in the
  /// text of an edit it meets, else as an edit of its own.
  static List<(int, int, int)> _withInsertion(
    List<(int, int, int)> edits,
    int at,
    int length,
  ) {
    final out = <(int, int, int)>[];
    var shift = 0, placed = false;
    for (final (start, end, replaced) in edits) {
      if (!placed && at <= start + shift + replaced) {
        if (at < start + shift) {
          out.add((at - shift, at - shift, length));
          out.add((start, end, replaced));
        } else {
          out.add((start, end, replaced + length));
        }
        placed = true;
      } else {
        out.add((start, end, replaced));
      }
      shift += replaced - (end - start);
    }
    if (!placed) out.add((at - shift, at - shift, length));
    return out;
  }

  /// Whether [next] shows the text typed at [typedAt] first on its line.
  static bool _startsLine(FlarkDocument next, int typedAt) {
    final row = next.rowAt(typedAt);
    final d = row.displayForSource(typedAt).$1;
    return row.displayLineAt(d).$1 == d;
  }

  /// The edit that made [text] from the current source: where the two first
  /// differ, where that difference ends in the source, and its length in
  /// [text].
  (int, int, int) _changeTo(String text) {
    final shorter = source.length < text.length ? source.length : text.length;
    var p = 0;
    while (p < shorter && source.codeUnitAt(p) == text.codeUnitAt(p)) {
      p++;
    }
    var q = 0;
    while (q < shorter - p &&
        source.codeUnitAt(source.length - 1 - q) ==
            text.codeUnitAt(text.length - 1 - q)) {
      q++;
    }
    return (p, source.length - q, text.length - q - p);
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
    final k = holder.lineIndexOf(next.model, at);
    if (holder.kind == RowKind.paragraph &&
        (k < 0 ||
            k >= holder.prefixStarts.length ||
            holder.prefixStarts[k] < 0 ||
            holder.prefixStarts[k] >= holder.contentStarts[k])) {
      return false;
    }
    if (!old.withinContainerKindsOf(holder)) return false;
    for (var s = old.shells.length; s < holder.shells.length; s++) {
      final shell = holder.shells[s];
      if (shell.kind != ShellKind.list &&
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
    final outer = continuationPrefix(source, m, line, m.blockStart(c), c);
    if (m.blockKind(c) == BlockKind.footnoteDefinition) {
      final top = m.blockKind(m.blockParent(c)) == BlockKind.document;
      return '${top ? '' : outer}$footnoteIndent';
    }
    final end = m.itemMarkerEnd(c);
    final marker = source
        .substring(m.blockStart(c), end)
        .replaceAll(notTab, ' ');
    // A marker right after a quote's `>` left it no optional space, and the
    // first of the spaces put in its place would be read as one.
    final pad =
        outer.isNotEmpty && !FlarkEditor._isSpace(outer, outer.length - 1)
        ? ' '
        : '';
    // A marker with no padding in the source starts an item with a blank
    // line, whose content is one column past it.
    return FlarkEditor._isSpace(source, end - 1)
        ? '$outer$pad$marker'
        : '$outer$pad$marker ';
  }

  /// The lead for text typed on the first line of [item], an empty item,
  /// from [lineStart]: its line up to the marker (or the task checkbox) and
  /// one space, the padding Markdown puts its content after. Wider padding
  /// would move the item's content column, and indentation past four
  /// columns would make the text code.
  String _emptyItemLead(Shell item, int lineStart) {
    final m = _doc.model, start = m.blockStart(item.block);
    var end = m.itemMarkerEnd(item.block);
    while (end > start && FlarkEditor._isSpace(source, end - 1)) {
      end--;
    }
    if (item.task && item.checkboxEnd > end) end = item.checkboxEnd;
    return '${source.substring(lineStart, end)} ';
  }

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
