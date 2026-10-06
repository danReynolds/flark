part of 'editor.dart';

/// The styles a pair of delimiters spells, outermost first where they nest:
/// pending strong emphasis opens with `***`.
const _delimitedStyles = [
  Style.strong,
  Style.emphasis,
  Style.strikethrough,
  Style.code,
];

/// The delimiter that spells [style] on each side of its text, or null for a
/// style no delimiter pair spells.
String? _styleDelimiter(int style) => switch (style) {
  Style.emphasis => '*',
  Style.strong => '**',
  Style.strikethrough => '~~',
  Style.code => '`',
  _ => null,
};

FlarkStyleValue _selectedStyleValue(FlarkDocument doc, int style) {
  final sel = doc.selection;
  var on = false, off = false;
  final first = doc.displayOf(sel.start), last = doc.displayOf(sel.end);
  for (var i = first.row; i <= last.row; i++) {
    final row = doc.projection.rows[i];
    final start = i == first.row ? first.offset : 0;
    final end = i == last.row ? last.offset : row.text.length;
    for (final segment in row.segments) {
      final a = segment.displayStart.clamp(start, end);
      final b = segment.displayEnd.clamp(start, end);
      // Separating whitespace cannot carry emphasis on its own in Markdown.
      if (a == b || row.text.substring(a, b).trim().isEmpty) continue;
      if (segment.styles & style != 0) {
        on = true;
      } else {
        off = true;
      }
      if (on && off) return FlarkStyleValue.mixed;
    }
  }
  return on ? FlarkStyleValue.on : FlarkStyleValue.off;
}

extension _InlineFormatting on FlarkEditor {
  ({int start, int end}) _styleRange() {
    // Select All can include a heading/list/quote prefix. Inline formatting
    // targets that block's visible content, never its structural marker.
    final first = _doc.rowAt(selection.start), last = _doc.rowAt(selection.end);
    return _contentRange(
      selection.start.clamp(first.sourceStart, first.sourceEnd),
      selection.end.clamp(last.sourceStart, last.sourceEnd),
    );
  }

  /// The owners of [style] in [row]'s block, in source order.
  List<Owner> _styleOwners(ProjectedRow row, int style) => [
    for (final owner in _doc.ownersOf(row))
      if (owner.style == style) owner,
  ];

  FlarkStyleState _styleState(int style) {
    if (sourceMode || _styleDelimiter(style) == null) {
      return const FlarkStyleState(
        value: FlarkStyleValue.off,
        canEnable: false,
        canDisable: false,
      );
    }
    final sel = selection, row = _doc.caretRow;
    final value = sel.isCollapsed
        ? (typingContext & style != 0
              ? FlarkStyleValue.on
              : FlarkStyleValue.off)
        : _selectedStyleValue(_doc, style);
    var enabled = switch (row.kind) {
      RowKind.paragraph ||
      RowKind.heading ||
      RowKind.tableCell ||
      RowKind.blank => true,
      _ => false,
    };
    if (sel.isCollapsed) {
      // Literal code cannot interpret a newly inserted emphasis delimiter.
      if (style != Style.code &&
          value != FlarkStyleValue.on &&
          sel.tableCell == null &&
          _doc.ownersAt(sel.extent).any((o) => o.style == Style.code)) {
        enabled = false;
      }
    } else {
      final range = _styleRange();
      enabled =
          enabled &&
          _supportedRange(range.start, range.end) &&
          _doc.visibleText(range.start, range.end).trim().isNotEmpty;
      // Partial-owner and cross-block transformations retain the kernel's
      // bounded editing contract. Never claim that a destructive rewrite is
      // available merely because the caret endpoint is in a paragraph.
      if (enabled) {
        for (final owner in _styleOwners(row, style)) {
          if (owner.contentStart >= range.end ||
              owner.contentEnd <= range.start) {
            continue;
          }
          if (owner.contentStart < range.start ||
              owner.contentEnd > range.end) {
            enabled = false;
            break;
          }
        }
      }
      if (style != Style.code &&
          value != FlarkStyleValue.on &&
          row.segments.any(
            (s) =>
                s.sourceStart < range.end &&
                s.sourceEnd > range.start &&
                s.styles & Style.code != 0,
          )) {
        enabled = false;
      }
    }
    return FlarkStyleState(
      value: value,
      canEnable: enabled,
      canDisable: enabled,
    );
  }

