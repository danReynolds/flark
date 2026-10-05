part of 'editor.dart';

extension _CodeEditing on FlarkEditor {
  bool? _delegateCodeEdit(CodeEditingAction action, {String text = ''}) {
    final delegate = codeEditing;
    if (delegate == null || composing) return null;
    final row = _doc.rowAt(selection.extent);
    if (row.kind != RowKind.codeBlock ||
        !row.fenced ||
        _doc.rowAt(selection.base).index != row.index) {
      return null;
    }
    // Virtual spaces inside a container tab have no independent source range.
    // Keep the existing conservative path for that projection.
    if (row.segments.any((s) => !s.exact && !s.lineBreak)) return null;
    final language = delegate.resolveLanguage(row.text, _codeInfo(row));
    final edit = delegate.propose(
      row.text,
      language: language,
      base: row.displayForSource(selection.base).$1,
      extent: row.displayForSource(selection.extent).$1,
      action: action,
      text: text,
      indentUnit: codeIndentUnit(row.text, language),
    );
    if (edit == null) return null;
    return _applyCodeEdit(row, edit, action, text: text);
  }

  /// Clipboard text is literal code. Its line breaks still need the enclosing
  /// Markdown prefixes, which are source syntax rather than snippet content.
  /// So is any other edit of a fenced body that the delegate does not
  /// propose: [text] replaces [from]..[to] (the selection by default) through
  /// [_applyCodeEdit], which keeps the block whole. Spliced into the source
  /// as it is, a typed, deleted or replaced backtick could close the block
  /// early and turn the rest of the code into prose. [typing] coalesces the
  /// edit with others like it, as ordinary typing and deletion do.
  bool? _pasteCode(String text, {int? from, int? to, bool typing = false}) {
    from ??= selection.start;
    to ??= selection.end;
    final row = _doc.rowAt(to);
    if (!row.fenced || _doc.rowAt(from).index != row.index) return null;
    final start = row.displayForSource(from).$1;
    final end = row.displayForSource(to).$1;
    final normalized = text.replaceAll('\r\n', '\n');
    final caret = start + normalized.length;
    return _applyCodeEdit(
      row,
      CodeEditProposal(start, end, normalized, caret, caret),
      CodeEditingAction.insert,
      typing: typing,
    );
  }

