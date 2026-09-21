import 'dart:math' as math;

import 'package:characters/characters.dart';
import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:fleury/fleury_core.dart';

import 'controller.dart';
import 'theme.dart';

/// Host-only geometry. Offsets in a projected row are UTF-16; columns are cells.
final class CellGlyph {
  const CellGlyph(
    this.text,
    this.col,
    this.width,
    this.start,
    this.end,
    this.style,
  );
  final String text;
  final int col, width, start, end;
  final CellStyle style;
}

final class CellLine {
  CellLine(this.row, this.start, this.prefix, {this.sourceStart = 0});
  final ProjectedRow? row;
  final int start, sourceStart;
  final String prefix;
  int left = 0, right = 0;
  List<CellLine>? cells;
  String? rule;
  int blockLeft = 0;
  bool? codeEdgeTop;
  bool headingRule = false;
  int? headingLabelColumn;
  bool caretLine = true;
  InlineResource? image;
  InlineResource? imageLabel;
  bool labelVisible(FlarkSelection selection) =>
      imageLabel == null ||
      (selection.start <= imageLabel!.contentEnd &&
          selection.end >= imageLabel!.contentStart);
  Iterable<CellLine> get fragments => cells ?? [this];
  bool get editable =>
      rule == null && image == null && codeEdgeTop == null && !headingRule;

  CellLine cellAt(int col) {
    final parts = cells;
    if (parts == null) return this;
    return parts.firstWhere(
      (part) => col < part.right,
      orElse: () => parts.last,
    );
  }

  /// Only the painted checkbox is actionable, never its continuation padding.
  int get taskColumn {
    if (row?.shells.any((shell) => shell.task) != true) return -1;
    for (var i = 0; i < prefix.length; i++) {
      if (prefix[i] == '[') return left + i;
    }
    return -1;
  }

  int get taskWidth => 3;
  final glyphs = <CellGlyph>[];
  late int end = start;
  int get endColumn => glyphs.isEmpty
      ? left + prefix.length
      : glyphs.last.col + glyphs.last.width;

  int sourceAt(int offset, {Anchor anchor = Anchor.after}) {
    if (row?.kind == RowKind.tableCell) {
      offset = math.min(offset, glyphs.lastOrNull?.end ?? start);
    }
    return row?.sourceForDisplay(offset, anchor: anchor) ??
        sourceStart + offset;
  }

  (int, bool) hit(int col) {
    for (final glyph in glyphs) {
      if (col < glyph.col) return (glyph.start, true);
      if (col < glyph.col + glyph.width) {
        final leading = (col - glyph.col) * 2 < glyph.width;
        return (leading ? glyph.start : glyph.end, leading);
      }
    }
    return (
      row?.kind == RowKind.tableCell ? (glyphs.lastOrNull?.end ?? start) : end,
      false,
    );
  }

  int columnAt(int offset) {
    for (final glyph in glyphs) {
      if (offset < glyph.end) return glyph.col;
    }
    return endColumn;
  }
}

