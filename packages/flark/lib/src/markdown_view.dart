import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'dart:async';

import 'package:flark_core/flark_core.dart';

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
    this.onSemanticTarget,
    this.debugPaintObserver,
    super.key,
  });

  final FlarkEditorController controller;
  final TextStyle textStyle;
  final EdgeInsets padding;
  final ValueChanged<FlarkSemanticTarget>? onSemanticTarget;
  final ValueChanged<FlarkSurfacePaintObservation>? debugPaintObserver;

  @override
  State<FlarkMarkdownView> createState() => _FlarkMarkdownViewState();
}

final class _FlarkMarkdownViewState extends State<FlarkMarkdownView> {
  final GlobalKey _surfaceKey = GlobalKey();
  final Map<int, Offset> _pointerDown = <int, Offset>{};

  RenderFlarkSurface? get _surface =>
      _surfaceKey.currentContext?.findRenderObject() as RenderFlarkSurface?;

  Map<Type, GestureRecognizerFactory> get _gestures => {
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

  Future<void> _activateTarget(Offset localPosition) async {
    final fact = _surface?.positionForOffset(localPosition)?.semanticTargetFact;
    if (fact == null || widget.onSemanticTarget == null) return;
    final target = await widget.controller.querySemanticTarget(fact);
    if (mounted && target != null) widget.onSemanticTarget?.call(target);
  }

  void _rememberPointerDown(PointerDownEvent event) {
    _pointerDown[event.pointer] = event.position;
  }

  void _forgetPointer(PointerEvent event) {
    _pointerDown.remove(event.pointer);
  }

  void _activatePointerUp(PointerUpEvent event) {
    final down = _pointerDown.remove(event.pointer);
    if (down == null || (event.position - down).distance > kTouchSlop) {
      return;
    }
    final localPosition = _surface?.globalToLocal(event.position);
    if (localPosition != null) unawaited(_activateTarget(localPosition));
  }

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
    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: _rememberPointerDown,
      onPointerCancel: _forgetPointer,
      onPointerUp: _activatePointerUp,
      onPointerSignal: (event) {
        if (event is PointerScrollEvent) {
          _surface?.scrollBy(event.scrollDelta.dy);
        }
      },
      onPointerPanZoomUpdate: (event) {
        _surface?.scrollBy(-event.localPanDelta.dy);
      },
      child: MouseRegion(
        cursor: SystemMouseCursors.basic,
        child: RawGestureDetector(
          behavior: HitTestBehavior.opaque,
          gestures: _gestures,
          child: FlarkRenderSurfaceWidget(
            key: _surfaceKey,
            controller: widget.controller,
            textStyle: widget.textStyle,
            padding: widget.padding,
            caretColor: const Color(0x00000000),
            selectionColor: const Color(0x00000000),
            includeEditingState: false,
            debugPaintObserver: widget.debugPaintObserver,
          ),
        ),
      ),
    );
  }
}
