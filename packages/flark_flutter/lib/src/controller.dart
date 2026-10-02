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

/// One Flutter-facing publication of the kernel. Platform values are input
/// messages; the editor's snapshot remains the sole document authority.
abstract interface class FlarkSurfaceController implements Listenable {
  FlarkDocumentState get editor;
  String get text;
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
  TextEditingValue? _lastReceived;
  int _lastReceivedRevision = -1;
  String? notice;
  bool _disposed = false;
  bool _batching = false;
  @override
  String get text => editor.source;

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
    final changed = editor.applyAfterComposition(
      command,
      expectedRevision: expectedRevision,
    );
    if (!editor.composing) {
      _composing = TextRange.empty;
      _compositionSource = null;
    }
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
    final opened = isComposing && !editor.composing;
    if (opened) {
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
    var changed = false, committedWord = false;
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
      var inserted = next.text.substring(start, nextEnd);
      // An input method may finish its word and type Return in one value:
      // Android's LatinIME commits the composed word and "\n" inside one
      // batch edit. That break is a typed Return, not composed text. Commit
      // the word as the composition, then apply Return as when they arrive
      // apart, so lists, quotes, fences and tables continue after it.
      final typedReturn =
          editor.composing &&
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
        _endComposition();
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
        final related = start <= sel.end && end >= sel.start;
        if (!related) {
          if (opened) _abandonComposition();
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
      if (opened) _abandonComposition();
    }
    if (!isComposing && editor.composing) _endComposition();
    _lastReceived = next;
    _lastReceivedRevision = editor.revision;
    _changed();
    return changed || committedWord;
  }

  /// The platform ended its composition. Keep what it composed as one history
  /// step, or restore the exact prior state when it left the source as it was.
  void _endComposition() {
    if (editor.source == _compositionSource) {
      editor.cancelComposition();
    } else {
      editor.commitComposition();
    }
    _compositionSource = null;
  }

  /// Ends a composition that a rejected platform value opened. The platform
  /// is resynchronized without it, and while the kernel still composed, the
  /// editor would leave every editing key to the input method. The value
  /// changed nothing, so ending it records no history.
  void _abandonComposition() {
    editor.commitComposition();
    _compositionSource = null;
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