final class CellDocumentLayout {
  CellDocumentLayout(this.controller, this.cols, this.theme, this.policy)
    : source = controller.editor.source,
      projection = controller.editor.sourceMode
          ? null
          : controller.editor.projection {
    final editor = controller.editor;
    if (editor.sourceMode) {
      // Keep raw-source geometry bounded even for the kernel's 1 MiB ceiling.
      // Movement remains in full source coordinates; reaching an edge recenters.
      var start = (editor.selection.extent - 4096).clamp(
        0,
        editor.source.length,
      );
      var end = (start + 8192).clamp(0, editor.source.length);
      if (start > 0 && _lowSurrogate(editor.source.codeUnitAt(start))) start--;
      if (end < editor.source.length &&
          _lowSurrogate(editor.source.codeUnitAt(end))) {
        end++;
      }
      sourceWindow = (start, end);
      _addText(
        null,
        editor.source.substring(start, end),
        '',
        sourceStart: start,
      );
    } else {
      final rows = editor.projection.rows;
      _measureMarkers(rows);
      for (var i = 0; i < rows.length;) {
        final row = rows[i];
        if (row.kind == RowKind.tableCell) {
          var end = i + 1;
          while (end < rows.length && rows[end].tableBlock == row.tableBlock) {
            end++;
          }
          _addTable(rows.sublist(i, end));
          i = end;
        } else {
          final prefix = _prefix(row);
          final left = math.min(prefix.length, math.max(0, cols - 2));
          final image = _imageResources.elementAtOrNull(_imageIndex);
          final standalone =
              row.shells.isEmpty &&
              theme.imagePreviewRows > 0 &&
              image != null &&
              image.start == row.sourceStart &&
              image.end == row.sourceEnd;
          if (!standalone) _addText(row, row.text, prefix);
          _addImages(
            row,
            lines,
            left,
            cols - left,
            prefix: _continuation(prefix.substring(0, left), row),
          );
          if (standalone) {
            final first = lines.length;
            _addText(row, row.text, prefix);
            for (final line in lines.skip(first)) {
              line.imageLabel = image;
              final shift = math.max(
                0,
                (cols - line.endColumn - left - 1) ~/ 2,
              );
              _shift(line, shift);
            }
          }
          i++;
        }
      }
    }
    if (lines.isEmpty) lines.add(CellLine(null, 0, ''));
    for (var i = 0; i < lines.length; i++) {
      for (final line in lines[i].fragments) {
        final row = line.row;
        if (row != null && line.editable && line.caretLine) {
          _rowLines.putIfAbsent(row.index, () => []).add((i, line));
        }
      }
    }
    colorRevision = controller.colorRevision;
    builds++;
  }

  /// Layouts built in this isolate. A frame that rebuilds geometry it could
  /// have reused shows up here, which is what the reuse regression asserts.
  static int builds = 0;

  final FlarkCellController controller;
  final String source;
  final Projection? projection;
  final int cols;
  late final int headingGutter =
      theme.headingGutter && projection != null && cols >= 8 ? 3 : 0;
  final FlarkCellTheme theme;
  final CellWidthPolicy policy;
  final lines = <CellLine>[];
  final _seenItems = <int>{};
  final _itemLists = <int, int>{};
  final _markerWidths = <int, int>{};
  (int, int)? sourceWindow;
  final _rowLines = <int, List<(int, CellLine)>>{};
  final images = <CellImageSlot>[];
  late final _imageResources =
      controller.editor.document.resources
          .where((resource) => resource.isImage)
          .toList()
        ..sort((a, b) => a.contentStart.compareTo(b.contentStart));
  int _imageIndex = 0;
  late final int colorRevision;
  static const _widths = DefaultWidthResolver();

  /// Whether this geometry still describes [controller] under these settings.
  /// Laying the whole document out again per frame allocates a glyph for every
  /// grapheme in it while only the viewport is painted.
  bool describes(
    FlarkCellController other,
    int cols,
    FlarkCellTheme theme,
    CellWidthPolicy policy,
  ) =>
      identical(controller, other) &&
      this.cols == cols &&
      this.theme == theme &&
      this.policy == policy &&
      colorRevision == other.colorRevision &&
      source == other.editor.source &&
      (projection == null) == other.editor.sourceMode &&
      (sourceWindow == null ||
          (other.editor.selection.extent >= sourceWindow!.$1 &&
              other.editor.selection.extent <= sourceWindow!.$2));
  static bool _lowSurrogate(int unit) => unit >= 0xdc00 && unit <= 0xdfff;

