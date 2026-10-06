/// Questions about projected rows that several kernel paths ask: which of
/// the projection's special presentations a row is, whether two rows sit in
/// the same containers, and which of a row's lines, source or displayed,
/// holds an offset. The projection makes those choices from the render
/// model; this library answers them once, so callers do not decode a row's
/// fields to recognize them. It is kernel-internal and not exported: hosts
/// read [ProjectedRow] itself.
library;

import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'projection.dart';

extension RowQueries on ProjectedRow {
  /// A fenced code block with no body line. Its lines hold no caret; its one
  /// anchor is its source end ([Projection.lineSpans]).
  bool get bodyless => fenced && contentStarts.every((start) => start < 0);

  /// A table's delimiter line shown as its source, in a table without body
  /// rows (the editing view): a cell of no table row.
  bool get delimiterSource => kind == RowKind.tableCell && tableRowBlock < 0;

  /// The kinds of the containers this row sits in, outermost first, as a new
  /// list the caller may change.
  List<ShellKind> get containerKinds => [
    for (final shell in shells) shell.kind,
  ];

  /// Whether this row's containers are of [kinds], outermost first.
  bool hasContainerKinds(List<ShellKind> kinds) {
    if (shells.length != kinds.length) return false;
    for (var i = 0; i < kinds.length; i++) {
      if (shells[i].kind != kinds[i]) return false;
    }
    return true;
  }

  /// Whether this row and [other] sit in containers of the same kinds.
  bool sameContainerKinds(ProjectedRow other) =>
      shells.length == other.shells.length && _kindsLead(other);

  /// Whether this row sits in containers of the kinds of [outer]'s, or of
  /// their outer part only: a line that left inner containers and entered
  /// none.
  bool withinContainerKindsOf(ProjectedRow outer) =>
      shells.length <= outer.shells.length && _kindsLead(outer);

  /// Whether this row of [model] and [other] of [otherModel] sit in the same
  /// containers: of the same kinds, each starting where the other's does.
  bool sameContainersAs(
    ProjectedRow other,
    RenderModel model,
    RenderModel otherModel,
  ) {
    if (shells.length != other.shells.length) return false;
    for (var i = 0; i < shells.length; i++) {
      final a = shells[i], b = other.shells[i];
      if (a.kind != b.kind ||
          model.blockStart(a.block) != otherModel.blockStart(b.block)) {
        return false;
      }
    }
    return true;
  }

  /// The display range of the line of this row's text that holds display
  /// offset [d]. A line feed the row displays ends a line, except one a
  /// replacement displays, such as `&#10;`: that is a character of its line.
  (int, int) displayLineAt(int d) {
    var start = d;
    while (start > 0) {
      final k = text.lastIndexOf('\n', start - 1);
      if (k < 0 || _endsLine(k)) {
        start = k + 1;
        break;
      }
      start = k;
    }
    for (var end = d; ;) {
      final k = text.indexOf('\n', end);
      if (k < 0) return (start, text.length);
      if (_endsLine(k)) return (start, k);
      end = k + 1;
    }
  }

  /// Whether the line feed at display offset [k] ends a line: it is not one
  /// a replacement displays.
  bool _endsLine(int k) {
    var lo = 0, hi = segments.length;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (segments[mid].displayEnd <= k) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo == segments.length ||
        segments[lo].exact ||
        segments[lo].lineBreak;
  }

  /// The index into this row's per-line lists ([ProjectedRow.contentStarts]
  /// and the others) of the line of [model] that holds source [offset]:
  /// negative before the row's first line, [ProjectedRow.lineCount] or more
  /// after its last.
  int lineIndexOf(RenderModel model, int offset) =>
      model.lineOfUtf16(offset) - firstLine;

  /// [lineIndexOf], held to the row's own lines.
  int nearestLineIndexOf(RenderModel model, int offset) =>
      lineIndexOf(model, offset).clamp(0, lineCount - 1);

  /// Whether this row's containers, outermost first, are of the kinds that
  /// [other]'s start with. [other] has at least as many.
  bool _kindsLead(ProjectedRow other) {
    for (var i = 0; i < shells.length; i++) {
      if (shells[i].kind != other.shells[i].kind) return false;
    }
    return true;
  }
}

extension ProjectionQueries on Projection {
  /// Whether [row] shows a bare empty heading or item marker as paragraph
  /// text, the presentation the edit profile gives a bare prefix until a
  /// space commits its block. The row's block is that heading or item.
  bool isBarePrefix(ProjectedRow row) =>
      row.kind == RowKind.paragraph &&
      row.block >= 0 &&
      (model.blockKind(row.block) == BlockKind.heading ||
          model.blockKind(row.block) == BlockKind.item);
}
