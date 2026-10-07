import 'package:flark/rendering.dart';
import 'package:flark/flark.dart';
import 'package:characters/characters.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Whether [start]..[end] of [text] is the one grapheme a platform delete
/// removes beside the caret: a character, a surrogate pair, or a CRLF, which
/// the platform holds as one LF.
bool _oneGrapheme(String text, int start, int end) =>
    end > start && text.substring(start, end).characters.length == 1;

/// [text] without the halves of surrogate pairs it splits.
String _paired(String text) {
  bool high(int unit) => unit >= 0xd800 && unit <= 0xdbff;
  bool low(int unit) => unit >= 0xdc00 && unit <= 0xdfff;
  if (!text.codeUnits.any((unit) => high(unit) || low(unit))) return text;
  final out = StringBuffer();
  for (var i = 0; i < text.length; i++) {
    final unit = text.codeUnitAt(i);
    if (high(unit) && i + 1 < text.length && low(text.codeUnitAt(i + 1))) {
      out.write(text.substring(i, i + 2));
      i++;
    } else if (!high(unit) && !low(unit)) {
      out.writeCharCode(unit);
    }
  }
  return out.toString();
}

/// What an open composition's composing range holds. An input method can
/// commit part of what it composes and compose on in the same composition
/// (a Korean syllable as the next one's first letter is typed, a Japanese
/// clause converted ahead of the rest), and so move its composing range off
/// text the composition inserted.
enum _Composing {
  /// Text that was there before the composition began.
  existing,

  /// All that the composition inserted at a caret.
  inserted,

  /// All that the composition put in place of a selection.
  replaced,

  /// Part of what the composition inserted or put in place of a selection:
  /// the input method also committed text outside the range, the start of
  /// what it composed (before the range) or a correction of a word beside
  /// it. A cancel removes only the range.
  rest,

  /// Text elsewhere: the range moved off what the composition inserted, or
  /// the input method corrected a word beside a range over text that was
  /// there before.
  moved,
}

/// One Flutter-facing publication of the kernel. Platform values are input
/// messages; the editor's snapshot remains the sole document authority.
abstract interface class FlarkSurfaceController implements Listenable {
  FlarkDocumentState get editor;
  String get text;

  /// Whether an input method is composing in the document. A reader never
  /// composes.
  bool get composing;
  bool command(FlarkCommand command, {int? expectedRevision});
  void sourceMode(bool enabled);
}

class FlarkController extends ChangeNotifier implements FlarkSurfaceController {
  FlarkController(this.editor) {
    editor.addListener(_changed);
  }
  @override
  final FlarkEditor editor;

  TextRange _composing = TextRange.empty;
  String? _compositionSource;

  /// What the open composition's composing range holds.
  _Composing _compositionHolds = _Composing.existing;

  /// The selection the open composition replaced, if it began over one.
  (int, int)? _compositionReplaced;
  TextEditingValue? _lastReceived;
  int _lastReceivedRevision = -1;
  String? notice;
  bool _disposed = false;
  bool _batching = false;
  @override
  String get text => editor.source;
  @override
  bool get composing => editor.composing;

  /// Current selection/typing style. Re-read when this controller notifies.
  FlarkStyleState styleState(int style) => editor.styleState(style);

  /// Idempotent formatting; shares command history and IME handling.
  bool setStyle(int style, {required bool enabled}) =>
      command(SetStyle(style, enabled: enabled));
  TextEditingValue get value => TextEditingValue(
    text: text,
    selection: TextSelection(
      baseOffset: editor.selection.base,
      extentOffset: editor.selection.extent,
    ),
    composing: _composing,
  );

