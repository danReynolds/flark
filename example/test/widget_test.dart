import 'package:example/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  testWidgets('example exposes editor and preview surfaces', (tester) async {
    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    expect(find.byType(SovereignEditor), findsOneWidget);
    expect(find.byType(SovereignMarkdownView), findsOneWidget);
    expect(find.text('Sovereign'), findsOneWidget);
    expect(find.text('Split'), findsOneWidget);
  });

  testWidgets('mode switch can show preview only', (tester) async {
    await tester.pumpWidget(const SovereignExampleApp());
    await tester.pump();

    await tester.tap(find.byIcon(Icons.visibility_outlined));
    await tester.pump();

    expect(find.byType(SovereignEditor), findsNothing);
    expect(find.byType(SovereignMarkdownView), findsOneWidget);
  });
}