  bool _applyCodeEdit(
    ProjectedRow row,
    CodeEditProposal edit,
    CodeEditingAction action, {
    String text = '',
    bool? typing,
  }) {
    if (edit.start < 0 || edit.end < edit.start || edit.end > row.text.length) {
      throw StateError('Invalid code edit range');
    }
    final body = row.text.replaceRange(edit.start, edit.end, edit.text);
    // Only the lines the edit touches need checking. The rest of the body is
    // the row's own valid text, and a line break neither ends a surrogate
    // pair nor leaves a carriage return before it bare.
    final first = edit.start == 0
        ? 0
        : body.lastIndexOf('\n', edit.start - 1) + 1;
    final last = body.indexOf('\n', edit.start + edit.text.length);
    try {
      validateFlarkSource(
        body.substring(first, last < 0 ? body.length : last + 1),
      );
    } on FormatException {
      _lastRejection = FlarkRejection.invalidSource;
      return false;
    }
    if (edit.base < 0 ||
        edit.extent < 0 ||
        edit.base > body.length ||
        edit.extent > body.length) {
      throw StateError('Invalid code edit selection');
    }
    if (body == row.text) {
      // A line shift that moves nothing is refused, as one with no line to
      // move is. Any other edit that leaves the code as it is, such as text
      // typed over the same text or Return over a selected line break, has
      // nothing to commit or undo: as with an edit that leaves the source as
      // it is, the selection goes where the edit puts it, or it is inert.
      if (action == CodeEditingAction.indent ||
          action == CodeEditingAction.outdent) {
        return false;
      }
      return _commit(
        source,
        FlarkSelection(
          row.sourceForDisplay(edit.base),
          row.sourceForDisplay(edit.extent),
        ),
        coalesce: false,
      );
    }
    if (row.bodyless) {
      return _createEmptyCodeBody(row, body, edit.base, edit.extent);
    }
    if (action == CodeEditingAction.indent ||
        action == CodeEditingAction.outdent) {
      // Explicit line shifts retain each original line's exact container
      // prefix and line ending, even when equivalent prefixes differ.
      final lines = body.split('\n');
      final indices = [
        for (var i = 0; i < row.lineCount; i++)
          if (row.contentStarts[i] >= 0) i,
      ];
      if (lines.length != indices.length) {
        throw StateError('Code shift changed lines');
      }
      final edits = <(int, int, String)>[];
      for (var j = 0; j < indices.length; j++) {
        final i = indices[j];
        edits.add((row.contentStarts[i], row.contentEnds[i], lines[j]));
      }
      final (candidate, map) = _edited(edits);
      int position(int offset) {
        var start = 0;
        for (var j = 0; j < lines.length; j++) {
          if (offset <= start + lines[j].length) {
            // _edited maps the end of a replacement; derive its new start.
            final i = indices[j];
            return map(row.contentEnds[i]) - lines[j].length + offset - start;
          }
          start += lines[j].length + 1;
        }
        throw StateError('Invalid code shift caret');
      }

      return _commitCodeEdit(
        row,
        body,
        candidate,
        FlarkSelection(position(edit.base), position(edit.extent)),
        typing: false,
      );
    }
    final start = row.sourceForDisplay(edit.start);
    final end = row.sourceForDisplay(edit.end);
    final line = _doc.model.lineOfUtf16(start);
    final lineStart = _doc.model.lineStartUtf16(line);
    var prefix = source.substring(
      lineStart,
      row.contentStarts[line - row.firstLine],
    );
    for (final segment in row.segments) {
      if (!segment.exact &&
          !segment.lineBreak &&
          segment.sourceStart == row.contentStarts[line - row.firstLine]) {
        // A partially consumed prefix tab contributes visible code spaces.
        // New lines need only its hidden columns, or every pasted newline
        // would acquire extra literal indentation. Existing prefix bytes stay.
        prefix = _hiddenCodePrefix(prefix, segment.displayLength);
        break;
      }
    }
    // An empty line in a list item or footnote needs none of its container's
    // indentation, so its source can lack the prefix that keeps text inside
    // the block. Text an edit puts on such a line, and lines it starts there,
    // take the prefix the fence's own lines continue with, as a new body does.
    var from = start, lead = '';
    if ((edit.start == 0 || row.text.codeUnitAt(edit.start - 1) == 10) &&
        (edit.start == row.text.length ||
            row.text.codeUnitAt(edit.start) == 10)) {
      final block = _doc.model.blockAt(row.block);
      final continued = continuationPrefix(
        source,
        _doc.model,
        block.firstLine,
        block.startUtf16,
        block.index,
      );
      // A tab's columns depend on where it lands; leave those prefixes be.
      if (!continued.contains('\t')) {
        prefix = continued;
        if (edit.start < body.length && body.codeUnitAt(edit.start) != 10) {
          (from, lead) = (lineStart, continued);
        }
      }
    }
    // New lines end the way the edited line does, so a CRLF block stays
    // CRLF, even in a document whose other lines end differently.
    final newline = _lineBreakAt(start);
    String expand(String value) => value.replaceAll('\n', '$newline$prefix');
    final inserted = '$lead${expand(edit.text)}';
    int position(int offset) {
      if (offset < edit.start) return row.sourceForDisplay(offset);
      if (offset <= edit.start + edit.text.length) {
        return from +
            lead.length +
            expand(edit.text.substring(0, offset - edit.start)).length;
      }
      return row.sourceForDisplay(
            offset - edit.text.length + edit.end - edit.start,
          ) +
          inserted.length -
          (end - from);
    }

    var candidate = source.replaceRange(from, end, inserted);
    var selected = FlarkSelection(position(edit.base), position(edit.extent));
    final began = _composition?.source;
    if (began != null &&
        edit.text.isEmpty &&
        (edit.start == 0 || body.codeUnitAt(edit.start - 1) == 10) &&
        (edit.start == body.length || body.codeUnitAt(edit.start) == 10)) {
      // Text composed on an empty line takes the prefix the fence's lines
      // continue with, as above. Erased again before the composition ends,
      // the line gets back the prefix it began with, which lacks at most
      // some trailing whitespace, so ending the composition changes nothing.
      final cut = candidate.length - began.length;
      if (cut > 0 &&
          cut <= from - lineStart &&
          candidate.substring(from - cut, from).trim().isEmpty &&
          candidate.replaceRange(from - cut, from, '') == began) {
        candidate = began;
        selected = FlarkSelection.collapsed(from - cut);
      }
    }
    // Whether a delegate's edit is ordinary typing is worked out only when
    // the caller has not said, as it maps the selection through the row.
    bool ordinaryTyping() =>
        action == CodeEditingAction.insert &&
        edit.start == row.displayForSource(selection.start).$1 &&
        edit.end == row.displayForSource(selection.end).$1 &&
        edit.text == text &&
        FlarkEditor._keystroke(text);
    return _commitCodeEdit(
      row,
      body,
      candidate,
      selected,
      typing: typing ?? ordinaryTyping(),
    );
  }