  bool _setStyle(int style, bool enabled) {
    final state = styleState(style);
    // SetStyle asking for the formatting the caret or selection already has
    // changes nothing: a successful no-op, which records no step.
    if (_styleDelimiter(style) != null &&
        state.value == (enabled ? FlarkStyleValue.on : FlarkStyleValue.off)) {
      _inert = true;
      return false;
    }
    if (!(enabled ? state.canEnable : state.canDisable)) return false;
    if (selection.isCollapsed) return _toggleCaretStyle(style);
    final range = _styleRange();
    final owners = _styleOwners(_doc.caretRow, style)
        .where(
          (o) => o.contentStart >= range.start && o.contentEnd <= range.end,
        )
        .toList();
    // Include the selected owners' delimiters, including outside anchors.
    var start = range.start, end = range.end;
    for (final owner in owners) {
      if (owner.start < start) start = owner.start;
      if (owner.end > end) end = owner.end;
    }
    final cuts = <(int, int)>[
      for (final o in owners) ...[
        (o.start, o.contentStart),
        (o.contentEnd, o.end),
      ],
    ]..sort((a, b) => b.$1.compareTo(a.$1));
    var content = source.substring(start, end);
    for (final (a, b) in cuts) {
      content = content.replaceRange(a - start, b - start, '');
    }
    var selectedStart = start, selectedEnd = start + content.length;
    if (enabled) {
      final delimiter = _styleDelimiter(style)!;
      // Keep whitespace outside emphasis delimiters so they remain valid.
      final leading = content.length - content.trimLeft().length;
      final trailing = content.trimRight().length;
      content =
          '${content.substring(0, leading)}$delimiter${content.substring(leading, trailing)}$delimiter${content.substring(trailing)}';
      selectedStart += leading + delimiter.length;
      selectedEnd = start + trailing + delimiter.length;
    }
    final nextSelection = selection.base <= selection.extent
        ? FlarkSelection(selectedStart, selectedEnd)
        : FlarkSelection(selectedEnd, selectedStart);
    final before = _formattingContent(_doc, style);
    return _commit(
      source.replaceRange(start, end, content),
      nextSelection,
      coalesce: false,
      accept: (next) =>
          _selectedStyleValue(next, style) ==
              (enabled ? FlarkStyleValue.on : FlarkStyleValue.off) &&
          _sameFormattingContent(before, _formattingContent(next, style)),
    );
  }

  /// SetStyle at a caret: across an owner of [style] at its edge, unwrapping
  /// one around the caret, or toggling [style] in the pending style the next
  /// typed text takes.
  bool _toggleCaretStyle(int style) {
    final sel = selection;
    final caret = sel.extent;
    // At an edge of an owner, step across its delimiter: out when inside,
    // in when outside. Strictly inside, unwrap it if what it held still
    // shows as it did, as a selection's toggle requires.
    for (final o in sel.tableCell == null ? _doc.ownersAt(caret) : <Owner>[]) {
      if (o.style != style) continue;
      if (caret == o.contentEnd) {
        return _select(FlarkSelection.collapsed(o.end));
      }
      if (caret == o.contentStart) {
        return _select(FlarkSelection.collapsed(o.start));
      }
      final s = source
          .replaceRange(o.contentEnd, o.end, '')
          .replaceRange(o.start, o.contentStart, '');
      final before = _formattingContent(_doc, style);
      return _commit(
        s,
        FlarkSelection.collapsed(caret - (o.contentStart - o.start)),
        coalesce: false,
        accept: (next) =>
            _sameFormattingContent(before, _formattingContent(next, style)),
      );
    }
    for (final o
        in sel.tableCell == null ? _doc.ownersTouching(caret) : <Owner>[]) {
      if (o.style != style) continue;
      return _select(
        FlarkSelection.collapsed(
          caret == o.end ? o.contentEnd : o.contentStart,
        ),
      );
    }
    final mask = (_pending?.styles ?? 0) ^ style;
    // Recombine the supported formatting intents before materializing them.
    // Link/image destinations are separate pending closures, not style bits we
    // can reconstruct without their original owner records.
    if (_delimitedStyles.fold(mask, (rest, s) => rest & ~s) != 0) return false;
    final delimiters = [
      for (final s in _delimitedStyles)
        if (mask & s != 0) _styleDelimiter(s)!,
    ];
    _pending = mask == 0
        ? null
        : PendingStyle(delimiters.join(), delimiters.reversed.join(), mask);
    history.breakCoalescing();
    _goalColumn = null;
    return true;
  }
}

// Formatting must preserve visible text, block structure and all other styles.
// Compare runs after coalescing segmentation changes caused by new wrappers.
List<Object> _formattingContent(FlarkDocument doc, int exceptStyle) {
  final out = <Object>[];
  for (final row in doc.projection.rows) {
    out.add((row.kind, row.text, row.headingLevel));
    var end = 0, mask = -1;
    for (final s in row.segments) {
      final nextMask = s.styles & ~exceptStyle;
      if (mask != nextMask && mask >= 0) out.add((end, mask));
      mask = nextMask;
      end = s.displayEnd;
    }
    if (mask >= 0) out.add((end, mask));
  }
  return out;
}

bool _sameFormattingContent(List<Object> a, List<Object> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