  void _changed() {
    if (!editor.composing) {
      _composing = TextRange.empty;
      _compositionSource = null;
    }
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

  @override
  bool command(FlarkCommand command, {int? expectedRevision}) =>
      _batch(() => _command(command, expectedRevision: expectedRevision));
  bool _command(FlarkCommand command, {int? expectedRevision}) {
    // Undo with nothing to undo (Command-Z on a fresh document) is no edit
    // refused: it said "This edit needs source mode."
    final nothing =
        (command is Undo && !editor.history.canUndo) ||
        (command is Redo && !editor.history.canRedo);
    final ended = editor.composing;
    final changed = editor.applyAfterComposition(
      command,
      expectedRevision: expectedRevision,
    );
    if (!editor.composing) {
      _composing = TextRange.empty;
      _compositionSource = null;
    }
    // A command that applied can still have withdrawn the composition it
    // ended (typing refused what was composed).
    notice = changed
        ? (ended ? _rejectionNotice : null)
        : nothing
        ? notice
        : _rejectionNotice;
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
    return receive(next, authenticated: true);
  }

  /// Applies a platform value. An [authenticated] value is known to edit the
  /// text this controller last gave the platform, as a delta batch whose old
  /// text matched does: a replacement away from the selection is then the
  /// platform's own correction, not a stale value from an older connection.
  /// A [resynchronized] platform was given this controller's value since its
  /// last input, so a value repeating that input is a new edit, not a copy.
  bool receive(
    TextEditingValue next, {
    int? expectedRevision,
    bool authenticated = false,
    bool resynchronized = false,
  }) => _batch(
    () => _receive(
      next,
      expectedRevision: expectedRevision,
      authenticated: authenticated,
      resynchronized: resynchronized,
    ),
  );
  bool _receive(
    TextEditingValue next, {
    int? expectedRevision,
    required bool authenticated,
    required bool resynchronized,
  }) {
    if (expectedRevision != null && expectedRevision != editor.revision) {
      return false;
    }
    // A full value received again before anything changed is a duplicate.
    // An authenticated value edits the value the platform holds now, and so
    // does a full value from a platform given this controller's value since:
    // after the kernel reshaped an edit and the platform was resynchronized,
    // a new edit can produce the earlier value again (Return, Backspace,
    // Return; or Android's Chrome deleting a list item's tab, then "-").
    if (!authenticated &&
        !resynchronized &&
        _lastReceived == next &&
        _lastReceivedRevision == editor.revision) {
      return false;
    }
    final before = value;
    if (next == before) return false;
    if (!next.selection.isValid || next.selection.end > next.text.length) {
      return false;
    }
    final isComposing = next.composing.isValid && !next.composing.isCollapsed;
    final opened = isComposing && !editor.composing;
    if (opened) {
      _compositionSource = text;
      final selection = editor.selection;
      _compositionHolds =
          next.composing.start != selection.start ||
              next.text.length - before.text.length !=
                  next.composing.end -
                      next.composing.start -
                      (selection.end - selection.start)
          ? _Composing.existing
          : selection.isCollapsed
          ? _Composing.inserted
          : _Composing.replaced;
      _compositionReplaced = editor.selection.isCollapsed
          ? null
          : (editor.selection.start, editor.selection.end);
      editor.beginComposition();
    } else if (isComposing && editor.composing) {
      _composingMoved(before, next);
    }
    // An input method that cancels removes the text it composed. Typing it
    // can have added structure around it (a fenced block's first line
    // break), which the platform then holds too; removing only the composed
    // text still restores the state before the composition. Once the input
    // method committed some of it, removing what it still composes is no
    // cancel.
    final cancelled =
        editor.composing &&
        !isComposing &&
        (next.text == _compositionSource ||
            (_compositionHolds == _Composing.inserted &&
                _composing.isValid &&
                !_composing.isCollapsed &&
                next.text ==
                    text.replaceRange(_composing.start, _composing.end, '')));
    if (cancelled) {
      _composing = TextRange.empty;
      _compositionSource = null;
      editor.cancelComposition();
      // The source is as it was, but the caret is where the platform put it:
      // after a word typed over its own selection, or where Gboard moved it
      // as it finished composing.
      if (text == next.text && !_atPlatformSelection(next)) {
        editor.apply(
          SetSelection(next.selection.baseOffset, next.selection.extentOffset),
        );
      }
      _lastReceived = next;
      _lastReceivedRevision = editor.revision;
      return true;
    }
    // A composition over a selection replaced it, and cancelling leaves the
    // selected text deleted, as a platform text field does. Read it so:
    // removing the composed text by range can be refused for what it made
    // (a backtick composed after another closes a code span, whose closer is
    // hidden), which kept the cancelled text.
    final replaced = _compositionReplaced, source = _compositionSource;
    if (editor.composing &&
        !isComposing &&
        replaced != null &&
        source != null &&
        next.text == source.replaceRange(replaced.$1, replaced.$2, '')) {
      _composing = TextRange.empty;
      _compositionSource = null;
      editor.cancelComposition();
      editor.apply(const DeleteBackward());
      _lastReceived = next;
      _lastReceivedRevision = editor.revision;
      return true;
    }
    var changed = false, committedWord = false;
    if (next.text == before.text) {
      // A value that keeps the selection's offsets (one that only sets a
      // composing region) keeps the selection as it is: a caret in an
      // unwritten table cell shares its offset with the cell before it.
      if (!_atPlatformSelection(next)) {
        final was = editor.selection;
        changed = editor.apply(
          SetSelection(next.selection.baseOffset, next.selection.extentOffset),
        );
        // A caret the platform steps onto no caret position of its own
        // (between one table cell's text and the next) goes back where it
        // was, so the keyboard's cursor control could not leave the cell:
        // it moves on as an arrow would, the way the platform moved it.
        final to = next.selection.extentOffset;
        if (was.isCollapsed &&
            next.selection.isCollapsed &&
            to != was.extent &&
            editor.selection == was) {
          changed =
              editor.apply(
                MoveCaret(
                  to < was.extent
                      ? MoveDirection.backward
                      : MoveDirection.forward,
                ),
              ) ||
              changed;
        }
      }
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
      // A platform can split a surrogate pair: iOS deletes one UTF-16 unit
      // before the caret unless its code point is an emoji, which leaves
      // half of a CJK Extension B or mathematical letter. That half is not
      // text the kernel can take; read the edit without it.
      var inserted = _paired(next.text.substring(start, nextEnd));
      // An input method may finish its word and type Return in one value:
      // Android's LatinIME commits the composed word and "\n" inside one
      // batch edit, and iOS sends its autocorrection of the word with the
      // line break Return inserts. That break is a typed Return, not text.
      // Apply the word (committing a composition), then Return as when they
      // arrive apart, so lists, quotes, fences and tables continue after it.
      final typedReturn =
          (editor.composing || start < end) &&
          !isComposing &&
          inserted.endsWith('\n') &&
          sel.isCollapsed &&
          end == sel.end &&
          next.selection.isCollapsed &&
          next.selection.extentOffset == nextEnd;
      if (typedReturn) {
        inserted = inserted.substring(0, inserted.length - 1);
        if (start != end || inserted.isNotEmpty) {
          committedWord = editor.apply(
            start == end && inserted.isNotEmpty
                ? InsertText(inserted)
                : ReplaceRange(start, end, inserted),
          );
        }
        if (editor.composing) _endComposition();
      }
      final FlarkCommand command;
      if (typedReturn) {
        command = const Newline();
      } else if (editor.composing) {
        command = start == sel.start && end == sel.end && inserted.isNotEmpty
            ? InsertText(inserted)
            : ReplaceRange(start, end, inserted);
      } else if (inserted == '\n' && start == sel.start && end == sel.end) {
        command = const Newline();
      } else if (inserted.isEmpty &&
          sel.isCollapsed &&
          end == sel.extent &&
          _oneGrapheme(before.text, start, end)) {
        command = const DeleteBackward();
      } else if (inserted.isEmpty &&
          sel.isCollapsed &&
          start == sel.extent &&
          _oneGrapheme(before.text, start, end)) {
        command = const DeleteForward();
      } else if (start == sel.start && end == sel.end && inserted.isNotEmpty) {
        command = InsertText(inserted);
      } else {
        // Replacements may originate in IME/autocorrect, but cannot silently
        // replace unrelated text from an older full-value input connection.
        // An authenticated delta edits the text the platform was sent.
        final related = start <= sel.end && end >= sel.start;
        // A full value's difference leaves out the unchanged text between a
        // correction and the caret: the space typed after a word autocorrect
        // replaces, or the one the double-space period turns into ". ". The
        // platform keeps its caret past that text, on the caret's line.
        final beforeCaret =
            sel.isCollapsed &&
            next.selection.isCollapsed &&
            end <= sel.start &&
            next.selection.extentOffset - nextEnd == sel.start - end &&
            !before.text.substring(end, sel.start).contains('\n');
        if (!related && !beforeCaret && !authenticated) {
          if (opened) _abandonComposition();
          notice = 'Input was resynchronized.';
          _changed();
          return false;
        }
        command = ReplaceRange(start, end, inserted);
      }
      changed = editor.apply(command);
      // A replacement ends the kernel's caret at its text, but a correction
      // away from the caret (autocorrect fixing the word before a space the
      // platform already holds) leaves the platform's caret where it was.
      // When the kernel made exactly the platform's edit, keep its selection.
      if (changed &&
          command is ReplaceRange &&
          text == next.text &&
          !_atPlatformSelection(next)) {
        editor.apply(
          SetSelection(next.selection.baseOffset, next.selection.extentOffset),
        );
      }
    }
    if (changed || next.text == text) {
      // The kernel composes text as the platform holds it, so the platform's
      // composing range is the kernel's, wherever the caret was legalized.
      // Text the kernel reshaped (code opening its first body line) moved
      // the range with the caret.
      final shift = next.text == text
          ? 0
          : editor.selection.extent - next.selection.extentOffset;
      _composing = isComposing
          ? TextRange(
              start: (next.composing.start + shift).clamp(0, text.length),
              end: (next.composing.end + shift).clamp(0, text.length),
            )
          : TextRange.empty;
      // A notice stays while the platform composes: taking it away moves
      // the document, and a browser with accessibility on ends its
      // composition when the editor's semantics move.
      if (!isComposing) notice = null;
    } else {
      notice = _rejectionNotice ?? 'This edit needs source mode.';
      if (opened) _abandonComposition();
    }
    // A commit types what was composed, which can change the document.
    final revision = editor.revision;
    if (!isComposing && editor.composing) _endComposition();
    _lastReceived = next;
    _lastReceivedRevision = editor.revision;
    _changed();
    return changed || committedWord || editor.revision != revision;
  }

  /// Whether the editor's selection has the offsets of [next]'s.
  bool _atPlatformSelection(TextEditingValue next) =>
      editor.selection.base == next.selection.baseOffset &&
      editor.selection.extent == next.selection.extentOffset;

  /// Follows what the composing range holds as [next] continues the open
  /// composition. An input method that commits part of it and composes on,
  /// in one update (Android's batch edit, iOS's deltas of one run loop
  /// turn), starts its composing range past what it committed; one that
  /// corrects a word beside it edits the text before or after the range,
  /// whether or not that moves the range. The composition goes on, but
  /// removing the range no longer cancels it. A start the platform reported
  /// before stands: a browser keeps reporting where its composition began
  /// after the editor moved the composed text (code opening its first body
  /// line).
  void _composingMoved(TextEditingValue before, TextEditingValue next) {
    final from = _composing, start = next.composing.start;
    final reported = _lastReceived?.composing;
    if (!from.isValid ||
        from.isCollapsed ||
        (start != from.start &&
            reported != null &&
            reported.isValid &&
            start == reported.start)) {
      return;
    }
    // A source the app changed under the composition, past this controller,
    // can leave the range beyond its text: what the range held is unknown,
    // so a cancel keeps it, as when the range moves.
    if (from.end > before.text.length) {
      _compositionHolds = _Composing.moved;
      return;
    }
    final head = before.text.substring(0, from.start);
    final tail = before.text.substring(from.end);
    if (next.text.length < head.length + tail.length ||
        !next.text.startsWith(head) ||
        !next.text.endsWith(tail)) {
      // The text before or after the range changed: the input method
      // corrected a word beside it. A cancel then removes only the range,
      // and the correction stays. Over text that was there before, the range
      // holds nothing of its own to remove, and a cancel keeps it all.
      _compositionHolds =
          _compositionHolds == _Composing.existing ||
              _compositionHolds == _Composing.moved
          ? _Composing.moved
          : _Composing.rest;
      return;
    }
    if (start == from.start) return;
    // A later start at the same end, with the text around the composition as
    // it was, leaves the input method composing the end of what it composed.
    final grown = next.text.length - before.text.length;
    final rest =
        _compositionHolds != _Composing.existing &&
        _compositionHolds != _Composing.moved &&
        start > from.start &&
        next.composing.end == from.end + grown;
    _compositionHolds = rest ? _Composing.rest : _Composing.moved;
  }

  /// The platform ended its composition. Keep what it composed, typed, as one
  /// history step, or restore the exact prior state when it left the source
  /// as it was.
  void _endComposition() {
    if (editor.source == _compositionSource) {
      editor.cancelComposition();
    } else {
      _commitComposition();
    }
    _compositionSource = null;
  }

  /// Typing can refuse what was composed, which withdraws it: say why.
  void _commitComposition() {
    if (!editor.composing) return;
    editor.commitComposition();
    if (editor.lastRejection != null) notice = _rejectionNotice;
  }

  /// Ends a composition that a rejected platform value opened. The platform
  /// is resynchronized without it, and while the kernel still composed, the
  /// editor would leave every editing key to the input method. The value
  /// changed nothing, so ending it records no history.
  void _abandonComposition() {
    editor.commitComposition();
    _compositionSource = null;
  }

  /// Ends the composition. Cancelled, it leaves what the input method
  /// committed of it: only the text it still composes goes, or, when its
  /// composing range moved to other text, nothing.
  void finishComposition({bool cancel = false}) =>
      _batch(() => _finishComposition(cancel: cancel));
  void _finishComposition({bool cancel = false}) {
    final composing = _composing;
    _composing = TextRange.empty;
    if (!cancel) {
      _commitComposition();
    } else if (_compositionHolds != _Composing.rest &&
        _compositionHolds != _Composing.moved) {
      editor.cancelComposition();
    } else {
      // The input method committed text outside its range: what it composed
      // before it, or a correction beside it.
      if (editor.composing &&
          _compositionHolds == _Composing.rest &&
          composing.isValid &&
          !composing.isCollapsed &&
          composing.end <= text.length) {
        editor.apply(ReplaceRange(composing.start, composing.end, ''));
      }
      if (editor.composing) _endComposition();
    }
    _compositionSource = null;
    _changed();
  }

  @override
  void sourceMode(bool enabled) => _batch(() {
    finishComposition();
    editor.setSourceMode(enabled);
    notice = null;
  });

  String get selectedText {
    final sel = editor.selection;
    if (editor.sourceMode) return text.substring(sel.start, sel.end);
    return editor.document.visibleText(sel.start, sel.end);
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    editor.removeListener(_changed);
    super.dispose();
  }
}
