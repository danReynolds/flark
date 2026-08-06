// RFC 024 Gate G4 — shared document + selection model.
//
// Deliberately NOT the flark engine. Plain `String` blocks only, so the
// bake-off measures the input surface and nothing else (RFC 024 §6.1).
//
// Every coordinate in this file is a *model* coordinate: (block index, UTF-16
// offset within that block). Nothing here knows about widgets, render objects,
// scroll offsets or whether a block was ever built. That is the whole point:
// selection must survive its anchor being disposed.

import 'package:flutter/foundation.dart';

/// Separator used when blocks are flattened back into one source string.
const String kG4BlockSeparator = '\n\n';

/// A position in the document: which block, and a UTF-16 offset inside it.
@immutable
class G4Position implements Comparable<G4Position> {
  const G4Position(this.block, this.offsetUtf16);

  final int block;
  final int offsetUtf16;

  @override
  int compareTo(G4Position other) {
    if (block != other.block) {
      return block.compareTo(other.block);
    }
    return offsetUtf16.compareTo(other.offsetUtf16);
  }

  bool operator <(G4Position other) => compareTo(other) < 0;
  bool operator <=(G4Position other) => compareTo(other) <= 0;
  bool operator >(G4Position other) => compareTo(other) > 0;
  bool operator >=(G4Position other) => compareTo(other) >= 0;

  G4Position copyWith({int? block, int? offsetUtf16}) =>
      G4Position(block ?? this.block, offsetUtf16 ?? this.offsetUtf16);

  @override
  bool operator ==(Object other) =>
      other is G4Position && other.block == block && other.offsetUtf16 == offsetUtf16;

  @override
  int get hashCode => Object.hash(block, offsetUtf16);

  @override
  String toString() => 'G4Position($block:$offsetUtf16)';
}

/// A directional selection. [base] is the anchor (where the gesture started),
/// [extent] is the moving end (where the caret is).
@immutable
class G4Selection {
  const G4Selection({required this.base, required this.extent});

  const G4Selection.collapsed(G4Position at) : base = at, extent = at;

  final G4Position base;
  final G4Position extent;

  bool get isCollapsed => base == extent;

  G4Position get start => base <= extent ? base : extent;
  G4Position get end => base <= extent ? extent : base;

  /// Same range, always base <= extent.
  G4Selection get normalized => G4Selection(base: start, extent: end);

  bool get isMultiBlock => start.block != end.block;

  /// True when [block] is fully or partially covered by this selection.
  bool touchesBlock(int block) => block >= start.block && block <= end.block;

  /// The portion of this selection that falls inside [block], expressed as
  /// UTF-16 offsets inside that block, or null if the block is untouched.
  /// [blockLength] is required to clamp the trailing end.
  ({int start, int end})? clipToBlock(int block, int blockLength) {
    if (!touchesBlock(block)) {
      return null;
    }
    final int s = block == start.block ? start.offsetUtf16 : 0;
    final int e = block == end.block ? end.offsetUtf16 : blockLength;
    return (start: s.clamp(0, blockLength), end: e.clamp(0, blockLength));
  }

  G4Selection copyWith({G4Position? base, G4Position? extent}) =>
      G4Selection(base: base ?? this.base, extent: extent ?? this.extent);

  @override
  bool operator ==(Object other) =>
      other is G4Selection && other.base == base && other.extent == extent;

  @override
  int get hashCode => Object.hash(base, extent);

  @override
  String toString() => 'G4Selection($base -> $extent)';
}

/// One undoable step.
@immutable
class _G4UndoEntry {
  const _G4UndoEntry(this.blocks, this.selection);
  final List<String> blocks;
  final G4Selection? selection;
}

/// The document. Source of truth. Edits are expressed only in model
/// coordinates; there is no rich-text model and no widget state anywhere.
class G4Document extends ChangeNotifier {
  G4Document(List<String> blocks) : _blocks = blocks.isEmpty ? <String>[''] : List<String>.of(blocks);

  List<String> _blocks;
  int _version = 0;

  final List<_G4UndoEntry> _undo = <_G4UndoEntry>[];
  final List<_G4UndoEntry> _redo = <_G4UndoEntry>[];

  /// Bumped on every mutation. Cheap change detection for views.
  int get version => _version;

  int get blockCount => _blocks.length;

  String blockAt(int index) => _blocks[index];

  int blockLength(int index) => _blocks[index].length;

  List<String> get blocks => List<String>.unmodifiable(_blocks);

