/// Questions about projected rows that several kernel paths ask: which of
/// the projection's special presentations a row is, and whether two rows sit
/// in the same containers. The projection makes those choices from the
/// render model; this library names them once, so callers do not decode a
/// row's fields to recognize them. It is kernel-internal and not exported:
/// hosts read [ProjectedRow] itself.
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
