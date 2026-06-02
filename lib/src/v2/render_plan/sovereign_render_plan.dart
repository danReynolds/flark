import '../core/extension/sovereign_extension.dart';
import '../core/selection/sovereign_selection.dart';
import '../core/transaction/sovereign_source_range.dart';
import '../core/transaction/sovereign_transaction.dart';
import '../markdown/parse/sovereign_markdown_parse_result.dart';
import '../projection/sovereign_projection.dart';

abstract base class SovereignRenderPlanExtension extends SovereignExtension {
  const SovereignRenderPlanExtension();

  SovereignRenderPlan transformRenderPlan(
    SovereignRenderPlanContext context,
  ) {
    return context.renderPlan;
  }
}

final class SovereignRenderPlanContext {
  const SovereignRenderPlanContext({
    required this.parseResult,
    required this.projection,
    required this.renderPlan,
  });

  final SovereignMarkdownParseResult parseResult;
  final SovereignProjection projection;
  final SovereignRenderPlan renderPlan;
}

final class SovereignRenderPlan {
  SovereignRenderPlan({
    required Iterable<SovereignRenderBlock> blocks,
    Map<String, Object?> metadata = const {},
  })  : blocks = List<SovereignRenderBlock>.unmodifiable(blocks),
        metadata = Map<String, Object?>.unmodifiable(metadata);

  factory SovereignRenderPlan.fromParseResult({
    required SovereignMarkdownParseResult parseResult,
    SovereignProjection? projection,
  }) {
    final effectiveProjection =
        projection ?? SovereignProjection.fromParseResult(parseResult);
    return SovereignRenderPlan(
      blocks: parseResult.blocks.map(
        (block) => SovereignRenderBlock.fromMarkdownBlock(
          block,
          projection: effectiveProjection,
          inlineTokens: parseResult.inlineTokens,
        ),
      ),
      metadata: {
        'schemaVersion': parseResult.schemaVersion,
        'revision': parseResult.revision,
      },
    );
  }

  final List<SovereignRenderBlock> blocks;
  final Map<String, Object?> metadata;

  Iterable<SovereignRenderBlock> get allBlocks sync* {
    for (final block in blocks) {
      yield* _blockAndDescendants(block);
    }
  }

  Iterable<SovereignRenderInlineRun> get allInlineRuns sync* {
    for (final block in allBlocks) {
      yield* block.inlineRuns;
    }
  }

  Iterable<SovereignRenderInlineRun> get linkRuns {
    return allInlineRuns.where(
      (run) => run.action?.kind == SovereignRenderInlineActionKind.link,
    );
  }

  Iterable<SovereignRenderInlineRun> get imageRuns {
    return allInlineRuns.where(
      (run) => run.action?.kind == SovereignRenderInlineActionKind.image,
    );
  }

  Iterable<SovereignRenderBlock> get tableBlocks {
    return allBlocks.where((block) => block.table != null);
  }

  Iterable<SovereignRenderBlock> get listItemBlocks {
    return allBlocks.where((block) => block.listItem != null);
  }

  Iterable<SovereignRenderBlock> get taskListItemBlocks {
    return allBlocks.where((block) => block.taskListItem != null);
  }

  Iterable<SovereignRenderBlock> get codeBlocks {
    return allBlocks.where((block) => block.codeBlock != null);
  }

  SovereignRenderBlock? blockAtDisplayOffset(int displayOffset) {
    SovereignRenderBlock? best;
    for (final block in allBlocks) {
      if (!block.displayRange.containsOffset(displayOffset)) continue;
      if (best == null ||
          block.displayRange.length < best.displayRange.length) {
        best = block;
      }
    }
    return best;
  }

  SovereignRenderInlineRun? inlineRunAtDisplayOffset(int displayOffset) {
    for (final run in allInlineRuns) {
      if (run.displayRange.containsOffset(displayOffset)) return run;
    }
    return null;
  }

  SovereignRenderOverlayPlan overlayPlan() {
    return SovereignRenderOverlayPlan.fromRenderPlan(this);
  }

