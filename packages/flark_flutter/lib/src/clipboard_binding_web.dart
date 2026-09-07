import 'dart:js_interop';
import 'package:web/web.dart' as web;

/// The browser's default copy would export the input method's source mirror.
/// Override only clipboard events owned by this focused editor. A browser cut
/// and paste commit the same semantic commands as native Flutter, without a
/// raw DOM edit that loses whether text was pasted or typed.
class EditorClipboardBinding {
  EditorClipboardBinding({
    required bool Function() isActive,
    required String? Function() selectedText,
    required void Function() cut,
    required void Function(String) paste,
  }) {
    _listener = ((web.Event event) {
      if (!isActive()) return;
      final data = (event as web.ClipboardEvent).clipboardData;
      if (data == null) return;
      if (event.type == 'paste') {
        final text = data.getData('text/plain');
        if (text.isEmpty) return;
        event.preventDefault();
        paste(text);
        return;
      }
      final text = selectedText();
      if (text == null) return;
      data.setData('text/plain', text);
      event.preventDefault();
      if (event.type == 'cut') cut();
    }).toJS;
    web.document.addEventListener('copy', _listener, true.toJS);
    web.document.addEventListener('cut', _listener, true.toJS);
    web.document.addEventListener('paste', _listener, true.toJS);
  }

  late final JSFunction _listener;
  void dispose() {
    web.document.removeEventListener('copy', _listener, true.toJS);
    web.document.removeEventListener('cut', _listener, true.toJS);
    web.document.removeEventListener('paste', _listener, true.toJS);
  }
}
