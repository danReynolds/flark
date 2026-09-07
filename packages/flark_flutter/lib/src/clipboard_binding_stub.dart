/// Native clipboard delivery belongs to Flutter's platform actions.
class EditorClipboardBinding {
  EditorClipboardBinding({
    required bool Function() isActive,
    required String? Function() selectedText,
    required void Function() cut,
    required void Function(String) paste,
  });
  void dispose() {}
}
