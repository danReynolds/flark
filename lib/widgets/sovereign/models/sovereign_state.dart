import 'package:flutter/widgets.dart';

/// The canonical immutable state of the [SovereignEditor].
///
/// Principles:
/// 1. Truth is a String (wrapped in [TextEditingValue]).
/// 2. Revision is monotonic and tracks *text* changes only.
class SovereignState {
  /// The canonical truth (Text + Selection + Composing).
  final TextEditingValue value;

  /// Monotonic text revision number.
  /// Incremented ONLY when [value.text] changes.
  final int revision;

  const SovereignState({required this.value, required this.revision});

  /// Creates an initial empty state.
  factory SovereignState.empty() {
    return const SovereignState(value: TextEditingValue.empty, revision: 0);
  }

  SovereignState copyWith({TextEditingValue? value, int? revision}) {
    return SovereignState(
      value: value ?? this.value,
      revision: revision ?? this.revision,
    );
  }

  @override
  String toString() =>
      'SovereignState(rev: $revision, text: "${value.text.length} chars")';

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SovereignState &&
          runtimeType == other.runtimeType &&
          value == other.value &&
          revision == other.revision;

  @override
  int get hashCode => Object.hash(value, revision);
}
