// The live-rendered block editor: per-block reconciliation, the widget
// instance cache and content signatures, the focus coordinator, and the
// synthetic source-host machinery that keeps gaps and the document tail
// editable. See doc/architecture/live_rendered_rebuild_isolation.md.

part of '../flark_projected_editable_text.dart';

final class _FlarkLiveRenderedBlockEditor extends StatefulWidget {
  const _FlarkLiveRenderedBlockEditor({
    required this.controller,
    this.focusNode,
    this.style,
    this.cursorColor = const Color(0xFF006ADC),
    this.backgroundCursorColor = const Color(0x00000000),
    this.minLines,
    this.maxLines,
    this.expands = false,
    this.autofocus = false,
    this.shortcuts = const <ShortcutActivator, Intent>{},
  });

  final FlarkFlutterController controller;
  final FocusNode? focusNode;
  final TextStyle? style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final int? minLines;
  final int? maxLines;
  final bool expands;
  final bool autofocus;
  final Map<ShortcutActivator, Intent> shortcuts;

  @override
  State<_FlarkLiveRenderedBlockEditor> createState() {
    return _FlarkLiveRenderedBlockEditorState();
  }
}

final class _FlarkLiveRenderedBlockEditorState
    extends State<_FlarkLiveRenderedBlockEditor> {
  final _focusCoordinator = _LiveRenderedBlockFocusCoordinator();
  final _blockReconciler = FlarkLiveBlockReconciler();
  final _blockWidgetCache = <String, _CachedLiveBlock>{};
  List<_LiveRenderedBlockEntry> _currentBlockEntries =
      const <_LiveRenderedBlockEntry>[];
  TextStyle? _cacheStyle;
  Color? _cacheCursorColor;
  Color? _cacheBackgroundCursorColor;
  final _contentBoundsKey = GlobalKey();
  int? _appendHostOffset;

  @override
  void dispose() {
    _focusCoordinator.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(_FlarkLiveRenderedBlockEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (identical(oldWidget.controller, widget.controller)) return;
    _blockWidgetCache.clear();
    _currentBlockEntries = const <_LiveRenderedBlockEntry>[];
    _appendHostOffset = null;
    _blockReconciler.reset();
    _focusCoordinator.reset();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.controller,
      builder: (context, _) {
        final baseStyle = DefaultTextStyle.of(
          context,
        ).style.merge(widget.style);
        final displayText = _projectedText(widget.controller);
        final hasRenderPlan = widget.controller.hasUsableRenderPlan;
        final blocks = hasRenderPlan
            ? _editableBlocks(
                    widget.controller.renderPlan.blocks,
                    controller: widget.controller,
                  )
                  .where((block) => _isVisibleEditableBlock(block, displayText))
                  .toList(growable: false)
            : const <FlarkRenderBlock>[];

        if (blocks.isEmpty ||
            !_requiresBlockWidgetEditing(blocks, displayText)) {
          return _FlarkProjectedEditableHost(
            controller: widget.controller,
            focusNode: widget.focusNode,
            style: widget.style,
            cursorColor: widget.cursorColor,
            backgroundCursorColor: widget.backgroundCursorColor,
            minLines: widget.minLines,
            maxLines: widget.maxLines,
            expands: widget.expands,
            autofocus: widget.autofocus,
            shortcuts: widget.shortcuts,
            liveRendered: true,
          );
        }

        final appendHostOffset = _activeAppendHostOffset();
        final editableBlocks = _blocksWithSourceGapHosts(
          blocks: blocks,
          controller: widget.controller,
          appendHostOffset: appendHostOffset,
        );
        // Stable identity across re-parses: an unchanged block keeps its id
        // even after earlier edits shift its offsets, and the edited block keeps
        // its id (preserving its widget State/focus/IME). See
        // FlarkLiveBlockReconciler.
        final blockIds = _blockReconciler.assignIds(
          editableBlocks,
          displayText,
        );
        final blockEntries = [
          for (var index = 0; index < editableBlocks.length; index += 1)
            _LiveRenderedBlockEntry(
              id: blockIds[index],
              block: editableBlocks[index],
            ),
        ];
        _focusCoordinator.reconcile(blockEntries);
        _focusCoordinator.scheduleSelectionSync(
          entries: blockEntries,
          controller: widget.controller,
          autofocus: widget.autofocus || appendHostOffset != null,
          restoreSelectionFocus: _selectionTargetsTerminalAppendHost(
            editableBlocks,
            widget.controller,
          ),
          externalFirstFocusNode: widget.focusNode,
          isMounted: () => mounted,
        );

        _currentBlockEntries = blockEntries;
        final blockWidgets = _buildBlockWidgets(
          entries: blockEntries,
          displayText: displayText,
          baseStyle: baseStyle,
        );
        final content = KeyedSubtree(
          key: _contentBoundsKey,
          child: Column(
            key: const Key('FlarkLiveBlockEditor'),
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: blockWidgets,
          ),
        );
        Widget editor = LayoutBuilder(
          builder: (context, constraints) {
            final scrollable = SingleChildScrollView(child: content);
            if (widget.expands) {
              return SizedBox.expand(child: scrollable);
            }
            if (constraints.hasBoundedHeight) {
              return scrollable;
            }
            return content;
          },
        );
        editor = GestureDetector(
          behavior: HitTestBehavior.translucent,
          onTapUp: _handleEditorTapUp,
          child: editor,
        );
        editor = FlarkCommandActions(
          controller: widget.controller,
          child: editor,
        );
        if (widget.shortcuts.isNotEmpty) {
          editor = Shortcuts(
            shortcuts: <ShortcutActivator, Intent>{...widget.shortcuts},
            child: editor,
          );
        }
        return editor;
      },
    );
  }

  static bool _requiresBlockWidgetEditing(
    Iterable<FlarkRenderBlock> blocks,
    String displayText,
  ) {
    return blocks.any((block) {
      return block.table != null ||
          block.codeBlock != null ||
          block.listItem != null ||
          block.taskListItem != null ||
          block.kind == FlarkMarkdownBlockKind.blockquote ||
          _standaloneImageRunForBlock(block, displayText) != null;
    });
  }

  static String _projectedText(FlarkFlutterController controller) {
    try {
      return controller.projection.projectText(controller.markdown);
    } on ArgumentError {
      return controller.markdown;
    }
  }

  List<Widget> _buildBlockWidgets({
    required List<_LiveRenderedBlockEntry> entries,
    required String displayText,
    required TextStyle baseStyle,
  }) {
    // Style/colours are editor-global and intentionally not part of a block's
    // content signature, so invalidate the whole instance cache when they
    // change (e.g. a theme switch).
    if (_cacheStyle != baseStyle ||
        _cacheCursorColor != widget.cursorColor ||
        _cacheBackgroundCursorColor != widget.backgroundCursorColor) {
      _blockWidgetCache.clear();
      _cacheStyle = baseStyle;
      _cacheCursorColor = widget.cursorColor;
      _cacheBackgroundCursorColor = widget.backgroundCursorColor;
    }

    // Block editables sync their caret/selection from the parent rebuild, so a
    // block must rebuild when *its* selection state changes even if its content
    // did not. The per-block selection key captures that.
    final selection = widget.controller.selection;

    final widgets = <Widget>[];
    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final block = entry.block;
      final signature = liveBlockContentSignature(
        block,
        displayText,
        markdown: widget.controller.markdown,
      );
      final selectionKey = _blockSelectionKey(block, selection);
      final cached = _blockWidgetCache[entry.id];
      final handle =
          cached?.handle ??
          _LiveRenderedBlockHandle(entry: entry, displayText: displayText);
      handle.update(entry: entry, displayText: displayText);
      // Reuse the same widget instance only when the block is byte-identical in
      // content, navigation context, AND selection state. Flutter
      // returns immediately for a reference-identical widget, skipping the
      // entire subtree — so an unchanged block costs nothing to rebuild. The
      // mutable handle keeps edit/source mapping current for offset shifts.
      if (cached != null &&
          cached.signature == signature &&
          cached.selectionKey == selectionKey &&
          cached.index == index &&
          cached.count == entries.length) {
        widgets.add(cached.widget);
        continue;
      }

      final isLast = index + 1 >= entries.length;
      final builtWidget = RepaintBoundary(
        // The per-block ValueKey preserves each block's editable state across
        // rebuilds and reorders.
        key: ValueKey(entry.id),
        child: _FlarkLiveRenderedBlock(
          controller: widget.controller,
          handle: handle,
          style: baseStyle,
          cursorColor: widget.cursorColor,
          backgroundCursorColor: widget.backgroundCursorColor,
          autofocus: widget.autofocus && index == 0,
          focusNode: _focusCoordinator.focusNodeForBlock(
            entry: entry,
            index: index,
            externalFirstFocusNode: widget.focusNode,
          ),
          // Resolve the neighbour by id at call time so a reused instance's
          // navigation callbacks stay valid as neighbours change.
          onMoveToPreviousBlock: index == 0
              ? null
              : () => _moveToPreviousBlock(entry.id),
          onMoveToNextBlock: (isLast && block.codeBlock == null)
              ? null
              : () => _moveToNextBlock(entry.id),
        ),
      );
      _blockWidgetCache[entry.id] = _CachedLiveBlock(
        handle: handle,
        signature: signature,
        selectionKey: selectionKey,
        index: index,
        count: entries.length,
        widget: builtWidget,
      );
      widgets.add(builtWidget);
    }

    final liveIds = {for (final entry in entries) entry.id};
    _blockWidgetCache.removeWhere((id, _) => !liveIds.contains(id));
    return widgets;
  }

  void _moveToPreviousBlock(String id) {
    final entries = _currentBlockEntries;
    final index = _indexOfBlockId(entries, id);
    if (index <= 0) return;
    _moveSelectionToBlockBoundary(entries[index - 1].block, after: true);
  }

  void _moveToNextBlock(String id) {
    final entries = _currentBlockEntries;
    final index = _indexOfBlockId(entries, id);
    if (index < 0) return;
    if (index + 1 >= entries.length) {
      if (entries[index].block.codeBlock != null) {
        _moveSelectionToDocumentBoundary(entries[index].block, after: true);
      }
      return;
    }
    _moveSelectionToBlockBoundary(entries[index + 1].block, after: false);
  }

  int _indexOfBlockId(List<_LiveRenderedBlockEntry> entries, String id) {
    for (var i = 0; i < entries.length; i += 1) {
      if (entries[i].id == id) return i;
    }
    return -1;
  }

  // A block's selection state, relative to its source start so it is invariant
  // under offset shifts. 'n' when the selection does not touch the block.
  String _blockSelectionKey(FlarkRenderBlock block, FlarkSelection selection) {
    final blockStart = block.sourceRange.start;
    final blockEnd = block.sourceRange.end;
    final selectionStart = selection.start;
    final selectionEnd = selection.end;
    if (selectionEnd < blockStart || selectionStart > blockEnd) return 'n';
    return '${selectionStart - blockStart}:'
        '${selectionEnd - blockStart}:'
        '${selection.isCollapsed}';
  }

  void _moveSelectionToBlockBoundary(
    FlarkRenderBlock block, {
    required bool after,
  }) {
    widget.controller.applySelection(
      FlarkSelection.collapsed(
        _sourceNavigationBoundaryOffset(
          markdown: widget.controller.markdown,
          block: block,
          after: after,
        ),
      ),
      userEvent: after
          ? 'selection.liveBlock.verticalBoundary.previous'
          : 'selection.liveBlock.verticalBoundary.next',
    );
  }

  void _moveSelectionToDocumentBoundary(
    FlarkRenderBlock block, {
    required bool after,
  }) {
    final offset = (after ? block.sourceRange.end : block.sourceRange.start)
        .clamp(0, widget.controller.markdown.length);
    _appendHostOffset = after ? offset : null;
    widget.controller.applySelection(
      FlarkSelection.collapsed(offset),
      userEvent: after
          ? 'selection.liveBlock.verticalBoundary.documentEnd'
          : 'selection.liveBlock.verticalBoundary.documentStart',
    );
  }

  int? _activeAppendHostOffset() {
    final offset = _appendHostOffset;
    if (offset == null) return null;
    final selection = widget.controller.selection;
    final extentOffset = selection.extentOffset;
    if (!selection.isCollapsed ||
        extentOffset < offset ||
        extentOffset > widget.controller.markdown.length ||
        offset < 0 ||
        offset > widget.controller.markdown.length ||
        !_isAppendHostContinuation(
          widget.controller.markdown.substring(offset, extentOffset),
        )) {
      _appendHostOffset = null;
      return null;
    }
    return offset;
  }

  bool _isAppendHostContinuation(String text) {
    if (text.isEmpty) return true;
    if (!_isLineBreakCodeUnit(text.codeUnitAt(0))) {
      return !text.contains('\n');
    }
    final contentStart = _leadingLineBreakCount(text);
    if (contentStart > 2) return false;
    return !text.substring(contentStart).contains('\n');
  }

  void _handleEditorTapUp(TapUpDetails details) {
    if (!_tapIsBelowRenderedContent(details.globalPosition)) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final offset = widget.controller.markdown.length;
      _appendHostOffset = offset;
      widget.controller.applySelection(
        FlarkSelection.collapsed(offset),
        userEvent: 'selection.liveBlock.appendHost',
      );
      if (mounted) setState(() {});
    });
  }

  bool _tapIsBelowRenderedContent(Offset globalPosition) {
    final renderObject = _contentBoundsKey.currentContext?.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.attached) return false;
    final localPosition = renderObject.globalToLocal(globalPosition);
    final lastChildBottom = _lastChildBottomIn(renderObject);
    if (lastChildBottom != null && localPosition.dy > lastChildBottom) {
      return true;
    }
    return localPosition.dy > renderObject.size.height;
  }

  double? _lastChildBottomIn(RenderBox renderObject) {
    RenderBox? lastChild;
    renderObject.visitChildren((child) {
      if (child is RenderBox && child.attached) lastChild = child;
    });
    final child = lastChild;
    if (child == null) return null;
    final childBottomGlobal = child.localToGlobal(Offset(0, child.size.height));
    return renderObject.globalToLocal(childBottomGlobal).dy;
  }

  static List<FlarkRenderBlock> _editableBlocks(
    Iterable<FlarkRenderBlock> blocks, {
    required FlarkFlutterController controller,
  }) {
    return [
      for (final block in blocks)
        for (final editable in _editableBlockAndDescendants(block))
          _normalizePredictedEditableBlock(editable, controller),
    ];
  }

  static bool _selectionTargetsTerminalAppendHost(
    Iterable<FlarkRenderBlock> blocks,
    FlarkFlutterController controller,
  ) {
    final selection = controller.selection;
    if (!selection.isCollapsed) return false;
    return blocks.any((block) {
      final stableId = block.attributes['stableId'];
      return stableId is String &&
          stableId.startsWith('terminalAppendHost:') &&
          _blockOwnsSourceSelection(
            markdown: controller.markdown,
            block: block,
            selection: selection,
          );
    });
  }

  static FlarkRenderBlock _normalizePredictedEditableBlock(
    FlarkRenderBlock block,
    FlarkFlutterController controller,
  ) {
    if (block.codeBlock == null) return block;
    final markdown = controller.markdown;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null || !context.isClosed) return block;
    final closedEnd = context.closingLineEndWithBreak ?? context.closingLineEnd;
    if (closedEnd == null || closedEnd >= block.sourceRange.end) return block;
    final sourceRange = FlarkSourceRange(
      block.sourceRange.start,
      closedEnd,
    ).validate(markdown.length);
    return FlarkRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: sourceRange,
      displayRange: FlarkSourceRange(
        controller.projection.sourceToDisplayOffset(sourceRange.start),
        controller.projection.sourceToDisplayOffset(sourceRange.end),
      ),
      styleToken: block.styleToken,
      inlineRuns: block.inlineRuns,
      children: block.children,
      table: block.table,
      listItem: block.listItem,
      taskListItem: block.taskListItem,
      codeBlock: block.codeBlock,
      attributes: block.attributes,
    );
  }

  static Iterable<FlarkRenderBlock> _editableBlockAndDescendants(
    FlarkRenderBlock block,
  ) sync* {
    if (block.kind == FlarkMarkdownBlockKind.document ||
        block.kind == FlarkMarkdownBlockKind.list) {
      for (final child in block.children) {
        yield* _editableBlockAndDescendants(child);
      }
      return;
    }
    yield block;
  }

  static List<FlarkRenderBlock> _blocksWithSourceGapHosts({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) {
    final next = [
      ...blocks,
      ..._parserOmittedLineHosts(
        blocks: blocks,
        controller: controller,
        appendHostOffset: appendHostOffset,
      ),
    ];
    final appendHost = _terminalAppendHost(
      blocks: blocks,
      controller: controller,
      appendHostOffset: appendHostOffset,
    );
    if (appendHost != null) {
      next.add(appendHost);
      return _sortedBlocks(next);
    }
    final selection = controller.selection;
    if (!selection.isCollapsed) return _sortedBlocks(next);
    final useTerminalCodeFenceHost = _selectionIsAfterTerminalCodeFence(
      markdown: controller.markdown,
      blocks: blocks,
      selection: selection,
    );
    if (!useTerminalCodeFenceHost &&
        next.any(
          (block) => _blockOwnsSourceSelection(
            markdown: controller.markdown,
            block: block,
            selection: selection,
          ),
        )) {
      return _sortedBlocks(next);
    }

    final displaySelection = controller.projection.sourceSelectionToDisplay(
      selection,
    );
    final sourceOffset = selection.extentOffset.clamp(
      0,
      controller.markdown.length,
    );
    final displayOffset = displaySelection.extentOffset.clamp(
      0,
      controller.projection.textLength,
    );
    final selectionHost = FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSelectionHost',
      sourceRange: FlarkSourceRange(sourceOffset, sourceOffset),
      displayRange: FlarkSourceRange(displayOffset, displayOffset),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: const {'synthetic': true, 'reason': 'selectionHost'},
    );
    next.add(selectionHost);
    return _sortedBlocks(next);
  }

  static FlarkRenderBlock? _terminalAppendHost({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) {
    if (appendHostOffset == null) return null;
    final markdown = controller.markdown;
    if (appendHostOffset != markdown.length) return null;
    final terminal = _terminalEditableBlock(blocks, appendHostOffset);
    if (terminal == null) return null;
    final displayOffset = controller.projection.sourceToDisplayOffset(
      appendHostOffset,
    );
    return FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSelectionHost',
      sourceRange: FlarkSourceRange(appendHostOffset, appendHostOffset),
      displayRange: FlarkSourceRange(displayOffset, displayOffset),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: {
        'synthetic': true,
        'reason': 'terminalAppendHost',
        'stableId': 'terminalAppendHost:$appendHostOffset',
        'sourcePrefix': _terminalAppendPrefix(markdown, terminal),
      },
    );
  }

  static FlarkRenderBlock? _terminalEditableBlock(
    List<FlarkRenderBlock> blocks,
    int sourceOffset,
  ) {
    FlarkRenderBlock? best;
    for (final block in blocks) {
      if (block.sourceRange.end != sourceOffset) continue;
      if (best == null || block.sourceRange.start > best.sourceRange.start) {
        best = block;
      }
    }
    return best;
  }

  static String _terminalAppendPrefix(String markdown, FlarkRenderBlock block) {
    final requiredBreaks =
        block.listItem != null ||
            block.taskListItem != null ||
            block.kind == FlarkMarkdownBlockKind.blockquote
        ? 2
        : 1;
    final missingBreaks = requiredBreaks - _trailingLineBreakCount(markdown);
    if (missingBreaks <= 0) return '';
    return '\n' * missingBreaks;
  }

  static int _trailingLineBreakCount(String markdown) {
    var count = 0;
    for (var index = markdown.length - 1; index >= 0; index--) {
      if (!_isLineBreakCodeUnit(markdown.codeUnitAt(index))) break;
      count++;
    }
    return count;
  }

  static int _leadingLineBreakCount(String markdown) {
    var count = 0;
    for (var index = 0; index < markdown.length; index++) {
      if (!_isLineBreakCodeUnit(markdown.codeUnitAt(index))) break;
      count++;
    }
    return count;
  }

  static bool _selectionIsAfterTerminalCodeFence({
    required String markdown,
    required List<FlarkRenderBlock> blocks,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) return false;
    final offset = selection.extentOffset;
    if (offset < 0 || offset > markdown.length) return false;
    if (!FlarkMarkdownFencedCodeScanner.isWhitespace(
      markdown.substring(offset),
    )) {
      return false;
    }

    for (final block in blocks) {
      if (block.codeBlock == null || block.sourceRange.end != offset) {
        continue;
      }
      final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
        markdown,
        block.sourceRange.start,
      );
      if (context?.isClosed ?? false) return true;
    }
    return false;
  }

  static Iterable<FlarkRenderBlock> _parserOmittedLineHosts({
    required List<FlarkRenderBlock> blocks,
    required FlarkFlutterController controller,
    required int? appendHostOffset,
  }) sync* {
    final markdown = controller.markdown;
    if (markdown.isEmpty) return;
    final selection = controller.selection;
    var lineStart = 0;
    while (lineStart <= markdown.length) {
      final newline = markdown.indexOf('\n', lineStart);
      final lineEnd = newline < 0 ? markdown.length : newline;
      final lineText = markdown.substring(lineStart, lineEnd);
      final lineRange = FlarkSourceRange(lineStart, lineEnd);
      final selectedLine =
          selection.isCollapsed &&
          selection.extentOffset >= lineRange.start &&
          selection.extentOffset <= lineRange.end;
      final shouldHost = lineText.trim().isEmpty || selectedLine;
      if (shouldHost &&
          !_isFocusedTerminalBlockquoteExitSeparator(
            markdown: markdown,
            blocks: blocks,
            lineRange: lineRange,
            selection: selection,
          ) &&
          !_sourceRangeCoveredByBlocks(lineRange, blocks) &&
          !_sourceOffsetHasCollapsedBlock(lineStart, blocks)) {
        yield _syntheticSourceLineHost(
          controller: controller,
          sourceRange: lineRange,
          stableId: _appendContinuationStableId(
            appendHostOffset: appendHostOffset,
            blocks: blocks,
            markdown: markdown,
            lineRange: lineRange,
            selection: selection,
          ),
        );
      }
      if (newline < 0) break;
      lineStart = newline + 1;
    }
  }

  static bool _isFocusedTerminalBlockquoteExitSeparator({
    required String markdown,
    required List<FlarkRenderBlock> blocks,
    required FlarkSourceRange lineRange,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed || !lineRange.isCollapsed) return false;
    final separatorOffset = lineRange.start;
    if (selection.extentOffset != separatorOffset + 1 ||
        selection.extentOffset != markdown.length ||
        separatorOffset <= 0 ||
        separatorOffset >= markdown.length) {
      return false;
    }
    if (!_isLineBreakCodeUnit(markdown.codeUnitAt(separatorOffset)) ||
        !_isLineBreakCodeUnit(markdown.codeUnitAt(separatorOffset - 1))) {
      return false;
    }
    return blocks.any(
      (block) =>
          block.kind == FlarkMarkdownBlockKind.blockquote &&
          block.sourceRange.end == separatorOffset,
    );
  }

  static bool _sourceRangeCoveredByBlocks(
    FlarkSourceRange range,
    Iterable<FlarkRenderBlock> blocks,
  ) {
    for (final block in blocks) {
      final blockRange = block.sourceRange;
      if (range.isCollapsed) {
        if (blockRange.start <= range.start && range.start < blockRange.end) {
          return true;
        }
        if (block.codeBlock != null && blockRange.end == range.start) {
          return true;
        }
      } else if (blockRange.start < range.end && range.start < blockRange.end) {
        return true;
      }
    }
    return false;
  }

  static bool _sourceOffsetHasCollapsedBlock(
    int sourceOffset,
    Iterable<FlarkRenderBlock> blocks,
  ) {
    return blocks.any(
      (block) =>
          block.sourceRange.isCollapsed &&
          block.sourceRange.start == sourceOffset,
    );
  }

  static FlarkRenderBlock _syntheticSourceLineHost({
    required FlarkFlutterController controller,
    required FlarkSourceRange sourceRange,
    String? stableId,
  }) {
    final displayStart = controller.projection.sourceToDisplayOffset(
      sourceRange.start,
    );
    final displayEnd = controller.projection.sourceToDisplayOffset(
      sourceRange.end,
    );
    return FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.paragraph,
      type: 'syntheticSourceLineHost',
      sourceRange: sourceRange,
      displayRange: FlarkSourceRange(displayStart, displayEnd),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      attributes: stableId == null
          ? const {'synthetic': true, 'reason': 'parserOmittedLine'}
          : {
              'synthetic': true,
              'reason': 'parserOmittedLine',
              'stableId': stableId,
            },
    );
  }

  static String? _appendContinuationStableId({
    required int? appendHostOffset,
    required List<FlarkRenderBlock> blocks,
    required String markdown,
    required FlarkSourceRange lineRange,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) return null;
    final stableOffset =
        appendHostOffset ??
        _inferredAppendContinuationOffset(
          blocks: blocks,
          markdown: markdown,
          lineRange: lineRange,
        );
    if (stableOffset == null) return null;
    if (lineRange.start <= stableOffset) return null;
    if (selection.extentOffset < lineRange.start ||
        selection.extentOffset > lineRange.end) {
      return null;
    }
    return 'terminalAppendHost:$stableOffset';
  }

  static int? _inferredAppendContinuationOffset({
    required List<FlarkRenderBlock> blocks,
    required String markdown,
    required FlarkSourceRange lineRange,
  }) {
    if (lineRange.start <= 0 || lineRange.start > markdown.length) {
      return null;
    }
    int? best;
    for (final block in blocks) {
      if (!_supportsTerminalAppendContinuation(block)) continue;
      final appendOffset = _appendContinuationBaseOffset(
        markdown: markdown,
        block: block,
        lineStart: lineRange.start,
      );
      if (appendOffset == null) continue;
      if (!FlarkMarkdownFencedCodeScanner.isWhitespace(
        markdown.substring(appendOffset, lineRange.start),
      )) {
        continue;
      }
      if (best == null || appendOffset > best) best = appendOffset;
    }
    return best;
  }

  static int? _appendContinuationBaseOffset({
    required String markdown,
    required FlarkRenderBlock block,
    required int lineStart,
  }) {
    final end = block.sourceRange.end;
    if (end < 0 || end > lineStart || end > markdown.length) return null;
    if (end > 0 && _isLineBreakCodeUnit(markdown.codeUnitAt(end - 1))) {
      return end - 1;
    }
    if (end == lineStart) return null;
    return end;
  }

  static bool _supportsTerminalAppendContinuation(FlarkRenderBlock block) {
    return block.codeBlock != null ||
        block.listItem != null ||
        block.taskListItem != null ||
        block.kind == FlarkMarkdownBlockKind.blockquote;
  }

  static List<FlarkRenderBlock> _sortedBlocks(List<FlarkRenderBlock> blocks) {
    blocks.sort((a, b) {
      final bySource = a.sourceRange.start.compareTo(b.sourceRange.start);
      if (bySource != 0) return bySource;
      final byLength = b.sourceRange.length.compareTo(a.sourceRange.length);
      if (byLength != 0) return byLength;
      return a.type.compareTo(b.type);
    });
    return blocks;
  }
}

