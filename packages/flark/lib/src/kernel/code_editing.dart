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
  bool? _pasteCode(String text) {
    final row = _doc.rowAt(selection.extent);
    if (!row.fenced || _doc.rowAt(selection.base).index != row.index) {
      return null;
    }
    final start = row.displayForSource(selection.start).$1;
    final end = row.displayForSource(selection.end).$1;
    final normalized = text.replaceAll('\r\n', '\n');
    final caret = start + normalized.length;
    return _applyCodeEdit(
      row,
      CodeEditProposal(start, end, normalized, caret, caret),
      CodeEditingAction.insert,
    );
  }

  bool _applyCodeEdit(
    ProjectedRow row,
    CodeEditProposal edit,
    CodeEditingAction action, {
    String text = '',
  }) {
    if (edit.start < 0 || edit.end < edit.start || edit.end > row.text.length) {
      throw StateError('Invalid code edit range');
    }
    final body = row.text.replaceRange(edit.start, edit.end, edit.text);
    try {
      validateFlarkSource(body);
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
    if (body == row.text) return false;
    if (row.contentStarts.every((start) => start < 0)) {
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
    var prefix = source.substring(
      _doc.model.lineStartUtf16(line),
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
    final newline = source.contains('\r\n') ? '\r\n' : '\n';
    String expand(String value) => value.replaceAll('\n', '$newline$prefix');
    final inserted = expand(edit.text);
    int position(int offset) {
      if (offset < edit.start) return row.sourceForDisplay(offset);
      if (offset <= edit.start + edit.text.length) {
        return start +
            expand(edit.text.substring(0, offset - edit.start)).length;
      }
      return row.sourceForDisplay(
            offset - edit.text.length + edit.end - edit.start,
          ) +
          inserted.length -
          (end - start);
    }

    final ordinaryTyping =
        action == CodeEditingAction.insert &&
        edit.start == row.displayForSource(selection.start).$1 &&
        edit.end == row.displayForSource(selection.end).$1 &&
        edit.text == text &&
        text != '\n' &&
        text.characters.length == 1;
    return _commitCodeEdit(
      row,
      body,
      source.replaceRange(start, end, inserted),
      FlarkSelection(position(edit.base), position(edit.extent)),
      typing: ordinaryTyping,
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
    final lineStart = model.lineStartUtf16(block.firstLine);
    var prefix = source.substring(lineStart, block.startUtf16);
    // Use the same parser-owned item ranges as typed fence completion.
    var child = block;
    for (var parent = block.parent; parent != noParent;) {
      final ancestor = model.blockAt(parent);
      if (ancestor.kind == BlockKind.item &&
          ancestor.firstLine == block.firstLine) {
        final from = ancestor.startUtf16 - lineStart;
        final to = child.startUtf16 - lineStart;
        final padding = prefix
            .substring(from, to)
            .split('')
            .map((char) => char == '\t' ? '\t' : ' ')
            .join();
        prefix = prefix.replaceRange(from, to, padding);
      }
      child = ancestor;
      parent = ancestor.parent;
    }
    final newline = source.contains('\r\n') ? '\r\n' : '\n';
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
    var run = 0, length = block.attr0;
    for (final unit in body.codeUnits) {
      run = unit == marker ? run + 1 : 0;
      if (run >= length) length = run + 1;
    }
    if (length == block.attr0 &&
        row.contentStarts.any((start) => start >= 0) &&
        !row.segments.any((s) => !s.exact && !s.lineBreak)) {
      return _commit(candidate, selected, typing: typing);
    }
    final openingGrowth = length - block.attr0;
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
      if (end - start < block.attr0) return false;
      if (length > end - start) {
        final at = end + candidate.length - source.length;
        candidate = candidate.replaceRange(
          at,
          at,
          fenceCharacter * (length - (end - start)),
        );
      }
    }
    final openingEnd = block.startUtf16 + block.attr0;
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
      typing: typing,
      acceptSourceMode: true,
      accept: (next) {
        final nextRow = next.rowAt(nextSelection.extent);
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
      },
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
    if (info.substring(0, end) == replacement) return false;
    final (candidate, map) = _edited([
      (row.codeInfoStart, row.codeInfoStart + end, replacement),
    ]);
    return _commit(
      candidate,
      FlarkSelection(map(selection.base), map(selection.extent)),
      typing: false,
    );
  }

  bool _codeNewline(ProjectedRow row, int start, int end) {
    if (_doc.rowAt(end).index != row.index) return false;
    final delegated = _delegateCodeEdit(CodeEditingAction.newline);
    if (delegated != null) return delegated;
    final line = _doc.model.lineOfUtf16(start), i = line - row.firstLine;
    final contentStart = row.contentStarts[i];
    if (contentStart < 0) return false;
    final prefix = source.substring(
      _doc.model.lineStartUtf16(line),
      contentStart,
    );
    final before = source.substring(contentStart, start);
    final indentation = codeLeadingWhitespace(before);
    final newline = source.contains('\r\n') ? '\r\n' : '\n';
    final first = '$newline$prefix$indentation';
    var inserted = first, to = end;
    return _commit(
      source.replaceRange(start, to, inserted),
      FlarkSelection.collapsed(start + first.length),
      typing: false,
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
    final first = _doc.model.lineOfUtf16(selection.start) - row.firstLine;
    var last = _doc.model.lineOfUtf16(selection.end) - row.firstLine;
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
        typing: false,
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
        edits.add((at, at, unit));
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
    if (edits.isEmpty) return false;
    final (candidate, map) = _edited(edits);
    return _commit(
      candidate,
      FlarkSelection(map(selection.base), map(selection.extent)),
      typing: false,
    );
  }
}
