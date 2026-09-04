/// The closed set of logical editing actions. Every platform route reduces
/// to one of these before the kernel sees it.
library;

sealed class FlarkCommand { const FlarkCommand(); }

enum MoveUnit { grapheme, word, line, row }
enum MoveDirection { backward, forward }

/// Insert text at the caret, replacing the selection.
final class InsertText extends FlarkCommand { const InsertText(this.text); final String text; }
/// Delete the previous rendered grapheme, or the selection.
final class DeleteBackward extends FlarkCommand { const DeleteBackward(); }
/// Delete the next rendered grapheme, or the selection.
final class DeleteForward extends FlarkCommand { const DeleteForward(); }
/// Return: a line break, continuing or exiting a list or quote; a paragraph
/// break when [paragraph] is set.
final class Newline extends FlarkCommand { const Newline({this.paragraph = false}); final bool paragraph; }
/// Replace an explicit source range.
final class ReplaceRange extends FlarkCommand { const ReplaceRange(this.start, this.end, this.text); final int start, end; final String text; }
/// Set the selection to source offsets (made legal).
final class SetSelection extends FlarkCommand { const SetSelection(this.base, this.extent); const SetSelection.caret(int offset) : base = offset, extent = offset; final int base, extent; }
/// Place the caret from a display position, taking the anchor from the
/// glyph half the host reports ([leadingHalf]).
final class PlaceCaret extends FlarkCommand { const PlaceCaret(this.row, this.offset, {this.leadingHalf = true, this.extend = false}); final int row, offset; final bool leadingHalf, extend; }
/// Move the caret by a unit, optionally extending the selection.
final class MoveCaret extends FlarkCommand { const MoveCaret(this.direction, {this.unit = MoveUnit.grapheme, this.extend = false}); final MoveDirection direction; final MoveUnit unit; final bool extend; }
final class Undo extends FlarkCommand { const Undo(); }
final class Redo extends FlarkCommand { const Redo(); }
/// Toggle a task checkbox on the caret's item.
final class ToggleTask extends FlarkCommand { const ToggleTask(); }
final class Indent extends FlarkCommand { const Indent(); }
final class Outdent extends FlarkCommand { const Outdent(); }
/// Toggle an inline style over the selection or the word at the caret.
final class ToggleStyle extends FlarkCommand { const ToggleStyle(this.style); final int style; }
/// Set the heading level of the caret's row (0 = paragraph).
final class SetHeadingLevel extends FlarkCommand { const SetHeadingLevel(this.level); final int level; }
/// Paste text as-is at the caret.
final class Paste extends FlarkCommand { const Paste(this.text); final String text; }
