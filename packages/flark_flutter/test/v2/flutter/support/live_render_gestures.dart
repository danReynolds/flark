import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/gestures.dart'
    show kDoubleTapMinTime, kLongPressTimeout;
import 'package:flutter_test/flutter_test.dart';

import 'live_render_sequence.dart';

/// Gesture drivers for [LiveRenderSequence], layered on its public
/// [LiveRenderSequence.caretRectForSource] so a suite can long-press,
/// double-tap, or drag at a real *source* position and then assert the
/// source-space `controller.selection` the gesture produced.
///
/// Each method issues a genuine pointer stream through the same
/// `TextSelectionGestureDetectorBuilder` the production editor wires (see
/// `flarkEditableTextGestureDetector`). Nothing here synthesizes a selection;
/// the resulting selection is whatever Flark's projected-selection mapping
/// computes for a real tap/drag — which is exactly the mapping under test.
///
/// Word-selecting gestures are platform-sensitive: a double-tap selects a word
/// on every platform, but a long-press only selects a word on non-Apple target
/// platforms (on iOS/macOS it positions the caret). A suite that asserts word
/// selection from a long-press must therefore pin the target platform via
/// `debugDefaultTargetPlatformOverride`.
extension LiveRenderGestures on LiveRenderSequence {
  /// Long-presses at the caret center for [sourceOffset] and settles.
  ///
  /// Holds the pointer down past [kLongPressTimeout] so the long-press
  /// recognizer fires, then releases. Throws if no editable currently renders
  /// [sourceOffset] (a null [LiveRenderSequence.caretRectForSource]).
  Future<void> longPressAtSource(int sourceOffset) async {
    final center = caretRectForSource(sourceOffset)!.center;
    final gesture = await tester.startGesture(center);
    // Cross the long-press threshold without moving so the recognizer wins.
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 50));
    await gesture.up();
    await tester.pumpAndSettle();
  }

  /// Double-taps at the caret center for [sourceOffset] and settles.
  ///
  /// Two taps separated by more than [kDoubleTapMinTime] and less than the
  /// double-tap timeout, so the second tap-down is recognized as a double-tap
  /// and selects the word under it. Throws if no editable renders
  /// [sourceOffset].
  Future<void> doubleTapAtSource(int sourceOffset) async {
    final center = caretRectForSource(sourceOffset)!.center;
    await tester.tapAt(center);
    // Long enough to clear the double-tap minimum, short enough to stay inside
    // the double-tap window (kDoubleTapTimeout, 300ms).
    await tester.pump(kDoubleTapMinTime + const Duration(milliseconds: 25));
    await tester.tapAt(center);
    await tester.pumpAndSettle();
  }

  /// Presses at the [fromSource] caret center, drags to the [toSource] caret
  /// center, and releases — a character-granularity drag selection.
  ///
  /// Uses a mouse pointer: a *touch* drag over an editable scrolls rather than
  /// selects (touch selection needs a long-press-then-drag), so a plain
  /// press-move-release selects text only for the mouse device kind — the same
  /// pointer kind the existing live-editor drag tests use.
  ///
  /// Throws if no editable renders either endpoint.
  Future<void> dragSelectSource(int fromSource, int toSource) async {
    final from = caretRectForSource(fromSource)!.center;
    final to = caretRectForSource(toSource)!.center;
    final gesture = await tester.startGesture(
      from,
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await gesture.moveTo(to);
    await tester.pump();
    await gesture.up();
    await tester.pumpAndSettle();
  }
}
