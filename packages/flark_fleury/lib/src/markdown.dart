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

/// The opening of [markdown] that the source fallback shows, about 1,024
/// code units. The fallback is also where text with an unpaired surrogate
/// ends up, so the cut never splits a surrogate pair and an unpaired one
/// shows as U+FFFD: cut in two, an emoji at the cut was not shown at all.
/// Copying still takes the source exactly as written.
String _sourcePreview(String markdown) {
  const limit = 1024;
  final preview = StringBuffer();
  var i = 0;
  while (i < markdown.length && i < limit) {
    final unit = markdown.codeUnitAt(i);
    final next = i + 1 < markdown.length ? markdown.codeUnitAt(i + 1) : 0;
    if (unit >= 0xD800 && unit <= 0xDBFF && next >= 0xDC00 && next <= 0xDFFF) {
      preview.writeCharCode(0x10000 + ((unit - 0xD800) << 10) + next - 0xDC00);
      i += 2;
    } else {
      preview.writeCharCode(unit >= 0xD800 && unit <= 0xDFFF ? 0xFFFD : unit);
      i++;
    }
  }
  if (i < markdown.length) preview.write('…');
  return preview.toString();
}

class _ReadCellController implements FlarkCellController {
  _ReadCellController(this.reader);
  final FlarkReader reader;
  @override
  FlarkReadDocument get editor => reader.document!;
  @override
  String languageInfo(ProjectedRow row) => '';
  @override
  CodeHighlight? colorsFor(ProjectedRow row) => null;
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
          Text(_sourcePreview(widget.markdown)),
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
      // A detector hears keys only while focus is inside it, so it wraps the
      // reader's focus node: beneath it, Ctrl/Cmd+A and +C never arrived.
      child: KeyDetector(
        onKey: (event) {
          // Command arrives as super, as the editor reads it.
          if (!widget.selectable || !(event.hasCtrl || event.hasSuper)) {
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
        child: Focus(
          focusNode: _focus,
          child: GestureDetector(
            onTapDown: (event) => _select(event.globalPosition),
            onDragUpdate: (event) =>
                _select(event.globalPosition, extend: true),
            onTapUp: (event) {
              // Only a link's own painted text, or an image in it, opens it.
              final at = event.globalPosition, doc = _reader.document!;
              final hit = _viewport.resourceAt(
                doc,
                at.col,
                at.row,
                caret: false,
              );
              final link = hit?.isImage != true
                  ? hit
                  : doc.document.resourceAt(
                      FlarkSelection(hit!.start, hit.end),
                      image: false,
                    );
              if (link == null || !doc.selection.isCollapsed) return;
              final uri = flarkOpenableUri(link.destination, widget.baseUri);
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
