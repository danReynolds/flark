import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('Sovereign renderer exposes basic render telemetry counters', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '# heading');
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            enableTestShortcuts: true,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(controller.renderCallCount, greaterThan(0));
    expect(controller.renderLastMicros, greaterThan(0));
    expect(controller.renderMaxMicros, greaterThan(0));

    controller.resetRenderTelemetryForTesting();
    expect(controller.renderCallCount, 0);
    expect(controller.renderLastMicros, 0);
    expect(controller.renderMaxMicros, 0);

    controller.value = TextEditingValue(
      text: '# heading\n\n> quote',
      selection: const TextSelection.collapsed(offset: 17),
    );
    await tester.pump();

    expect(controller.renderCallCount, greaterThan(0));
    expect(controller.renderLastMicros, greaterThan(0));
    expect(
      controller.renderMaxMicros,
      greaterThanOrEqualTo(controller.renderLastMicros),
    );
  });
}
