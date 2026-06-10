import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  testWidgets('FlarkMarkdownEditor wires parsing into live-rendered editing', (
    tester,
  ) async {
    final backend = _ImmediateParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      extensions: FlarkMarkdownEditingExtensions.standard(),
      parseBackend: backend,
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(controller: controller),
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
        child: FlarkMarkdown(
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

  testWidgets('FlarkMarkdownEditor owns controller and reports changes', (
    tester,
  ) async {
    final changes = <String>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
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
        child: FlarkMarkdownEditor(
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
        child: FlarkMarkdownEditor(
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

  testWidgets('readOnly editors render but reject edits and commands', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      'hello',
      extensions: FlarkMarkdownEditingExtensions.standard(),
      parseBackend: _ImmediateParagraphParseBackend(),
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          controller: controller,
          readOnly: true,
          editingMode: FlarkMarkdownEditingMode.source,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.readOnly, isTrue);
    expect(editable.controller.text, 'hello');

    // Flipping back to editable re-enables input on the same widget tree.
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          controller: controller,
          readOnly: false,
          editingMode: FlarkMarkdownEditingMode.source,
        ),
      ),
    );
    await tester.pump();
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).readOnly,
      isFalse,
    );

    await tester.enterText(find.byType(EditableText), 'hello!');
    await tester.pump();
    expect(controller.markdown, 'hello!');
  });

  testWidgets('disabled form fields render a read-only editor', (tester) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Form(
          child: FlarkMarkdownEditorFormField(
            initialMarkdown: 'locked',
            parseBackend: _ImmediateParagraphParseBackend(),
            parseDebounce: Duration.zero,
            editingMode: FlarkMarkdownEditingMode.source,
            enabled: false,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(
      tester.widget<EditableText>(find.byType(EditableText)).readOnly,
      isTrue,
    );
  });

  testWidgets('selectable previews wrap content in a selection region', (
    tester,
  ) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        // SelectableRegion requires an Overlay ancestor (normally provided
        // by MaterialApp/WidgetsApp).
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => FlarkMarkdown(
                markdown: 'hello world',
                parseBackend: _ImmediateParagraphParseBackend(),
                parseDebounce: Duration.zero,
                selectable: true,
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(find.byType(SelectableRegion), findsOneWidget);
    expect(find.textContaining('hello world'), findsWidgets);
  });

  testWidgets('readOnly applies to every live-rendered block editable', (
    tester,
  ) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          initialMarkdown: 'alpha\n\nbeta',
          readOnly: true,
          editingMode: FlarkMarkdownEditingMode.liveRendered,
          parseBackend: _ImmediateParagraphParseBackend(),
          parseDebounce: Duration.zero,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final editables = tester.widgetList<EditableText>(
      find.byType(EditableText),
    );
    expect(editables, isNotEmpty);
    for (final editable in editables) {
      expect(editable.readOnly, isTrue);
    }
    expect(tester.takeException(), isNull);
  });

  testWidgets('swapping only onParseError does not restart the parser', (
    tester,
  ) async {
    final backend = _ImmediateParagraphParseBackend();

    Widget editorWith(void Function(Object, StackTrace) onParseError) {
      return Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          initialMarkdown: 'hello',
          parseBackend: backend,
          parseDebounce: Duration.zero,
          editingMode: FlarkMarkdownEditingMode.source,
          onParseError: onParseError,
        ),
      );
    }

    await tester.pumpWidget(editorWith((error, stack) {}));
    await tester.pump();
    await tester.pump();
    final requestsAfterFirstBuild = backend.requests.length;

    // Rebuilding with a fresh inline closure used to dispose and recreate
    // the parse scheduler each frame; it must now swap the callback in
    // place without issuing a new parse.
    for (var i = 0; i < 3; i++) {
      await tester.pumpWidget(editorWith((error, stack) {}));
      await tester.pump();
    }
    expect(backend.requests.length, requestsAfterFirstBuild);
  });

  testWidgets('shared controller drives a single parser across surfaces', (
    tester,
  ) async {
    final backend = _RecordingParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: backend,
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            Expanded(child: FlarkMarkdownEditor(controller: controller)),
            Expanded(child: FlarkMarkdown(controller: controller)),
          ],
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    // One document, one parse of the initial revision — not one per surface.
    expect(backend.requests, hasLength(1));
    expect(controller.isParsing, isTrue);
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
  });

  testWidgets('controller.parseNow bypasses the debounce window', (
    tester,
  ) async {
    final backend = _RecordingParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      'hello',
      parseBackend: backend,
      parseDebounce: const Duration(days: 1),
    );
    addTearDown(controller.dispose);

    controller.ensureParsing();
    controller.ensureParsing();
    await controller.parseNow();

    expect(backend.requests, hasLength(1));
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
  });

  test('high-level surface constructors reject ambiguous ownership', () {
    final controller = FlarkFlutterController.fromMarkdown('hello');
    addTearDown(controller.dispose);

    expect(
      () =>
          FlarkMarkdownEditor(controller: controller, initialMarkdown: 'hello'),
      throwsAssertionError,
    );
    expect(
      () => FlarkMarkdownEditor(
        controller: controller,
        extensions: FlarkMarkdownEditingExtensions.standard(),
      ),
      throwsAssertionError,
    );
    expect(
      () => FlarkMarkdown(markdown: 'hello', controller: controller),
      throwsAssertionError,
    );
    expect(
      () => FlarkMarkdown(
        controller: controller,
        parseBackend: _ImmediateParagraphParseBackend(),
      ),
      throwsAssertionError,
    );
  });

  testWidgets('high-level surfaces require default Comrak parsing', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseDebounce: Duration.zero,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            FlarkMarkdownEditor(controller: controller),
            const FlarkMarkdown(
              markdown: '# Preview',
              parseDebounce: Duration.zero,
            ),
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
    final editorErrors = <Object>[];
    final previewErrors = <Object>[];
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: const _FailingParseBackend(),
      parseDebounce: Duration.zero,
      onParseError: (error, stackTrace) {
        editorErrors.add(error);
      },
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            FlarkMarkdownEditor(controller: controller),
            FlarkMarkdown(
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

  testWidgets('live-rendered immediate parses use editor backend and profile', (
    tester,
  ) async {
    final backend = _RecordingParseBackend();

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          initialMarkdown: '',
          editingMode: FlarkMarkdownEditingMode.liveRendered,
          parseBackend: backend,
          profile: FlarkMarkdownProfile.commonMarkCore,
          parseDebounce: const Duration(days: 1),
        ),
      ),
    );

    await tester.pump();
    await tester.pump();
    expect(backend.requests, hasLength(1));

    await tester.enterText(find.byType(EditableText), '```');
    await tester.pump();

    expect(backend.requests, hasLength(2));
    expect(backend.requests.last.markdown, contains('```'));
    expect(backend.requests.last.profile, FlarkMarkdownProfile.commonMarkCore);
    expect(tester.takeException(), isNull);
  });

  testWidgets('live-rendered immediate parse failures use onParseError', (
    tester,
  ) async {
    final backend = _RecordingParseBackend(failOnRequest: 2);
    final errors = <Object>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkMarkdownEditor(
          initialMarkdown: '',
          editingMode: FlarkMarkdownEditingMode.liveRendered,
          parseBackend: backend,
          parseDebounce: const Duration(days: 1),
          onParseError: (error, stackTrace) {
            errors.add(error);
          },
        ),
      ),
    );

    await tester.pump();
    await tester.pump();
    expect(backend.requests, hasLength(1));
    expect(errors, isEmpty);

    await tester.enterText(find.byType(EditableText), '```');
    await tester.pump();

    expect(backend.requests, hasLength(2));
    expect(errors.single, isA<StateError>());
    expect(tester.takeException(), isNull);
  });
}

final class _ImmediateParagraphParseBackend
    implements FlarkMarkdownParseBackend {
  final requests = <FlarkMarkdownParseRequest>[];

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
    requests.add(request);
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

final class _RecordingParseBackend implements FlarkMarkdownParseBackend {
  _RecordingParseBackend({this.failOnRequest});

  final int? failOnRequest;
  final requests = <FlarkMarkdownParseRequest>[];

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'recording',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [
          FlarkMarkdownProfile.commonMarkCore,
          FlarkMarkdownProfile.commonMarkGfm,
        ],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    requests.add(request);
    if (requests.length == failOnRequest) {
      throw StateError('recorded parse failed');
    }
    return FlarkMarkdownParseResult(
      schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: [
        if (request.markdown.isNotEmpty)
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
