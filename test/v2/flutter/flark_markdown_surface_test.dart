import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  testWidgets('MarkdownEditor wires parsing into projected editing', (
    tester,
  ) async {
    final backend = _ImmediateParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MarkdownEditor(
          controller: controller,
          parseBackend: backend,
          parseDebounce: Duration.zero,
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    expect(backend.requests.single.markdown, '# Title');
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'Title',
    );
  });

  testWidgets('Markdown owns a v2 preview controller', (tester) async {
    final backend = _ImmediateParseBackend();

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Markdown(
          markdown: '# Title',
          parseBackend: backend,
          parseDebounce: Duration.zero,
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    expect(backend.requests.single.markdown, '# Title');
    expect(find.text('Title'), findsOneWidget);
  });

  testWidgets('MarkdownEditor owns controller and reports changes', (
    tester,
  ) async {
    final changes = <String>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MarkdownEditor(
          initialMarkdown: 'hello',
          parseBackend: _ImmediateParagraphParseBackend(),
          parseDebounce: Duration.zero,
          editingMode: FlarkMarkdownEditingMode.source,
          onChanged: changes.add,
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'hello',
    );

    await tester.enterText(find.byType(EditableText), 'hello!');
    await tester.pump();

    expect(changes, contains('hello!'));

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MarkdownEditor(
          initialMarkdown: 'reset',
          parseBackend: _ImmediateParagraphParseBackend(),
          parseDebounce: Duration.zero,
          editingMode: FlarkMarkdownEditingMode.source,
          onChanged: changes.add,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'hello!',
    );

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MarkdownEditor(
          key: const ValueKey('reset-document'),
          initialMarkdown: 'reset',
          parseBackend: _ImmediateParagraphParseBackend(),
          parseDebounce: Duration.zero,
          editingMode: FlarkMarkdownEditingMode.source,
          onChanged: changes.add,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'reset',
    );
  });

  test('high-level surface constructors reject ambiguous ownership', () {
    final controller = FlarkFlutterController.fromMarkdown('hello');
    addTearDown(controller.dispose);

    expect(
      () => MarkdownEditor(controller: controller, initialMarkdown: 'hello'),
      throwsAssertionError,
    );
    expect(
      () => MarkdownEditor(
        controller: controller,
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
      throwsAssertionError,
    );
    expect(
      () => Markdown(markdown: 'hello', controller: controller),
      throwsAssertionError,
    );
    expect(
      () => Markdown(
        controller: controller,
        parseBackend: _ImmediateParagraphParseBackend(),
      ),
      throwsAssertionError,
    );
  });

  testWidgets('high-level surfaces require default Comrak parsing', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('# Title');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            MarkdownEditor(
              controller: controller,
              parseDebounce: Duration.zero,
            ),
            const Markdown(markdown: '# Preview', parseDebounce: Duration.zero),
          ],
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(controller.hasAuthoritativeRenderPlan, isTrue);
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'Title',
    );
    expect(find.text('Preview'), findsOneWidget);
  });

  testWidgets('high-level surfaces report background parse failures', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown('# Title');
    final editorErrors = <Object>[];
    final previewErrors = <Object>[];
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            MarkdownEditor(
              controller: controller,
              parseBackend: const _FailingParseBackend(),
              parseDebounce: Duration.zero,
              onParseError: (error, stackTrace) {
                editorErrors.add(error);
              },
            ),
            Markdown(
              markdown: '# Preview',
              parseBackend: const _FailingParseBackend(),
              parseDebounce: Duration.zero,
              onParseError: (error, stackTrace) {
                previewErrors.add(error);
              },
            ),
          ],
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    expect(editorErrors.single, isA<StateError>());
    expect(previewErrors.single, isA<StateError>());
    expect(tester.takeException(), isNull);
    expect(controller.hasAuthoritativeRenderPlan, isFalse);
  });
}

final class _ImmediateParagraphParseBackend
    implements FlarkMarkdownParseBackend {
  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'immediate-paragraph',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    return FlarkMarkdownParseResult(
      schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: [
        FlarkMarkdownBlockNode(
          kind: FlarkMarkdownBlockKind.paragraph,
          type: 'paragraph',
          sourceRange: FlarkSourceRange(0, request.markdown.length),
        ),
      ],
      inlineTokens: const [],
      hiddenRanges: const [],
    );
  }
}

final class _ImmediateParseBackend implements FlarkMarkdownParseBackend {
  final requests = <FlarkMarkdownParseRequest>[];

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'immediate',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    requests.add(request);
    return FlarkMarkdownParseResult(
      schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: [
        FlarkMarkdownBlockNode(
          kind: FlarkMarkdownBlockKind.heading,
          type: 'heading',
          sourceRange: FlarkSourceRange(0, request.markdown.length),
          attributes: const {'level': 1},
        ),
      ],
      inlineTokens: const [],
      hiddenRanges: [
        FlarkMarkdownHiddenRange(
          kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
          type: 'markdownMarker',
          sourceRange: FlarkSourceRange(0, 2),
        ),
      ],
    );
  }
}

final class _FailingParseBackend implements FlarkMarkdownParseBackend {
  const _FailingParseBackend();

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'failing',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    throw StateError('parse failed');
  }
}
