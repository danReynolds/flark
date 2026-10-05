/// Candidate edits as values: sorted, disjoint splices of one source.
///
/// The editor answers most structural commands by trying spellings of an
/// edit, the edit as asked and then faithful respellings, and committing the
/// first the parser reads as it must. Each spelling is [Edits] of the current
/// source. Applying them and mapping offsets through them happens here, once,
/// so that every check reads a candidate the same way.
library;

import 'document.dart';
import 'history.dart';

/// One splice: the source from `start` to `end` replaced with `text`.
typedef Edit = (int start, int end, String text);

/// Sorted, disjoint [Edit]s of one source.
final class Edits {
  /// [edits], sorted by start, none overlapping the next ([isDisjoint]).
  Edits(Iterable<Edit> edits) : list = List.unmodifiable(edits) {
    assert(isDisjoint(list), 'overlapping edits $list');
  }

  /// The one edit that makes [now] from [was]: from where the two first
  /// differ to where they last do.
  factory Edits.between(String was, String now) {
    final shorter = was.length < now.length ? was.length : now.length;
    var head = 0;
    while (head < shorter && was.codeUnitAt(head) == now.codeUnitAt(head)) {
      head++;
    }
    var tail = 0;
    while (tail < shorter - head &&
        was.codeUnitAt(was.length - 1 - tail) ==
            now.codeUnitAt(now.length - 1 - tail)) {
      tail++;
    }
    return Edits([
      (head, was.length - tail, now.substring(head, now.length - tail)),
    ]);
  }

  /// The splices, in source order.
  final List<Edit> list;

  /// Whether [edits] are sorted and none overlaps the next, so that they
  /// splice a source in order.
  static bool isDisjoint(List<Edit> edits) {
    for (var i = 1; i < edits.length; i++) {
      if (edits[i].$1 < edits[i - 1].$2) return false;
    }
    return true;
  }

  /// [source] with the splices made.
  String apply(String source) {
    final out = StringBuffer();
    var copied = 0;
    for (final (start, end, text) in list) {
      out
        ..write(source.substring(copied, start))
        ..write(text);
      copied = end;
    }
    out.write(source.substring(copied));
    return out.toString();
  }

  /// Where [offset] of the source the edits are made to lies in the source
  /// they make: the place of the text that was there. Where an edit starts
  /// the offset stays before what the edit put there, and inside a range an
  /// edit replaced it has no place: -1.
  ///
  /// With [caret], the offset is a caret's, which goes with the text typed
  /// at it: past what an edit inserted where it was, and from inside a range
  /// an edit replaced to the start of what replaced it.
  int forward(int offset, {bool caret = false}) {
    var shift = 0;
    for (final (start, end, text) in list) {
      if (caret ? offset < start : offset <= start) break;
      if (offset < end) return caret ? start + shift : -1;
      shift += text.length - (end - start);
    }
    return offset + shift;
  }

  /// Where [offset] of the source the edits make lay in the source they are
  /// made to, or -1 in text an edit put there.
  int back(int offset) {
    var shift = 0;
    for (final (start, end, text) in list) {
      if (offset < start + shift) break;
      if (offset < start + shift + text.length) return -1;
      shift += text.length - (end - start);
    }
    return offset - shift;
  }

  /// These edits with [text] put at [at] of the source they make as well:
  /// into the text of an edit it meets, else as an edit of its own.
  Edits withInsertion(int at, String text) {
    final out = <Edit>[];
    var shift = 0, placed = false;
    for (final (start, end, replaced) in list) {
      if (!placed && at <= start + shift + replaced.length) {
        if (at < start + shift) {
          out
            ..add((at - shift, at - shift, text))
            ..add((start, end, replaced));
        } else {
          final within = at - start - shift;
          out.add((start, end, replaced.replaceRange(within, within, text)));
        }
        placed = true;
      } else {
        out.add((start, end, replaced));
      }
      shift += replaced.length - (end - start);
    }
    if (!placed) out.add((at - shift, at - shift, text));
    return Edits(out);
  }
}

/// One spelling of an edit: the [edits] that make it from the current
/// source, the [selection] after them and the [pending] style that keeps
/// the typing intent. One spelling is the edit [asAsked]; the others are
/// respellings the kernel chose to keep what the edit means (an escape, a
/// blank line that keeps blocks apart, a respelled prefix). Only the edit as
/// asked may leave the live tier unchecked (EP1-RESULT-PRESENTATION-001): a
/// respelling must stay where the parser checks it.
final class Spelling {
  const Spelling(
    this.edits,
    this.selection, {
    this.pending,
    this.asAsked = false,
  });

  /// The spelling of [edits] with [selection] carried through them, as a
  /// caret goes with its text ([Edits.forward]).
  factory Spelling.carrying(
    Edits edits,
    FlarkSelection selection, {
    PendingStyle? pending,
    bool asAsked = false,
  }) => Spelling(
    edits,
    FlarkSelection(
      edits.forward(selection.base, caret: true),
      edits.forward(selection.extent, caret: true),
    ),
    pending: pending,
    asAsked: asAsked,
  );

  final Edits edits;
  final FlarkSelection selection;
  final PendingStyle? pending;
  final bool asAsked;
}