  late final String _quoteRail =
      '${_widths.widthOfText('▎', policy) == 1 ? '▎' : '|'}${' ' * (theme.quoteIndent - 1)}';
  // Some terminals give this glyph two cells. Preserve the one-cell marker
  // plus one-cell gap contract there with an ASCII bullet.
  late final String _bullet = _widths.widthOfText('●', policy) == 1
      ? '● '
      : '* ';
  String get tableRail => _widths.widthOfText('│', policy) == 1 ? '│' : '|';
  late final bool edgeTableFrame = [
    '🭼',
    '🭽',
    '🭾',
    '🭿',
    '▏',
    '▕',
    '▔',
    '▁',
  ].every((glyph) => _widths.widthOfText(glyph, policy) == 1);
  String get tableLeftRail => edgeTableFrame ? '▏' : tableRail;
  String get tableRightRail => edgeTableFrame ? '▕' : tableRail;

  String _rawMarker(Shell shell) => shell.task
      ? (shell.checked ? '[x]' : '[ ]')
      : shell.ordered
      ? '${shell.start + shell.itemIndex}.'
      : _bullet.trimRight();

  void _measureMarkers(List<ProjectedRow> rows) {
    for (final row in rows) {
      int? list;
      for (final shell in row.shells) {
        if (shell.kind == ShellKind.list) list = shell.block;
        if (shell.kind != ShellKind.item || list == null) continue;
        _itemLists[shell.block] = list;
        _markerWidths[list] = math.max(
          _markerWidths[list] ?? theme.listIndent - 1,
          _rawMarker(shell).length,
        );
      }
    }
  }

  String _markerFor(Shell shell) {
    final marker = _rawMarker(shell);
    final width = _markerWidths[_itemLists[shell.block]] ?? marker.length;
    final before = shell.ordered
        ? width - marker.length
        : (width - marker.length) ~/ 2;
    return '${' ' * before}${marker.padRight(width - before)} ';
  }

  String _prefix(ProjectedRow row) {
    final out = StringBuffer(' ' * headingGutter);
    for (final shell in row.shells) {
      if (shell.kind == ShellKind.blockQuote) out.write(_quoteRail);
      if (shell.kind == ShellKind.item) {
        final marker = _markerFor(shell);
        out.write(_seenItems.add(shell.block) ? marker : ' ' * marker.length);
      }
    }
    if (row.kind == RowKind.codeBlock) {
      out.write(' ' * theme.codePaddingColumns);
    }

    return out.toString();
  }

