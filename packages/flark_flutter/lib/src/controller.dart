import 'package:flark/flark.dart';
import 'package:characters/characters.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'code_colors.dart';

/// One Flutter-facing publication of the kernel. Platform values are input
/// messages; the editor's snapshot remains the sole document authority.
class FlarkController extends ChangeNotifier {
  FlarkController(this.editor, {this.codeColors}) {
    if (codeColors != null && !identical(codeColors!.editor, editor)) {
      throw ArgumentError('Colors must belong to this editor');
    }
    editor.addListener(_changed);
  }
  final FlarkEditor editor;

  /// Owned optional decoration service; disposed with this controller.
  final FlarkCodeColors? codeColors;
  TextRange _composing = TextRange.empty;
  String? _compositionSource;
  TextEditingValue? _lastReceived;
  int _lastReceivedRevision = -1;
  String? notice;
  bool _disposed = false;
  bool _batching = false;
  String get text => editor.source;
  TextEditingValue get value => TextEditingValue(
    text: text,
    selection: TextSelection(
      baseOffset: editor.selection.base,
      extentOffset: editor.selection.extent,
    ),
    composing: _composing,
  );

  void _changed() {
    if (!_disposed && !_batching) notifyListeners();
  }

  T _batch<T>(T Function() action) {
    final nested = _batching;
    final revision = editor.revision,
        before = value,
        previousNotice = notice,
        composing = editor.composing;
    _batching = true;
    try {
      return action();
    } finally {
      _batching = nested;
      if (!nested &&
          !_disposed &&
          (editor.revision != revision ||
              value != before ||
              notice != previousNotice ||
              editor.composing != composing)) {
        notifyListeners();
      }
    }
  }

  bool command(FlarkCommand command, {int? expectedRevision}) =>
      _batch(() => _command(command, expectedRevision: expectedRevision));
  bool _command(FlarkCommand command, {int? expectedRevision}) {
    if (editor.composing) {
      editor.commitComposition();
      _composing = TextRange.empty;
      _compositionSource = null;
    }
    final changed = editor.apply(command, expectedRevision: expectedRevision);
    notice = changed ? null : _rejectionNotice;
    _changed();
    return changed;
  }

  String? get _rejectionNotice => switch (editor.lastRejection) {
    FlarkRejection.unsupportedEdit => 'This edit needs source mode.',
    FlarkRejection.sourceLimit =>
      'This document exceeds the writable source limit.',
    FlarkRejection.invalidSource =>
      'The inserted text is not valid Unicode text.',
    FlarkRejection.extractionDeviation =>
      'This edit could not be rendered. Source mode is available.',
    FlarkRejection.staleRevision => 'Input was resynchronized.',
    null => null,
  };

  /// Delta clients authenticate their old text before applying the batch.
  bool receiveDeltas(List<TextEditingDelta> deltas) {
    var next = value;
    for (final delta in deltas) {
      if (delta.oldText != next.text) {
        notice = 'Input was resynchronized.';
        _changed();
        return false;
      }
      next = delta.apply(next);
    }
    return receive(next);
  }

