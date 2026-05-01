import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';

void main() {
  group('Sovereign session diagnostics', () {
    test('sessionState telemetry reflects parse scheduler counters', () async {
      final controller = SovereignController(
        text: '',
        syntaxEngine: const _NeverCompletingSyntaxEngine(),
      );
      addTearDown(controller.dispose);
      controller.resetParseTelemetryForTesting();

      controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
      );
      controller.value = const TextEditingValue(
        text: 'ab',
        selection: TextSelection.collapsed(offset: 2),
      );
      await Future<void>.delayed(Duration.zero);

      final session = controller.sessionState;
      expect(session.document.value.text, 'ab');
      expect(
        session.telemetry.parsePendingReplaceCount,
        controller.parsePendingReplaceCount,
      );
      expect(
        session.telemetry.parseStaleDropCount,
        controller.parseStaleDropCount,
      );
      expect(
        session.telemetry.parsePendingReplaceCount,
        greaterThanOrEqualTo(0),
      );

      controller.resetParseTelemetryForTesting();
      final reset = controller.sessionState;
      expect(reset.telemetry.parsePendingReplaceCount, 0);
      expect(reset.telemetry.parseStaleDropCount, 0);
    });
  });
}

class _NeverCompletingSyntaxEngine implements SyntaxEngine {
  const _NeverCompletingSyntaxEngine();

  static final Future<SyntaxSnapshot> _never =
      Completer<SyntaxSnapshot>().future;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) => _never;

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return SyntaxPrediction(
      revision: request.revision,
      markerRanges: const [],
      exclusionRanges: const [],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
      ambiguityZones: const [],
    );
  }
}