  SovereignRenderPlan predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int revision,
    required int textLengthAfter,
  }) {
    final predictedBlocks = blocks
        .map(
          (block) => block.predictThroughTransaction(
            transaction: transaction,
            projection: projection,
            textLengthAfter: textLengthAfter,
          ),
        )
        .whereType<SovereignRenderBlock>();

    return SovereignRenderPlan(
      blocks: predictedBlocks,
      metadata: {
        ...metadata,
        'revision': revision,
        'stale': true,
        'predictive': true,
      },
    );
  }
}

SovereignRenderPlan applySovereignRenderPlanExtensions({
  required SovereignRenderPlan renderPlan,
  required SovereignMarkdownParseResult parseResult,
  required SovereignProjection projection,
  required SovereignExtensionSet extensions,
}) {
  var current = renderPlan;
  for (final extension
      in extensions.whereType<SovereignRenderPlanExtension>()) {
    current = extension.transformRenderPlan(
      SovereignRenderPlanContext(
        parseResult: parseResult,
        projection: projection,
        renderPlan: current,
      ),
    );
  }
  return current;
}

final class SovereignRenderBlock {
  SovereignRenderBlock({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.displayRange,
    required this.styleToken,
    required Iterable<SovereignRenderInlineRun> inlineRuns,
    required Iterable<SovereignRenderBlock> children,
    this.table,
    this.listItem,
    this.taskListItem,
    this.codeBlock,
    Map<String, Object?> attributes = const {},
  })  : inlineRuns = List<SovereignRenderInlineRun>.unmodifiable(inlineRuns),
        children = List<SovereignRenderBlock>.unmodifiable(children),
        attributes = Map<String, Object?>.unmodifiable(attributes);

  factory SovereignRenderBlock.fromMarkdownBlock(
    SovereignMarkdownBlockNode block, {
    required SovereignProjection projection,
    required Iterable<SovereignMarkdownInlineToken> inlineTokens,
  }) {
    final blockInlineTokens = _directInlineTokens(block, inlineTokens);

    return SovereignRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: block.sourceRange,
      displayRange: _displayRange(projection, block.sourceRange),
      styleToken: _blockStyleToken(block),
      inlineRuns: blockInlineTokens.map(
        (token) => SovereignRenderInlineRun.fromMarkdownInline(
          token,
          projection: projection,
        ),
      ),
      children: block.children.map(
        (child) => SovereignRenderBlock.fromMarkdownBlock(
          child,
          projection: projection,
          inlineTokens: inlineTokens,
        ),
      ),
      table: _tableDescriptor(block, projection),
      listItem: _listItemDescriptor(block),
      taskListItem: _taskListItemDescriptor(block),
      codeBlock: _codeBlockDescriptor(block),
      attributes: block.attributes,
    );
  }

  final SovereignMarkdownBlockKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final SovereignSourceRange displayRange;
  final SovereignRenderTextStyleToken styleToken;
  final List<SovereignRenderInlineRun> inlineRuns;
  final List<SovereignRenderBlock> children;
  final SovereignRenderTableDescriptor? table;
  final SovereignRenderListItemDescriptor? listItem;
  final SovereignRenderTaskListItemDescriptor? taskListItem;
  final SovereignRenderCodeBlockDescriptor? codeBlock;
  final Map<String, Object?> attributes;

  SovereignRenderBlock? predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    final predictedTable = table?.predictThroughTransaction(
      transaction: transaction,
      projection: projection,
      textLengthAfter: textLengthAfter,
    );

    return copyWithPredictedRanges(
      sourceRange: predictedSourceRange,
      displayRange: _displayRange(projection, predictedSourceRange),
      inlineRuns: inlineRuns
          .map(
            (run) => run.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<SovereignRenderInlineRun>(),
      children: children
          .map(
            (child) => child.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<SovereignRenderBlock>(),
      table: predictedTable,
    );
  }

  SovereignRenderBlock copyWithPredictedRanges({
    required SovereignSourceRange sourceRange,
    required SovereignSourceRange displayRange,
    required Iterable<SovereignRenderInlineRun> inlineRuns,
    required Iterable<SovereignRenderBlock> children,
    required SovereignRenderTableDescriptor? table,
  }) {
    return SovereignRenderBlock(
      kind: kind,
      type: type,
      sourceRange: sourceRange,
      displayRange: displayRange,
      styleToken: styleToken,
      inlineRuns: inlineRuns,
      children: children,
      table: table,
      listItem: listItem,
      taskListItem: taskListItem,
      codeBlock: codeBlock,
      attributes: attributes,
    );
  }
}