final class _LiveRenderedBlockEntry {
  const _LiveRenderedBlockEntry({required this.id, required this.block});

  final String id;
  final FlarkRenderBlock block;
}

final class _LiveRenderedBlockHandle {
  _LiveRenderedBlockHandle({
    required _LiveRenderedBlockEntry entry,
    required String displayText,
  }) : _entry = entry,
       _displayText = displayText;

  _LiveRenderedBlockEntry _entry;
  String _displayText;

  String get id => _entry.id;
  FlarkRenderBlock get block => _entry.block;
  String get displayText => _displayText;

  void update({
    required _LiveRenderedBlockEntry entry,
    required String displayText,
  }) {
    _entry = entry;
    _displayText = displayText;
  }
}

/// A cached live block widget plus the inputs that, if all unchanged, make the
/// cached instance safe to reuse (skipping its rebuild).
final class _CachedLiveBlock {
  const _CachedLiveBlock({
    required this.handle,
    required this.signature,
    required this.selectionKey,
    required this.index,
    required this.count,
    required this.widget,
  });

  final _LiveRenderedBlockHandle handle;
  final String signature;
  final String selectionKey;
  final int index;
  final int count;
  final Widget widget;
}

final class _LiveRenderedBlockFocusCoordinator {
  final Map<String, FocusNode> _ownedBlockFocusNodes = {};
  int _focusSyncGeneration = 0;
  bool _hasObservedEditorFocus = false;
  bool _needsFocusRestoreAfterReconcile = false;

