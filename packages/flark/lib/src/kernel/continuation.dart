/// The container prefix a line takes to continue the containers of the line
/// above, shared by the editor, which writes it on new lines, and the
/// projection, which shows a table's delimiter line only under it. Every
/// range comes from the parser's blocks.
library;

import '../parse/render_model.dart';
import '../parse/schema.g.dart';

/// Where [line] of [text] starts, past the byte order mark that may lead the
/// first line. comrak skips the mark, so it belongs to the document rather
/// than to that line: removing the line must keep it, and a prefix copied
/// from the line must not carry it into the middle of the document, where it
/// is text that would stop the copied markers from being read.
int lineStartPastMark(String text, RenderModel model, int line) {
  final start = model.lineStartUtf16(line);
  return start == 0 && text.isNotEmpty && text.codeUnitAt(0) == 0xFEFF
      ? 1
      : start;
}

/// comrak's footnote continuation indent: a line continues a footnote
/// definition when it is indented at least four columns past the containers
/// around the definition (`parse_footnote_definition_block_prefix`). The
/// render model gives continuation lines this indent as their prefix range,
/// but a label's line has no such range to copy.
const footnoteIndent = '    ';

/// Any character of an item marker but a tab, which keeps its own width.
final notTab = RegExp(r'[^\t]');

/// The container prefix that continues [line] of [text] up to [end], for a
/// line inside [block] (whose own markers stay out of it): the line's prefix
/// with the markers of the items and footnote definitions that open on [line]
/// turned into the indentation that continues them. A copied marker would
/// open another item or definition, where a continuation line belongs to the
/// open ones. An item continues at its marker's width, tabs kept; a footnote
/// definition at [footnoteIndent].
String continuationPrefix(
  String text,
  RenderModel model,
  int line,
  int end,
  int block,
) {
  final lineStart = lineStartPastMark(text, model, line);
  var prefix = text.substring(lineStart, end);
  var child = block;
  bool blank(int i) =>
      prefix.codeUnitAt(i) == 0x20 || prefix.codeUnitAt(i) == 0x09;
  for (var parent = model.blockParent(block); parent != noParent;) {
    final kind = model.blockKind(parent);
    if ((kind == BlockKind.item || kind == BlockKind.footnoteDefinition) &&
        model.blockFirstLine(parent) == line) {
      var from = model.blockStart(parent) - lineStart;
      final to = model.blockStart(child) - lineStart;
      if (from >= 0 && from < to && to <= prefix.length) {
        final marker = prefix.substring(from, to);
        // The indent counts from the end of the containers' prefix, not from
        // the label, so the label's own indentation goes with it; kept, it
        // would indent what follows past the definition's content, turning
        // an item's next marker into an underline or adding spaces to code.
        // That end is known for a definition at the document level, where it
        // is the line's start. An item opening on the line pads up to the
        // label; the model records no end for a quote's prefix or an earlier
        // item's indentation, so a label indented inside those keeps it.
        if (kind == BlockKind.footnoteDefinition &&
            model.blockKind(model.blockParent(parent)) == BlockKind.document) {
          while (from > 0 && blank(from - 1)) {
            from--;
          }
        }
        // A marker right after a quote's `>` left it no optional space, and
        // the first of the spaces put in its place would be read as one.
        final pad = from > 0 && !' \t\r\n'.contains(prefix[from - 1]);
        prefix = prefix.replaceRange(
          from,
          to,
          (pad ? ' ' : '') +
              (kind == BlockKind.item
                  ? marker.replaceAll(notTab, ' ')
                  : footnoteIndent),
        );
      }
    }
    child = parent;
    parent = model.blockParent(parent);
  }
  return prefix;
}
