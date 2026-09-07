import 'dart:convert';
import 'dart:typed_data';

import 'backend.dart';
import 'model.dart';

enum CodeEditAction { insert, newline, indent, outdent }

/// One replacement and resulting selection, in code-body UTF-16 coordinates.
/// The host applies it in its own document/history transaction.
final class CodeEdit {
  CodeEdit._(
    this._source,
    this.start,
    this.end,
    this.text,
    this.base,
    this.extent,
  );
  final String _source;
  final int start, end;
  final String text;
  final int base, extent;

  String applyTo(String source) {
    if (source != _source) {
      throw StateError('Code edit belongs to a different source revision.');
    }
    return source.replaceRange(start, end, text);
  }
}

bool codePositionIsValid(String source, int position) =>
    position >= 0 &&
    position <= source.length &&
    (position == source.length ||
        source.codeUnitAt(position) < 0xdc00 ||
        source.codeUnitAt(position) > 0xdfff) &&
    !(position > 0 &&
        position < source.length &&
        source.codeUnitAt(position - 1) == 13 &&
        source.codeUnitAt(position) == 10);

CodeEdit decodeCodeEdit(Uint8List bytes, String source, CodeLanguage language) {
  try {
    final value = jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
    if (value['version'] != 4 || value['language'] != language.index) {
      throw const FormatException('response identity');
    }
    final start = value['start'] as int, end = value['end'] as int;
    final text = value['text'] as String;
    final base = value['base'] as int, extent = value['extent'] as int;
    validateCodeSource(text);
    if (start > end ||
        !codePositionIsValid(source, start) ||
        !codePositionIsValid(source, end)) {
      throw const FormatException('replacement range');
    }
    final candidate = source.replaceRange(start, end, text);
    if (!codePositionIsValid(candidate, base) ||
        !codePositionIsValid(candidate, extent)) {
      throw const FormatException('result selection');
    }
    return CodeEdit._(source, start, end, text, base, extent);
  } catch (e) {
    throw CodeException('Invalid edit response: $e');
  }
}
