import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../render_plan/render_plan.dart';
import 'flark_code_syntax_highlighting.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_interactions.dart';

typedef FlarkPreviewBlockWidgetBuilder =
    Widget? Function(
      BuildContext context,
      FlarkRenderBlock block,
      String displayText,
      TextStyle baseStyle,
    );

final class FlarkReadOnlyPreview extends StatelessWidget {
  const FlarkReadOnlyPreview({
    super.key,
    required this.controller,
    this.textStyle,
    this.blockBuilder,
  });

  final FlarkFlutterController controller;
  final TextStyle? textStyle;
  final FlarkPreviewBlockWidgetBuilder? blockBuilder;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        final style = textStyle ?? DefaultTextStyle.of(context).style;
        final displayText = _displayText();
        final blocks = controller.renderPlan.blocks;
        if (!controller.hasAuthoritativeRenderPlan || blocks.isEmpty) {
          return Text(displayText, style: style);
        }

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final block in blocks)
              blockBuilder?.call(context, block, displayText, style) ??
                  _PreviewBlock(
                    block: block,
                    displayText: displayText,
                    baseStyle: style,
                  ),
          ],
        );
      },
    );
  }

  String _displayText() {
    try {
      return controller.projection.projectText(controller.markdown);
    } on ArgumentError {
      return controller.markdown;
    }
  }
}

final class _PreviewBlock extends StatelessWidget {
  const _PreviewBlock({
    required this.block,
    required this.displayText,
    required this.baseStyle,
  });

  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle baseStyle;

  @override
  Widget build(BuildContext context) {
    final blockStyle = _blockStyle(baseStyle, block);
    final textSpan = block.codeBlock == null
        ? TextSpan(style: blockStyle, children: _inlineSpans(context))
        : buildFlarkHighlightedCodeSpan(
                source: displayText.substring(
                  block.displayRange.start,
                  block.displayRange.end,
                ),
                language: block.codeBlock?.language,
                baseStyle: blockStyle,
              ) ??
              TextSpan(style: blockStyle, children: _inlineSpans(context));
    final content = Text.rich(textSpan);

    if (block.codeBlock != null) {
      final source = displayText.substring(
        block.displayRange.start,
        block.displayRange.end,
      );
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: SizedBox(
          key: const Key('FlarkReadOnlyPreviewCodeBlock'),
          width: double.infinity,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: const Color(0xFFF1F4F8),
              border: Border.all(color: const Color(0xFFD7DEE8)),
              borderRadius: const BorderRadius.all(Radius.circular(6)),
            ),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(10, 8, 8, 8),
              child: Stack(
                children: [
                  Padding(
                    padding: const EdgeInsets.only(right: 54),
                    child: content,
                  ),
                  Positioned(
                    top: 0,
                    right: 0,
                    child: _PreviewCodeCopyButton(source: source),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
    }

    if (block.kind == FlarkMarkdownBlockKind.blockquote) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: SizedBox(
          key: const Key('FlarkReadOnlyPreviewBlockquote'),
          width: double.infinity,
          child: DecoratedBox(
            decoration: const BoxDecoration(
              color: Color(0xFFF8FAFC),
              border: Border(
                left: BorderSide(color: Color(0xFF7A8CA3), width: 3),
              ),
            ),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(10, 6, 8, 6),
              child: content,
            ),
          ),
        ),
      );
    }

    if (block.taskListItem != null) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _PreviewTaskCheckbox(checked: block.taskListItem!.checked),
            const SizedBox(width: 8),
            Expanded(child: content),
          ],
        ),
      );
    }

    if (block.table != null) {
      return _PreviewTable(
        block: block,
        displayText: displayText,
        baseStyle: baseStyle,
      );
    }

    return content;
  }

  List<InlineSpan> _inlineSpans(BuildContext context) {
    final spans = <InlineSpan>[];
    final runs = [...block.inlineRuns]
      ..sort((a, b) => a.displayRange.start.compareTo(b.displayRange.start));
    var cursor = block.displayRange.start;

    for (final run in runs) {
      if (run.displayRange.start > cursor) {
        spans.add(
          TextSpan(text: displayText.substring(cursor, run.displayRange.start)),
        );
      }
      spans.add(_inlineSpanForRun(context, run));
      cursor = run.displayRange.end;
    }

    if (cursor < block.displayRange.end) {
      spans.add(
        TextSpan(text: displayText.substring(cursor, block.displayRange.end)),
      );
    }
    if (spans.isEmpty) {
      spans.add(
        TextSpan(
          text: displayText.substring(
            block.displayRange.start,
            block.displayRange.end,
          ),
        ),
      );
    }
    return spans;
  }

  InlineSpan _inlineSpanForRun(BuildContext context, FlarkRenderInlineRun run) {
    final text = displayText.substring(
      run.displayRange.start,
      run.displayRange.end,
    );
    final action = run.action;
    if (action?.kind == FlarkRenderInlineActionKind.image) {
      final target = FlarkRenderOverlayTarget(
        kind: FlarkRenderOverlayKind.image,
        sourceRange: run.sourceRange,
        displayRange: run.displayRange,
        action: action,
      );
      return WidgetSpan(
        alignment: PlaceholderAlignment.middle,
        child: _PreviewImageCard(
          label: action!.label ?? text,
          destination: action.destination,
          title: action.title,
          interactions: FlarkMarkdownInteractions.maybeOf(context),
          target: target,
        ),
      );
    }
    if (action?.kind == FlarkRenderInlineActionKind.link) {
      final interactions = FlarkMarkdownInteractions.maybeOf(context);
      if (interactions != null && interactions.config.enableLinkMenus) {
        return WidgetSpan(
          alignment: PlaceholderAlignment.baseline,
          baseline: TextBaseline.alphabetic,
          child: _InlineLinkMenu(
            interactions: interactions,
            target: FlarkRenderOverlayTarget(
              kind: FlarkRenderOverlayKind.link,
              sourceRange: run.sourceRange,
              displayRange: run.displayRange,
              action: action,
            ),
            text: text,
            style: _inlineStyle(run),
          ),
        );
      }
    }

    return TextSpan(text: text, style: _inlineStyle(run));
  }
}