  void dispose() {
    for (final node in _ownedBlockFocusNodes.values) {
      node.dispose();
    }
    _ownedBlockFocusNodes.clear();
  }

  void reset() {
    for (final node in _ownedBlockFocusNodes.values) {
      node.dispose();
    }
    _ownedBlockFocusNodes.clear();
    _focusSyncGeneration += 1;
    _hasObservedEditorFocus = false;
    _needsFocusRestoreAfterReconcile = false;
  }

  void reconcile(List<_LiveRenderedBlockEntry> entries) {
    final currentIds = entries.map((entry) => entry.id).toSet();
    final staleIds = [
      for (final id in _ownedBlockFocusNodes.keys)
        if (!currentIds.contains(id)) id,
    ];
    for (final id in staleIds) {
      final node = _ownedBlockFocusNodes.remove(id);
      if (node == null) continue;
      if (node.hasFocus) _needsFocusRestoreAfterReconcile = true;
      node.dispose();
    }
  }

  FocusNode focusNodeForBlock({
    required _LiveRenderedBlockEntry entry,
    required int index,
    required FocusNode? externalFirstFocusNode,
  }) {
    if (index == 0 && externalFirstFocusNode != null) {
      return externalFirstFocusNode;
    }
    return _ownedBlockFocusNodes.putIfAbsent(
      entry.id,
      () => FocusNode(debugLabel: entry.id),
    );
  }