  void _addText(
    ProjectedRow? row,
    String text,
    String prefix, {
    int sourceStart = 0,
    int? width,
    List<CellLine>? into,
  }) {
    final outerCols = width ?? this.cols;
    final code = row?.kind == RowKind.codeBlock;
    final heading = row?.kind == RowKind.heading
        ? theme.headingFor(row!.headingLevel)
        : null;
    final inlineLevel =
        heading != null &&
        headingGutter == 0 &&
        (heading.showLevel || theme.headingGutter) &&
        outerCols - prefix.length >= 5;
    final cols = code
        ? math.max(1, outerCols - theme.codePaddingColumns)
        : outerCols - (inlineLevel ? 3 : 0);
    final lines = into ?? this.lines;
    final first = lines.length;
    final blockLeft = code
        ? (prefix.length - theme.codePaddingColumns)
              .clamp(0, math.max(0, outerCols - 1))
              .toInt()
        : 0;
    if (code && theme.codePaddingRows > 0) {
      lines.add(
        CellLine(row, 0, _continuation(prefix, row))
          ..caretLine = false
          ..codeEdgeTop = true
          ..blockLeft = blockLeft,
      );
    }
    // Always leave a cell for a caret, even in a deeply nested narrow viewport.
    prefix = prefix.substring(
      0,
      math.min(prefix.length, math.max(cols - 2, 0)),
    );
    var line = CellLine(row, 0, prefix, sourceStart: sourceStart);
    lines.add(line);
    var offset = 0;
    var col = prefix.length;
    final styleFor = _stylesFor(row);
    var paintedEnd = text.length;
    if (row?.kind == RowKind.tableCell) {
      // Comrak's inline leaves exclude table delimiter padding. Projection
      // gaps keep its source addressable, but it must not add visible columns.
      for (final segment in row!.segments.reversed) {
        if (segment.displayEnd != paintedEnd ||
            segment.run >= 0 ||
            text
                .substring(segment.displayStart, segment.displayEnd)
                .trim()
                .isNotEmpty) {
          break;
        }
        paintedEnd = segment.displayStart;
      }
    }
    for (final grapheme in text.substring(0, paintedEnd).characters) {
      final end = offset + grapheme.length;
      if (grapheme == '\n' || grapheme == '\r\n') {
        line.end = offset;
        line = CellLine(
          row,
          end,
          _continuation(prefix, row),
          sourceStart: sourceStart,
        );
        lines.add(line);
        offset = end;
        col = prefix.length;
        continue;
      }
      var safe = grapheme == '\t' ? ' ' : sanitizeForDisplay(grapheme);
      var width = grapheme == '\t'
          ? 4 - (col - prefix.length) % 4
          : _widths.widthOfText(safe, policy);
      if (width == 0) {
        safe = '◌$safe';
        width = 1;
      }
      final capacity = math.max(cols - prefix.length - 1, 1);
      if (width > capacity) {
        safe = '�';
        width = 1;
      }
      if (col + width > cols - 1 && line.glyphs.isNotEmpty) {
        line.end = offset;
        line = CellLine(
          row,
          offset,
          _continuation(prefix, row),
          sourceStart: sourceStart,
        );
        lines.add(line);
        col = prefix.length;
      }
      line.glyphs.add(
        CellGlyph(safe, col, width, offset, end, styleFor(offset)),
      );
      col += width;
      offset = end;
      line.end = end;
    }
    line.end = text.length;
    if (heading != null) {
      final textLines = lines.skip(first).toList();
      for (final line in textLines) {
        line.blockLeft = line.prefix.length;
      }
      if (headingGutter > 0) {
        textLines.first.headingLabelColumn = 0;
      } else if (inlineLevel) {
        textLines.last.headingLabelColumn = textLines.last.endColumn + 1;
      }
      if (heading.divider) {
        lines.add(
          CellLine(row, text.length, _continuation(prefix, row))
            ..headingRule = true
            ..caretLine = false
            ..blockLeft = prefix.length,
        );
      }
    }
    if (code) {
      for (final line in lines.skip(first)) {
        line.blockLeft = blockLeft;
      }
      if (theme.codePaddingRows > 0) {
        lines.add(
          CellLine(row, text.length, _continuation(prefix, row))
            ..caretLine = false
            ..codeEdgeTop = false
            ..blockLeft = blockLeft,
        );
      }
    }
  }

  void _shift(CellLine line, int shift) {
    line.left += shift;
    for (var g = 0; g < line.glyphs.length; g++) {
      final old = line.glyphs[g];
      line.glyphs[g] = CellGlyph(
        old.text,
        old.col + shift,
        old.width,
        old.start,
        old.end,
        old.style,
      );
    }
  }

  /// A wrapped line keeps the rails that continue — a quote bar, a code
  /// block's indent — and blanks the ones that do not. Repeating a bullet or a
  /// task box would claim the wrap is a second item, and the host's own task
  /// hit test reads this prefix, so the checkbox would be clickable there too.
  String _continuation(String prefix, ProjectedRow? row) {
    if (row == null) return ' ' * prefix.length;
    final out = StringBuffer(' ' * headingGutter);
    for (final shell in row.shells) {
      if (shell.kind == ShellKind.blockQuote) {
        out.write(_quoteRail);
      } else if (shell.kind == ShellKind.item) {
        out.write(' ' * _markerFor(shell).length);
      }
    }
    if (row.kind == RowKind.codeBlock) {
      out.write(' ' * theme.codePaddingColumns);
    }
    final continued = out.toString();
    return continued.length == prefix.length ? continued : ' ' * prefix.length;
  }

