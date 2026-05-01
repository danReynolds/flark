import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'syntax_snapshot.dart';

/// CommonMark core is the default compliance target.
///
/// GFM is a strict superset mode for opt-in extension behaviors
/// (for example tables/task lists/strikethrough/autolinks).
enum MarkdownSyntaxProfile { commonMarkCore, commonMarkGfm }

@immutable
class SyntaxParseRequest {
  final int revision;
  final String text;
  final List<TextRange> priorityRanges;
  final MarkdownSyntaxProfile profile;

  const SyntaxParseRequest({
    required this.revision,
    required this.text,
    this.priorityRanges = const [],
    this.profile = MarkdownSyntaxProfile.commonMarkCore,
  }) : assert(revision >= 0);
}

@immutable
class SyntaxPredictRequest {
  final int revision;
  final String text;
  final List<TextRange> priorityRanges;
  final TextRange? editRange;
  final SyntaxSnapshot? previousSnapshot;
  final int? timeBudgetMicros;
  final int? spanBudget;
  final int? charLimit;
  final MarkdownSyntaxProfile profile;

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

abstract interface class SyntaxEngine {
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request);

  SyntaxPrediction predict(SyntaxPredictRequest request);
}