  void scheduleSelectionSync({
    required List<_LiveRenderedBlockEntry> entries,
    required FlarkFlutterController controller,
    required bool autofocus,
    required bool restoreSelectionFocus,
    required FocusNode? externalFirstFocusNode,
    required bool Function() isMounted,
  }) {
    final hasFocus = _editorHasFocus(externalFirstFocusNode);
    if (hasFocus) _hasObservedEditorFocus = true;
    final hadFocusAtSchedule = hasFocus;
    final canRestoreFocus =
        autofocus ||
        _needsFocusRestoreAfterReconcile ||
        (restoreSelectionFocus &&
            (_hasObservedEditorFocus || controller.state.revision > 0));
    if (!hasFocus && !canRestoreFocus) return;
    final target = _blockEntryForSelection(entries, controller);
    if (target == null) {
      _needsFocusRestoreAfterReconcile = false;
      return;
    }

    final generation = ++_focusSyncGeneration;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (generation != _focusSyncGeneration) return;
      if (!isMounted()) return;
      final currentHasFocus = _editorHasFocus(externalFirstFocusNode);
      if (currentHasFocus) _hasObservedEditorFocus = true;
      final canStillRestoreFocus =
          hadFocusAtSchedule ||
          autofocus ||
          _needsFocusRestoreAfterReconcile ||
          (restoreSelectionFocus &&
              (_hasObservedEditorFocus || controller.state.revision > 0));
      if (!currentHasFocus && !canStillRestoreFocus) return;
      final currentTarget = _blockEntryForSelection(entries, controller);
      if (currentTarget == null) {
        _needsFocusRestoreAfterReconcile = false;
        return;
      }
      final targetIndex = entries.indexOf(currentTarget);
      final node = focusNodeForBlock(
        entry: currentTarget,
        index: targetIndex,
        externalFirstFocusNode: externalFirstFocusNode,
      );
      if (!node.hasFocus) node.requestFocus();
      _hasObservedEditorFocus = true;
      _needsFocusRestoreAfterReconcile = false;
    });
  }

  bool _editorHasFocus(FocusNode? externalFirstFocusNode) {
    if (externalFirstFocusNode?.hasFocus ?? false) return true;
    return _ownedBlockFocusNodes.values.any((node) => node.hasFocus);
  }

  _LiveRenderedBlockEntry? _blockEntryForSelection(
    List<_LiveRenderedBlockEntry> entries,
    FlarkFlutterController controller,
  ) {
    _LiveRenderedBlockEntry? best;
    for (final entry in entries) {
      if (!_blockOwnsSelection(entry.block, controller)) continue;
      if (best == null ||
          entry.block.sourceRange.start > best.block.sourceRange.start) {
        best = entry;
      }
    }
    return best;
  }

  bool _blockOwnsSelection(
    FlarkRenderBlock block,
    FlarkFlutterController controller,
  ) {
    return _blockOwnsSourceSelection(
      markdown: controller.markdown,
      block: block,
      selection: controller.selection,
    );
  }
}

