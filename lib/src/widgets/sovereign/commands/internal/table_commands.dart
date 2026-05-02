import 'package:flutter/services.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_command_editing_service.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_context.dart';
import 'command_transaction.dart';

abstract final class SovereignTableCommands {
  static const TableCommandEditingService _tableEditing =
      TableCommandEditingService();

  static SovereignCommandResult insertTable(
    SovereignController controller, {
    int columns = 2,
    int bodyRows = 1,
  }) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final mutation = _tableEditing.insertTable(
      text: context.text,
      selection: context.selection,
      columns: columns,
      bodyRows: bodyRows,
    );
    return _commit(context, mutation);
  }

  static SovereignCommandResult insertTableRowBelow(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final mutation = _tableEditing.insertRowBelow(
      text: context.text,
      selection: context.selection,
      lineIndex: controller.lineIndex,
      isLineInsideFencedGeometry: (lineStart) =>
          _isLineInsideFencedGeometry(controller, lineStart),
    );
    if (mutation == null) {
      return SovereignCommandNoOp.code(SovereignCommandReasonCode.noChange);
    }
    return _commit(context, mutation);
  }

  static SovereignCommandResult deleteTableRow(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final mutation = _tableEditing.deleteRow(
      text: context.text,
      selection: context.selection,
      lineIndex: controller.lineIndex,
      isLineInsideFencedGeometry: (lineStart) =>
          _isLineInsideFencedGeometry(controller, lineStart),
    );
    if (mutation == null) {
      return SovereignCommandNoOp.code(SovereignCommandReasonCode.noChange);
    }
    return _commit(context, mutation);
  }

  static SovereignCommandResult insertTableColumnRight(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final mutation = _tableEditing.insertColumnRight(
      text: context.text,
      selection: context.selection,
      lineIndex: controller.lineIndex,
      isLineInsideFencedGeometry: (lineStart) =>
          _isLineInsideFencedGeometry(controller, lineStart),
    );
    if (mutation == null) {
      return SovereignCommandNoOp.code(SovereignCommandReasonCode.noChange);
    }
    return _commit(context, mutation);
  }

  static SovereignCommandResult deleteTableColumn(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final mutation = _tableEditing.deleteColumn(
      text: context.text,
      selection: context.selection,
      lineIndex: controller.lineIndex,
      isLineInsideFencedGeometry: (lineStart) =>
          _isLineInsideFencedGeometry(controller, lineStart),
    );
    if (mutation == null) {
      return SovereignCommandNoOp.code(SovereignCommandReasonCode.noChange);
    }
    return _commit(context, mutation);
  }

  static SovereignCommandResult _commit(
    SovereignCommandContext context,
    TableCommandEditingResult mutation,
  ) {
    return commitCommandMutation(context, (
      text: mutation.text,
      selection: mutation.selection,
      composing: TextRange.empty,
    ));
  }

  static bool _isLineInsideFencedGeometry(
    SovereignController controller,
    int lineStartOffset,
  ) {
    for (final block in controller.geometry.codeBlocks) {
      if (lineStartOffset >= block.startOffset &&
          lineStartOffset < block.endOffset) {
        return true;
      }
    }
    return false;
  }
}
