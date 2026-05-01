import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

const int _kLargeTableEnterP95BudgetMicros = 80000;
const int _kLargeTableTabInsertP95BudgetMicros = 90000;

void main() {
  group('Sovereign table performance baseline', () {
    test(
      'Large-table Enter formatting and Tab row insertion stay within budget',
      () {
        final table = _buildEstablishedTable(rows: 80, columns: 8);

        final enterSamples = <int>[];
        final tabSamples = <int>[];

        // Warmup
        for (var i = 0; i < 3; i++) {
          final enterController = SovereignController(
            text: table,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          enterController.selection = TextSelection.collapsed(
            offset: enterController.text.length,
          );
          enterController.handleEnter();
          enterController.dispose();

          final tabController = SovereignController(
            text: table,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          tabController.selection = TextSelection.collapsed(
            offset: tabController.text.lastIndexOf('r79c7'),
          );
          tabController.handleTabKey(reverse: false);
          tabController.dispose();
        }

        for (var i = 0; i < 15; i++) {
          final enterController = SovereignController(
            text: table,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          enterController.selection = TextSelection.collapsed(
            offset: enterController.text.length,
          );
          final swEnter = Stopwatch()..start();
          enterController.handleEnter();
          swEnter.stop();
          enterSamples.add(swEnter.elapsedMicroseconds);
          expect(enterController.text, contains('\n|'));
          enterController.dispose();

          final tabController = SovereignController(
            text: table,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          tabController.selection = TextSelection.collapsed(
            offset: tabController.text.lastIndexOf('r79c7'),
          );
          final swTab = Stopwatch()..start();
          final handled = tabController.handleTabKey(reverse: false);
          swTab.stop();
          expect(handled, isTrue);
          tabSamples.add(swTab.elapsedMicroseconds);
          expect(tabController.text, contains('\n|'));
          tabController.dispose();
        }

        enterSamples.sort();
        tabSamples.sort();
        final enterP95 = _percentileMicros(enterSamples, 0.95);
        final tabP95 = _percentileMicros(tabSamples, 0.95);

        // Log for local tuning and CI trend checks.
        // ignore: avoid_print
        print(
          'Table perf baseline: enter p50=${(_percentileMicros(enterSamples, 0.50) / 1000).toStringAsFixed(2)}ms '
          'p95=${(enterP95 / 1000).toStringAsFixed(2)}ms; '
          'tab-insert p50=${(_percentileMicros(tabSamples, 0.50) / 1000).toStringAsFixed(2)}ms '
          'p95=${(tabP95 / 1000).toStringAsFixed(2)}ms',
        );

        expect(
          enterP95,
          lessThanOrEqualTo(_kLargeTableEnterP95BudgetMicros.toDouble()),
        );
        expect(
          tabP95,
          lessThanOrEqualTo(_kLargeTableTabInsertP95BudgetMicros.toDouble()),
        );
      },
    );
  });
}

String _buildEstablishedTable({required int rows, required int columns}) {
  final b = StringBuffer();
  b.writeln('| ${List.generate(columns, (c) => 'head_$c').join(' | ')} |');
  b.writeln('| ${List.generate(columns, (_) => '---').join(' | ')} |');
  for (var r = 0; r < rows; r++) {
    b.writeln('| ${List.generate(columns, (c) => 'r${r}c$c').join(' | ')} |');
  }
  final out = b.toString();
  // Trim trailing newline so Enter/Tab paths exercise line-end insertion logic.
  return out.endsWith('\n') ? out.substring(0, out.length - 1) : out;
}

double _percentileMicros(List<int> sortedMicros, double percentile) {
  if (sortedMicros.isEmpty) return 0;
  final position = (sortedMicros.length - 1) * percentile;
  final lower = position.floor();
  final upper = position.ceil();
  if (lower == upper) return sortedMicros[lower].toDouble();
  final weight = position - lower;
  return sortedMicros[lower] +
      (sortedMicros[upper] - sortedMicros[lower]) * weight;
}
