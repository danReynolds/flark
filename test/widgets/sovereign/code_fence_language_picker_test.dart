import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  testWidgets('Code fence language picker updates opening fence info string', (
    WidgetTester tester,
  ) async {
    final engine = _DelayedStalePredictEngine();
    final controller = SovereignController(
      text: '```\nfinal x = 1;\n```',
      syntaxEngine: engine,
    );
    final focusNode = FocusNode();

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(useMaterial3: false),
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: focusNode),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.pump(const Duration(milliseconds: 30));

    // Place caret inside the fenced code content (start of "final").
    controller.selection = const TextSelection.collapsed(offset: 4);
    await tester.pumpAndSettle();

    final picker = find.byKey(const Key('SovereignCodeFenceLanguagePicker'));
    expect(picker, findsOneWidget);

    await tester.tap(picker);
    await tester.pumpAndSettle();

    await tester.tap(find.text('Dart'));
    await tester.pump();

    expect(controller.text, equals('```dart\nfinal x = 1;\n```'));
    expect(controller.selection.baseOffset, equals(8));
    expect(engine.hasPendingParse, isTrue);
    expect(
      controller.decoration.hiddenRanges,
      contains(const TextRange(start: 3, end: 7)),
      reason:
          'Fence info string from the language picker should hide immediately, '
          'before the delayed authoritative parse completes.',
    );

    await tester.pumpAndSettle();

    // Switch back to Plain.
    await tester.tap(picker);
    await tester.pumpAndSettle();

    await tester.tap(find.text('Plain'));
    await tester.pumpAndSettle();

    expect(controller.text, equals('```\nfinal x = 1;\n```'));
    expect(controller.selection.baseOffset, equals(4));
  });
}

class _DelayedStalePredictEngine implements SyntaxEngine {
  final V1SyntaxEngineAdapter _delegate = V1SyntaxEngineAdapter();
  SyntaxSnapshot? _latestSnapshot;
  bool hasPendingParse = false;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    hasPendingParse = true;
    await Future<void>.delayed(const Duration(milliseconds: 25));
    final snapshot = await _delegate.parse(request);
    _latestSnapshot = snapshot;
    hasPendingParse = false;
    return snapshot;
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    final stale = _latestSnapshot;
    if (stale == null) {
      return _delegate.predict(request);
    }
    return SyntaxPrediction(
      revision: request.revision,
      markerRanges: stale.markerRanges,
      exclusionRanges: stale.exclusionRanges,
      ambiguityZones: const [],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
    );
  }
}
