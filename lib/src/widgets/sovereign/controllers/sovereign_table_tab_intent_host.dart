part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

class _ControllerTableTabIntentHost implements TableTabIntentHost {
  _ControllerTableTabIntentHost(this._c);

  final SovereignController _c;

  @override
  TextEditingValue get value => _c.value;

  @override
  TextSelection get selection => _c.selection;

  @override
  set selection(TextSelection value) => _c.selection = value;

  @override
  LineIndex get lineIndex => _c._lineIndex;

  @override
  void commitProgrammaticTextEdit(TextEditingValue newValue) =>
      _c._commitProgrammaticTextEdit(newValue);

  @override
  bool isCaretInsideFence(String text, int caret) =>
      _c._fenceContextForCaret(text, caret, includeUnclosedEof: true) != null;

  @override
  ParsedTableLine? parseTableLineAt(String text, int line) =>
      _c._parseTableLineAt(text, line);

  @override
  bool tableRegionHasSeparator(String text, int line, int columnCount) =>
      _c._tableRegionHasSeparator(text, line, columnCount);

  @override
  int? tableCellIndexForCaret(ParsedTableLine row, int caret) =>
      _c._tableCellIndexForCaret(row, caret);

  @override
  ParsedTableLine? findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    bool skipSeparator = false,
  }) =>
      _c._findAdjacentTableLine(
        text: text,
        line: line,
        columnCount: columnCount,
        forward: forward,
        skipSeparator: skipSeparator,
      );

  @override
  String emptyRowTemplate(int columns, {required String indent}) =>
      _TablePolicy._emptyRowTemplate(columns, indent: indent);

  @override
  TableTabFormattingResult? formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) {
    final formatted = _TablePolicy._formatEstablishedTableAroundCaret(
      text,
      caret,
    );
    if (formatted == null) return null;
    return TableTabFormattingResult(
      text: formatted.text,
      caret: formatted.caret,
    );
  }
}
