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

  TextEditingValue get value => TextEditingValue(
    text: source.text.substring(start, end),
    selection: _selection(source.selection, -start),
    composing: _range(source.composing, -start),
  );

  /// Reconstruct the complete proposed source before the kernel validates it.
  /// Invalid local ranges never get translated into neighboring document text.
  TextEditingValue? expand(TextEditingValue local) {
    if (!local.selection.isValid ||
        local.selection.end > local.text.length ||
        (local.composing.isValid && local.composing.end > local.text.length)) {
      return null;
    }
    return TextEditingValue(
      text: source.text.replaceRange(start, end, local.text),
      selection: _selection(local.selection, start),
      composing: _range(local.composing, start),
    );
  }

  static TextSelection _selection(TextSelection s, int offset) => s.copyWith(
    baseOffset: s.baseOffset + offset,
    extentOffset: s.extentOffset + offset,
  );
  static TextRange _range(TextRange r, int offset) => r.isValid
      ? TextRange(start: r.start + offset, end: r.end + offset)
      : TextRange.empty;
}