final class SovereignRenderInlineRun {
  SovereignRenderInlineRun({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.displayRange,
    required this.styleToken,
    this.action,
    Map<String, Object?> attributes = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes);

  factory SovereignRenderInlineRun.fromMarkdownInline(
    SovereignMarkdownInlineToken token, {
    required SovereignProjection projection,
  }) {
    return SovereignRenderInlineRun(
      kind: token.kind,
      type: token.type,
      sourceRange: token.sourceRange,
      displayRange: _displayRange(projection, token.sourceRange),
      styleToken: _inlineStyleToken(token),
      action: _inlineActionDescriptor(token),
      attributes: token.attributes,
    );
  }

  final SovereignMarkdownInlineKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final SovereignSourceRange displayRange;
  final SovereignRenderTextStyleToken styleToken;
  final SovereignRenderInlineActionDescriptor? action;
  final Map<String, Object?> attributes;

  SovereignRenderInlineRun? predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return SovereignRenderInlineRun(
      kind: kind,
      type: type,
      sourceRange: predictedSourceRange,
      displayRange: _displayRange(projection, predictedSourceRange),
      styleToken: styleToken,
      action: action,
      attributes: attributes,
    );
  }
}

enum SovereignRenderTextStyleToken {
  body,
  heading1,
  heading2,
  heading3,
  heading4,
  heading5,
  heading6,
  emphasis,
  strong,
  inlineCode,
  strikethrough,
  link,
  image,
  rawHtml,
  unknown,
}

enum SovereignRenderTableColumnAlignment {
  none,
  left,
  center,
  right,
  unknown,
}

final class SovereignRenderTableDescriptor {
  SovereignRenderTableDescriptor({
    required Iterable<SovereignRenderTableColumnAlignment> columnAlignments,
    Iterable<SovereignRenderTableRowDescriptor> rows = const [],
  })  : columnAlignments =
            List<SovereignRenderTableColumnAlignment>.unmodifiable(
          columnAlignments,
        ),
        rows = List<SovereignRenderTableRowDescriptor>.unmodifiable(rows);

  final List<SovereignRenderTableColumnAlignment> columnAlignments;
  final List<SovereignRenderTableRowDescriptor> rows;

  SovereignRenderTableDescriptor predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int textLengthAfter,
  }) {
    return SovereignRenderTableDescriptor(
      columnAlignments: columnAlignments,
      rows: rows
          .map(
            (row) => row.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<SovereignRenderTableRowDescriptor>(),
    );
  }
}

final class SovereignRenderTableRowDescriptor {
  SovereignRenderTableRowDescriptor({
    required this.header,
    required this.sourceRange,
    required this.displayRange,
    required Iterable<SovereignRenderTableCellDescriptor> cells,
  }) : cells = List<SovereignRenderTableCellDescriptor>.unmodifiable(cells);

  final bool header;
  final SovereignSourceRange sourceRange;
  final SovereignSourceRange displayRange;
  final List<SovereignRenderTableCellDescriptor> cells;

  SovereignRenderTableRowDescriptor? predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictDescriptorRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return SovereignRenderTableRowDescriptor(
      header: header,
      sourceRange: predictedSourceRange,
      displayRange: _displayRange(projection, predictedSourceRange),
      cells: cells
          .map(
            (cell) => cell.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<SovereignRenderTableCellDescriptor>(),
    );
  }
}

final class SovereignRenderTableCellDescriptor {
  const SovereignRenderTableCellDescriptor({
    required this.sourceRange,
    required this.displayRange,
  });

  final SovereignSourceRange sourceRange;
  final SovereignSourceRange displayRange;

