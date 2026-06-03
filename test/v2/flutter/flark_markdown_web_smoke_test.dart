import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('native Comrak backend loads the required WASM bridge on web', () async {
    final backend = FlarkNativeComrakParseBackend.withNativeBridge();
    final result = await backend.parse(
      const FlarkMarkdownParseRequest(
        revision: 7,
        markdown: '| A | B |\n| - | - |\n| **x** | y |\n',
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );

    expect(
      result.diagnostics.where(
        (diagnostic) => diagnostic.extensions['isError'] == true,
      ),
      isEmpty,
      reason: result.diagnostics
          .map((diagnostic) => '${diagnostic.code}: ${diagnostic.message}')
          .join('\n'),
    );
    expect(result.extensions['nativeParser'], 'comrak');
    expect(result.extensions['nativeRevision'], 7);
    expect(result.blocks.map((block) => block.type), contains('table'));
    expect(result.inlineTokens.map((token) => token.type), contains('strong'));
  });

  testWidgets('promoted v2 surfaces require Comrak by default on web', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '# Web',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            MarkdownEditor(controller: controller),
            const Markdown(markdown: '# Preview', parseDebounce: Duration.zero),
          ],
        ),
      ),
    );
    await _waitForAuthoritativeRenderPlan(tester, controller);

    expect(controller.hasAuthoritativeRenderPlan, isTrue);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'Web',
    );
    expect(find.text('Preview'), findsOneWidget);
    expect(find.text('# Preview'), findsNothing);
  });

  testWidgets('promoted v2 surfaces render Comrak WASM plans', (tester) async {
    final backend = FlarkNativeComrakParseBackend.withNativeBridge();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Web',
      parseBackend: backend,
    );
    addTearDown(controller.dispose);

    final parseResult = await tester.runAsync(() async {
      return backend.parse(
        FlarkMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: '# Web',
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );
    });

    expect(controller.applyParseResult(parseResult!), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MarkdownEditor(controller: controller),
      ),
    );
    await tester.pump();

    expect(controller.hasAuthoritativeRenderPlan, isTrue);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'Web',
    );
  });
}

Future<void> _waitForAuthoritativeRenderPlan(
  WidgetTester tester,
  FlarkFlutterController controller,
) async {
  await tester.pump();
  for (var attempt = 0; attempt < 50; attempt++) {
    if (controller.hasAuthoritativeRenderPlan) break;
    await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    });
    await tester.pump();
  }
}