  /// The whole document as one source string.
  String get text => _blocks.join(kG4BlockSeparator);

  G4Position get documentStart => const G4Position(0, 0);

  G4Position get documentEnd => G4Position(_blocks.length - 1, _blocks.last.length);

  G4Selection get selectAll => G4Selection(base: documentStart, extent: documentEnd);

  G4Position clampPosition(G4Position p) {
    final int block = p.block.clamp(0, _blocks.length - 1);
    final int offset = p.offsetUtf16.clamp(0, _blocks[block].length);
    return G4Position(block, offset);
  }

  G4Selection clampSelection(G4Selection s) =>
      G4Selection(base: clampPosition(s.base), extent: clampPosition(s.extent));

  /// Exact text for [selection], across block boundaries, joined with
  /// [kG4BlockSeparator].
  ///
  /// Reads from [_blocks] only — a block that was never built by any widget is
  /// indistinguishable from one that was. That is the property Gate G4 case 3
  /// and case 4 exist to prove.
  String extractRange(G4Selection selection) {
    final G4Selection s = clampSelection(selection).normalized;
    if (s.isCollapsed) {
      return '';
    }
    if (s.base.block == s.extent.block) {
      return _blocks[s.base.block].substring(s.base.offsetUtf16, s.extent.offsetUtf16);
    }
    final StringBuffer out = StringBuffer();
    out.write(_blocks[s.base.block].substring(s.base.offsetUtf16));
    for (int i = s.base.block + 1; i < s.extent.block; i++) {
      out.write(kG4BlockSeparator);
      out.write(_blocks[i]);
    }
    out.write(kG4BlockSeparator);
    out.write(_blocks[s.extent.block].substring(0, s.extent.offsetUtf16));
    return out.toString();
  }

  /// Replace [selection] with [replacement] and return the resulting collapsed
  /// caret position.
  ///
  /// [replacement] may contain [kG4BlockSeparator], in which case the affected
  /// blocks split. Blocks merge when a selection spans a boundary.
  G4Position replaceRange(G4Selection selection, String replacement, {bool recordUndo = true}) {
    final G4Selection s = clampSelection(selection).normalized;
    if (recordUndo) {
      _pushUndo(selection);
    }

    final String prefix = _blocks[s.base.block].substring(0, s.base.offsetUtf16);
    final String suffix = _blocks[s.extent.block].substring(s.extent.offsetUtf16);

    final String head = prefix + replacement;
    final List<String> headParts = head.split(kG4BlockSeparator);
    final List<String> allParts = (head + suffix).split(kG4BlockSeparator);

    _blocks.replaceRange(s.base.block, s.extent.block + 1, allParts);
    if (_blocks.isEmpty) {
      _blocks = <String>[''];
    }

    _version++;
    notifyListeners();

    return clampPosition(
      G4Position(s.base.block + headParts.length - 1, headParts.last.length),
    );
  }

  /// Insert [text] at [at]. Returns the caret position after the insert.
  G4Position insert(G4Position at, String text) =>
      replaceRange(G4Selection.collapsed(at), text);

  /// Delete [selection]. Returns the caret position after the delete.
  G4Position delete(G4Selection selection) => replaceRange(selection, '');

  // ---------------------------------------------------------------------
  // Undo / redo. Owned here, NOT by any editable widget.
  // ---------------------------------------------------------------------

  bool get canUndo => _undo.isNotEmpty;
  bool get canRedo => _redo.isNotEmpty;

  void _pushUndo(G4Selection? selectionBefore) {
    _undo.add(_G4UndoEntry(List<String>.of(_blocks), selectionBefore));
    _redo.clear();
  }

  /// Returns the selection that was active before the undone edit, if any.
  G4Selection? undo() {
    if (_undo.isEmpty) {
      return null;
    }
    final _G4UndoEntry entry = _undo.removeLast();
    _redo.add(_G4UndoEntry(List<String>.of(_blocks), entry.selection));
    _blocks = List<String>.of(entry.blocks);
    _version++;
    notifyListeners();
    return entry.selection;
  }

  G4Selection? redo() {
    if (_redo.isEmpty) {
      return null;
    }
    final _G4UndoEntry entry = _redo.removeLast();
    _undo.add(_G4UndoEntry(List<String>.of(_blocks), entry.selection));
    _blocks = List<String>.of(entry.blocks);
    _version++;
    notifyListeners();
    return entry.selection;
  }

  // ---------------------------------------------------------------------
  // Text boundaries. Used by word/paragraph selection and by the caret-motion
  // intents both variants intercept.
  // ---------------------------------------------------------------------