final class _InlineLinkMenu extends StatefulWidget {
  const _InlineLinkMenu({
    required this.interactions,
    required this.target,
    required this.text,
    required this.style,
  });

  final FlarkMarkdownInteractions interactions;
  final FlarkRenderOverlayTarget target;
  final String text;
  final TextStyle? style;

  @override
  State<_InlineLinkMenu> createState() => _InlineLinkMenuState();
}

final class _InlineLinkMenuState extends State<_InlineLinkMenu> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final style = widget.style;
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          GestureDetector(
            key: const Key('FlarkInlineLinkMenuButton'),
            behavior: HitTestBehavior.opaque,
            onTap: () => setState(() => _open = !_open),
            child: Text(widget.text, style: style),
          ),
          if (_open)
            Padding(
              padding: const EdgeInsets.only(top: 3),
              child: DecoratedBox(
                key: const Key('FlarkInlineLinkMenu'),
                decoration: BoxDecoration(
                  color: const Color(0xFFFFFFFF),
                  border: Border.all(color: const Color(0xFFD7DEE8)),
                  borderRadius: const BorderRadius.all(Radius.circular(6)),
                ),
                child: Padding(
                  padding: const EdgeInsets.all(3),
                  child: Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      _InlineLinkMenuAction(
                        label: 'Open',
                        onTap: () {
                          widget.interactions.openTarget(widget.target);
                          setState(() => _open = false);
                        },
                      ),
                      if (widget.interactions.editable)
                        _InlineLinkMenuAction(
                          label: 'Edit',
                          onTap: () {
                            widget.interactions.editTarget(
                              context,
                              widget.target,
                            );
                            setState(() => _open = false);
                          },
                        ),
                      _InlineLinkMenuAction(
                        label: 'Copy',
                        onTap: () {
                          widget.interactions.copyTarget(widget.target);
                          setState(() => _open = false);
                        },
                      ),
                      if (widget.interactions.editable)
                        _InlineLinkMenuAction(
                          label: 'Remove',
                          onTap: () {
                            widget.interactions.removeLink(widget.target);
                            setState(() => _open = false);
                          },
                        ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

final class _InlineLinkMenuAction extends StatelessWidget {
  const _InlineLinkMenuAction({
    super.key,
    required this.label,
    required this.onTap,
  });

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0xFFF8FAFC),
          border: Border.all(color: const Color(0xFFD7DEE8)),
          borderRadius: const BorderRadius.all(Radius.circular(4)),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
          child: Text(
            label,
            style: style.copyWith(fontSize: (style.fontSize ?? 14) - 1),
          ),
        ),
      ),
    );
  }
}

final class _PreviewCodeCopyButton extends StatelessWidget {
  const _PreviewCodeCopyButton({required this.source});

  final String source;

  @override
  Widget build(BuildContext context) {
    return _InlineLinkMenuAction(
      key: const Key('FlarkReadOnlyPreviewCodeCopyButton'),
      label: 'Copy',
      onTap: () {
        Clipboard.setData(ClipboardData(text: source));
      },
    );
  }
}

