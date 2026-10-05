/// Questions about projected rows that several kernel paths ask: which of
/// the projection's special presentations a row is. The projection makes
/// those choices from the render model; this library names them once, so
/// callers do not decode a row's fields to recognize them. It is
/// kernel-internal and not exported: hosts read [ProjectedRow] itself.
library;

import '../parse/schema.g.dart';
import 'projection.dart';

extension RowQueries on ProjectedRow {
  /// A fenced code block with no body line. Its lines hold no caret; its one
  /// anchor is its source end ([Projection.lineSpans]).
  bool get bodyless => fenced && contentStarts.every((start) => start < 0);

  /// A table's delimiter line shown as its source, in a table without body
  /// rows (the editing view): a cell of no table row.
  bool get delimiterSource => kind == RowKind.tableCell && tableRowBlock < 0;
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