bool _blockOwnsSourceSelection({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSelection selection,
}) {
  final focusRange = _sourceFocusRangeForBlock(markdown, block);
  return selection.start >= focusRange.start && selection.end <= focusRange.end;
}

FlarkSourceRange _sourceFocusRangeForBlock(
  String markdown,
  FlarkRenderBlock block,
) {
  final start = block.sourceRange.start.clamp(0, markdown.length);
  var end = block.sourceRange.end.clamp(start, markdown.length);
  final lineEnd = markdown.indexOf('\n', start);
  if (lineEnd < 0) {
    end = markdown.length;
  } else if (lineEnd >= start && lineEnd > end) {
    end = lineEnd;
  }
  return FlarkSourceRange(start, end);
}

int _sourceNavigationBoundaryOffset({
  required String markdown,
  required FlarkRenderBlock block,
  required bool after,
}) {
  if (block.codeBlock != null) {
    final bodyRange = FlarkLiveCodeFenceInputPolicy.bodyRange(markdown, block);
    if (bodyRange != null) return after ? bodyRange.end : bodyRange.start;
  }
  final focusRange = _sourceFocusRangeForBlock(markdown, block);
  return after
      ? _sourceRangeEditableEnd(markdown, focusRange)
      : focusRange.start;
}

int _sourceRangeEditableEnd(String markdown, FlarkSourceRange range) {
  var end = range.end.clamp(range.start, markdown.length);
  while (end > range.start &&
      _isLineBreakCodeUnit(markdown.codeUnitAt(end - 1))) {
    end--;
  }
  return end;
}

