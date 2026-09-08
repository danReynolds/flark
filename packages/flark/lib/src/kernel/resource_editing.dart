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
    if (!_supportedRange(start, end)) return false;
    final row = _doc.rowAt(start);
    if (row.kind == RowKind.codeBlock ||
        _doc.rowAt(end).index != row.index ||
        destination.isEmpty ||
        _resourceControl(destination) ||
        (label != null && _resourceControl(label)) ||
        (title != null && _resourceControl(title))) {
      return false;
    }
    // No nested links or partial replacement of another resource. Images may
    // remain inside an existing link when that image itself is being edited.
    if (existing == null &&
        _doc.resources.any(
          (r) =>
              start < r.end && end > r.start ||
              start == end && r.contentStart <= start && start <= r.contentEnd,
        )) {
      return false;
    }
    final from = existing?.contentStart ?? start;
    final to = existing?.contentEnd ?? end;
    if (_doc.ownersAt(from).any((o) => o.kind == RunKind.code)) return false;
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
    final candidate = source.replaceRange(start, end, replacement);
    final contentEnd = start + open.length + content.length;
    return _commit(
      candidate,
      FlarkSelection.collapsed(contentEnd),
      typing: false,
      accept: (doc) => doc.resources.any(
        (r) =>
            r.start == start &&
            r.end == start + replacement.length &&
            r.isImage == image &&
            r.destination == destination &&
            r.title == resolvedTitle &&
            r.contentEnd == contentEnd,
      ),
    );
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
