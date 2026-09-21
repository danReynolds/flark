import 'package:flark/rendering.dart';
import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/session.dart';
import 'package:flark/resources.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'controller.dart';
import 'code_colors.dart';
import 'surface.dart';
import 'theme.dart';
import 'image_previews.dart';

/// Read-only Markdown in the parent's layout and scroll surface. It has no
/// editing controller, history, keyboard input connection or indentation lane.
class FlarkMarkdown extends StatefulWidget {
  const FlarkMarkdown({
    super.key,
    required this.markdown,
    this.selectable = false,
    this.theme,
    this.onOpenLink,
    this.baseUri,
    this.imageProvider,
  });
  final String markdown;
  final bool selectable;
  final FlarkThemeData? theme;
  final ValueChanged<Uri>? onOpenLink;
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  @override
  State<FlarkMarkdown> createState() => _MarkdownState();
}

class _ReadSurfaceController extends ChangeNotifier
    implements FlarkSurfaceController {
  _ReadSurfaceController(this.reader) {
    reader.addListener(notifyListeners);
  }
  final FlarkReader reader;
  @override
  FlarkReadDocument get editor => reader.document!;
  @override
  FlarkCodeColors? get codeColors => null;
  @override
  String get text => editor.source;
  @override
  void sourceMode(bool enabled) {}
  @override
  bool command(FlarkCommand command, {int? expectedRevision}) {
    if (expectedRevision != null && expectedRevision != editor.revision) {
      return false;
    }
    final before = editor.revision;
    switch (command) {
      case SetSelection(:final base, :final extent):
        reader.select(FlarkSelection(base, extent));
      case PlaceCaret(:final row, :final offset, :final extend):
        if (editor.sourceMode) return false;
        final source = editor.projection.rows[row].sourceForDisplay(offset);
        reader.select(
          FlarkSelection(extend ? editor.selection.base : source, source),
        );
      case SelectAll():
        reader.select(FlarkSelection(0, text.length));
      default:
        return false;
    }
    return editor.revision != before;
  }

  @override
  void dispose() {
    reader.removeListener(notifyListeners);
    super.dispose();
  }
}

class _MarkdownState extends State<FlarkMarkdown> {
  late final _reader = FlarkReader(widget.markdown)..addListener(_changed);
  late final _controller = _ReadSurfaceController(_reader);
  final _surfaceKey = GlobalKey();
  final _focus = FocusNode(debugLabel: 'Markdown selection');
  bool _dragging = false;
  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;
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
    _controller.dispose();
    _reader.removeListener(_changed);
    _reader.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _copy() {
    final doc = _reader.document!;
    final text = doc.sourceMode
        ? doc.source
        : doc.document.visibleText(doc.selection.start, doc.selection.end);
    unawaited(Clipboard.setData(ClipboardData(text: text)));
  }

  @override
  Widget build(BuildContext context) {
    if (_reader.status == FlarkStatus.loading) {
      return const Text('Loading Markdown…');
    }
    if (_reader.status != FlarkStatus.ready) {
      return TextButton(
        onPressed: () {
          unawaited(_reader.retry().catchError((Object _) {}));
        },
        child: const Text('Unable to render Markdown · Retry'),
      );
    }
    if (_reader.document!.sourceMode) {
      // Explicit fallback, with access to every source character. Never present
      // a clipped source page as a complete rendered article.
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Rendered preview unavailable for this document.'),
          TextButton(
            onPressed: _copy,
            child: const Text('Copy complete Markdown'),
          ),
          Text(
            widget.markdown.length > 1024
                ? '${widget.markdown.substring(0, 1024)}…'
                : widget.markdown,
          ),
        ],
      );
    }
    return Focus(
      focusNode: _focus,
      onFocusChange: (_) => _changed(),
      canRequestFocus: widget.selectable,
      onKeyEvent: (_, event) {
        if (!widget.selectable ||
            event is! KeyDownEvent ||
            !(HardwareKeyboard.instance.isControlPressed ||
                HardwareKeyboard.instance.isMetaPressed)) {
          return KeyEventResult.ignored;
        }
        if (event.logicalKey == LogicalKeyboardKey.keyA) {
          _controller.command(const SelectAll());
          return KeyEventResult.handled;
        }
        if (event.logicalKey == LogicalKeyboardKey.keyC) {
          _copy();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Listener(
        onPointerDown: (event) {
          final surface = _surface;
          if (surface == null) return;
          final point = surface.globalToLocal(event.position);
          final link = surface.linkAt(point);
          final uri = link == null
              ? null
              : flarkOpenableUri(link.destination, widget.baseUri);
          if (uri != null && widget.onOpenLink != null) {
            widget.onOpenLink!(uri);
            return;
          }
          if (widget.selectable) {
            _focus.requestFocus();
            _dragging = true;
            surface.place(point);
          }
        },
        onPointerMove: (event) {
          final surface = _surface;
          if (_dragging && surface != null) {
            surface.place(surface.globalToLocal(event.position), extend: true);
          }
        },
        onPointerUp: (_) {
          _dragging = false;
        },
        onPointerCancel: (_) {
          _dragging = false;
        },
        child: FlarkSurface(
          key: _surfaceKey,
          controller: _controller,
          theme: FlarkThemeData.resolve(context, overrides: widget.theme),
          textScaler: MediaQuery.textScalerOf(context),
          focused: _focus.hasFocus,
          selectable: widget.selectable,
          onCopy: widget.selectable ? _copy : null,
          onFocus: widget.selectable ? _focus.requestFocus : null,
          readOnly: true,
          viewportHeight: 0,
          scrollOffset: 0,
          imageProvider: widget.imageProvider,
          baseUri: widget.baseUri,
        ),
      ),
    );
  }
}