bool _isLineBreakCodeUnit(int codeUnit) {
  return codeUnit == 0x0A || codeUnit == 0x0D;
}

Color _selectionColorForCursor(Color cursorColor) {
  return cursorColor.withValues(alpha: 0.24);
}

bool _isVisibleEditableBlock(FlarkRenderBlock block, String displayText) {
  if (_rangeOverlapsText(block.displayRange, displayText)) return true;
  // A standalone image whose alt text is empty (`![](url)`) projects to no
  // display text, but the block still renders the picture, so keep it visible.
  if (_standaloneImageRunForBlock(block, displayText) != null) return true;
  // Blocks whose markers are fully hidden still render as (empty) editable
  // widgets, so e.g. `### ` styles as an empty heading immediately instead
  // of flashing raw source until the first content character arrives.
  return (block.kind == FlarkMarkdownBlockKind.blockquote ||
          block.kind == FlarkMarkdownBlockKind.listItem ||
          block.kind == FlarkMarkdownBlockKind.codeBlock ||
          block.kind == FlarkMarkdownBlockKind.heading) &&
      block.displayRange.isCollapsed;
}

/// The single image run when [block] is a paragraph whose entire visible
/// content is one inline image (`![alt](url)` alone on its line), else null.
///
/// Such a block renders the real picture as a non-editable preview above its
/// normal (alt-text) caption editable — the editable keeps all caret, focus,
/// navigation, and deletion behavior identical to any other paragraph, so the
/// image block needs no special caret handling. Surrounding whitespace (the
/// block's trailing newline, leading indent) is ignored so an image still
/// counts as standalone in the middle of a document.
FlarkRenderInlineRun? _standaloneImageRunForBlock(
  FlarkRenderBlock block,
  String displayText,
) {
  if (block.kind != FlarkMarkdownBlockKind.paragraph) return null;
  if (block.children.isNotEmpty) return null;
  if (block.inlineRuns.length != 1) return null;
  final run = block.inlineRuns.single;
  if (run.action?.kind != FlarkRenderInlineActionKind.image) return null;
  final blockStart = block.displayRange.start.clamp(0, displayText.length);
  final blockEnd = block.displayRange.end.clamp(0, displayText.length);
  final runStart = run.displayRange.start.clamp(blockStart, blockEnd);
  final runEnd = run.displayRange.end.clamp(blockStart, blockEnd);
  if (run.displayRange.start < blockStart || run.displayRange.end > blockEnd) {
    return null;
  }
  // Only whitespace may surround the image within the block.
  if (displayText.substring(blockStart, runStart).trim().isNotEmpty) {
    return null;
  }
  if (displayText.substring(runEnd, blockEnd).trim().isNotEmpty) return null;
  return run;
}