  /// An imported opener/closer pair has no body source range yet. Create that
  /// range in the same transaction as its first edit, retaining both fences.
  bool _createEmptyCodeBody(
    ProjectedRow row,
    String body,
    int base,
    int extent,
  ) {
    final model = _doc.model;
    final block = model.blockAt(row.block);
    // Use the same parser-owned container ranges as typed fence completion.
    var prefix = continuationPrefix(
      source,
      model,
      block.firstLine,
      block.startUtf16,
      block.index,
    );
    // The body line ends the way the opening fence's line does.
    final newline = _lineBreakAt(block.startUtf16);
    final closed = block.flags & 2 != 0;
    final at = closed
        ? model.lineStartUtf16(block.firstLine + block.lineCount - 1)
        : block.endUtf16;
    final leading = closed || at > 0 && source.codeUnitAt(at - 1) == 10
        ? ''
        : newline;
    if (prefix.contains('\t')) {
      // A prefix tab can contribute virtual code spaces. Authenticate its
      // hidden portion on an unpublished blank body before adding literal text.
      final scaffold = source.replaceRange(
        at,
        at,
        '$leading$prefix${closed ? newline : ''}',
      );
      final prepared = _buildSnapshot(
        scaffold,
        FlarkSelection.collapsed(at + leading.length + prefix.length),
      );
      // This uncommon bodyless/tab shape needs authenticated hidden columns.
      // If even its blank scaffold exceeds live admission, leave it unchanged;
      // the host can offer source mode instead of parsing beyond the bound.
      if (prepared is! FlarkLiveSnapshot) return false;
      final preparedRow = prepared.document.rowAt(
        at + leading.length + prefix.length,
      );
      if (!preparedRow.fenced || preparedRow.block != row.block) return false;
      final contentStart = preparedRow.contentStarts.firstWhere(
        (start) => start >= 0,
        orElse: () => -1,
      );
      if (contentStart < 0) return false;
      prefix = scaffold.substring(at + leading.length, contentStart);
      for (final segment in preparedRow.segments) {
        if (!segment.exact &&
            !segment.lineBreak &&
            segment.sourceStart == contentStart) {
          prefix = _hiddenCodePrefix(prefix, segment.displayLength);
          break;
        }
      }
    }
    String expand(String value) => value.replaceAll('\n', '$newline$prefix');
    final inserted = '$leading$prefix${expand(body)}${closed ? newline : ''}';
    final bodyStart = at + leading.length + prefix.length;
    return _commitCodeEdit(
      row,
      body,
      source.replaceRange(at, at, inserted),
      FlarkSelection(
        bodyStart + expand(body.substring(0, base)).length,
        bodyStart + expand(body.substring(0, extent)).length,
      ),
      typing: false,
    );
  }

  String _hiddenCodePrefix(String prefix, int virtualSpaces) {
    final expanded = StringBuffer();
    for (final unit in prefix.codeUnits) {
      if (unit == 9) {
        expanded.write(' ' * (4 - expanded.length % 4));
      } else {
        expanded.writeCharCode(unit);
      }
    }
    return expanded.toString().substring(0, expanded.length - virtualSpaces);
  }

