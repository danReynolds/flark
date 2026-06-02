import '../core/extension/sovereign_extension.dart';
import '../core/selection/sovereign_selection.dart';
import '../core/transaction/sovereign_source_range.dart';
import '../core/transaction/sovereign_transaction.dart';
import '../markdown/parse/sovereign_markdown_parse_result.dart';
import '../projection/sovereign_projection.dart';

abstract base class FlarkRenderPlanExtension extends FlarkExtension {
  const FlarkRenderPlanExtension();

  FlarkRenderPlan transformRenderPlan(FlarkRenderPlanContext context) {
    return context.renderPlan;
  }
}

final class FlarkRenderPlanContext {
  const FlarkRenderPlanContext({
    required this.parseResult,
    required this.projection,
    required this.renderPlan,
  });

  final FlarkMarkdownParseResult parseResult;
  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}

final class FlarkRenderPlan {
  FlarkRenderPlan({
    required Iterable<FlarkRenderBlock> blocks,
    Map<String, Object?> metadata = const {},
  }) : blocks = List<FlarkRenderBlock>.unmodifiable(blocks),
       metadata = Map<String, Object?>.unmodifiable(metadata);

  factory FlarkRenderPlan.fromParseResult({
    required FlarkMarkdownParseResult parseResult,
    FlarkProjection? projection,
  }) {
    final effectiveProjection =
        projection ?? FlarkProjection.fromParseResult(parseResult);
    final inlineTokens = parseResult.inlineTokens.toList(growable: false)
      ..sort((left, right) {
        final startComparison = left.sourceRange.start.compareTo(
          right.sourceRange.start,
        );
        if (startComparison != 0) return startComparison;
        return left.sourceRange.end.compareTo(right.sourceRange.end);
      });
    return FlarkRenderPlan(
      blocks: _renderBlocksFromMarkdown(
        parseResult.blocks,
        projection: effectiveProjection,
        inlineTokens: inlineTokens,
      ),
      metadata: {
        'schemaVersion': parseResult.schemaVersion,
        'revision': parseResult.revision,
      },
    );
  }

  final List<FlarkRenderBlock> blocks;
  final Map<String, Object?> metadata;

  Iterable<FlarkRenderBlock> get allBlocks sync* {
    for (final block in blocks) {
      yield* _blockAndDescendants(block);
    }
  }

  Iterable<FlarkRenderInlineRun> get allInlineRuns sync* {
    for (final block in allBlocks) {
      yield* block.inlineRuns;
    }
  }

  Iterable<FlarkRenderInlineRun> get linkRuns {
    return allInlineRuns.where(
      (run) => run.action?.kind == FlarkRenderInlineActionKind.link,
    );
  }

  Iterable<FlarkRenderInlineRun> get imageRuns {
    return allInlineRuns.where(
      (run) => run.action?.kind == FlarkRenderInlineActionKind.image,
    );
  }

  Iterable<FlarkRenderBlock> get tableBlocks {
    return allBlocks.where((block) => block.table != null);
  }

  Iterable<FlarkRenderBlock> get listItemBlocks {
    return allBlocks.where((block) => block.listItem != null);
  }

  Iterable<FlarkRenderBlock> get taskListItemBlocks {
    return allBlocks.where((block) => block.taskListItem != null);
  }

  Iterable<FlarkRenderBlock> get codeBlocks {
    return allBlocks.where((block) => block.codeBlock != null);
  }

  FlarkRenderBlock? blockAtDisplayOffset(int displayOffset) {
    FlarkRenderBlock? best;
    for (final block in allBlocks) {
      if (!block.displayRange.containsOffset(displayOffset)) continue;
      if (best == null ||
          block.displayRange.length < best.displayRange.length) {
        best = block;
      }
    }
    return best;
  }

  FlarkRenderInlineRun? inlineRunAtDisplayOffset(int displayOffset) {
    for (final run in allInlineRuns) {
      if (run.displayRange.containsOffset(displayOffset)) return run;
    }
    return null;
  }

  FlarkRenderOverlayPlan overlayPlan() {
    return FlarkRenderOverlayPlan.fromRenderPlan(this);
  }

  FlarkRenderPlan predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
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
        .whereType<FlarkRenderBlock>();