bool _isSyntheticSourceHost(FlarkRenderBlock block) {
  return block.attributes['synthetic'] == true &&
      (block.type == 'syntheticSourceLineHost' ||
          block.type == 'syntheticSelectionHost');
}

FlarkSourceRange? _syntheticSourceHostRange(
  String markdown,
  FlarkRenderBlock block,
) {
  if (!_isSyntheticSourceHost(block)) return null;
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start > block.sourceRange.end ||
      block.sourceRange.end > markdown.length) {
    return null;
  }
  return block.sourceRange;
}

FlarkLiveBlockSourceEdit _syntheticSourceHostEdit({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSourceRange range,
  required TextEditingValue value,
}) {
  final oldText = markdown.substring(range.start, range.end);
  final replacementValue =
      FlarkLiveCodeFenceInputPolicy.valueAfterCompletingStandaloneOpener(
        oldDisplayText: oldText,
        oldSelection: TextSelection.collapsed(offset: oldText.length),
        newValue: value,
      ) ??
      value;
  final sourcePrefix = block.attributes['sourcePrefix'];
  final explicitPrefix = sourcePrefix is String ? sourcePrefix : null;
  final needsLeadingLineBreak =
      range.isCollapsed &&
      explicitPrefix == null &&
      replacementValue.text.isNotEmpty &&
      !_syntheticSourceHostFollowsClosedCodeFence(markdown, range.start) &&
      _syntheticSourceHostNeedsLeadingLineBreak(markdown, range.start);
  final prefix = explicitPrefix ?? (needsLeadingLineBreak ? '\n' : '');
  final prefixLength = prefix.length;
  final closingSuffix = _syntheticSourceHostClosingFenceSuffix(
    markdown: markdown,
    range: range,
    replacementText: replacementValue.text,
  );
  final editableRangeAfter = _syntheticSourceHostEditableRangeAfter(
    range: range,
    prefixLength: prefixLength,
    replacementText: replacementValue.text,
  );
  return FlarkLiveBlockSourceEdit(
    range: range,
    replacementText: '$prefix${replacementValue.text}$closingSuffix',
    editableRangeAfter: editableRangeAfter,
    selectionAfter: FlarkSelection(
      baseOffset:
          range.start + prefixLength + replacementValue.selection.baseOffset,
      extentOffset:
          range.start + prefixLength + replacementValue.selection.extentOffset,
    ),
  );
}

String _syntheticSourceHostClosingFenceSuffix({
  required String markdown,
  required FlarkSourceRange range,
  required String replacementText,
}) {
  if (!range.isCollapsed || range.end >= markdown.length) return '';
  if (markdown.substring(range.end).trim().isEmpty) return '';
  final openingLineEnd = replacementText.indexOf('\n');
  if (openingLineEnd < 0) return '';
  final openingLine = replacementText.substring(0, openingLineEnd);
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(openingLine);
  if (fence == null || fence.infoString != null) return '';
  final marker =
      '${fence.indent}${List.filled(fence.markerLength, fence.marker).join()}';
  final bodyText = replacementText.substring(openingLineEnd + 1);
  final breakBeforeCloser =
      bodyText.isNotEmpty && !_textEndsWithLineBreak(bodyText) ? '\n' : '';
  final breakAfterCloser =
      _textStartsWithLineBreak(markdown.substring(range.end)) ? '' : '\n';
  return '$breakBeforeCloser$marker$breakAfterCloser';
}

bool _textEndsWithLineBreak(String text) {
  if (text.isEmpty) return false;
  final last = text.codeUnitAt(text.length - 1);
  return last == 0x0A || last == 0x0D;
}

bool _textStartsWithLineBreak(String text) {
  if (text.isEmpty) return false;
  final first = text.codeUnitAt(0);
  return first == 0x0A || first == 0x0D;
}

FlarkSourceRange _syntheticSourceHostEditableRangeAfter({
  required FlarkSourceRange range,
  required int prefixLength,
  required String replacementText,
}) {
  final sourceStart = range.start + prefixLength;
  final openingLineEnd = replacementText.indexOf('\n');
  if (openingLineEnd >= 0) {
    final openingLine = replacementText.substring(0, openingLineEnd);
    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(openingLine);
    if (fence != null && fence.infoString == null) {
      return FlarkSourceRange(
        sourceStart + openingLineEnd + 1,
        sourceStart + replacementText.length,
      );
    }
  }
  return FlarkSourceRange(sourceStart, sourceStart + replacementText.length);
}