final class _PreviewTaskCheckbox extends StatelessWidget {
  const _PreviewTaskCheckbox({required this.checked});

  final bool checked;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      key: const Key('FlarkReadOnlyPreviewTaskCheckbox'),
      width: 14,
      height: 14,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: checked ? const Color(0xFF2E7D32) : const Color(0xFFFFFFFF),
          border: Border.all(
            color: checked ? const Color(0xFF2E7D32) : const Color(0xFF7A8CA3),
          ),
          borderRadius: const BorderRadius.all(Radius.circular(3)),
        ),
        child: checked
            ? const Center(
                child: Text(
                  '✓',
                  style: TextStyle(
                    color: Color(0xFFFFFFFF),
                    fontSize: 9,
                    fontWeight: FontWeight.w700,
                    height: 1,
                  ),
                ),
              )
            : const SizedBox.shrink(),
      ),
    );
  }
}

final class _PreviewTable extends StatelessWidget {
  const _PreviewTable({
    required this.block,
    required this.displayText,
    required this.baseStyle,
  });

  final FlarkRenderBlock block;
  final String displayText;
  final TextStyle baseStyle;

  @override
  Widget build(BuildContext context) {
    final rows = _tableRowsFromRenderPlan(block, displayText);
    if (rows.isEmpty) {
      return Text(
        displayText.substring(block.displayRange.start, block.displayRange.end),
        style: baseStyle,
      );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: DecoratedBox(
        key: const Key('FlarkReadOnlyPreviewTable'),
        decoration: BoxDecoration(
          border: Border.all(color: const Color(0xFFD7DEE8)),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: ClipRRect(
          borderRadius: const BorderRadius.all(Radius.circular(6)),
          child: Table(
            defaultVerticalAlignment: TableCellVerticalAlignment.middle,
            border: TableBorder.symmetric(
              inside: const BorderSide(color: Color(0xFFE2E8F0)),
            ),
            children: [
              for (var rowIndex = 0; rowIndex < rows.length; rowIndex++)
                TableRow(
                  decoration: BoxDecoration(
                    color: rowIndex == 0
                        ? const Color(0xFFF1F4F8)
                        : const Color(0xFFFFFFFF),
                  ),
                  children: [
                    for (final cell in rows[rowIndex])
                      Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 6,
                        ),
                        child: Text(
                          cell,
                          style: rowIndex == 0
                              ? baseStyle.copyWith(fontWeight: FontWeight.w700)
                              : baseStyle,
                        ),
                      ),
                  ],
                ),
            ],
          ),
        ),
      ),
    );
  }
}

final class _PreviewImageCard extends StatelessWidget {
  const _PreviewImageCard({
    required this.label,
    required this.destination,
    this.title,
    this.interactions,
    this.target,
  });

  final String label;
  final String destination;
  final String? title;
  final FlarkMarkdownInteractions? interactions;
  final FlarkRenderOverlayTarget? target;

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style;
    final effectiveLabel = label.isEmpty ? destination : label;
    final card = Semantics(
      image: true,
      label: 'Image: $effectiveLabel',
      value: destination,
      child: DecoratedBox(
        key: const Key('FlarkReadOnlyPreviewImageCard'),
        decoration: BoxDecoration(
          color: const Color(0xFFF8FAFC),
          border: Border.all(color: const Color(0xFFD7DEE8)),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              DecoratedBox(
                decoration: BoxDecoration(
                  color: const Color(0xFFE2E8F0),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 5,
                    vertical: 2,
                  ),
                  child: Text(
                    'IMG',
                    style: style.copyWith(
                      color: const Color(0xFF42526E),
                      fontSize: (style.fontSize ?? 14) - 3,
                      fontWeight: FontWeight.w700,
                      height: 1,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 6),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 220),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      effectiveLabel,
                      overflow: TextOverflow.ellipsis,
                      style: style.copyWith(fontWeight: FontWeight.w700),
                    ),
                    Text(
                      title == null ? destination : '$destination - $title',
                      overflow: TextOverflow.ellipsis,
                      style: style.copyWith(
                        color: const Color(0xFF5B6B7F),
                        fontSize: (style.fontSize ?? 14) - 2,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );

    final interactions = this.interactions;
    final target = this.target;
    if (interactions == null || target == null) {
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 2),
        child: card,
      );
    }

    return _PreviewImageActionMenu(
      interactions: interactions,
      target: target,
      child: card,
    );
  }
}

final class _PreviewImageActionMenu extends StatefulWidget {
  const _PreviewImageActionMenu({
    required this.interactions,
    required this.target,
    required this.child,
  });

  final FlarkMarkdownInteractions interactions;
  final FlarkRenderOverlayTarget target;
  final Widget child;

