part of 'editor.dart';

extension _ResourceEditing on FlarkEditor {
  bool _setResource(
    bool image,
    String destination,
    String? label,
    String? title,
  ) {
    final target = _resourceTarget(image);
    if (target == null ||
        destination.isEmpty ||
        _resourceControl(destination) ||
        (label != null && _resourceControl(label)) ||
        (title != null && _resourceControl(title))) {
      return false;
    }
    final (:existing, :start, :end) = target;
    // Asking a link or image for the destination and title it already has
    // changes nothing. Rewriting it anyway would respell parser-authenticated
    // source: an inline destination gains angle brackets, and a reference or
    // automatic link becomes inline, stranding its definition.
    if (existing != null &&
        label == null &&
        existing.destination == destination &&
        (title == null || title == existing.title)) {
      _inert = true;
      return false;
    }
    final from = existing?.contentStart ?? start;
    final to = existing?.contentEnd ?? end;
    final content = label != null
        ? _literalResourceText(label)
        : from == to && existing == null
        ? _literalResourceText(image ? 'Image' : destination)
        : source.substring(from, to);
    final resolvedTitle = title ?? existing?.title ?? '';
    final open = image ? '![' : '[';
    final suffix = resolvedTitle.isEmpty
        ? ''
        : ' "${_resourceAttribute(resolvedTitle)}"';
    final replacement =
        '$open$content](<${_resourceAttribute(destination)}>$suffix)';
    // A new resource at a caret goes where typed text would, so in an empty
    // closed heading its closing sequence stays hidden.
    final (at, gap) = start == end
        ? _sequencePlace(_doc.rowAt(start), start, replacement)
        : (start, '');
    final candidate = source.replaceRange(
      at,
      at + end - start,
      '$replacement$gap',
    );
    final contentEnd = at + open.length + content.length;
    final row = _doc.rowAt(start);
    return _commit(
      candidate,
      FlarkSelection.collapsed(contentEnd),
      coalesce: false,
      accept: (doc) =>
          doc.resources.any(
            (r) =>
                r.start == at &&
                r.end == at + replacement.length &&
                r.isImage == image &&
                r.destination == destination &&
                r.title == resolvedTitle &&
                r.contentEnd == contentEnd,
          ) &&
          // Every other row keeps its kind and containers, the edited row
          // its kind (an empty row takes the resource in the containers it
          // shows), and nothing hidden shows: a link written after a rule's
          // dashes would paint them, one on an empty line before indented
          // code would make it the link's paragraph.
          (row.kind == RowKind.blank || doc.rowAt(at).kind == row.kind) &&
          doc.rowAt(at).sameContainerKinds(row) &&
          _keepsStructure(
            doc,
            Edits([(at, at + end - start, '$replacement$gap')]),
            {row.index},
            shells: true,
          ),
    );
  }

  bool _canSetResource(bool image) => _resourceTarget(image) != null;

  /// What SetLink or SetImage edits: the resource of its kind around the
  /// selection, or the selection to make one of; null where neither can be.
  ({InlineResource? existing, int start, int end})? _resourceTarget(
    bool image,
  ) {
    final existing = _doc.resourceAt(selection, image: image);
    final start = existing?.start ?? selection.start;
    final end = existing?.end ?? selection.end;
    if (!_supportedRange(start, end)) return null;
    final row = _doc.rowAt(start);
    // Code shows its source; so do a link reference definition, whose label
    // a link would end, and a rule, whose dashes text beside it would paint.
    if (row.kind == RowKind.codeBlock ||
        row.kind == RowKind.definition ||
        row.kind == RowKind.thematicBreak ||
        _doc.rowAt(end).index != row.index) {
      return null;
    }
    if (existing == null &&
        _doc.resources.any(
          (r) =>
              start < r.end && end > r.start ||
              start == end && r.contentStart <= start && start <= r.contentEnd,
        )) {
      return null;
    }
    final from = existing?.contentStart ?? start;
    if (_doc.ownersAt(from).any((o) => o.kind == RunKind.code)) return null;
    return (existing: existing, start: start, end: end);
  }