bool _syntheticSourceHostNeedsLeadingLineBreak(String markdown, int offset) {
  if (offset <= 0 || offset > markdown.length) return false;
  var lineBreaksBeforeOffset = 0;
  for (var index = offset - 1; index >= 0; index--) {
    if (!_isLineBreakCodeUnit(markdown.codeUnitAt(index))) break;
    lineBreaksBeforeOffset += 1;
  }
  return lineBreaksBeforeOffset < 2;
}

bool _syntheticSourceHostFollowsClosedCodeFence(String markdown, int offset) {
  if (offset <= 0 || offset > markdown.length) return false;
  var previousContentEnd = offset;
  var lineBreaksBeforeOffset = 0;
  while (previousContentEnd > 0 &&
      _isLineBreakCodeUnit(markdown.codeUnitAt(previousContentEnd - 1))) {
    previousContentEnd -= 1;
    lineBreaksBeforeOffset += 1;
  }
  if (lineBreaksBeforeOffset == 0) return false;
  if (previousContentEnd <= 0) return false;

  final closingLineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    markdown,
    previousContentEnd,
  );
  final closingLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    markdown,
    closingLineStart,
  );
  final closingFence = FlarkMarkdownFencedCodeScanner.fenceLine(
    markdown.substring(closingLineStart, closingLineEnd),
  );
  if (closingFence == null || !closingFence.canClose) return false;

  FlarkMarkdownFenceLine? openFence;
  var lineStart = 0;
  while (lineStart < closingLineStart) {
    final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      markdown,
      lineStart,
    );
    final lineFence = FlarkMarkdownFencedCodeScanner.fenceLine(
      markdown.substring(lineStart, lineEnd),
    );
    if (lineFence != null) {
      final activeFence = openFence;
      if (activeFence == null) {
        openFence = lineFence;
      } else if (_fenceLineClosesLine(activeFence, lineFence)) {
        openFence = null;
      }
    }
    final nextLineStart = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
      markdown,
      lineStart,
    );
    if (nextLineStart <= lineStart || nextLineStart >= closingLineStart) {
      break;
    }
    lineStart = nextLineStart;
  }

  final activeFence = openFence;
  return activeFence != null && _fenceLineClosesLine(activeFence, closingFence);
}

bool _fenceLineClosesLine(
  FlarkMarkdownFenceLine openingFence,
  FlarkMarkdownFenceLine closingFence,
) {
  return closingFence.canClose &&
      closingFence.marker == openingFence.marker &&
      closingFence.markerLength >= openingFence.markerLength;
}

/// Test-only counter of live-rendered block widget builds, used to verify
/// per-block rebuild isolation. Reset to zero and read it across an edit.
@visibleForTesting
int flarkDebugLiveBlockBuildCount = 0;

final class _FlarkLiveRenderedBlock extends StatelessWidget {
  const _FlarkLiveRenderedBlock({
    required this.controller,
    required this.handle,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final _LiveRenderedBlockHandle handle;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  @override
  Widget build(BuildContext context) {
    assert(() {
      flarkDebugLiveBlockBuildCount += 1;
      return true;
    }());
    final block = handle.block;
    final displayText = handle.displayText;
    if (block.table != null) {
      return _EditableTableBlock(
        controller: controller,
        block: block,
        blockHandle: handle,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
      );
    }
    if (block.codeBlock != null) {
      return _EditableCodeBlock(
        controller: controller,
        block: block,
        blockHandle: handle,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.taskListItem != null) {
      return _EditableTaskListItemBlock(
        controller: controller,
        block: block,
        blockHandle: handle,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.listItem != null) {
      return _EditableListItemBlock(
        controller: controller,
        block: block,
        blockHandle: handle,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    if (block.kind == FlarkMarkdownBlockKind.blockquote) {
      final theme = FlarkMarkdownTheme.of(context);
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: DecoratedBox(
          key: const Key('FlarkLiveBlockBlockquote'),
          decoration: BoxDecoration(
            color: theme.quoteBackgroundColor,
            border: Border(
              left: BorderSide(color: theme.quoteRailColor, width: 3),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(10, 6, 8, 6),
            child: _EditableProjectedBlockText(
              controller: controller,
              block: block,
              blockHandle: handle,
              displayText: displayText,
              style: style.copyWith(color: theme.quoteTextColor),
              cursorColor: cursorColor,
              backgroundCursorColor: backgroundCursorColor,
              focusNode: focusNode,
              autofocus: autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: onMoveToPreviousBlock,
              onMoveToNextBlock: onMoveToNextBlock,
            ),
          ),
        ),
      );
    }
    final standaloneImage = _standaloneImageRunForBlock(block, displayText);
    if (standaloneImage != null) {
      return _EditableImageBlock(
        controller: controller,
        block: block,
        blockHandle: handle,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      );
    }
    final syntheticSourceHost = _isSyntheticSourceHost(block);
    return Padding(
      padding: _plainBlockPadding(block),
      child: _EditableProjectedBlockText(
        controller: controller,
        block: block,
        blockHandle: handle,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
        focusNode: focusNode,
        autofocus: autofocus,
        markdownInputPolicy: true,
        sourceRangeForEdits: syntheticSourceHost
            ? _syntheticSourceHostRange
            : null,
        sourceEditForReplacement: syntheticSourceHost
            ? _syntheticSourceHostEdit
            : null,
        onMoveToPreviousBlock: onMoveToPreviousBlock,
        onMoveToNextBlock: onMoveToNextBlock,
      ),
    );
  }

  EdgeInsets _plainBlockPadding(FlarkRenderBlock block) {
    if (block.kind == FlarkMarkdownBlockKind.heading) {
      return const EdgeInsets.only(bottom: 6);
    }
    return const EdgeInsets.symmetric(vertical: 2);
  }
}