  CellStyle Function(int) _stylesFor(ProjectedRow? row) {
    var base = theme.body;
    if (row == null) return (_) => base;
    if (row.header) base = base.merge(theme.tableHeader);
    if (row.kind == RowKind.heading) {
      base = theme
          .headingSurface(row.headingLevel)
          .merge(theme.heading)
          .merge(theme.headingFor(row.headingLevel).style);
    }
    if (row.shells.any((s) => s.kind == ShellKind.blockQuote)) {
      base = base.merge(theme.quote);
    }
    if (row.kind == RowKind.codeBlock) base = base.merge(theme.code);
    final colors = row.kind == RowKind.codeBlock
        ? controller.colorsFor(row)
        : null;
    var segmentIndex = 0, colorIndex = 0;
    // Glyphs, projection segments and code spans advance together; a long
    // highlighted row must not rescan every span for every character.
    return (offset) {
      var style = base;
      if (colors != null) {
        while (colorIndex < colors.spans.length &&
            colors.spans[colorIndex].end <= offset) {
          colorIndex++;
        }
        if (colorIndex < colors.spans.length) {
          final span = colors.spans[colorIndex];
          if (offset >= span.start) {
            final role = codeSyntaxRole(
              span.scopes.lastOrNull?.split('.').first,
            );
            style = style.merge(theme.syntax[role] ?? CellStyle.none);
          }
        }
      }
      while (segmentIndex < row.segments.length &&
          row.segments[segmentIndex].displayEnd <= offset) {
        segmentIndex++;
      }
      if (segmentIndex < row.segments.length &&
          row.segments[segmentIndex].displayStart <= offset) {
        final bits = row.segments[segmentIndex].styles;
        if (bits & Style.strong != 0) {
          style = style.merge(const CellStyle(bold: true));
        }
        if (bits & Style.emphasis != 0) {
          style = style.merge(const CellStyle(italic: true));
        }
        if (bits & Style.strikethrough != 0) {
          style = style.merge(const CellStyle(strikethrough: true));
        }
        if (bits & Style.code != 0) style = style.merge(theme.code);
        if (bits & (Style.link | Style.image) != 0) {
          style = style.merge(theme.link);
        }
      }
      return style;
    };
  }

  /// Pick an editable fragment, skipping table rules and image preview rows.
  CellLine lineAt(int index, int col, {int direction = 1}) {
    index = index.clamp(0, lines.length - 1);
    var next = index;
    while (!lines[next].cellAt(col).editable) {
      next += direction;
      if (next < 0 || next >= lines.length) {
        next = index;
        direction = -direction;
      }
    }
    return lines[next].cellAt(col);
  }

  CellOffset get caretPosition => positionFor(
    controller.editor.selection.extent,
    tableCell: controller.editor.selection.tableCell,
  );

  CellOffset positionFor(int source, {int? tableCell}) {
    final position = projection?.displayForSource(source, tableCell: tableCell);
    final candidates = position == null
        ? [for (var i = 0; i < lines.length; i++) (i, lines[i])]
        : _rowLines[position.row] ?? const <(int, CellLine)>[];
    var result = const CellOffset(0, 0);
    for (final (y, line) in candidates) {
      final offset = position?.offset ?? source - line.sourceStart;
      if (offset < line.start) break;
      result = CellOffset(line.columnAt(offset), y);
      if (offset < line.end) break;
    }
    return result;
  }

  void _addImages(
    ProjectedRow row,
    List<CellLine> target,
    int left,
    int width, {
    String prefix = '',
  }) {
    if (theme.imagePreviewRows == 0) return;
    while (_imageIndex < _imageResources.length &&
        _imageResources[_imageIndex].contentStart <= row.sourceEnd) {
      final resource = _imageResources[_imageIndex++];
      if (resource.contentStart < row.sourceStart) continue;
      final top = target.length;
      for (var y = 0; y < theme.imagePreviewRows; y++) {
        target.add(
          CellLine(row, row.text.length, prefix)
            ..left = left
            ..right = left + width
            ..image = resource,
        );
      }
      if (identical(target, lines)) {
        images.add(
          CellImageSlot(resource, left, top, width, theme.imagePreviewRows),
        );
      }
    }
  }

