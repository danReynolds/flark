part of 'editor.dart';

extension _ResourceEditing on FlarkEditor {
  bool _setResource(
    bool image,
    String destination,
    String? label,
    String? title,
  ) {
    final existing = _doc.resourceAt(selection, image: image);
    final start = existing?.start ?? selection.start;
    final end = existing?.end ?? selection.end;
    if (!_canSetResource(image) ||
        destination.isEmpty ||
        _resourceControl(destination) ||
        (label != null && _resourceControl(label)) ||
        (title != null && _resourceControl(title))) {
      return false;
    }
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
      typing: false,
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
          FlarkEditor._sameShells(doc.rowAt(at), row) &&
          _keepsStructure(
            doc,
            [(at, at + end - start, replacement.length + gap.length)],
            {row.index},
            shells: true,
          ),
    );
  }

  bool _canSetResource(bool image) {
    final existing = _doc.resourceAt(selection, image: image);
    final start = existing?.start ?? selection.start;
    final end = existing?.end ?? selection.end;
    if (!_supportedRange(start, end)) return false;
    final row = _doc.rowAt(start);
    // Code shows its source; so do a link reference definition, whose label
    // a link would end, and a rule, whose dashes text beside it would paint.
    if (row.kind == RowKind.codeBlock ||
        row.kind == RowKind.definition ||
        row.kind == RowKind.thematicBreak ||
        _doc.rowAt(end).index != row.index) {
      return false;
    }
    if (existing == null &&
        _doc.resources.any(
          (r) =>
              start < r.end && end > r.start ||
              start == end && r.contentStart <= start && start <= r.contentEnd,
        )) {
      return false;
    }
    final from = existing?.contentStart ?? start;
    return !_doc.ownersAt(from).any((o) => o.kind == RunKind.code);
  }

  bool _removeResource(bool image) {
    final resource = _doc.resourceAt(selection, image: image);
    if (resource == null) return false;
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
      final normalized = _normalizeInlineEdges(
        range.start,
        range.end,
        '',
        range.start,
        pending: pending,
      );
      return _commit(
        normalized.text,
        FlarkSelection.collapsed(normalized.caret),
        typing: false,
        pending: normalized.pending,
      );
    }
    var content = source.substring(resource.contentStart, resource.contentEnd);
    if (_doc.model.runAt(resource.run).kind == RunKind.autolink) {
      content = _literalResourceText(resource.text);
    } else {
      // Unwrapping a URL label must not immediately turn it into a GFM
      // automatic link. Escape punctuation in parser-owned text leaves only;
      // retain inline formatting, code, images and already escaped text.
      for (final run in _doc.model.runs.toList().reversed) {
        if (run.kind != RunKind.text ||
            run.startUtf16 < resource.contentStart ||
            run.endUtf16 > resource.contentEnd) {
          continue;
        }
        final a = run.startUtf16 - resource.contentStart;
        final b = run.endUtf16 - resource.contentStart;
        content = content.replaceRange(
          a,
          b,
          source
              .substring(run.startUtf16, run.endUtf16)
              .replaceAllMapped(RegExp(r'[:@.]'), (m) => '\\${m[0]}'),
        );
      }
    }
    return _commit(
      source.replaceRange(resource.start, resource.end, content),
      FlarkSelection.collapsed(resource.start + content.length),
      typing: false,
    );
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