  bool _commitCodeEdit(
    ProjectedRow row,
    String body,
    String candidate,
    FlarkSelection selected, {
    required bool typing,
  }) {
    final block = _doc.model.blockAt(row.block);
    final marker = source.codeUnitAt(block.startUtf16);
    // Encode the literal body inside the parser-authenticated fence. Choosing
    // a delimiter longer than any same-character body run needs no Markdown
    // recognition, including when a paste joins two existing marker fragments.
    var run = 0, length = block.attr;
    for (var i = 0; i < body.length; i++) {
      run = body.codeUnitAt(i) == marker ? run + 1 : 0;
      if (run >= length) length = run + 1;
    }
    if (length == block.attr &&
        row.contentStarts.any((start) => start >= 0) &&
        !row.segments.any((s) => !s.exact && !s.lineBreak)) {
      return _commit(candidate, selected, coalesce: typing);
    }
    // The block keeps its start, fences, containers and the literal body.
    bool keeps(FlarkDocument next, int caret) {
      final nextRow = next.rowAt(caret);
      if (!nextRow.fenced ||
          nextRow.block != row.block ||
          nextRow.text != body ||
          nextRow.shells.length != row.shells.length) {
        return false;
      }
      final nextBlock = next.model.blockAt(nextRow.block);
      if (nextBlock.startUtf16 != block.startUtf16 ||
          nextBlock.flags & 3 != block.flags & 3) {
        return false;
      }
      for (var i = 0; i < row.shells.length; i++) {
        if (nextRow.shells[i].block != row.shells[i].block ||
            nextRow.shells[i].kind != row.shells[i].kind) {
          return false;
        }
      }
      return true;
    }

    // A run as long as the fence closes the block only alone on its line, so
    // a body can already hold one, indented or beside other text. Where the
    // parser still reads the block as it was, its fences stay as they are:
    // an edit elsewhere in that body must not respell them.
    if (length > block.attr &&
        _commit(
          candidate,
          selected,
          coalesce: typing,
          accept: (next) => keeps(next, selected.extent),
        )) {
      return true;
    }
    if (_lastRejection != null) return false;
    final openingGrowth = length - block.attr;
    final fenceCharacter = String.fromCharCode(marker);
    if (block.flags & 2 != 0) {
      // The model identifies the closing line. Preserve its container prefix,
      // existing longer marker run, and trailing horizontal whitespace exactly.
      final lineStart = _doc.model.lineStartUtf16(
        block.firstLine + block.lineCount - 1,
      );
      var end = block.endUtf16;
      while (end > lineStart &&
          (source.codeUnitAt(end - 1) == 32 ||
              source.codeUnitAt(end - 1) == 9)) {
        end--;
      }
      var start = end;
      while (start > lineStart && source.codeUnitAt(start - 1) == marker) {
        start--;
      }
      if (end - start < block.attr) return false;
      if (length > end - start) {
        final at = end + candidate.length - source.length;
        candidate = candidate.replaceRange(
          at,
          at,
          fenceCharacter * (length - (end - start)),
        );
      }
    }
    final openingEnd = block.startUtf16 + block.attr;
    candidate = candidate.replaceRange(
      openingEnd,
      openingEnd,
      fenceCharacter * openingGrowth,
    );
    final nextSelection = FlarkSelection(
      selected.base + openingGrowth,
      selected.extent + openingGrowth,
    );
    return _commit(
      candidate,
      nextSelection,
      coalesce: typing,
      acceptSourceMode: true,
      accept: (next) => keeps(next, nextSelection.extent),
    );
  }

  String _codeInfo(ProjectedRow row) => row.codeInfoStart < 0
      ? ''
      : source.substring(row.codeInfoStart, row.codeInfoEnd);

  bool _setCodeLanguage(String language) {
    final row = _doc.rowAt(selection.extent);
    if (!row.fenced ||
        row.codeInfoStart < 0 ||
        (language.isNotEmpty &&
            !RegExp(r'^[a-zA-Z0-9_+.#-]{1,40}$').hasMatch(language))) {
      return false;
    }
    final info = _codeInfo(row);
    final tokenEnd = info.indexOf(RegExp(r'\s'));
    final end = tokenEnd < 0 ? info.length : tokenEnd;
    // Keep any info-string metadata. An explicit auto tag preserves its
    // position when clearing the first token would reinterpret it as a language.
    final replacement = language.isEmpty && end < info.length
        ? 'auto'
        : language;
    if (info.substring(0, end) == replacement) {
      // Choosing the language a fence already has is a successful no-op.
      _inert = true;
      return false;
    }
    final (candidate, map) = _edited([
      (row.codeInfoStart, row.codeInfoStart + end, replacement),
    ]);
    return _commit(
      candidate,
      FlarkSelection(map(selection.base), map(selection.extent)),
      coalesce: false,
    );
  }

