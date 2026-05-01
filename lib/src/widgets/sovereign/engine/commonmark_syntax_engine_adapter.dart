import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/logic/sovereign_style_scanner.dart';

/// CommonMark adapter that combines authoritative backend parsing with
/// low-latency predictive scans for in-flight edits.
class CommonMarkSyntaxEngineAdapter implements SyntaxEngine {
  final int predictiveScanTimeBudgetMicros;
  final int predictiveScanSpanBudget;
  final CommonMarkParseBackend parseBackend;
  final V1SyntaxEngineAdapter _predictiveDelegate;

  const CommonMarkSyntaxEngineAdapter({
    required this.parseBackend,
    this.predictiveScanTimeBudgetMicros =
        SovereignStyleScanner.kTimeBudgetMicros,
    this.predictiveScanSpanBudget = SovereignStyleScanner.kSpanBudget,
    V1SyntaxEngineAdapter predictiveDelegate = const V1SyntaxEngineAdapter(),
  }) : _predictiveDelegate = predictiveDelegate;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    return parseBackend.parse(request);
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return _predictiveDelegate.predict(
      SyntaxPredictRequest(
        revision: request.revision,
        text: request.text,
        priorityRanges: request.priorityRanges,
        editRange: request.editRange,
        previousSnapshot: request.previousSnapshot,
        timeBudgetMicros:
            request.timeBudgetMicros ?? predictiveScanTimeBudgetMicros,
        spanBudget: request.spanBudget ?? predictiveScanSpanBudget,
        charLimit: request.charLimit,
        profile: request.profile,
      ),
    );
  }
}
