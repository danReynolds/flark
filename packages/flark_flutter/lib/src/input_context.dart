import 'dart:math' as math;
import 'package:characters/characters.dart';
import 'package:flutter/services.dart';

/// Exact surrounding source for the platform input method. The controller
/// still owns the full document and global offsets; this is only its mirror.
/// Selection and composition are never clipped to the ordinary context size.
class InputContext {
  const InputContext(this.source, this.start, this.end, {this.rebased = false});

  final TextEditingValue source;
  final int start, end;
  final bool rebased;

  factory InputContext.of(TextEditingValue source, {InputContext? previous}) {
    final text = source.text;
    if (previous != null && source == previous.source) {
      return InputContext(source, previous.start, previous.end);
    }
    var first = source.selection.start, last = source.selection.end;
    if (source.composing.isValid) {
      first = math.min(first, source.composing.start);
      last = math.max(last, source.composing.end);
    }
    if (previous != null) {
      final end = previous.end + text.length - previous.source.text.length;
      final contains = previous.start <= first && last <= end;
      final margin =
          (previous.start == 0 || first - previous.start >= 64) &&
          (end == text.length || end - last >= 64);
      // Keep an active composition's coordinates stable even as it grows.
      final composing =
          source.composing.isValid &&
          !source.composing.isCollapsed &&
          previous.source.composing.isValid &&
          !previous.source.composing.isCollapsed;
      if (contains &&
          (margin || composing) &&
          (end - previous.start <= math.max(1024, last - first + 512) ||
              composing) &&
          text.startsWith(previous.source.text.substring(0, previous.start)) &&
          text.endsWith(previous.source.text.substring(previous.end))) {
        return InputContext(source, previous.start, end);
      }
    }
    var start = 0, end = text.length;
    if (text.length > 1024) {
      final left = CharacterRange.at(text, math.max(0, first - 256));
      final right = CharacterRange.at(text, math.min(text.length, last + 256));
      start = left.stringBeforeLength;
      end = right.stringBeforeLength + right.current.length;
    }
    return InputContext(source, start, end, rebased: previous != null);
  }

  /// The window as the platform holds it, each CRLF as LF. Browsers keep a
  /// textarea's value with LF line breaks only: sent a CRLF window, they
  /// returned every edit without its CRs and the caret one place further on
  /// for each line break before it, so no typing in a CRLF document was
  /// accepted. No platform needs the CRs, and the kernel admits no bare CR.
  TextEditingValue get value {
    final window = source.text.substring(start, end);
    final crs = _crlfs(window);
    if (crs.isEmpty) {
      return TextEditingValue(
        text: window,
        selection: _selection(source.selection, -start),
        composing: _range(source.composing, -start),
      );
    }
    int local(int offset) => _withoutCrs(crs, offset - start);
    final composing = source.composing;
    return TextEditingValue(
      text: window.replaceAll('\r\n', '\n'),
      selection: source.selection.copyWith(
        baseOffset: local(source.selection.baseOffset),
        extentOffset: local(source.selection.extentOffset),
      ),
      composing: composing.isValid
          ? TextRange(start: local(composing.start), end: local(composing.end))
          : TextRange.empty,
    );
  }