  bool receive(TextEditingValue next, {int? expectedRevision}) =>
      _batch(() => _receive(next, expectedRevision: expectedRevision));
  bool _receive(TextEditingValue next, {int? expectedRevision}) {
    if (expectedRevision != null && expectedRevision != editor.revision) {
      return false;
    }
    if (_lastReceived == next && _lastReceivedRevision == editor.revision) {
      return false;
    }
    final before = value;
    if (next == before) return false;
    if (!next.selection.isValid || next.selection.end > next.text.length) {
      return false;
    }
    final isComposing = next.composing.isValid && !next.composing.isCollapsed;
    if (isComposing && !editor.composing) {
      _compositionSource = text;
      editor.beginComposition();
    }
    if (editor.composing && !isComposing && next.text == _compositionSource) {
      _composing = TextRange.empty;
      _compositionSource = null;
      editor.cancelComposition();
      _lastReceived = next;
      _lastReceivedRevision = editor.revision;
      return true;
    }
    var changed = false;
    if (next.text == before.text) {
      changed = editor.apply(
        SetSelection(next.selection.baseOffset, next.selection.extentOffset),
      );
    } else {
      var start = 0;
      var end = before.text.length, nextEnd = next.text.length;
      final sel = editor.selection;
      // Repeated characters admit several equally small diffs. Authenticate
      // the edit at the current selection first; a greedy common prefix can
      // put a typed space after its identical neighbor or move a deletion.
      bool atRange(int a, int b) {
        if (a < 0 || b < a || b > before.text.length) return false;
        final count = next.text.length - (before.text.length - (b - a));
        if (count < 0 ||
            !next.text.startsWith(before.text.substring(0, a)) ||
            !next.text.endsWith(before.text.substring(b))) {
          return false;
        }
        start = a;
        end = b;
        nextEnd = a + count;
        return true;
      }

      final removed = before.text.length - next.text.length;
      final atSelection =
          atRange(sel.start, sel.end) ||
          (sel.isCollapsed &&
              next.selection.isCollapsed &&
              removed > 0 &&
              ((next.selection.extentOffset == sel.extent - removed &&
                      atRange(sel.extent - removed, sel.extent)) ||
                  (next.selection.extentOffset == sel.extent &&
                      atRange(sel.extent, sel.extent + removed))));
      if (!atSelection) {
        while (start < before.text.length &&
            start < next.text.length &&
            before.text.codeUnitAt(start) == next.text.codeUnitAt(start)) {
          start++;
        }
        while (end > start &&
            nextEnd > start &&
            before.text.codeUnitAt(end - 1) ==
                next.text.codeUnitAt(nextEnd - 1)) {
          end--;
          nextEnd--;
        }
      }
      // A platform diff may split a surrogate or a combining sequence. Widen
      // it over the unchanged prefix/suffix so the kernel receives whole
      // rendered graphemes without losing their base characters.
      final startRange = CharacterRange.at(before.text, start);
      if (startRange.isNotEmpty) start = startRange.stringBeforeLength;
      final endRange = CharacterRange.at(before.text, end);
      if (endRange.isNotEmpty) {
        final widened = before.text.length - endRange.stringAfterLength;
        nextEnd += widened - end;
        end = widened;
      }
      final inserted = next.text.substring(start, nextEnd);
      final FlarkCommand command;
      if (editor.composing) {
        command = start == sel.start && end == sel.end && inserted.isNotEmpty
            ? InsertText(inserted)
            : ReplaceRange(start, end, inserted);
      } else if (inserted == '\n' && start == sel.start && end == sel.end) {
        command = const Newline();
      } else if (inserted.isEmpty &&
          sel.isCollapsed &&
          end == sel.extent &&
          end - start == 1) {
        command = const DeleteBackward();
      } else if (inserted.isEmpty &&
          sel.isCollapsed &&
          start == sel.extent &&
          end - start == 1) {
        command = const DeleteForward();
      } else if (start == sel.start && end == sel.end && inserted.isNotEmpty) {
        command = InsertText(inserted);
      } else {
        // Replacements may originate in IME/autocorrect, but cannot silently
        // replace unrelated text from an older full-value input connection.
        final related = start <= sel.end && end >= sel.start;
        if (!related) {
          notice = 'Input was resynchronized.';
          _changed();
          return false;
        }
        command = ReplaceRange(start, end, inserted);
      }
      changed = editor.apply(command);
    }
    if (changed || next.text == text) {
      final shift = editor.selection.extent - next.selection.extentOffset;
      _composing = isComposing
          ? TextRange(
              start: (next.composing.start + shift).clamp(0, text.length),
              end: (next.composing.end + shift).clamp(0, text.length),
            )
          : TextRange.empty;
      notice = null;
    } else {
      notice = _rejectionNotice ?? 'This edit needs source mode.';
    }
    if (!isComposing && editor.composing) {
      if (editor.source == _compositionSource) {
        editor.cancelComposition();
      } else {
        editor.commitComposition();
      }
      _compositionSource = null;
    }
    _lastReceived = next;
    _lastReceivedRevision = editor.revision;
    _changed();
    return changed;
  }

  void finishComposition({bool cancel = false}) =>
      _batch(() => _finishComposition(cancel: cancel));
  void _finishComposition({bool cancel = false}) {
    _composing = TextRange.empty;
    _compositionSource = null;
    if (cancel) {
      editor.cancelComposition();
    } else {
      editor.commitComposition();
    }
    _changed();
  }

  void sourceMode(bool enabled) => _batch(() {
    finishComposition();
    editor.setSourceMode(enabled);
    notice = null;
  });

  String get selectedText {
    final sel = editor.selection;
    if (editor.sourceMode) return text.substring(sel.start, sel.end);
    final doc = editor.document;
    final start = doc.displayOf(sel.start), end = doc.displayOf(sel.end);
    return [
      for (var i = start.row; i <= end.row; i++)
        doc.projection.rows[i].text.substring(
          i == start.row ? start.offset : 0,
          i == end.row ? end.offset : doc.projection.rows[i].text.length,
        ),
    ].join('\n');
  }

  @override
  void dispose() {
    _disposed = true;
    editor.removeListener(_changed);
    codeColors?.dispose();
    super.dispose();
  }
}