  /// Enter on a final blank body line finishes the snippet. Null means this
  /// gesture does not apply; a rejected exit must not fall through to newline.
  bool? _exitCodeOnBlankLine(ProjectedRow row) {
    if (!row.fenced || composing) return null;
    final lastBreak = row.text.lastIndexOf('\n');
    if (lastBreak < 0 ||
        codeLeadingWhitespace(row.text.substring(lastBreak + 1)).length !=
            row.text.length - lastBreak - 1) {
      return null;
    }
    final line = _doc.model.lineOfUtf16(selection.extent);
    final i = line - row.firstLine;
    if (i < 0 ||
        i >= row.contentStarts.length ||
        row.contentStarts[i] < 0 ||
        row.contentStarts.skip(i + 1).any((start) => start >= 0)) {
      return null;
    }

    final model = _doc.model;
    final block = model.blockAt(row.block);
    final lineStart = model.lineStartUtf16(line);
    // The fence's containers continue on the lines the exit writes. A blank
    // line needs none of their indentation, so its own prefix can be
    // shorter: the fence's opening line gives it.
    final prefix = continuationPrefix(
      source,
      model,
      model.lineOfUtf16(block.startUtf16),
      block.startUtf16,
      row.block,
    );
    final newline = source.substring(row.contentEnds[i - 1], lineStart);
    final edits = <(int, int, String)>[];
    late int destination;
    if (block.flags & 2 != 0) {
      final closingLine = block.firstLine + block.lineCount - 1;
      edits.add((lineStart, model.lineStartUtf16(closingLine), ''));
      final after = closingLine + 1;
      // Reuse the paragraph created along with a typed fence, when present.
      // Otherwise insert a fresh line without consuming any following prose.
      if (after < model.lineCount &&
          source.substring(
                model.lineStartUtf16(after),
                _sourceLineEnd(model.lineStartUtf16(after)),
              ) ==
              prefix) {
        destination = _sourceLineEnd(model.lineStartUtf16(after));
      } else {
        final end = _sourceLineEnd(model.lineStartUtf16(closingLine));
        edits.add((end, end, '$newline$prefix'));
        destination = end;
      }
    } else {
      // Imported unclosed fences need an actual closer; moving the caret past
      // their source range alone would leave the next character inside code.
      final marker = String.fromCharCode(source.codeUnitAt(block.startUtf16));
      edits.add((
        lineStart,
        row.contentEnds[i],
        '$prefix${marker * block.attr}$newline$prefix',
      ));
      destination = row.contentEnds[i];
    }
    final (edited, map) = _edited(edits);
    final caret = map(destination);
    var candidate = edited;
    final followingStart = candidate.indexOf('\n', caret) + 1;
    if (followingStart > 0 && followingStart < candidate.length) {
      final end = candidate.indexOf('\n', followingStart);
      final following = candidate
          .substring(followingStart, end < 0 ? candidate.length : end)
          .trim();
      // Keep the new paragraph separate from any following content after the
      // next character is typed, including inside a quote or list item.
      if (following.isNotEmpty && following != prefix.trim()) {
        candidate = candidate.replaceRange(caret, caret, '$newline$prefix');
      }
    }
    return _commit(
      candidate,
      FlarkSelection.collapsed(caret),
      coalesce: false,
      acceptSourceMode: true,
      accept: (next) =>
          next.selection.extent == caret &&
          next.rowAt(caret).kind == RowKind.blank &&
          next.projection.rows[row.index].fenced &&
          next.projection.rows[row.index].text ==
              row.text.substring(0, lastBreak),
    );
  }

