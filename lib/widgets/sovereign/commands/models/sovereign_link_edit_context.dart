import 'package:flutter/services.dart';

class SovereignLinkEditContext {
  final TextRange replaceRange;
  final String label;
  final String url;
  final bool isExisting;

  const SovereignLinkEditContext({
    required this.replaceRange,
    required this.label,
    required this.url,
    required this.isExisting,
  });

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
