import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'syntax_snapshot.dart';

/// CommonMark core is the default compliance target.
///
/// GFM is a strict superset mode for opt-in extension behaviors
/// (for example tables/task lists/strikethrough/autolinks).
enum MarkdownSyntaxProfile {
  /// CommonMark core compliance mode.
  commonMarkCore,

  /// GitHub Flavored Markdown extension mode.
  commonMarkGfm,
}

/// Full parse request passed to a [SyntaxEngine].
@immutable
class SyntaxParseRequest {
  /// Controller revision associated with [text].
  final int revision;

  /// Markdown source text to parse.
  final String text;

  /// Text ranges the engine may prioritize for incremental work.
  final List<TextRange> priorityRanges;

  /// Markdown dialect requested by the caller.
  final MarkdownSyntaxProfile profile;

  /// Creates a full parse request.
  const SyntaxParseRequest({
    required this.revision,
    required this.text,
    this.priorityRanges = const [],
    this.profile = MarkdownSyntaxProfile.commonMarkCore,
  }) : assert(revision >= 0);
}

/// Predictive parse request for low-latency syntax feedback.
@immutable
class SyntaxPredictRequest {
  /// Controller revision associated with [text].
  final int revision;

  /// Markdown source text to inspect.
  final String text;

  /// Text ranges the engine may prioritize for prediction.
  final List<TextRange> priorityRanges;

  /// Edited range that triggered the prediction, if known.
  final TextRange? editRange;

  /// Most recent authoritative syntax snapshot, if available.
  final SyntaxSnapshot? previousSnapshot;

  /// Optional prediction time budget in microseconds.
  final int? timeBudgetMicros;

  /// Optional maximum number of spans to scan.
  final int? spanBudget;

  /// Optional maximum number of characters to scan.
  final int? charLimit;

  /// Markdown dialect requested by the caller.
  final MarkdownSyntaxProfile profile;

  /// Creates a predictive parse request.
  const SyntaxPredictRequest({
    required this.revision,
    required this.text,
    this.priorityRanges = const [],
    this.editRange,
    this.previousSnapshot,
    this.timeBudgetMicros,
    this.spanBudget,
    this.charLimit,
    this.profile = MarkdownSyntaxProfile.commonMarkCore,
  }) : assert(revision >= 0);
}

/// Parser abstraction used by the controller and preview widgets.
abstract interface class SyntaxEngine {
  /// Produces an authoritative syntax snapshot for [request].
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request);

  /// Produces a synchronous prediction for low-latency editor feedback.
  SyntaxPrediction predict(SyntaxPredictRequest request);
}