  SovereignRenderTableCellDescriptor? predictThroughTransaction({
    required SovereignTransaction transaction,
    required SovereignProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictDescriptorRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return SovereignRenderTableCellDescriptor(
      sourceRange: predictedSourceRange,
      displayRange: _displayRange(projection, predictedSourceRange),
    );
  }
}

final class SovereignRenderTaskListItemDescriptor {
  const SovereignRenderTaskListItemDescriptor({
    required this.checked,
  });

  final bool checked;
}

enum SovereignRenderListKind {
  unordered,
  ordered,
  unknown,
}

final class SovereignRenderListItemDescriptor {
  const SovereignRenderListItemDescriptor({
    required this.kind,
  });

  final SovereignRenderListKind kind;
}

final class SovereignRenderCodeBlockDescriptor {
  const SovereignRenderCodeBlockDescriptor({
    this.language,
  });

  final String? language;
}

enum SovereignRenderInlineActionKind {
  link,
  image,
  unknown,
}

final class SovereignRenderInlineActionDescriptor {
  const SovereignRenderInlineActionDescriptor({
    required this.kind,
    required this.destination,
    this.title,
    this.label,
  });

  final SovereignRenderInlineActionKind kind;
  final String destination;
  final String? title;
  final String? label;
}

enum SovereignRenderOverlayKind {
  link,
  image,
  taskListItem,
  table,
  codeBlock,
}

final class SovereignRenderOverlayTarget {
  const SovereignRenderOverlayTarget({
    required this.kind,
    required this.sourceRange,
    required this.displayRange,
    this.action,
    this.taskListItem,
    this.table,
    this.codeBlock,
  });

  final SovereignRenderOverlayKind kind;
  final SovereignSourceRange sourceRange;
  final SovereignSourceRange displayRange;
  final SovereignRenderInlineActionDescriptor? action;
  final SovereignRenderTaskListItemDescriptor? taskListItem;
  final SovereignRenderTableDescriptor? table;
  final SovereignRenderCodeBlockDescriptor? codeBlock;
}

final class SovereignRenderOverlayPlan {
  SovereignRenderOverlayPlan({
    required Iterable<SovereignRenderOverlayTarget> targets,
  }) : targets = List<SovereignRenderOverlayTarget>.unmodifiable(targets);

  factory SovereignRenderOverlayPlan.fromRenderPlan(
    SovereignRenderPlan renderPlan,
  ) {
    return SovereignRenderOverlayPlan(
      targets: [
        for (final run in renderPlan.linkRuns)
          SovereignRenderOverlayTarget(
            kind: SovereignRenderOverlayKind.link,
            sourceRange: run.sourceRange,
            displayRange: run.displayRange,
            action: run.action,
          ),
        for (final run in renderPlan.imageRuns)
          SovereignRenderOverlayTarget(
            kind: SovereignRenderOverlayKind.image,
            sourceRange: run.sourceRange,
            displayRange: run.displayRange,
            action: run.action,
          ),
        for (final block in renderPlan.taskListItemBlocks)
          SovereignRenderOverlayTarget(
            kind: SovereignRenderOverlayKind.taskListItem,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            taskListItem: block.taskListItem,
          ),
        for (final block in renderPlan.tableBlocks)
          SovereignRenderOverlayTarget(
            kind: SovereignRenderOverlayKind.table,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            table: block.table,
          ),
        for (final block in renderPlan.codeBlocks)
          SovereignRenderOverlayTarget(
            kind: SovereignRenderOverlayKind.codeBlock,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            codeBlock: block.codeBlock,
          ),
      ],
    );
  }

  final List<SovereignRenderOverlayTarget> targets;