  @override
  State<_PreviewImageActionMenu> createState() =>
      _PreviewImageActionMenuState();
}

final class _PreviewImageActionMenuState
    extends State<_PreviewImageActionMenu> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 2),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          GestureDetector(
            key: const Key('FlarkReadOnlyPreviewImageMenuButton'),
            behavior: HitTestBehavior.opaque,
            onTap: () => setState(() => _open = !_open),
            child: widget.child,
          ),
          if (_open)
            Padding(
              padding: const EdgeInsets.only(top: 3),
              child: DecoratedBox(
                key: const Key('FlarkReadOnlyPreviewImageMenu'),
                decoration: BoxDecoration(
                  color: const Color(0xFFFFFFFF),
                  border: Border.all(color: const Color(0xFFD7DEE8)),
                  borderRadius: const BorderRadius.all(Radius.circular(6)),
                ),
                child: Padding(
                  padding: const EdgeInsets.all(3),
                  child: Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      _InlineLinkMenuAction(
                        label: 'Open',
                        onTap: () {
                          widget.interactions.openTarget(widget.target);
                          setState(() => _open = false);
                        },
                      ),
                      _InlineLinkMenuAction(
                        label: 'Copy',
                        onTap: () {
                          widget.interactions.copyTarget(widget.target);
                          setState(() => _open = false);
                        },
                      ),
                      if (widget.interactions.editable)
                        _InlineLinkMenuAction(
                          label: 'Edit',
                          onTap: () {
                            widget.interactions.editTarget(
                              context,
                              widget.target,
                            );
                            setState(() => _open = false);
                          },
                        ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

List<List<String>> _tableRowsFromRenderPlan(
  FlarkRenderBlock block,
  String displayText,
) {
  final table = block.table;
  if (table == null || table.rows.isEmpty) return const [];

  final columnCount = _resolvedRenderTableColumnCount(table);
  if (columnCount <= 0) return const [];

  return [
    for (final row in table.rows)
      [
        for (var index = 0; index < columnCount; index++)
          if (index < row.cells.length)
            _displayCellText(displayText, row.cells[index].displayRange)
          else
            '',
      ],
  ];
}

int _resolvedRenderTableColumnCount(FlarkRenderTableDescriptor table) {
  if (table.columnAlignments.isNotEmpty) return table.columnAlignments.length;
  var columnCount = 0;
  for (final row in table.rows) {
    if (row.cells.length > columnCount) columnCount = row.cells.length;
  }
  return columnCount;
}

String _displayCellText(String displayText, FlarkSourceRange range) {
  final start = range.start.clamp(0, displayText.length);
  final end = range.end.clamp(start, displayText.length);
  return displayText.substring(start, end).trim();
}

TextStyle _blockStyle(TextStyle baseStyle, FlarkRenderBlock block) {
  if (block.codeBlock != null) {
    return baseStyle.copyWith(
      color: const Color(0xFF17202A),
      fontFamily: 'monospace',
      height: 1.35,
    );
  }

  if (block.kind == FlarkMarkdownBlockKind.blockquote) {
    return baseStyle.copyWith(color: const Color(0xFF42526E));
  }

  final headingLevel = switch (block.styleToken) {
    FlarkRenderTextStyleToken.heading1 => 1,
    FlarkRenderTextStyleToken.heading2 => 2,
    FlarkRenderTextStyleToken.heading3 => 3,
    FlarkRenderTextStyleToken.heading4 => 4,
    FlarkRenderTextStyleToken.heading5 => 5,
    FlarkRenderTextStyleToken.heading6 => 6,
    _ => null,
  };
  if (headingLevel == null) return baseStyle;
  final baseSize = baseStyle.fontSize ?? 14;
  return baseStyle.copyWith(
    fontSize: baseSize + (7 - headingLevel) * 2,
    fontWeight: FontWeight.w700,
  );
}

TextStyle? _inlineStyle(FlarkRenderInlineRun run) {
  return switch (run.styleToken) {
    FlarkRenderTextStyleToken.emphasis => const TextStyle(
      fontStyle: FontStyle.italic,
    ),
    FlarkRenderTextStyleToken.strong => const TextStyle(
      fontWeight: FontWeight.w700,
    ),
    FlarkRenderTextStyleToken.inlineCode => const TextStyle(
      fontFamily: 'monospace',
    ),
    FlarkRenderTextStyleToken.strikethrough => const TextStyle(
      decoration: TextDecoration.lineThrough,
    ),
    FlarkRenderTextStyleToken.link => const TextStyle(
      color: Color(0xFF0057B8),
      decoration: TextDecoration.underline,
    ),
    _ => null,
  };
}
