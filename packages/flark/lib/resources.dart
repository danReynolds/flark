/// Toolkit-independent resource actions. Hosts own presentation and lifetime.
library;

import 'flark.dart';

/// Resolve document-relative resources without interpreting Markdown.
Uri? flarkResourceUri(String value, Uri? base) {
  final uri = Uri.tryParse(value);
  return uri == null ? null : base?.resolveUri(uri) ?? uri;
}

Uri? flarkOpenableUri(String value, Uri? base) {
  final uri = flarkResourceUri(value, base);
  return uri != null && const {'https', 'http', 'mailto'}.contains(uri.scheme)
      ? uri
      : null;
}

/// A bounded editing request created by the host. All methods become inert
/// when its target changes, the editor disappears, or the presentation closes.
class FlarkResourceSession {
  FlarkResourceSession({
    required this.image,
    required this.resource,
    required this.selectedText,
    required bool Function() isActive,
    required bool Function(FlarkCommand) apply,
    void Function()? onOpen,
  }) : _isActive = isActive,
       _apply = apply,
       _onOpen = onOpen;
  final bool image;
  final InlineResource? resource;
  final String selectedText;
  final bool Function() _isActive;
  final bool Function(FlarkCommand) _apply;
  final void Function()? _onOpen;
  void Function()? get onOpen => _onOpen == null ? null : open;
  bool _closed = false;
  bool get active => !_closed && _isActive();
  String get label => resource?.text ?? selectedText;
  String get destination => resource?.destination ?? '';
  String get title => resource?.title ?? '';

  bool save({
    required String destination,
    required String label,
    String title = '',
  }) {
    if (!active) return false;
    if (resource != null &&
        destination.trim() == this.destination &&
        label == this.label &&
        title == this.title) {
      close();
      return true;
    }
    if (destination.trim().isEmpty) return false;
    final text = label == this.label && !(image && resource == null)
        ? null
        : label;
    final command = image
        ? SetImage(destination.trim(), alt: text, title: title)
        : SetLink(destination.trim(), text: text, title: title);
    return _submit(command);
  }

  bool remove() =>
      active &&
      resource != null &&
      _submit(image ? const RemoveImage() : const RemoveLink());
  void open() {
    if (active) _onOpen?.call();
  }

  bool _submit(FlarkCommand command) {
    final result = _apply(command);
    if (result) close();
    return result;
  }

  /// End the request without changing the document.
  void close() {
    _closed = true;
  }
}

/// Resource facts and host-guarded actions, shared by all popover designs.
class FlarkLinkActions {
  const FlarkLinkActions({
    required this.resource,
    required this.dismiss,
    this.open,
    this.edit,
    this.remove,
  });
  final InlineResource resource;
  final void Function() dismiss;
  final void Function()? open, edit, remove;
}
