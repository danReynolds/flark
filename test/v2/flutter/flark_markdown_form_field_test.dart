import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  testWidgets('validates and saves markdown through Flutter Form', (
    tester,
  ) async {
    final formKey = GlobalKey<FormState>();
    final changes = <String>[];
    String? saved;

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Form(
          key: formKey,
          child: MarkdownEditorFormField(
            initialMarkdown: 'hello',
            parseBackend: _ImmediateParagraphParseBackend(),
            parseDebounce: Duration.zero,
            editingMode: FlarkMarkdownEditingMode.source,
            onChanged: changes.add,
            validator: (markdown) {
              return markdown != null && markdown.contains('!')
                  ? null
                  : 'Add emphasis';
            },
            onSaved: (markdown) {
              saved = markdown;
            },
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(formKey.currentState!.validate(), isFalse);
    await tester.pump();
    expect(find.text('Add emphasis'), findsOneWidget);

    await tester.enterText(find.byType(EditableText), 'hello!');
    await tester.pump();

    expect(changes, contains('hello!'));
    expect(formKey.currentState!.validate(), isTrue);
    formKey.currentState!.save();
    expect(saved, 'hello!');
  });

  testWidgets('resets owned controller to initial markdown', (tester) async {
    final formKey = GlobalKey<FormState>();

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Form(
          key: formKey,
          child: MarkdownEditorFormField(
            initialMarkdown: 'start',
            parseBackend: _ImmediateParagraphParseBackend(),
            parseDebounce: Duration.zero,
            editingMode: FlarkMarkdownEditingMode.source,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    await tester.enterText(find.byType(EditableText), 'changed');
    await tester.pump();
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'changed',
    );

    formKey.currentState!.reset();
    await tester.pump();

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).controller.text,
      'start',
    );
  });

  testWidgets('resets shared controller without corrupting undo history', (
    tester,
  ) async {
    final formKey = GlobalKey<FormState>();
    final controller = FlarkFlutterController.fromMarkdown(
      'start',
      parseBackend: _ImmediateParagraphParseBackend(),
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Form(
          key: formKey,
          child: MarkdownEditorFormField(
            controller: controller,
            editingMode: FlarkMarkdownEditingMode.source,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    await tester.enterText(find.byType(EditableText), 'changed');
    await tester.pump();
    expect(controller.markdown, 'changed');

    formKey.currentState!.reset();
    await tester.pump();
    expect(controller.markdown, 'start');

    controller.undo();
    expect(controller.markdown, 'changed');
    controller.undo();
    expect(controller.markdown, 'start');
  });

  test('rejects ambiguous ownership', () {
    final controller = FlarkFlutterController.fromMarkdown('hello');
    addTearDown(controller.dispose);

    expect(
      () => MarkdownEditorFormField(
        controller: controller,
        initialMarkdown: 'hello',
      ),
      throwsAssertionError,
    );
    expect(
      () => MarkdownEditorFormField(
        controller: controller,
        parseBackend: _ImmediateParagraphParseBackend(),
      ),
      throwsAssertionError,
    );
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