  Iterable<SovereignRenderOverlayTarget> ofKind(
    SovereignRenderOverlayKind kind,
  ) {
    return targets.where((target) => target.kind == kind);
  }
}

SovereignSourceRange _displayRange(
  SovereignProjection projection,
  SovereignSourceRange sourceRange,
) {
  return SovereignSourceRange(
    projection.sourceToDisplayOffset(sourceRange.start),
    projection.sourceToDisplayOffset(sourceRange.end),
  );
}

SovereignSourceRange? _predictRangeThroughTransaction(
  SovereignSourceRange range, {
  required SovereignTransaction transaction,
  required int textLengthAfter,
}) {
  if (transaction.operations.any(
    (operation) =>
        !operation.replacedRange.isCollapsed &&
        operation.replacedRange.containsRange(range),
  )) {
    return null;
  }

  final start = transaction.mapOffset(
    range.start,
    affinity: SovereignMapAffinity.upstream,
  );
  final end = transaction.mapOffset(
    range.end,
    affinity: SovereignMapAffinity.downstream,
  );
  if (start < 0 || end > textLengthAfter || start >= end) return null;
  return SovereignSourceRange(start, end);
}

SovereignSourceRange? _predictDescriptorRangeThroughTransaction(
  SovereignSourceRange range, {
  required SovereignTransaction transaction,
  required int textLengthAfter,
}) {
  final start = transaction.mapOffset(
    range.start,
    affinity: SovereignMapAffinity.upstream,
  );
  final end = transaction.mapOffset(
    range.end,
    affinity: SovereignMapAffinity.downstream,
  );
  if (start < 0 || end > textLengthAfter || start > end) return null;
  return SovereignSourceRange(start, end);
}

Iterable<SovereignMarkdownInlineToken> _directInlineTokens(
  SovereignMarkdownBlockNode block,
  Iterable<SovereignMarkdownInlineToken> inlineTokens,
) {
  return inlineTokens.where((token) {
    if (!_contains(block.sourceRange, token.sourceRange)) return false;
    return !block.children.any(
      (child) => _contains(child.sourceRange, token.sourceRange),
    );
  });
}

bool _contains(SovereignSourceRange outer, SovereignSourceRange inner) {
  return inner.start >= outer.start && inner.end <= outer.end;
}

SovereignRenderTableDescriptor? _tableDescriptor(
  SovereignMarkdownBlockNode block,
  SovereignProjection projection,
) {
  if (block.kind != SovereignMarkdownBlockKind.table) return null;
  final alignments = block.attributes['alignments'];
  if (alignments is! List) {
    return SovereignRenderTableDescriptor(
      columnAlignments: const [],
      rows: _tableRows(block, projection),
    );
  }
  return SovereignRenderTableDescriptor(
    columnAlignments: alignments.map(_tableAlignment),
    rows: _tableRows(block, projection),
  );
}

Iterable<SovereignRenderTableRowDescriptor> _tableRows(
  SovereignMarkdownBlockNode table,
  SovereignProjection projection,
) {
  return table.children
      .where((row) => row.kind == SovereignMarkdownBlockKind.tableRow)
      .map(
        (row) => SovereignRenderTableRowDescriptor(
          header: row.attributes['header'] == true,
          sourceRange: row.sourceRange,
          displayRange: _displayRange(projection, row.sourceRange),
          cells: row.children
              .where(
                  (cell) => cell.kind == SovereignMarkdownBlockKind.tableCell)
              .map(
                (cell) => SovereignRenderTableCellDescriptor(
                  sourceRange: cell.sourceRange,
                  displayRange: _displayRange(projection, cell.sourceRange),
                ),
              ),
        ),
      );
}

SovereignRenderTableColumnAlignment _tableAlignment(Object? value) {
  return switch (value) {
    'left' => SovereignRenderTableColumnAlignment.left,
    'center' => SovereignRenderTableColumnAlignment.center,
    'right' => SovereignRenderTableColumnAlignment.right,
    'none' || null => SovereignRenderTableColumnAlignment.none,
    _ => SovereignRenderTableColumnAlignment.unknown,
  };
}

SovereignRenderTaskListItemDescriptor? _taskListItemDescriptor(
  SovereignMarkdownBlockNode block,
) {
  if (block.kind != SovereignMarkdownBlockKind.listItem) return null;
  final checked = block.attributes['checked'];
  if (checked is! bool) return null;
  return SovereignRenderTaskListItemDescriptor(checked: checked);
}

SovereignRenderListItemDescriptor? _listItemDescriptor(
  SovereignMarkdownBlockNode block,
) {
  if (block.kind != SovereignMarkdownBlockKind.listItem) return null;
  return SovereignRenderListItemDescriptor(
    kind: switch (block.attributes['listKind']) {
      'unordered' => SovereignRenderListKind.unordered,
      'ordered' => SovereignRenderListKind.ordered,
      _ => SovereignRenderListKind.unknown,
    },
  );
}

SovereignRenderCodeBlockDescriptor? _codeBlockDescriptor(
  SovereignMarkdownBlockNode block,
) {
  if (block.kind != SovereignMarkdownBlockKind.codeBlock) return null;
  return SovereignRenderCodeBlockDescriptor(
    language: _stringAttribute(block.attributes, 'language') ??
        _stringAttribute(block.attributes, 'fenceInfo'),
  );
}

SovereignRenderInlineActionDescriptor? _inlineActionDescriptor(
  SovereignMarkdownInlineToken token,
) {
  final destination = _stringAttribute(token.attributes, 'destination') ??
      _stringAttribute(token.attributes, 'href') ??
      _stringAttribute(token.attributes, 'src');
  if (destination == null || destination.isEmpty) return null;

  return switch (token.kind) {
    SovereignMarkdownInlineKind.link => SovereignRenderInlineActionDescriptor(
        kind: SovereignRenderInlineActionKind.link,
        destination: destination,
        title: _stringAttribute(token.attributes, 'title'),
        label: _stringAttribute(token.attributes, 'label'),
      ),
    SovereignMarkdownInlineKind.image => SovereignRenderInlineActionDescriptor(
        kind: SovereignRenderInlineActionKind.image,
        destination: destination,
        title: _stringAttribute(token.attributes, 'title'),
        label: _stringAttribute(token.attributes, 'alt') ??
            _stringAttribute(token.attributes, 'label'),
      ),
    _ => null,
  };
}

String? _stringAttribute(Map<String, Object?> attributes, String key) {
  final value = attributes[key];
  return value is String ? value : null;
}

SovereignRenderTextStyleToken _blockStyleToken(
  SovereignMarkdownBlockNode block,
) {
  if (block.kind != SovereignMarkdownBlockKind.heading) {
    return SovereignRenderTextStyleToken.body;
  }
  final level = block.attributes['level'];
  return switch (level) {
    1 => SovereignRenderTextStyleToken.heading1,
    2 => SovereignRenderTextStyleToken.heading2,
    3 => SovereignRenderTextStyleToken.heading3,
    4 => SovereignRenderTextStyleToken.heading4,
    5 => SovereignRenderTextStyleToken.heading5,
    6 => SovereignRenderTextStyleToken.heading6,
    _ => SovereignRenderTextStyleToken.heading1,
  };
}

SovereignRenderTextStyleToken _inlineStyleToken(
  SovereignMarkdownInlineToken token,
) {
  return switch (token.kind) {
    SovereignMarkdownInlineKind.emphasis =>
      SovereignRenderTextStyleToken.emphasis,
    SovereignMarkdownInlineKind.strong => SovereignRenderTextStyleToken.strong,
    SovereignMarkdownInlineKind.inlineCode =>
      SovereignRenderTextStyleToken.inlineCode,
    SovereignMarkdownInlineKind.strikethrough =>
      SovereignRenderTextStyleToken.strikethrough,
    SovereignMarkdownInlineKind.link => SovereignRenderTextStyleToken.link,
    SovereignMarkdownInlineKind.image => SovereignRenderTextStyleToken.image,
    SovereignMarkdownInlineKind.htmlInline =>
      SovereignRenderTextStyleToken.rawHtml,
    SovereignMarkdownInlineKind.unknown =>
      SovereignRenderTextStyleToken.unknown,
    _ => SovereignRenderTextStyleToken.body,
  };
}

Iterable<SovereignRenderBlock> _blockAndDescendants(
  SovereignRenderBlock block,
) sync* {
  yield block;
  for (final child in block.children) {
    yield* _blockAndDescendants(child);
  }
}