  static bool _isWordChar(int codeUnit) {
    // 0-9 A-Z _ a-z
    return (codeUnit >= 0x30 && codeUnit <= 0x39) ||
        (codeUnit >= 0x41 && codeUnit <= 0x5A) ||
        codeUnit == 0x5F ||
        (codeUnit >= 0x61 && codeUnit <= 0x7A) ||
        codeUnit > 0x7F;
  }

  /// The word containing (or adjacent to) [at], as a selection.
  G4Selection wordAt(G4Position at) {
    final G4Position p = clampPosition(at);
    final String block = _blocks[p.block];
    if (block.isEmpty) {
      return G4Selection.collapsed(p);
    }
    int i = p.offsetUtf16;
    if (i >= block.length) {
      i = block.length - 1;
    }
    if (!_isWordChar(block.codeUnitAt(i)) && i > 0 && _isWordChar(block.codeUnitAt(i - 1))) {
      i -= 1;
    }
    if (!_isWordChar(block.codeUnitAt(i))) {
      // Whitespace / punctuation run.
      int s = i;
      int e = i + 1;
      while (s > 0 && !_isWordChar(block.codeUnitAt(s - 1))) {
        s--;
      }
      while (e < block.length && !_isWordChar(block.codeUnitAt(e))) {
        e++;
      }
      return G4Selection(base: G4Position(p.block, s), extent: G4Position(p.block, e));
    }
    int s = i;
    int e = i + 1;
    while (s > 0 && _isWordChar(block.codeUnitAt(s - 1))) {
      s--;
    }
    while (e < block.length && _isWordChar(block.codeUnitAt(e))) {
      e++;
    }
    return G4Selection(base: G4Position(p.block, s), extent: G4Position(p.block, e));
  }

  /// The whole block containing [at].
  G4Selection blockSelectionAt(G4Position at) {
    final G4Position p = clampPosition(at);
    return G4Selection(
      base: G4Position(p.block, 0),
      extent: G4Position(p.block, _blocks[p.block].length),
    );
  }

  /// One position left of [p], crossing into the previous block if needed.
  G4Position positionBefore(G4Position p) {
    final G4Position c = clampPosition(p);
    if (c.offsetUtf16 > 0) {
      return G4Position(c.block, c.offsetUtf16 - 1);
    }
    if (c.block == 0) {
      return c;
    }
    return G4Position(c.block - 1, _blocks[c.block - 1].length);
  }

  /// One position right of [p], crossing into the next block if needed.
  G4Position positionAfter(G4Position p) {
    final G4Position c = clampPosition(p);
    if (c.offsetUtf16 < _blocks[c.block].length) {
      return G4Position(c.block, c.offsetUtf16 + 1);
    }
    if (c.block == _blocks.length - 1) {
      return c;
    }
    return G4Position(c.block + 1, 0);
  }

  /// Word boundary left of [p].
  G4Position wordBefore(G4Position p) {
    final G4Position c = clampPosition(p);
    if (c.offsetUtf16 == 0) {
      return positionBefore(c);
    }
    final String block = _blocks[c.block];
    int i = c.offsetUtf16;
    while (i > 0 && !_isWordChar(block.codeUnitAt(i - 1))) {
      i--;
    }
    while (i > 0 && _isWordChar(block.codeUnitAt(i - 1))) {
      i--;
    }
    return G4Position(c.block, i);
  }

  /// Word boundary right of [p].
  G4Position wordAfter(G4Position p) {
    final G4Position c = clampPosition(p);
    final String block = _blocks[c.block];
    if (c.offsetUtf16 >= block.length) {
      return positionAfter(c);
    }
    int i = c.offsetUtf16;
    while (i < block.length && !_isWordChar(block.codeUnitAt(i))) {
      i++;
    }
    while (i < block.length && _isWordChar(block.codeUnitAt(i))) {
      i++;
    }
    return G4Position(c.block, i);
  }
}

/// A deterministic ~400-block fixture. Fixed-width test glyphs make every
/// character offset land on an exact pixel, so gesture assertions are precise.
///
/// Layout of each block (29 UTF-16 units, no surrogate pairs):
///   `Block 000 alpha bravo charlie`
///    0     6   10    16    22
List<String> g4FixtureBlocks({int count = 400}) => <String>[
  for (int i = 0; i < count; i++) 'Block ${i.toString().padLeft(3, '0')} alpha bravo charlie',
];
