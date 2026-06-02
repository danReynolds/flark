import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  testWidgets('MarkdownEditor wires parsing into projected editing', (
    tester,
  ) async {
    final backend = _ImmediateParseBackend();
    final controller = SovereignFlutterController.fromMarkdown(
      '# Title',
      extensions: SovereignMarkdownEditingExtensions.standard(),
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
          editingMode: SovereignMarkdownEditingMode.source,
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
          editingMode: SovereignMarkdownEditingMode.source,
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
          editingMode: SovereignMarkdownEditingMode.source,
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
    final controller = SovereignFlutterController.fromMarkdown('hello');
    addTearDown(controller.dispose);

    expect(
      () => MarkdownEditor(controller: controller, initialMarkdown: 'hello'),
      throwsAssertionError,
    );
    expect(
      () => MarkdownEditor(
        controller: controller,
        extensions: SovereignMarkdownEditingExtensions.standard(),
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
    final controller = SovereignFlutterController.fromMarkdown('# Title');
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
    final controller = SovereignFlutterController.fromMarkdown('# Title');
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
    implements SovereignMarkdownParseBackend {
  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'immediate-paragraph',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) async {
    return SovereignMarkdownParseResult(
      schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: [
        SovereignMarkdownBlockNode(
          kind: SovereignMarkdownBlockKind.paragraph,
          type: 'paragraph',
          sourceRange: SovereignSourceRange(0, request.markdown.length),
        ),
      ],
      inlineTokens: const [],
      hiddenRanges: const [],
    );
  }
}

final class _ImmediateParseBackend implements SovereignMarkdownParseBackend {
  final requests = <SovereignMarkdownParseRequest>[];

  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'immediate',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) async {
    requests.add(request);
    return SovereignMarkdownParseResult(
      schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: [
        SovereignMarkdownBlockNode(
          kind: SovereignMarkdownBlockKind.heading,
          type: 'heading',
          sourceRange: SovereignSourceRange(0, request.markdown.length),
          attributes: const {'level': 1},
        ),
      ],
      inlineTokens: const [],
      hiddenRanges: [
        SovereignMarkdownHiddenRange(
          kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
          type: 'markdownMarker',
          sourceRange: SovereignSourceRange(0, 2),
        ),
      ],
    );
  }
}

final class _FailingParseBackend implements SovereignMarkdownParseBackend {
  const _FailingParseBackend();

  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'failing',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) async {
    throw StateError('parse failed');
  }
}
