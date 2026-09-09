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
  final glyphs = <CellGlyph>[];
  late int end = start;
  int get endColumn =>
      glyphs.isEmpty ? prefix.length : glyphs.last.col + glyphs.last.width;

  int sourceAt(int offset, {Anchor anchor = Anchor.after}) =>
      row?.sourceForDisplay(offset, anchor: anchor) ?? sourceStart + offset;

  (int, bool) hit(int col) {
    for (final glyph in glyphs) {
      if (col < glyph.col) return (glyph.start, true);
      if (col < glyph.col + glyph.width) {
        final leading = (col - glyph.col) * 2 < glyph.width;
        return (leading ? glyph.start : glyph.end, leading);
      }
    }
    return (end, false);
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
      for (final row in editor.projection.rows) {
        _addText(row, row.text, _prefix(row));
      }
    }
    if (lines.isEmpty) lines.add(CellLine(null, 0, ''));
    for (var i = 0; i < lines.length; i++) {
      final row = lines[i].row;
      if (row != null) _rowLine.putIfAbsent(row.index, () => i);
    }
    colorRevision = controller.colorRevision;
    builds++;
  }

  /// Layouts built in this isolate. A frame that rebuilds geometry it could
  /// have reused shows up here, which is what the reuse regression asserts.
  static int builds = 0;

  final FlarkFleuryController controller;
  final String source;
  final Projection? projection;
  final int cols;
  final FlarkCellTheme theme;
  final CellWidthPolicy policy;
  final lines = <CellLine>[];
  final _seenItems = <int>{};
  (int, int)? sourceWindow;
  final _rowLine = <int, int>{};
  late final int colorRevision;
  static const _widths = DefaultWidthResolver();

  /// Whether this geometry still describes [controller] under these settings.
  /// Laying the whole document out again per frame allocates a glyph for every
  /// grapheme in it while only the viewport is painted.
  bool describes(
    FlarkFleuryController other,
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

  late final String _quoteRail = _widths.widthOfText('▎', policy) == 1
      ? '▎ '
      : '| ';
  // Some terminals give this glyph two cells. Preserve the one-cell marker
  // plus one-cell gap contract there with an ASCII bullet.
  late final String _bullet = _widths.widthOfText('●', policy) == 1
      ? '● '
      : '* ';

  String _markerFor(Shell shell) => shell.task
      ? (shell.checked ? '[x] ' : '[ ] ')
      : shell.ordered
      ? '${shell.start + shell.itemIndex}. '
      : _bullet;

  String _prefix(ProjectedRow row) {
    final out = StringBuffer();
    for (final shell in row.shells) {
      if (shell.kind == ShellKind.blockQuote) out.write(_quoteRail);
      if (shell.kind == ShellKind.item) {
        final marker = _markerFor(shell);
        out.write(_seenItems.add(shell.block) ? marker : ' ' * marker.length);
      }
    }
    if (row.kind == RowKind.codeBlock) out.write('  ');
    if (row.kind == RowKind.tableCell) out.write('| ');
    return out.toString();
  }

  void _addText(
    ProjectedRow? row,
    String text,
    String prefix, {
    int sourceStart = 0,
  }) {
    // Always leave a cell for a caret, even in a deeply nested narrow viewport.
    prefix = prefix.substring(0, math.min(prefix.length, math.max(cols - 2, 0)));
    var line = CellLine(row, 0, prefix, sourceStart: sourceStart);
    lines.add(line);
    var offset = 0;
    var col = prefix.length;
    final styleFor = _stylesFor(row);
    for (final grapheme in text.characters) {
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
  }

  /// A wrapped line keeps the rails that continue — a quote bar, a code
  /// block's indent — and blanks the ones that do not. Repeating a bullet or a
  /// task box would claim the wrap is a second item, and the host's own task
  /// hit test reads this prefix, so the checkbox would be clickable there too.
  String _continuation(String prefix, ProjectedRow? row) {
    if (row == null) return ' ' * prefix.length;
    final out = StringBuffer();
    for (final shell in row.shells) {
      if (shell.kind == ShellKind.blockQuote) {
        out.write(_quoteRail);
      } else if (shell.kind == ShellKind.item) {
        out.write(' ' * _markerFor(shell).length);
      }
    }
    if (row.kind == RowKind.codeBlock) out.write('  ');
    if (row.kind == RowKind.tableCell) out.write('  ');
    final continued = out.toString();
    return continued.length == prefix.length
        ? continued
        : ' ' * prefix.length;
  }

  CellStyle Function(int) _stylesFor(ProjectedRow? row) {
    var base = theme.body;
    if (row == null) return (_) => base;
    if (row.kind == RowKind.heading || row.header) {
      base = base.merge(theme.heading);
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

  CellOffset positionFor(int source) {
    final position = projection?.displayForSource(source);
    // A row's lines are contiguous, so start at its first one rather than
    // scanning the document for it.
    var i = 0;
    if (position != null) {
      final start = _rowLine[position.row];
      if (start == null) return const CellOffset(0, 0);
      i = start;
    }
    var result = const CellOffset(0, 0);
    for (; i < lines.length; i++) {
      final line = lines[i];
      if (position != null && line.row?.index != position.row) break;
      final offset = position?.offset ?? source - line.sourceStart;
      if (offset < line.start) break;
      result = CellOffset(line.columnAt(offset), i);
      if (offset < line.end) break;
    }
    return result;
  }
}
