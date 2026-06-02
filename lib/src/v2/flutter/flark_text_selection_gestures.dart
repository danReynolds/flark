import 'package:flutter/cupertino.dart'
    show
        cupertinoDesktopTextSelectionHandleControls,
        cupertinoTextSelectionHandleControls;
import 'package:flutter/foundation.dart'
    show TargetPlatform, defaultTargetPlatform;
import 'package:flutter/material.dart'
    show
        desktopTextSelectionHandleControls,
        materialTextSelectionHandleControls;
import 'package:flutter/widgets.dart';

TextSelectionControls? flarkTextSelectionControlsForPlatform(
  BuildContext context,
) {
  if (Overlay.maybeOf(context) == null) return null;
  return switch (defaultTargetPlatform) {
    TargetPlatform.iOS => cupertinoTextSelectionHandleControls,
    TargetPlatform.macOS => cupertinoDesktopTextSelectionHandleControls,
    TargetPlatform.android ||
    TargetPlatform.fuchsia => materialTextSelectionHandleControls,
    TargetPlatform.linux ||
    TargetPlatform.windows => desktopTextSelectionHandleControls,
  };
}

Widget flarkEditableTextGestureDetector({
  Key? key,
  required GlobalKey<EditableTextState> editableTextKey,
  required Widget child,
}) {
  return TextSelectionGestureDetectorBuilder(
    delegate: _FlarkTextSelectionGestureDelegate(editableTextKey),
  ).buildGestureDetector(
    key: key,
    behavior: HitTestBehavior.translucent,
    child: child,
  );
}

final class _FlarkTextSelectionGestureDelegate
    implements TextSelectionGestureDetectorBuilderDelegate {
  const _FlarkTextSelectionGestureDelegate(this.editableTextKey);

  @override
  final GlobalKey<EditableTextState> editableTextKey;

  @override
  bool get forcePressEnabled => false;

  @override
  bool get selectionEnabled => true;
}