    return FlarkRenderPlan(
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

FlarkRenderPlan applyFlarkRenderPlanExtensions({
  required FlarkRenderPlan renderPlan,
  required FlarkMarkdownParseResult parseResult,
  required FlarkProjection projection,
  required FlarkExtensionSet extensions,
}) {
  var current = renderPlan;
  for (final extension in extensions.whereType<FlarkRenderPlanExtension>()) {
    current = extension.transformRenderPlan(
      FlarkRenderPlanContext(
        parseResult: parseResult,
        projection: projection,
        renderPlan: current,
      ),
    );
  }
  return current;
}

final class FlarkRenderBlock {
  FlarkRenderBlock({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.displayRange,
    required this.styleToken,
    required Iterable<FlarkRenderInlineRun> inlineRuns,
    required Iterable<FlarkRenderBlock> children,
    this.table,
    this.listItem,
    this.taskListItem,
    this.codeBlock,
    Map<String, Object?> attributes = const {},
  }) : inlineRuns = List<FlarkRenderInlineRun>.unmodifiable(inlineRuns),
       children = List<FlarkRenderBlock>.unmodifiable(children),
       attributes = Map<String, Object?>.unmodifiable(attributes);

  factory FlarkRenderBlock.fromMarkdownBlock(
    FlarkMarkdownBlockNode block, {
    required FlarkProjection projection,
    required Iterable<FlarkMarkdownInlineToken> inlineTokens,
  }) {
    final sortedInlineTokens = inlineTokens.toList(growable: false)
      ..sort((left, right) {
        final startComparison = left.sourceRange.start.compareTo(
          right.sourceRange.start,
        );
        if (startComparison != 0) return startComparison;
        return left.sourceRange.end.compareTo(right.sourceRange.end);
      });
    return _renderBlockFromMarkdown(
      block,
      projection: projection,
      inlineTokens: sortedInlineTokens,
    );
  }

  final FlarkMarkdownBlockKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final FlarkSourceRange displayRange;
  final FlarkRenderTextStyleToken styleToken;
  final List<FlarkRenderInlineRun> inlineRuns;
  final List<FlarkRenderBlock> children;
  final FlarkRenderTableDescriptor? table;
  final FlarkRenderListItemDescriptor? listItem;
  final FlarkRenderTaskListItemDescriptor? taskListItem;
  final FlarkRenderCodeBlockDescriptor? codeBlock;
  final Map<String, Object?> attributes;

  FlarkRenderBlock? predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
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
          .whereType<FlarkRenderInlineRun>(),
      children: children
          .map(
            (child) => child.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<FlarkRenderBlock>(),
      table: predictedTable,
    );
  }

  FlarkRenderBlock copyWithPredictedRanges({
    required FlarkSourceRange sourceRange,
    required FlarkSourceRange displayRange,
    required Iterable<FlarkRenderInlineRun> inlineRuns,
    required Iterable<FlarkRenderBlock> children,
    required FlarkRenderTableDescriptor? table,
  }) {
    return FlarkRenderBlock(
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

final class FlarkRenderInlineRun {
  FlarkRenderInlineRun({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.displayRange,
    required this.styleToken,
    this.action,
    Map<String, Object?> attributes = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes);

  factory FlarkRenderInlineRun.fromMarkdownInline(
    FlarkMarkdownInlineToken token, {
    required FlarkProjection projection,
  }) {
    return FlarkRenderInlineRun(
      kind: token.kind,
      type: token.type,
      sourceRange: token.sourceRange,
      displayRange: _displayRange(projection, token.sourceRange),
      styleToken: _inlineStyleToken(token),
      action: _inlineActionDescriptor(token),
      attributes: token.attributes,
    );
  }

  final FlarkMarkdownInlineKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final FlarkSourceRange displayRange;
  final FlarkRenderTextStyleToken styleToken;
  final FlarkRenderInlineActionDescriptor? action;
  final Map<String, Object?> attributes;

  FlarkRenderInlineRun? predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return FlarkRenderInlineRun(
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

enum FlarkRenderTextStyleToken {
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

enum FlarkRenderTableColumnAlignment { none, left, center, right, unknown }

final class FlarkRenderTableDescriptor {
  FlarkRenderTableDescriptor({
    required Iterable<FlarkRenderTableColumnAlignment> columnAlignments,
    Iterable<FlarkRenderTableRowDescriptor> rows = const [],
  }) : columnAlignments = List<FlarkRenderTableColumnAlignment>.unmodifiable(
         columnAlignments,
       ),
       rows = List<FlarkRenderTableRowDescriptor>.unmodifiable(rows);

  final List<FlarkRenderTableColumnAlignment> columnAlignments;
  final List<FlarkRenderTableRowDescriptor> rows;

  FlarkRenderTableDescriptor predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int textLengthAfter,
  }) {
    return FlarkRenderTableDescriptor(
      columnAlignments: columnAlignments,
      rows: rows
          .map(
            (row) => row.predictThroughTransaction(
              transaction: transaction,
              projection: projection,
              textLengthAfter: textLengthAfter,
            ),
          )
          .whereType<FlarkRenderTableRowDescriptor>(),
    );
  }
}

final class FlarkRenderTableRowDescriptor {
  FlarkRenderTableRowDescriptor({
    required this.header,
    required this.sourceRange,
    required this.displayRange,
    required Iterable<FlarkRenderTableCellDescriptor> cells,
  }) : cells = List<FlarkRenderTableCellDescriptor>.unmodifiable(cells);

  final bool header;
  final FlarkSourceRange sourceRange;
  final FlarkSourceRange displayRange;
  final List<FlarkRenderTableCellDescriptor> cells;

  FlarkRenderTableRowDescriptor? predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictDescriptorRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return FlarkRenderTableRowDescriptor(
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
          .whereType<FlarkRenderTableCellDescriptor>(),
    );
  }
}

final class FlarkRenderTableCellDescriptor {
  const FlarkRenderTableCellDescriptor({
    required this.sourceRange,
    required this.displayRange,
  });

  final FlarkSourceRange sourceRange;
  final FlarkSourceRange displayRange;

  FlarkRenderTableCellDescriptor? predictThroughTransaction({
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int textLengthAfter,
  }) {
    final predictedSourceRange = _predictDescriptorRangeThroughTransaction(
      sourceRange,
      transaction: transaction,
      textLengthAfter: textLengthAfter,
    );
    if (predictedSourceRange == null) return null;

    return FlarkRenderTableCellDescriptor(
      sourceRange: predictedSourceRange,
      displayRange: _displayRange(projection, predictedSourceRange),
    );
  }
}

final class FlarkRenderTaskListItemDescriptor {
  const FlarkRenderTaskListItemDescriptor({required this.checked});

  final bool checked;
}

enum FlarkRenderListKind { unordered, ordered, unknown }

final class FlarkRenderListItemDescriptor {
  const FlarkRenderListItemDescriptor({required this.kind});

  final FlarkRenderListKind kind;
}

final class FlarkRenderCodeBlockDescriptor {
  const FlarkRenderCodeBlockDescriptor({this.language});

  final String? language;
}

enum FlarkRenderInlineActionKind { link, image, unknown }

final class FlarkRenderInlineActionDescriptor {
  const FlarkRenderInlineActionDescriptor({
    required this.kind,
    required this.destination,
    this.title,
    this.label,
  });

  final FlarkRenderInlineActionKind kind;
  final String destination;
  final String? title;
  final String? label;
}

enum FlarkRenderOverlayKind { link, image, taskListItem, table, codeBlock }

final class FlarkRenderOverlayTarget {
  const FlarkRenderOverlayTarget({
    required this.kind,
    required this.sourceRange,
    required this.displayRange,
    this.action,
    this.taskListItem,
    this.table,
    this.codeBlock,
  });

  final FlarkRenderOverlayKind kind;
  final FlarkSourceRange sourceRange;
  final FlarkSourceRange displayRange;
  final FlarkRenderInlineActionDescriptor? action;
  final FlarkRenderTaskListItemDescriptor? taskListItem;
  final FlarkRenderTableDescriptor? table;
  final FlarkRenderCodeBlockDescriptor? codeBlock;
}

final class FlarkRenderOverlayPlan {
  FlarkRenderOverlayPlan({required Iterable<FlarkRenderOverlayTarget> targets})
    : targets = List<FlarkRenderOverlayTarget>.unmodifiable(targets);

  factory FlarkRenderOverlayPlan.fromRenderPlan(FlarkRenderPlan renderPlan) {
    return FlarkRenderOverlayPlan(
      targets: [
        for (final run in renderPlan.linkRuns)
          FlarkRenderOverlayTarget(
            kind: FlarkRenderOverlayKind.link,
            sourceRange: run.sourceRange,
            displayRange: run.displayRange,
            action: run.action,
          ),
        for (final run in renderPlan.imageRuns)
          FlarkRenderOverlayTarget(
            kind: FlarkRenderOverlayKind.image,
            sourceRange: run.sourceRange,
            displayRange: run.displayRange,
            action: run.action,
          ),
        for (final block in renderPlan.taskListItemBlocks)
          FlarkRenderOverlayTarget(
            kind: FlarkRenderOverlayKind.taskListItem,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            taskListItem: block.taskListItem,
          ),
        for (final block in renderPlan.tableBlocks)
          FlarkRenderOverlayTarget(
            kind: FlarkRenderOverlayKind.table,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            table: block.table,
          ),
        for (final block in renderPlan.codeBlocks)
          FlarkRenderOverlayTarget(
            kind: FlarkRenderOverlayKind.codeBlock,
            sourceRange: block.sourceRange,
            displayRange: block.displayRange,
            codeBlock: block.codeBlock,
          ),
      ],
    );
  }

  final List<FlarkRenderOverlayTarget> targets;

  Iterable<FlarkRenderOverlayTarget> ofKind(FlarkRenderOverlayKind kind) {
    return targets.where((target) => target.kind == kind);
  }
}

FlarkSourceRange _displayRange(
  FlarkProjection projection,
  FlarkSourceRange sourceRange,
) {
  return FlarkSourceRange(
    projection.sourceToDisplayOffset(sourceRange.start),
    projection.sourceToDisplayOffset(sourceRange.end),
  );
}

FlarkSourceRange? _predictRangeThroughTransaction(
  FlarkSourceRange range, {
  required FlarkTransaction transaction,
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
    affinity: FlarkMapAffinity.upstream,
  );
  final end = transaction.mapOffset(
    range.end,
    affinity: FlarkMapAffinity.downstream,
  );
  if (start < 0 || end > textLengthAfter || start >= end) return null;
  return FlarkSourceRange(start, end);
}

FlarkSourceRange? _predictDescriptorRangeThroughTransaction(
  FlarkSourceRange range, {
  required FlarkTransaction transaction,
  required int textLengthAfter,
}) {
  final start = transaction.mapOffset(
    range.start,
    affinity: FlarkMapAffinity.upstream,
  );
  final end = transaction.mapOffset(
    range.end,
    affinity: FlarkMapAffinity.downstream,
  );
  if (start < 0 || end > textLengthAfter || start > end) return null;
  return FlarkSourceRange(start, end);
}

List<FlarkRenderBlock> _renderBlocksFromMarkdown(
  Iterable<FlarkMarkdownBlockNode> blocks, {
  required FlarkProjection projection,
  required List<FlarkMarkdownInlineToken> inlineTokens,
}) {
  final blockList = blocks.toList(growable: false);
  final partition = _partitionInlineTokens(blockList, inlineTokens);
  return [
    for (var i = 0; i < blockList.length; i++)
      _renderBlockFromMarkdown(
        blockList[i],
        projection: projection,
        inlineTokens: partition.buckets[i],
      ),
  ];
}

FlarkRenderBlock _renderBlockFromMarkdown(
  FlarkMarkdownBlockNode block, {
  required FlarkProjection projection,
  required List<FlarkMarkdownInlineToken> inlineTokens,
}) {
  final children = block.children.toList(growable: false);
  final childPartition = _partitionInlineTokens(children, inlineTokens);

  return FlarkRenderBlock(
    kind: block.kind,
    type: block.type,
    sourceRange: block.sourceRange,
    displayRange: _displayRange(projection, block.sourceRange),
    styleToken: _blockStyleToken(block),
    inlineRuns: childPartition.unassigned.map(
      (token) => FlarkRenderInlineRun.fromMarkdownInline(
        token,
        projection: projection,
      ),
    ),
    children: [
      for (var i = 0; i < children.length; i++)
        _renderBlockFromMarkdown(
          children[i],
          projection: projection,
          inlineTokens: childPartition.buckets[i],
        ),
    ],
    table: _tableDescriptor(block, projection),
    listItem: _listItemDescriptor(block),
    taskListItem: _taskListItemDescriptor(block),
    codeBlock: _codeBlockDescriptor(block),
    attributes: block.attributes,
  );
}

_InlineTokenPartition _partitionInlineTokens(
  List<FlarkMarkdownBlockNode> blocks,
  List<FlarkMarkdownInlineToken> inlineTokens,
) {
  final buckets = [
    for (var i = 0; i < blocks.length; i++) <FlarkMarkdownInlineToken>[],
  ];
  if (blocks.isEmpty) {
    return _InlineTokenPartition(buckets: buckets, unassigned: inlineTokens);
  }

  final orderedBlocks =
      [
        for (var i = 0; i < blocks.length; i++)
          _IndexedMarkdownBlock(index: i, block: blocks[i]),
      ]..sort((left, right) {
        final startComparison = left.block.sourceRange.start.compareTo(
          right.block.sourceRange.start,
        );
        if (startComparison != 0) return startComparison;
        return left.block.sourceRange.end.compareTo(
          right.block.sourceRange.end,
        );
      });

  final unassigned = <FlarkMarkdownInlineToken>[];
  var blockCursor = 0;
  for (final token in inlineTokens) {
    while (blockCursor < orderedBlocks.length &&
        orderedBlocks[blockCursor].block.sourceRange.end <
            token.sourceRange.start) {
      blockCursor++;
    }

    var assigned = false;
    for (var i = blockCursor; i < orderedBlocks.length; i++) {
      final block = orderedBlocks[i].block;
      if (block.sourceRange.start > token.sourceRange.start) break;
      if (!_contains(block.sourceRange, token.sourceRange)) continue;
      buckets[orderedBlocks[i].index].add(token);
      assigned = true;
      break;
    }
    if (!assigned) unassigned.add(token);
  }

  return _InlineTokenPartition(buckets: buckets, unassigned: unassigned);
}

final class _InlineTokenPartition {
  const _InlineTokenPartition({
    required this.buckets,
    required this.unassigned,
  });

  final List<List<FlarkMarkdownInlineToken>> buckets;
  final List<FlarkMarkdownInlineToken> unassigned;
}

final class _IndexedMarkdownBlock {
  const _IndexedMarkdownBlock({required this.index, required this.block});

  final int index;
  final FlarkMarkdownBlockNode block;
}

bool _contains(FlarkSourceRange outer, FlarkSourceRange inner) {
  return inner.start >= outer.start && inner.end <= outer.end;
}

FlarkRenderTableDescriptor? _tableDescriptor(
  FlarkMarkdownBlockNode block,
  FlarkProjection projection,
) {
  if (block.kind != FlarkMarkdownBlockKind.table) return null;
  final alignments = block.attributes['alignments'];
  if (alignments is! List) {
    return FlarkRenderTableDescriptor(
      columnAlignments: const [],
      rows: _tableRows(block, projection),
    );
  }
  return FlarkRenderTableDescriptor(
    columnAlignments: alignments.map(_tableAlignment),
    rows: _tableRows(block, projection),
  );
}

Iterable<FlarkRenderTableRowDescriptor> _tableRows(
  FlarkMarkdownBlockNode table,
  FlarkProjection projection,
) {
  return table.children
      .where((row) => row.kind == FlarkMarkdownBlockKind.tableRow)
      .map(
        (row) => FlarkRenderTableRowDescriptor(
          header: row.attributes['header'] == true,
          sourceRange: row.sourceRange,
          displayRange: _displayRange(projection, row.sourceRange),
          cells: row.children
              .where((cell) => cell.kind == FlarkMarkdownBlockKind.tableCell)
              .map(
                (cell) => FlarkRenderTableCellDescriptor(
                  sourceRange: cell.sourceRange,
                  displayRange: _displayRange(projection, cell.sourceRange),
                ),
              ),
        ),
      );
}

FlarkRenderTableColumnAlignment _tableAlignment(Object? value) {
  return switch (value) {
    'left' => FlarkRenderTableColumnAlignment.left,
    'center' => FlarkRenderTableColumnAlignment.center,
    'right' => FlarkRenderTableColumnAlignment.right,
    'none' || null => FlarkRenderTableColumnAlignment.none,
    _ => FlarkRenderTableColumnAlignment.unknown,
  };
}

FlarkRenderTaskListItemDescriptor? _taskListItemDescriptor(
  FlarkMarkdownBlockNode block,
) {
  if (block.kind != FlarkMarkdownBlockKind.listItem) return null;
  final checked = block.attributes['checked'];
  if (checked is! bool) return null;
  return FlarkRenderTaskListItemDescriptor(checked: checked);
}

FlarkRenderListItemDescriptor? _listItemDescriptor(
  FlarkMarkdownBlockNode block,
) {
  if (block.kind != FlarkMarkdownBlockKind.listItem) return null;
  return FlarkRenderListItemDescriptor(
    kind: switch (block.attributes['listKind']) {
      'unordered' => FlarkRenderListKind.unordered,
      'ordered' => FlarkRenderListKind.ordered,
      _ => FlarkRenderListKind.unknown,
    },
  );
}

FlarkRenderCodeBlockDescriptor? _codeBlockDescriptor(
  FlarkMarkdownBlockNode block,
) {
  if (block.kind != FlarkMarkdownBlockKind.codeBlock) return null;
  return FlarkRenderCodeBlockDescriptor(
    language:
        _stringAttribute(block.attributes, 'language') ??
        _stringAttribute(block.attributes, 'fenceInfo'),
  );
}

FlarkRenderInlineActionDescriptor? _inlineActionDescriptor(
  FlarkMarkdownInlineToken token,
) {
  final destination =
      _stringAttribute(token.attributes, 'destination') ??
      _stringAttribute(token.attributes, 'href') ??
      _stringAttribute(token.attributes, 'src');
  if (destination == null || destination.isEmpty) return null;

  return switch (token.kind) {
    FlarkMarkdownInlineKind.link => FlarkRenderInlineActionDescriptor(
      kind: FlarkRenderInlineActionKind.link,
      destination: destination,
      title: _stringAttribute(token.attributes, 'title'),
      label: _stringAttribute(token.attributes, 'label'),
    ),
    FlarkMarkdownInlineKind.image => FlarkRenderInlineActionDescriptor(
      kind: FlarkRenderInlineActionKind.image,
      destination: destination,
      title: _stringAttribute(token.attributes, 'title'),
      label:
          _stringAttribute(token.attributes, 'alt') ??
          _stringAttribute(token.attributes, 'label'),
    ),
    _ => null,
  };
}

String? _stringAttribute(Map<String, Object?> attributes, String key) {
  final value = attributes[key];
  return value is String ? value : null;
}

FlarkRenderTextStyleToken _blockStyleToken(FlarkMarkdownBlockNode block) {
  if (block.kind != FlarkMarkdownBlockKind.heading) {
    return FlarkRenderTextStyleToken.body;
  }
  final level = block.attributes['level'];
  return switch (level) {
    1 => FlarkRenderTextStyleToken.heading1,
    2 => FlarkRenderTextStyleToken.heading2,
    3 => FlarkRenderTextStyleToken.heading3,
    4 => FlarkRenderTextStyleToken.heading4,
    5 => FlarkRenderTextStyleToken.heading5,
    6 => FlarkRenderTextStyleToken.heading6,
    _ => FlarkRenderTextStyleToken.heading1,
  };
}

FlarkRenderTextStyleToken _inlineStyleToken(FlarkMarkdownInlineToken token) {
  return switch (token.kind) {
    FlarkMarkdownInlineKind.emphasis => FlarkRenderTextStyleToken.emphasis,
    FlarkMarkdownInlineKind.strong => FlarkRenderTextStyleToken.strong,
    FlarkMarkdownInlineKind.inlineCode => FlarkRenderTextStyleToken.inlineCode,
    FlarkMarkdownInlineKind.strikethrough =>
      FlarkRenderTextStyleToken.strikethrough,
    FlarkMarkdownInlineKind.link => FlarkRenderTextStyleToken.link,
    FlarkMarkdownInlineKind.image => FlarkRenderTextStyleToken.image,
    FlarkMarkdownInlineKind.htmlInline => FlarkRenderTextStyleToken.rawHtml,
    FlarkMarkdownInlineKind.unknown => FlarkRenderTextStyleToken.unknown,
    _ => FlarkRenderTextStyleToken.body,
  };
}

Iterable<FlarkRenderBlock> _blockAndDescendants(FlarkRenderBlock block) sync* {
  yield block;
  for (final child in block.children) {
    yield* _blockAndDescendants(child);
  }
}
