import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_line_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/fence_context.dart'
    as structure;
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/list_marker_context.dart'
    as structure;
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/quote_context.dart'
    as structure;
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/task_marker_info.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/navigation_line_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/sovereign_navigation_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_line_parser.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_navigation_service.dart';
import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

class MarkdownStructureQueryService {
  const MarkdownStructureQueryService();

  static const SovereignNavigationHelpers _navigationHelpers =
      SovereignNavigationHelpers();

  int lineEndWithBreak({
    required LineIndex lineIndex,
    required String text,
    required int line,
  }) =>
      NavigationLineUtils.lineEndWithBreak(lineIndex, text, line);

  int lineContentEnd({
    required String text,
    required int lineStart,
    required int lineEndWithBreak,
  }) =>
      NavigationLineUtils.lineContentEnd(text, lineStart, lineEndWithBreak);

  structure.ListMarkerContext? listMarkerForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
        text,
        lineStart,
        lineEnd,
      );

  TaskMarkerInfo? taskMarkerInfo(String text, int markerEnd, int lineEnd) =>
      MarkdownLineHelpers.taskMarkerInfo(text, markerEnd, lineEnd);

  structure.ListMarkerContext? editableListMarkerForLine(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    final direct = listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (direct != null) return direct;

    var cursor = lineStart;
    while (cursor < lineEnd) {
      final cu = text.codeUnitAt(cursor);
      if (cu == 32 || cu == 9) {
        cursor++;
        continue;
      }
      break;
    }
    if (cursor == lineStart || cursor >= lineEnd) return null;

    return listMarkerForLineAllowingQuotePrefix(text, cursor, lineEnd);
  }

  structure.FenceContext? fenceContextForCaret({
    required String text,
    required int caret,
    required LineIndex lineIndex,
    required GeometryModel geometry,
    required bool includeUnclosedEof,
  }) =>
      _navigationHelpers.fenceContextForCaret(
        text: text,
        caret: caret,
        lineIndex: lineIndex,
        geometry: geometry,
        includeUnclosedEof: includeUnclosedEof,
      );

  structure.QuoteContext? quoteContextForLine({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) =>
      _navigationHelpers.quoteContextForLine(
        text: text,
        line: line,
        lineIndex: lineIndex,
        geometry: geometry,
      );

  bool isQuoteLineBodyBlank({
    required String text,
    required int line,
    required LineIndex lineIndex,
  }) =>
      _navigationHelpers.isQuoteLineBodyBlank(
        text: text,
        line: line,
        lineIndex: lineIndex,
      );

  bool isLineInsideFencedGeometry({
    required int lineStartOffset,
    required GeometryModel geometry,
  }) =>
      _navigationHelpers.isLineInsideFencedGeometry(
        lineStartOffset: lineStartOffset,
        geometry: geometry,
      );

  bool isUnclosedFenceAtEof({
    required String text,
    required MeasuredBlock block,
  }) =>
      _navigationHelpers.isUnclosedFenceAtEof(text: text, block: block);

  String? fenceLanguageForBlock({
    required String text,
    required int blockStartOffset,
  }) =>
      _navigationHelpers.fenceLanguageForBlock(
        text: text,
        blockStartOffset: blockStartOffset,
      );

  ParsedTableLine? parseTableLineAt({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required GeometryModel geometry,
    required TableRowShapeResolver rowShapeResolver,
  }) {
    return TableNavigationService.parseLineAt(
      text: text,
      line: line,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: (lineStart) => isLineInsideFencedGeometry(
        lineStartOffset: lineStart,
        geometry: geometry,
      ),
      rowShapeResolver: rowShapeResolver,
    );
  }

  TableLineShape? matchTableRowShape(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      TableLineParser.matchRowShape(text, lineStart, lineEnd);

  int? tableCellIndexForCaret(ParsedTableLine row, int caret) {
    return TableNavigationService.tableCellIndexForCaret(row, caret);
  }

  bool tableRegionHasSeparator({
    required String text,
    required int line,
    required int columnCount,
    required LineIndex lineIndex,
    required ParsedTableLine? Function(String text, int line) parseLineAt,
  }) {
    return TableNavigationService.tableRegionHasSeparator(
      text: text,
      line: line,
      columnCount: columnCount,
      lineIndex: lineIndex,
      parseLineAt: parseLineAt,
    );
  }

  ParsedTableLine? findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    required bool skipSeparator,
    required LineIndex lineIndex,
    required ParsedTableLine? Function(String text, int line) parseLineAt,
  }) {
    return TableNavigationService.findAdjacentTableLine(
      text: text,
      line: line,
      columnCount: columnCount,
      forward: forward,
      skipSeparator: skipSeparator,
      lineIndex: lineIndex,
      parseLineAt: parseLineAt,
    );
  }
}
