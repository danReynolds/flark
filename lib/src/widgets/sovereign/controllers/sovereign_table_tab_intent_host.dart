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
      _c._structureQueries.fenceContextForCaret(
        text: text,
        caret: caret,
        lineIndex: _c._lineIndex,
        geometry: _c._geometry,
        includeUnclosedEof: true,
      ) !=
      null;

  @override
  ParsedTableLine? parseTableLineAt(String text, int line) =>
      _c._structureQueries.parseTableLineAt(
        text: text,
        line: line,
        lineIndex: _c._lineIndex,
        geometry: _c._geometry,
        rowShapeResolver: _c._structureQueries.matchTableRowShape,
      );

  @override
  bool tableRegionHasSeparator(String text, int line, int columnCount) =>
      _c._structureQueries.tableRegionHasSeparator(
        text: text,
        line: line,
        columnCount: columnCount,
        lineIndex: _c._lineIndex,
        parseLineAt: parseTableLineAt,
      );

  @override
  int? tableCellIndexForCaret(ParsedTableLine row, int caret) =>
      _c._structureQueries.tableCellIndexForCaret(row, caret);

  @override
  ParsedTableLine? findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    bool skipSeparator = false,
  }) =>
      _c._structureQueries.findAdjacentTableLine(
        text: text,
        line: line,
        columnCount: columnCount,
        forward: forward,
        skipSeparator: skipSeparator,
        lineIndex: _c._lineIndex,
        parseLineAt: parseTableLineAt,
      );

  @override
  String emptyRowTemplate(int columns, {required String indent}) =>
      _c._structureTransforms.emptyTableRowTemplate(columns, indent: indent);

  @override
  TableTabFormattingResult? formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) {
    final formatted = _c._structureTransforms.formatEstablishedTableAroundCaret(
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
