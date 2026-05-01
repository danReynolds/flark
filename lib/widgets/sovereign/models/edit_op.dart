import 'package:flutter/services.dart';

/// Kind of editor operation represented by an [EditOp].
enum EditOpKind {
  /// A text mutation (insert, delete, replace).
  text,

  /// A selection or composing range change only.
  selection,
}

/// A strictly typed operation on the [SovereignState].
///
/// Guaranteed to be valid by the [SovereignController] before emission.
class EditOp {
  /// Monotonic ID for this operation.
  final int id;

  /// Whether the operation mutated text or selection/composing state only.
  final EditOpKind kind;

  /// State before this op applied.
  final TextEditingValue before;

  /// State after this op applied.
  final TextEditingValue after;

  /// The range in `before.text` that was replaced.
  /// Empty if [kind] is [EditOpKind.selection].
  final TextRange replacedRange;

  /// The text inserted into `after.text`.
  /// Empty if [kind] is [EditOpKind.selection].
  final String insertedText;

  /// The text that was replaced (for Undo).
  /// Empty if [kind] is [EditOpKind.selection].
  final String replacedText;

  /// Whether this op was generated programmatically (e.g. Smart Enter).
  final bool isSmartTransform;

  /// Group ID for Atomic Undo actions.
  final int undoGroupId;

  /// Creates a typed editor operation.
  const EditOp({
    required this.id,
    required this.kind,
    required this.before,
    required this.after,
    this.replacedRange = TextRange.empty,
    this.insertedText = '',
    this.replacedText = '',
    this.isSmartTransform = false,
    required this.undoGroupId,
  });

  /// Computed range in `after.text` affected by this op.
  ///
  /// Used for Undo Merging logic (detecting if subsequent op is "nearby").
  TextRange get affectedRange {
    if (kind == EditOpKind.selection) {
      return TextRange.empty;
    }
    return TextRange(
      start: replacedRange.start,
      end: replacedRange.start + insertedText.length,
    );
  }

  @override
  String toString() {
    return 'EditOp#$id($kind, group: $undoGroupId, range: $replacedRange, insert: "$insertedText")';
  }
}