  void _addTable(List<ProjectedRow> rows) {
    final columns = rows.map((r) => r.column).reduce(math.max) + 1;
    var prefix = _prefix(rows.first);
    final available = cols - prefix.length;
    // A very narrow host stacks cells in source order instead of clipping away
    // later columns. Numbered labels remain presentation, never source text.
    if (available < columns * 4 + 1) {
      for (final row in rows) {
        _addText(row, row.text, '$prefix${row.column + 1} $tableRail ');
        _addImages(
          row,
          lines,
          math.min(prefix.length, cols - 1),
          math.max(1, available),
        );
      }
      return;
    }
    final widths = List.generate(
      columns,
      (col) =>
          (available - columns - 1) ~/ columns +
          (col < (available - columns - 1) % columns ? 1 : 0),
    );
    String rule(String start, String join, String end) {
      final ascii = tableRail == '|';
      return prefix +
          (ascii ? '+' : start) +
          widths.map((w) => (ascii ? '-' : '─') * w).join(ascii ? '+' : join) +
          (ascii ? '+' : end);
    }

    void addRule(String text) => lines.add(CellLine(null, 0, '')..rule = text);
    String frameRule(bool top) =>
        prefix +
        (top ? '🭽' : '🭼') +
        (top ? '▔' : '▁') * math.max(0, available - 2) +
        (top ? '🭾' : '🭿');
    addRule(edgeTableFrame ? frameRule(true) : rule('┌', '┬', '┐'));
    prefix = _continuation(prefix, rows.first);
    for (var start = 0; start < rows.length;) {
      var end = start + 1;
      while (end < rows.length &&
          rows[end].tableRowBlock == rows[start].tableRowBlock) {
        end++;
      }
      final parts = <List<CellLine>>[];
      var left = prefix.length + 1;
      for (final row in rows.sublist(start, end)) {
        final width = widths[row.column];
        final wrapped = <CellLine>[];
        // One cell of padding and one reserved caret cell at the right edge.
        _addText(row, row.text, ' ', width: width, into: wrapped);
        for (final line in wrapped) {
          final used = line.endColumn - 1;
          final spare = math.max(0, width - 2 - used);
          final align = row.alignment == 3
              ? spare
              : row.alignment == 2
              ? spare ~/ 2
              : 0;
          line.left = left + align;
          line.right = left + width;
          for (var g = 0; g < line.glyphs.length; g++) {
            final old = line.glyphs[g];
            line.glyphs[g] = CellGlyph(
              old.text,
              old.col + left + align,
              old.width,
              old.start,
              old.end,
              old.style,
            );
          }
        }
        _addImages(row, wrapped, left, width);
        parts.add(wrapped);
        left += width + 1;
      }
      final height = parts.map((p) => p.length).reduce(math.max);
      for (var y = 0; y < height; y++) {
        final fragments = <CellLine>[];
        for (final part in parts) {
          if (y < part.length) {
            fragments.add(part[y]);
            if (part[y].image != null &&
                (y == 0 || part[y - 1].image != part[y].image)) {
              images.add(
                CellImageSlot(
                  part[y].image!,
                  part[y].left,
                  lines.length,
                  part[y].right - part[y].left,
                  theme.imagePreviewRows,
                ),
              );
            }
          } else {
            final last = part.last;
            fragments.add(
              CellLine(last.row, last.end, '')
                ..caretLine = false
                ..end = last.end
                ..left = last.left
                ..right = last.right,
            );
          }
        }
        lines.add(CellLine(rows[start], 0, prefix)..cells = fragments);
      }
      addRule(
        end == rows.length
            ? (edgeTableFrame ? frameRule(false) : rule('└', '┴', '┘'))
            : (edgeTableFrame ? rule('▏', '┼', '▕') : rule('├', '┼', '┤')),
      );
      start = end;
    }
  }
}

/// A fixed cell rectangle, independent of asynchronous image loading.
final class CellImageSlot {
  const CellImageSlot(
    this.resource,
    this.left,
    this.top,
    this.width,
    this.height,
  );
  final InlineResource resource;
  final int left, top, width, height;
}