  bool _codeNewline(ProjectedRow row, int start, int end) {
    if (_doc.rowAt(end).index != row.index) return false;
    final delegated = _delegateCodeEdit(CodeEditingAction.newline);
    if (delegated != null) return delegated;
    final line = _doc.model.lineOfUtf16(start), i = line - row.firstLine;
    final contentStart = row.contentStarts[i];
    if (contentStart < 0) return false;
    if (row.fenced) {
      // The new line keeps the indentation shown before the caret, and the
      // literal code path gives it the line's container prefix.
      final at = row.displayForSource(start).$1;
      final shown = at == 0 ? 0 : row.text.lastIndexOf('\n', at - 1) + 1;
      return _pasteCode(
            '\n${codeLeadingWhitespace(row.text.substring(shown, at))}',
            from: start,
            to: end,
          ) ??
          false;
    }
    // Copied markers of items that open on the line would open new items
    // (`- - -` is a rule); the indentation that continues them does not.
    final prefix = continuationPrefix(
      source,
      _doc.model,
      line,
      contentStart,
      row.block,
    );
    final before = source.substring(contentStart, start);
    final indentation = codeLeadingWhitespace(before);
    final newline = _lineBreakAt(start);
    final first = '$newline$prefix$indentation';
    var inserted = first, to = end;
    // Both parts stay code in the row's containers, or show nothing yet
    // (`-     -      -` left on an item's line would be a rule), and the
    // blocks around keep their kinds and containers. A new last line of
    // code that ends its item or footnote is a blank line Markdown leaves
    // outside them, and no spelling keeps it inside: it shows in their
    // outer containers, carrying their prefix and the code's indentation,
    // so text typed there continues the code, as Return leaves a
    // footnote's next line. A blank line in any other containers fails.
    return _commit(
      source.replaceRange(start, to, inserted),
      FlarkSelection.collapsed(start + first.length),
      coalesce: false,
      acceptSourceMode: true,
      accept: (next) =>
          [start, start + first.length].every((offset) {
            final now = next.rowAt(offset);
            return now.kind == RowKind.blank &&
                    now.withinContainerKindsOf(row) ||
                now.kind == RowKind.codeBlock && now.sameContainerKinds(row);
          }) &&
          _keepsStructure(
            next,
            [(start, to, inserted.length)],
            {row.index},
            shells: true,
          ),
    );
  }

  bool _shiftBlock({required bool outdent}) {
    final row = _doc.rowAt(selection.extent);
    if (row.kind != RowKind.codeBlock) return _shiftItem(outdent: outdent);
    if (_doc.rowAt(selection.start).index != row.index ||
        _doc.rowAt(selection.end).index != row.index) {
      return false;
    }
    final delegated = _delegateCodeEdit(
      outdent ? CodeEditingAction.outdent : CodeEditingAction.indent,
    );
    if (delegated != null) return delegated;
    final first = row.lineIndexOf(_doc.model, selection.start);
    var last = row.lineIndexOf(_doc.model, selection.end);
    if (row.contentStarts[first] < 0 || row.contentStarts[last] < 0) {
      return false;
    }
    final unit = codeIndentUnit(
      row.text,
      codeEditing?.resolveLanguage(row.text, _codeInfo(row)) ?? '',
    );
    if (!outdent && selection.isCollapsed) {
      final at = selection.extent;
      return _commit(
        source.replaceRange(at, at, unit),
        FlarkSelection.collapsed(at + unit.length),
        coalesce: false,
      );
    }
    if (!selection.isCollapsed &&
        last > first &&
        selection.end == row.contentStarts[last]) {
      last--;
    }
    final edits = <(int, int, String)>[];
    for (var i = first; i <= last; i++) {
      final at = row.contentStarts[i];
      if (at < 0) return false;
      if (!outdent) {
        // A blank line takes no indentation. In a quote or list item its
        // source can lack part of the container's prefix, which would absorb
        // some or all of the step, and there is no code on it to move.
        if (at != row.contentEnds[i]) edits.add((at, at, unit));
        continue;
      }
      final leading = codeLeadingWhitespace(
        source.substring(at, row.contentEnds[i]),
      );
      if (leading.isEmpty) continue;
      final length = leading.startsWith('\t')
          ? 1
          : leading.length.clamp(0, unit.length);
      edits.add((at, at + length, ''));
    }
    // Indenting only blank lines changes nothing. Like every Indent or
    // Outdent that cannot apply, that is no refusal (see [FlarkEditor.apply]).
    if (edits.isEmpty) return false;
    final (candidate, map) = _edited(edits);
    final shifted = FlarkSelection(map(selection.base), map(selection.extent));
    if (!row.fenced) return _commit(candidate, shifted, coalesce: false);
    // Outdented to three spaces, a body line of fence characters would close
    // the block, so the shifted body is committed as literal code.
    var body = row.text;
    for (final (a, b, text) in edits.reversed) {
      body = body.replaceRange(
        row.displayForSource(a).$1,
        row.displayForSource(b).$1,
        text,
      );
    }
    return _commitCodeEdit(row, body, candidate, shifted, typing: false);
  }
}