  /// Reconstruct the complete proposed source before the kernel validates it.
  /// Invalid local ranges never get translated into neighboring document text.
  TextEditingValue? expand(TextEditingValue local) {
    if (!local.selection.isValid ||
        local.selection.end > local.text.length ||
        (local.composing.isValid && local.composing.end > local.text.length)) {
      return null;
    }
    final window = source.text.substring(start, end);
    final crs = _crlfs(window);
    if (crs.isEmpty) {
      return TextEditingValue(
        text: source.text.replaceRange(start, end, local.text),
        selection: _selection(local.selection, start),
        composing: _range(local.composing, start),
      );
    }
    // The platform edited the LF text it was sent. Replace the source of the
    // one span it changed, so every other line keeps its CR. An edit at the
    // selection it was sent is read there: matched from either end, a line
    // break typed beside another reads as that one, past its CRLF.
    final sent = window.replaceAll('\r\n', '\n'), next = local.text;
    final selection = source.selection;
    final selectionStart = _withoutCrs(crs, selection.start - start);
    final selectionEnd = _withoutCrs(crs, selection.end - start);
    // A deletion beside the caret it was sent is read there too: Backspace
    // ends at it, Delete starts at it. Matched from either end, a line break
    // deleted before others read as the last of them, which with mixed line
    // endings is another one, away from the caret.
    final removed = sent.length - next.length;
    final caret = selectionStart;
    bool deletes(int from) =>
        from >= 0 &&
        from + removed <= sent.length &&
        next.startsWith(sent.substring(0, from)) &&
        next.endsWith(sent.substring(from + removed));
    var a = 0, s = 0;
    if (selection.isValid &&
        selection.isCollapsed &&
        local.selection.isCollapsed &&
        removed > 0 &&
        caret >= 0 &&
        caret <= sent.length &&
        ((local.selection.extentOffset == caret - removed &&
                deletes(caret - removed)) ||
            (local.selection.extentOffset == caret && deletes(caret)))) {
      a = local.selection.extentOffset;
      s = sent.length - a - removed;
    } else if (selection.isValid &&
        selectionStart >= 0 &&
        selectionEnd <= sent.length &&
        selectionStart + sent.length - selectionEnd <= next.length &&
        next.startsWith(sent.substring(0, selectionStart)) &&
        next.endsWith(sent.substring(selectionEnd))) {
      a = selectionStart;
      s = sent.length - selectionEnd;
      // Narrow a selection's replacement to the text it changed. A value
      // that only moved the selection returns the selected text as it was,
      // and its line breaks keep their CRs.
      while (a < sent.length - s &&
          a < next.length - s &&
          sent.codeUnitAt(a) == next.codeUnitAt(a)) {
        a++;
      }
      while (a < sent.length - s &&
          a < next.length - s &&
          sent.codeUnitAt(sent.length - 1 - s) ==
              next.codeUnitAt(next.length - 1 - s)) {
        s++;
      }
    } else {
      final shorter = math.min(sent.length, next.length);
      while (a < shorter && sent.codeUnitAt(a) == next.codeUnitAt(a)) {
        a++;
      }
      while (s < shorter - a &&
          sent.codeUnitAt(sent.length - 1 - s) ==
              next.codeUnitAt(next.length - 1 - s)) {
        s++;
      }
    }
    final from = _withCrs(crs, a), to = _withCrs(crs, sent.length - s);
    final inserted = next.length - s - a;
    int full(int offset) =>
        start +
        (offset <= a
            ? _withCrs(crs, offset)
            : offset <= a + inserted
            ? from + offset - a
            : from +
                  inserted -
                  to +
                  _withCrs(crs, offset - a - inserted + sent.length - s));
    final composing = local.composing;
    return TextEditingValue(
      text: source.text.replaceRange(
        start + from,
        start + to,
        next.substring(a, a + inserted),
      ),
      selection: local.selection.copyWith(
        baseOffset: full(local.selection.baseOffset),
        extentOffset: full(local.selection.extentOffset),
      ),
      composing: composing.isValid
          ? TextRange(start: full(composing.start), end: full(composing.end))
          : TextRange.empty,
    );
  }

  /// Where each CRLF of [window] begins.
  static List<int> _crlfs(String window) => [
    for (
      var i = window.indexOf('\r\n');
      i >= 0;
      i = window.indexOf('\r\n', i + 2)
    )
      i,
  ];

  /// The LF text's offset for [offset] in the window: less the CRs before it.
  static int _withoutCrs(List<int> crs, int offset) {
    var n = 0;
    while (n < crs.length && crs[n] < offset) {
      n++;
    }
    return offset - n;
  }

  /// The window offset for [offset] in the LF text. An offset at a line
  /// break stays before its CR.
  static int _withCrs(List<int> crs, int offset) {
    var n = 0;
    while (n < crs.length && crs[n] - n < offset) {
      n++;
    }
    return offset + n;
  }

  static TextSelection _selection(TextSelection s, int offset) => s.copyWith(
    baseOffset: s.baseOffset + offset,
    extentOffset: s.extentOffset + offset,
  );
  static TextRange _range(TextRange r, int offset) => r.isValid
      ? TextRange(start: r.start + offset, end: r.end + offset)
      : TextRange.empty;
}
