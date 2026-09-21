part of 'editor_view.dart';

/// Parent-laid-out Markdown, with no editing session, history or IME claimant.
class FlarkMarkdown extends StatefulWidget {
  const FlarkMarkdown({
    super.key,
    required this.markdown,
    this.selectable = false,
    this.theme,
    this.onOpenLink,
    this.baseUri,
    this.imagePreviewBuilder,
  });
  final String markdown;
  final bool selectable;
  final FlarkCellTheme? theme;
  final void Function(Uri)? onOpenLink;
  final Uri? baseUri;
  final FlarkFleuryImagePreviewBuilder? imagePreviewBuilder;
  @override
  State<FlarkMarkdown> createState() => _MarkdownState();
}

class _ReadCellController implements FlarkCellController {
  _ReadCellController(this.reader);
  final FlarkReader reader;
  @override
  FlarkReadDocument get editor => reader.document!;
  @override
  int get colorRevision => 0;
  @override
  String languageInfo(ProjectedRow row) => '';
  @override
  CodeAnalysis? colorsFor(ProjectedRow row) => null;
  @override
  void setVisibleRows(Iterable<int> rows) {}
}

class _MarkdownState extends State<FlarkMarkdown> {
  late final _reader = FlarkReader(widget.markdown)..addListener(_changed);
  late final _controller = _ReadCellController(_reader);
  final _viewport = _Viewport();
  final _focus = FocusNode(debugLabel: 'Markdown selection');
  @override
  void initState() {
    super.initState();
    _reader;
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void didUpdateWidget(FlarkMarkdown old) {
    super.didUpdateWidget(old);
    _reader.update(widget.markdown);
  }

  @override
  void dispose() {
    _reader.removeListener(_changed);
    _reader.dispose();
    _viewport.dispose();
    _focus.dispose();
    super.dispose();
  }

  int? _sourceAt(CellOffset position) {
    final layout = _viewport.layout;
    if (layout == null) return null;
    final col = position.col - _viewport.origin.col;
    final row = position.row - _viewport.origin.row;
    if (row < 0 || row >= layout.lines.length) return null;
    final line = layout.lineAt(row, col);
    return line.sourceAt(line.hit(col).$1);
  }

  void _select(CellOffset position, {bool extend = false}) {
    if (!widget.selectable) return;
    final source = _sourceAt(position);
    if (source == null) return;
    _focus.requestFocus();
    _reader.select(
      FlarkSelection(
        extend ? _reader.document!.selection.base : source,
        source,
      ),
    );
  }

  void _copy({bool complete = false}) {
    final doc = _reader.document!;
    final text = complete || doc.sourceMode
        ? doc.source
        : doc.document.visibleText(doc.selection.start, doc.selection.end);
    unawaited(ClipboardScope.of(context).write(text));
  }

  @override
  Widget build(BuildContext context) {
    if (_reader.status == FlarkStatus.loading) {
      return const Text('Loading Markdown…');
    }
    if (_reader.status != FlarkStatus.ready) {
      return Button(
        text: 'Unable to render Markdown · Retry',
        onPressed: () {
          unawaited(_reader.retry().catchError((Object _) {}));
        },
      );
    }
    if (_reader.document!.sourceMode) {
      return Column(
        children: [
          const Text('Rendered preview unavailable for this document.'),
          Button(
            text: 'Copy complete Markdown',
            onPressed: () => _copy(complete: true),
          ),
          Text(
            widget.markdown.length > 1024
                ? '${widget.markdown.substring(0, 1024)}…'
                : widget.markdown,
          ),
        ],
      );
    }
    return Semantics(
      role: SemanticRole.textArea,
      label: 'Markdown',
      value: _reader.document!.projection.rows
          .map((row) => row.text)
          .join('\n'),
      state: const SemanticState({'readOnly': true, 'textEditable': false}),
      actions: widget.selectable ? {SemanticAction.copy} : {},
      onAction: (action) {
        if (action == SemanticAction.copy) _copy();
      },
      child: Focus(
        focusNode: _focus,
        child: KeyDetector(
          onKey: (event) {
            if (!widget.selectable ||
                !(event.modifiers.contains(KeyModifier.ctrl) ||
                    event.modifiers.contains(KeyModifier.meta))) {
              return;
            }
            if (event.code == KeyCode.a) {
              _reader.select(FlarkSelection(0, widget.markdown.length));
              event.consume();
            } else if (event.code == KeyCode.c) {
              _copy();
              event.consume();
            }
          },
          child: GestureDetector(
            onTapDown: (event) => _select(event.globalPosition),
            onDragUpdate: (event) =>
                _select(event.globalPosition, extend: true),
            onTapUp: (event) {
              final source = _sourceAt(event.globalPosition);
              if (source == null || !_reader.document!.selection.isCollapsed) {
                return;
              }
              final resource = _reader.document!.document.resourceAt(
                FlarkSelection.collapsed(source),
                image: false,
              );
              final uri = resource == null
                  ? null
                  : flarkOpenableUri(resource.destination, widget.baseUri);
              if (uri != null) widget.onOpenLink?.call(uri);
            },
            child: BoundsObserver(
              notifier: _viewport.bounds,
              child: LayoutBuilder(
                builder: (context, constraints) {
                  final theme = widget.theme ?? FlarkCellTheme.of(context);
                  final policy = MediaQuery.textPolicyOf(context).widths;
                  final size = _viewport.prepare(
                    constraints,
                    _controller,
                    theme,
                    policy,
                    _focus,
                    fullDocument: true,
                  );
                  return SizedBox(
                    width: size.cols,
                    height: size.rows,
                    child: Stack(
                      children: [
                        _Surface(
                          _controller,
                          theme,
                          _focus,
                          _viewport,
                          policy,
                          fullDocument: true,
                        ),
                        for (final slot in _viewport.layout!.images.take(8))
                          Positioned(
                            left: slot.left,
                            top: slot.top,
                            width: slot.width,
                            height: slot.height,
                            child:
                                widget.imagePreviewBuilder?.call(
                                  context,
                                  slot.resource,
                                  flarkOpenableUri(
                                    slot.resource.destination,
                                    widget.baseUri ?? Uri.base,
                                  ),
                                ) ??
                                FlarkImagePreview(
                                  uri: flarkOpenableUri(
                                    slot.resource.destination,
                                    widget.baseUri ?? Uri.base,
                                  ),
                                  label: slot.resource.text,
                                ),
                          ),
                      ],
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
  }
}
