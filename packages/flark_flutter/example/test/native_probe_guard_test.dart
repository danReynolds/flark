import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../integration_test/semantics_native_probe.dart' as probe;

void main() {
  testWidgets('native probe cannot pass using framework-only semantics', (
    tester,
  ) async {
    // testWidgets requests a framework tree, not a macOS accessibility client.
    expect(tester.binding.semanticsEnabled, isTrue);
    expect(tester.binding.platformDispatcher.semanticsEnabled, isFalse);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await probe.main();
    await tester.pump();
    // Isolate native activation: foreground and frame delivery already qualify.
    expect(tester.binding.lifecycleState, AppLifecycleState.resumed);
    expect(tester.binding.framesEnabled, isTrue);
    await tester.tap(find.text('Start TextField accessibility probe'));
    await tester.pump();
    expect(
      find.textContaining(
        'Native accessibility and uninterrupted foreground required',
      ),
      findsOneWidget,
    );
    expect(find.text('Native accessibility comparison complete'), findsNothing);
    expect(find.byType(TextField), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
