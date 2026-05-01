import 'package:flutter/services.dart';

/// Editable markdown link or image target resolved from the current selection.
class SovereignLinkEditContext {
  /// Range in controller text that should be replaced by the edited target.
  final TextRange replaceRange;

  /// Current link label or image alt text.
  final String label;

  /// Current URL target.
  final String url;

  /// Whether the context came from an existing link/image target.
  final bool isExisting;

  /// Creates a link edit context.
  const SovereignLinkEditContext({
    required this.replaceRange,
    required this.label,
    required this.url,
    required this.isExisting,
  });

  /// Returns a copy with selected fields replaced.
  SovereignLinkEditContext copyWith({
    TextRange? replaceRange,
    String? label,
    String? url,
    bool? isExisting,
  }) {
    return SovereignLinkEditContext(
      replaceRange: replaceRange ?? this.replaceRange,
      label: label ?? this.label,
      url: url ?? this.url,
      isExisting: isExisting ?? this.isExisting,
    );
  }
}
