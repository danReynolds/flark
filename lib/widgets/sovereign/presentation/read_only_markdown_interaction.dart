import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';

import 'sovereign_inline_actions_overlay.dart';

class SovereignReadOnlyTapTracker {
  int? _activePointerId;
  Offset? _activePointerDownGlobal;
  bool _activePointerMovedBeyondTapSlop = false;

  void handlePointerDown(PointerDownEvent event) {
    _activePointerId = event.pointer;
    _activePointerDownGlobal = event.position;
    _activePointerMovedBeyondTapSlop = false;
  }

  void handlePointerMove(PointerMoveEvent event) {
    if (_activePointerId != event.pointer) return;
    final downGlobal = _activePointerDownGlobal;
    if (downGlobal == null) return;
    if ((event.position - downGlobal).distance > kTouchSlop) {
      _activePointerMovedBeyondTapSlop = true;
    }
  }

  void handlePointerCancel(PointerCancelEvent event) {
    if (_activePointerId != event.pointer) return;
    _reset();
  }

  bool consumeIsTap(PointerUpEvent event) {
    final pointerMatched = _activePointerId == event.pointer;
    final moved = _activePointerMovedBeyondTapSlop;
    _reset();
    return pointerMatched && !moved;
  }

  static bool tapInsideInlineTarget({
    required RenderParagraph renderObject,
    required SovereignResolvedInlineActionsTarget target,
    required Offset globalPosition,
    double hitSlop = 2,
  }) {
    final local = renderObject.globalToLocal(globalPosition);
    final boxes = renderObject.getBoxesForSelection(
      TextSelection(
        baseOffset: target.target.displayStart,
        extentOffset: target.target.displayEnd,
      ),
    );
    for (final box in boxes) {
      if (box.toRect().inflate(hitSlop).contains(local)) {
        return true;
      }
    }
    return false;
  }

  void _reset() {
    _activePointerId = null;
    _activePointerDownGlobal = null;
    _activePointerMovedBeyondTapSlop = false;
  }
}