  /// RemoveLink keeps the link's text as text, RemoveImage deletes the
  /// image, each checked as SetLink and SetImage are: every other row keeps
  /// its kind and containers, the edited row its containers and its kind
  /// (or none, left with nothing to show), and nothing hidden shows. The
  /// unlinked text shows exactly as the link did, and no link is left over
  /// it. Where Markdown would read the result otherwise, a faithful
  /// respelling is tried before refusing: unlinked text that would start a
  /// block at its line's start (`1. Intro` under an item's marker) escapes
  /// its first punctuation, text GFM would read as an address escapes its
  /// punctuation, and an image that starts its line's text takes the
  /// whitespace after it, which would otherwise indent the line (into code,
  /// or out of its table). Only the removal as asked may leave the live
  /// tier, as with other respellings: past it, a respelling is not checked.
  bool _removeResource(bool image) {
    final resource = _doc.resourceAt(selection, image: image);
    if (resource == null) return false;
    final row = _doc.rowAt(resource.start);
    bool keeps(FlarkDocument next, int caret, Edit edit) {
      final now = next.rowAt(caret);
      return (now.kind == row.kind ||
              now.kind == RowKind.blank && (image || row.text.isEmpty)) &&
          now.sameContainerKinds(row) &&
          (image || now.text == row.text) &&
          _keepsStructure(next, Edits([edit]), {row.index}, shells: true);
    }

    if (image) {
      final range = _rangeForEmptying(resource.start, resource.end);
      final owners = _doc
          .ownersAt(selection.extent)
          .where(
            (o) =>
                o.run != resource.run &&
                o.start >= range.start &&
                o.end <= range.end,
          )
          .toList();
      final pending = range.pending == null || owners.isEmpty
          ? null
          : PendingStyle(
              owners
                  .map((o) => source.substring(o.start, o.contentStart))
                  .join(),
              owners.reversed
                  .map((o) => source.substring(o.contentEnd, o.end))
                  .join(),
              owners.fold(0, (style, o) => style | o.style),
            );
      final heading = _emptySetext(
        range.start,
        range.end,
        '',
        typing: false,
        pending: pending,
      );
      if (heading != null) return heading;
      final m = _doc.model, line = m.lineOfUtf16(range.start);
      final i = (line - row.firstLine).clamp(0, row.lineCount - 1);
      final lineEnd = projection.lineContentEnd(line);
      var spaced = range.end;
      if (row.contentStarts[i] >= 0 &&
          row.displayForSource(range.start).$1 ==
              row.displayForSource(row.contentStarts[i]).$1) {
        while (spaced < lineEnd && FlarkEditor._isSpace(source, spaced)) {
          spaced++;
        }
      }
      // Where each spelling's removal ends: at the image, or past the
      // whitespace after it.
      final ends = <Spelling, int>{};
      for (final end in [range.end, if (spaced > range.end) spaced]) {
        final normalized = _normalizeInlineEdges(
          range.start,
          end,
          '',
          range.start,
          pending: pending,
        );
        final spelling = Spelling(
          Edits.between(source, normalized.text),
          FlarkSelection.collapsed(normalized.caret),
          pending: normalized.pending,
          asAsked: end == range.end,
        );
        ends[spelling] = end;
      }
      return _commitSpellings([...ends.keys], (next, spelling, _) {
            final removed = (range.start, ends[spelling]!, '');
            return keeps(next, spelling.selection.extent, removed);
          }, coalesce: false)
          is _Committed;
    }
    // The text as written goes first: only the parser knows whether GFM
    // reads an automatic link in it, alone or with the text beside it. The
    // fallback escapes where an address could be read: `:`, `@` and `.` in
    // the label's text (not its formatting, code, images or escapes), or all
    // of an automatic link's punctuation.
    final autolink = _doc.model.runAt(resource.run).kind == RunKind.autolink;
    final written = autolink
        ? resource.text
        : source.substring(resource.contentStart, resource.contentEnd);
    var escaped = autolink ? _literalResourceText(written) : written;
    if (!autolink) {
      final runs = _doc.model.runsOfBlock(resource.block).toList();
      for (final run in runs.reversed) {
        // An escape's character is a text run of its own, already escaped.
        final parent = run.parent;
        if (run.kind != RunKind.text ||
            parent != noParent &&
                _doc.model.runKind(parent) == RunKind.escape ||
            run.startUtf16 < resource.contentStart ||
            run.endUtf16 > resource.contentEnd) {
          continue;
        }
        final a = run.startUtf16 - resource.contentStart;
        final b = run.endUtf16 - resource.contentStart;
        escaped = escaped.replaceRange(
          a,
          b,
          source
              .substring(run.startUtf16, run.endUtf16)
              .replaceAllMapped(RegExp(r'[:@.]'), (m) => '\\${m[0]}'),
        );
      }
    }
    // A bare address written as it is changes no source: its check, that
    // no link is left, passes it over.
    return _commitSpellings(
          [
            for (final content in {written, escaped})
              for (final text in [
                content,
                if (_firstEscapable.matchAsPrefix(content) case final first?)
                  content.replaceRange(first.end - 1, first.end - 1, r'\'),
              ])
                Spelling(
                  Edits([(resource.start, resource.end, text)]),
                  FlarkSelection.collapsed(resource.start + text.length),
                  asAsked: text == written,
                ),
          ],
          (next, spelling, edits) {
            final caret = spelling.selection.extent;
            return keeps(next, caret, edits.list.single) &&
                !next.resources.any(
                  (r) =>
                      !r.isImage && r.start < caret && r.end > resource.start,
                );
          },
          coalesce: false,
        )
        is _Committed;
  }
}

bool _resourceControl(String text) =>
    text.codeUnits.any((c) => c < 32 || c == 127);

// Canonical serialization of explicit user values, never Markdown recognition.
String _literalResourceText(String text) => text.replaceAllMapped(
  RegExp(r'[!"#$%&\x27()*+,\-./:;<=>?@\[\]\\^_`{|}~]'),
  (m) => '\\${m[0]}',
);
String _resourceAttribute(String text) => text
    .replaceAll('&', '&amp;')
    .replaceAll('\\', '&#92;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
