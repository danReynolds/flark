import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';
import 'render_surface.dart';

/// A bounded read-only Markdown surface sharing Flark's projection, layout,
/// virtualization, and paint path with the editor.
///
/// The caller owns the controller lifecycle. This first public form is useful
/// when the same document is presented in editing and reading contexts without
/// creating a second parser or renderer.
final class FlarkMarkdownView extends StatefulWidget {
  const FlarkMarkdownView({
    required this.controller,
    this.textStyle = const TextStyle(
      color: Color(0xff202124),
      fontSize: 17,
      height: 1.45,
    ),
    this.padding = const EdgeInsets.symmetric(horizontal: 32, vertical: 28),
    super.key,
  });

  final FlarkEditorController controller;
  final TextStyle textStyle;
  final EdgeInsets padding;

  @override
  State<FlarkMarkdownView> createState() => _FlarkMarkdownViewState();
}

final class _FlarkMarkdownViewState extends State<FlarkMarkdownView> {
  final GlobalKey _surfaceKey = GlobalKey();

  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;

  Map<Type, GestureRecognizerFactory> get _touchScrollRecognizer => {
    VerticalDragGestureRecognizer:
        GestureRecognizerFactoryWithHandlers<VerticalDragGestureRecognizer>(
          () => VerticalDragGestureRecognizer(
            debugOwner: this,
            supportedDevices: const {
              PointerDeviceKind.touch,
              PointerDeviceKind.stylus,
              PointerDeviceKind.invertedStylus,
            },
          ),
          (recognizer) {
            recognizer.onUpdate = (details) {
              _surface?.scrollBy(-details.delta.dy);
            };
          },
        ),
  };

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.controller.continueParsing();
    });
  }

  @override
  void didUpdateWidget(FlarkMarkdownView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) widget.controller.continueParsing();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.basic,
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerSignal: (event) {
          if (event is PointerScrollEvent) {
            _surface?.scrollBy(event.scrollDelta.dy);
          }
        },
        onPointerPanZoomUpdate: (event) {
          _surface?.scrollBy(-event.localPanDelta.dy);
        },
        child: RawGestureDetector(
          behavior: HitTestBehavior.opaque,
          gestures: _touchScrollRecognizer,
          child: FlarkRenderSurfaceWidget(
            key: _surfaceKey,
            controller: widget.controller,
            textStyle: widget.textStyle,
            padding: widget.padding,
            caretColor: const Color(0x00000000),
            selectionColor: const Color(0x00000000),
            includeEditingState: false,
          ),
        ),
      ),
    );
  }
}
