import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignReadOnlyPreview', () {
    testWidgets('renders projected text while parser output is stale', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(find.text('**bold**'), findsOneWidget);
    });

    testWidgets('renders inline styles from the shared render plan', (
      tester,
    ) async {
      final semantics = tester.ensureSemantics();
      try {
        final controller = SovereignFlutterController.fromMarkdown('**bold**');
        addTearDown(controller.dispose);
        controller.applyParseResult(
          SovereignMarkdownParseResult(
            schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
            revision: controller.state.revision,
            sourceTextLength: controller.state.document.length,
            blocks: [
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.paragraph,
                type: 'paragraph',
                sourceRange: const SovereignSourceRange(0, 8),
              ),
            ],
            inlineTokens: [
              SovereignMarkdownInlineToken(
                kind: SovereignMarkdownInlineKind.strong,
                type: 'strong',
                sourceRange: const SovereignSourceRange(0, 8),
              ),
            ],
            hiddenRanges: [
              SovereignMarkdownHiddenRange(
                kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
                type: 'inlineMarker',
                sourceRange: const SovereignSourceRange(0, 2),
              ),
              SovereignMarkdownHiddenRange(
                kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
                type: 'inlineMarker',
                sourceRange: const SovereignSourceRange(6, 8),
              ),
            ],
          ),
        );

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: Markdown(controller: controller),
          ),
        );

        final richText = tester
            .widgetList<RichText>(find.byType(RichText))
            .singleWhere((widget) => widget.text.toPlainText() == 'bold');
        expect(richText.text.toPlainText(), 'bold');
        expect(_hasStrongSpan(richText.text), isTrue);
        expect(find.bySemanticsLabel('bold'), findsOneWidget);
        expect(find.bySemanticsLabel('**bold**'), findsNothing);
      } finally {
        semantics.dispose();
      }
    });

    testWidgets('renders image runs as default action cards', (tester) async {
      const markdown =
          'Architecture: ![Diagram](asset://diagram.png "System view")';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            const SovereignMarkdownParseRequest(
              revision: 0,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(width: 420, child: Markdown(controller: controller)),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewImageCard')),
        findsOneWidget,
      );
      expect(find.text('IMG'), findsOneWidget);
      expect(find.text('Diagram'), findsOneWidget);
      expect(find.text('asset://diagram.png - System view'), findsOneWidget);
      expect(find.textContaining('![Diagram]'), findsNothing);
    });

    testWidgets(
      'image action cards expose open, copy, and edit-source actions',
      (tester) async {
        const markdown =
            'Architecture: ![Diagram](asset://diagram.png "System view")';
        final opened = <String>[];
        final clipboardPayloads = <Object?>[];
        final messenger =
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
        messenger.setMockMethodCallHandler(SystemChannels.platform, (
          call,
        ) async {
          if (call.method == 'Clipboard.setData') {
            clipboardPayloads.add(call.arguments);
            return null;
          }
          return null;
        });
        addTearDown(() {
          messenger.setMockMethodCallHandler(SystemChannels.platform, null);
        });
        final controller = SovereignFlutterController.fromMarkdown(markdown);
        addTearDown(controller.dispose);
        final result =
            await SovereignNativeComrakParseBackend.withNativeBridge().parse(
              const SovereignMarkdownParseRequest(
                revision: 0,
                markdown: markdown,
                profile: SovereignMarkdownProfile.commonMarkGfm,
              ),
            );
        expect(controller.applyParseResult(result), isTrue);

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: SovereignMarkdownInteractions(
              controller: controller,
              config: SovereignMarkdownInteractionConfig(
                onOpenLink: opened.add,
              ),
              editable: true,
              child: SizedBox(
                width: 420,
                child: Markdown(controller: controller),
              ),
            ),
          ),
        );

        await tester.tap(
          find.byKey(const Key('SovereignReadOnlyPreviewImageMenuButton')),
        );
        await tester.pump();

        expect(
          find.byKey(const Key('SovereignReadOnlyPreviewImageMenu')),
          findsOneWidget,
        );
        expect(find.text('Open'), findsOneWidget);
        expect(find.text('Copy'), findsOneWidget);
        expect(find.text('Edit'), findsOneWidget);

        await tester.tap(find.text('Open'));
        await tester.pump();
        expect(opened, ['asset://diagram.png']);

        await tester.tap(
          find.byKey(const Key('SovereignReadOnlyPreviewImageMenuButton')),
        );
        await tester.pump();
        await tester.tap(find.text('Copy'));
        await tester.runAsync(() async {
          await Future<void>.delayed(Duration.zero);
        });
        await tester.pump();
        expect(clipboardPayloads, [
          {'text': 'asset://diagram.png'},
        ]);

        await tester.tap(
          find.byKey(const Key('SovereignReadOnlyPreviewImageMenuButton')),
        );
        await tester.pump();
        await tester.tap(find.text('Edit'));
        await tester.pump();
        final imageStart = markdown.indexOf('![');
        expect(
          controller.selection,
          SovereignSelection(
            baseOffset: imageStart,
            extentOffset: markdown.length,
          ),
        );
      },
    );

    testWidgets('opens and removes links through interaction menus', (
      tester,
    ) async {
      const markdown = '[Docs](https://example.com)';
      final opened = <String>[];
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            const SovereignMarkdownParseRequest(
              revision: 0,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignMarkdownInteractions(
            controller: controller,
            config: SovereignMarkdownInteractionConfig(onOpenLink: opened.add),
            editable: true,
            child: Markdown(controller: controller),
          ),
        ),
      );

      final button = find.byKey(const Key('SovereignInlineLinkMenuButton'));
      expect(button, findsOneWidget);
      await tester.tap(button);
      await tester.pump();
      expect(find.byKey(const Key('SovereignInlineLinkMenu')), findsOneWidget);

      await tester.tap(find.text('Open'));
      await tester.pump();
      expect(opened, ['https://example.com']);

      await tester.tap(button);
      await tester.pump();
      await tester.tap(find.text('Remove'));
      await tester.pump();

      expect(controller.markdown, 'Docs');

      controller.undo();
      await tester.pump();
      expect(controller.markdown, markdown);

      controller.redo();
      await tester.pump();
      expect(controller.markdown, 'Docs');
    });

    testWidgets('edit link action selects the source link range', (
      tester,
    ) async {
      const markdown = '[Docs](https://example.com)';
      final editedDestinations = <String?>[];
      final editedRanges = <SovereignSourceRange>[];
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            const SovereignMarkdownParseRequest(
              revision: 0,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignMarkdownInteractions(
            controller: controller,
            config: SovereignMarkdownInteractionConfig(
              onEditLink: (context, target) {
                editedDestinations.add(target.action?.destination);
                editedRanges.add(target.sourceRange);
              },
            ),
            editable: true,
            child: Markdown(controller: controller),
          ),
        ),
      );

      await tester.tap(find.byKey(const Key('SovereignInlineLinkMenuButton')));
      await tester.pump();
      await tester.tap(find.text('Edit'));
      await tester.pump();

      expect(editedDestinations, ['https://example.com']);
      expect(editedRanges, [const SovereignSourceRange(0, markdown.length)]);
      expect(
        controller.selection,
        const SovereignSelection(baseOffset: 0, extentOffset: markdown.length),
      );
      expect(find.byKey(const Key('SovereignInlineLinkMenu')), findsNothing);
    });

    testWidgets('copy link action writes the destination to the clipboard', (
      tester,
    ) async {
      const markdown = '[Docs](https://example.com)';
      final clipboardPayloads = <Object?>[];
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.setData') {
          clipboardPayloads.add(call.arguments);
          return null;
        }
        return null;
      });
      addTearDown(() {
        messenger.setMockMethodCallHandler(SystemChannels.platform, null);
      });
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            const SovereignMarkdownParseRequest(
              revision: 0,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignMarkdownInteractions(
            controller: controller,
            config: const SovereignMarkdownInteractionConfig(),
            editable: false,
            child: Markdown(controller: controller),
          ),
        ),
      );

      await tester.tap(find.byKey(const Key('SovereignInlineLinkMenuButton')));
      await tester.pump();
      await tester.tap(find.text('Copy'));
      await tester.runAsync(() async {
        await Future<void>.delayed(Duration.zero);
      });
      await tester.pump();

      expect(clipboardPayloads, [
        {'text': 'https://example.com'},
      ]);
      expect(find.byKey(const Key('SovereignInlineLinkMenu')), findsNothing);
    });

    testWidgets('non-editable link menus omit edit and remove actions', (
      tester,
    ) async {
      const markdown = '[Docs](https://example.com)';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            const SovereignMarkdownParseRequest(
              revision: 0,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignMarkdownInteractions(
            controller: controller,
            config: const SovereignMarkdownInteractionConfig(),
            editable: false,
            child: Markdown(controller: controller),
          ),
        ),
      );

      await tester.tap(find.byKey(const Key('SovereignInlineLinkMenuButton')));
      await tester.pump();

      expect(find.text('Open'), findsOneWidget);
      expect(find.text('Copy'), findsOneWidget);
      expect(find.text('Edit'), findsNothing);
      expect(find.text('Remove'), findsNothing);
      expect(controller.markdown, markdown);
    });

    testWidgets('rebuilds when the controller adopts a new render plan', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown('one');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );
      expect(find.text('one'), findsOneWidget);

      controller.applyTransaction(
        SovereignTransaction.single(
          const SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(0, 3),
            replacementText: 'two',
          ),
        ),
      );
      await tester.pump();

      expect(find.text('two'), findsOneWidget);
    });

    testWidgets('allows custom block rendering from render-plan metadata', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '```dart\nx\n```',
      );
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.codeBlock,
              type: 'codeBlock',
              sourceRange: const SovereignSourceRange(0, 13),
              attributes: const {'language': 'dart'},
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(0, 8),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(9, 13),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(
            controller: controller,
            blockBuilder: (context, block, displayText, baseStyle) {
              if (block.codeBlock == null) return null;
              return Text('code:${block.codeBlock!.language}:$displayText');
            },
          ),
        ),
      );

      expect(find.text('code:dart:x'), findsOneWidget);
      expect(find.text('x'), findsNothing);
    });

    testWidgets('renders fenced code blocks as default visual regions', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '```dart\nx\n```',
      );
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.codeBlock,
              type: 'codeBlock',
              sourceRange: const SovereignSourceRange(0, 13),
              attributes: const {'language': 'dart'},
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(0, 8),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(9, 13),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final codeBlockFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewCodeBlock'),
      );
      expect(codeBlockFinder, findsOneWidget);
      final richText = _codeBlockRichText(tester, codeBlockFinder);
      expect(richText.text.toPlainText(), 'x');
    });

    testWidgets(
      'copy code action writes preview code content to the clipboard',
      (tester) async {
        const markdown = '```dart\nx\n```';
        final clipboardPayloads = <Object?>[];
        final messenger =
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
        messenger.setMockMethodCallHandler(SystemChannels.platform, (
          call,
        ) async {
          if (call.method == 'Clipboard.setData') {
            clipboardPayloads.add(call.arguments);
            return null;
          }
          return null;
        });
        addTearDown(() {
          messenger.setMockMethodCallHandler(SystemChannels.platform, null);
        });
        final controller = SovereignFlutterController.fromMarkdown(markdown);
        addTearDown(controller.dispose);
        controller.applyParseResult(
          SovereignMarkdownParseResult(
            schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
            revision: controller.state.revision,
            sourceTextLength: controller.state.document.length,
            blocks: [
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.codeBlock,
                type: 'codeBlock',
                sourceRange: const SovereignSourceRange(0, markdown.length),
                attributes: const {'language': 'dart'},
              ),
            ],
            inlineTokens: const [],
            hiddenRanges: [
              SovereignMarkdownHiddenRange(
                kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
                type: 'markdownMarker',
                sourceRange: const SovereignSourceRange(0, 8),
              ),
              SovereignMarkdownHiddenRange(
                kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
                type: 'markdownMarker',
                sourceRange: const SovereignSourceRange(9, markdown.length),
              ),
            ],
          ),
        );

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: Markdown(controller: controller),
          ),
        );

        await tester.tap(
          find.byKey(const Key('SovereignReadOnlyPreviewCodeCopyButton')),
        );
        await tester.runAsync(() async {
          await Future<void>.delayed(Duration.zero);
        });
        await tester.pump();

        expect(clipboardPayloads, [
          {'text': 'x'},
        ]);
      },
    );

    testWidgets('highlights fenced code syntax from language metadata', (
      tester,
    ) async {
      const markdown = '```dart\nfinal value = 1;\n```';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.codeBlock,
              type: 'codeBlock',
              sourceRange: const SovereignSourceRange(0, markdown.length),
              attributes: const {'language': 'dart'},
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(0, 8),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
              type: 'markdownMarker',
              sourceRange: const SovereignSourceRange(24, markdown.length),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final codeBlockFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewCodeBlock'),
      );
      final richText = _codeBlockRichText(tester, codeBlockFinder);
      expect(
        _textSpanHasColor(richText.text, 'final', const Color(0xFF7C3AED)),
        isTrue,
      );
    });

    testWidgets('auto-highlights confident unlabeled fenced code', (
      tester,
    ) async {
      const markdown = '```\n{"name":"Ada","count":2}\n```';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applyParseResult(_codeOnlyParseResult(controller));

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final codeBlockFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewCodeBlock'),
      );
      final richText = _codeBlockRichText(tester, codeBlockFinder);
      expect(
        _textSpanHasColor(richText.text, '"Ada"', const Color(0xFF0F766E)),
        isTrue,
      );
      expect(
        _textSpanHasColor(richText.text, '2', const Color(0xFFB45309)),
        isTrue,
      );
    });

    testWidgets('explicit text fenced code stays plain', (tester) async {
      const markdown = '```text\n{"name":"Ada","count":2}\n```';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applyParseResult(_codeOnlyParseResult(controller));

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final codeBlockFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewCodeBlock'),
      );
      final richText = _codeBlockRichText(tester, codeBlockFinder);
      expect(_textSpanHasSyntaxColor(richText.text), isFalse);
    });

    testWidgets('renders blockquotes as default visual regions', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown('> quoted');
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.blockquote,
              type: 'blockquote',
              sourceRange: const SovereignSourceRange(0, 8),
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.blockMarker,
              type: 'blockMarker',
              sourceRange: const SovereignSourceRange(0, 2),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final blockquoteFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewBlockquote'),
      );
      expect(blockquoteFinder, findsOneWidget);
      final richText = tester.widget<RichText>(
        find.descendant(of: blockquoteFinder, matching: find.byType(RichText)),
      );
      expect(richText.text.toPlainText(), 'quoted');
    });

    testWidgets('renders multiline blockquotes with one continuous rail', (
      tester,
    ) async {
      const markdown = '> first\n> second';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.blockquote,
              type: 'blockquote',
              sourceRange: const SovereignSourceRange(0, markdown.length),
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.blockMarker,
              type: 'blockMarker',
              sourceRange: const SovereignSourceRange(0, 2),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.blockMarker,
              type: 'blockMarker',
              sourceRange: const SovereignSourceRange(8, 10),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      final blockquoteFinder = find.byKey(
        const Key('SovereignReadOnlyPreviewBlockquote'),
      );
      expect(blockquoteFinder, findsOneWidget);
      final richText = tester.widget<RichText>(
        find.descendant(of: blockquoteFinder, matching: find.byType(RichText)),
      );
      expect(richText.text.toPlainText(), 'first\nsecond');
    });

    testWidgets('renders task list items with default checkbox visuals', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown('- [x] done');
      addTearDown(controller.dispose);
      controller.applyParseResult(
        SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: controller.state.revision,
          sourceTextLength: controller.state.document.length,
          blocks: [
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.listItem,
              type: 'listItem',
              sourceRange: const SovereignSourceRange(0, 10),
              attributes: const {'checked': true},
            ),
          ],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.blockMarker,
              type: 'blockMarker',
              sourceRange: const SovereignSourceRange(0, 6),
            ),
          ],
        ),
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewTaskCheckbox')),
        findsOneWidget,
      );
      expect(find.text('done'), findsOneWidget);
    });

    testWidgets('renders tables with default grid visuals', (tester) async {
      const markdown =
          '| Area | Status |\n| --- | --- |\n| Preview | Guarded |';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewTable')),
        findsOneWidget,
      );
      expect(find.text('Area'), findsOneWidget);
      expect(find.text('Preview'), findsOneWidget);
      expect(find.textContaining('---'), findsNothing);
    });

    testWidgets('renders escaped pipes inside table cells', (tester) async {
      const markdown =
          r'| Area | Status |'
          '\n'
          r'| --- | --- |'
          '\n'
          r'| Ce\|ll | Guarded |';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewTable')),
        findsOneWidget,
      );
      expect(find.text('Ce|ll'), findsOneWidget);
      expect(find.text(r'Ce\'), findsNothing);
    });

    testWidgets('renders GFM table body rows with parser column count', (
      tester,
    ) async {
      const markdown =
          '| Area | Status |\n'
          '| --- | --- |\n'
          '| Preview |\n'
          '| Extra | Visible | Ignored |';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final result = await SovereignNativeComrakParseBackend.withNativeBridge()
          .parse(
            SovereignMarkdownParseRequest(
              revision: controller.state.revision,
              markdown: markdown,
              profile: SovereignMarkdownProfile.commonMarkGfm,
            ),
          );
      expect(controller.applyParseResult(result), isTrue);
      expect(controller.renderPlan.tableBlocks.single.table!.rows, isNotEmpty);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewTable')),
        findsOneWidget,
      );
      expect(find.text('Preview'), findsOneWidget);
      expect(find.text('Visible'), findsOneWidget);
      expect(find.text('Ignored'), findsNothing);
    });

    testWidgets('renders separator-looking table body rows as content', (
      tester,
    ) async {
      const markdown = '| Area | Status |\n| --- | --- |\n| --- | --- |\n';
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Markdown(controller: controller),
        ),
      );

      expect(
        find.byKey(const Key('SovereignReadOnlyPreviewTable')),
        findsOneWidget,
      );
      expect(find.text('---'), findsNWidgets(2));
    });
  });
}

bool _hasStrongSpan(InlineSpan span) {
  if (span is TextSpan &&
      span.text == 'bold' &&
      span.style?.fontWeight == FontWeight.w700) {
    return true;
  }
  if (span is! TextSpan || span.children == null) return false;
  return span.children!.any(_hasStrongSpan);
}

Future<void> _applyComrakParseResult(
  SovereignFlutterController controller,
) async {
  final result = await SovereignNativeComrakParseBackend.withNativeBridge()
      .parse(
        SovereignMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: controller.markdown,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );
  expect(controller.applyParseResult(result), isTrue);
}

RichText _codeBlockRichText(WidgetTester tester, Finder codeBlockFinder) {
  return tester
      .widgetList<RichText>(
        find.descendant(of: codeBlockFinder, matching: find.byType(RichText)),
      )
      .singleWhere((widget) => widget.text.toPlainText() != 'Copy');
}

bool _textSpanHasColor(InlineSpan span, String text, Color color) {
  if (span is TextSpan) {
    if ((span.text?.contains(text) ?? false) && span.style?.color == color) {
      return true;
    }
    final children = span.children;
    if (children != null) {
      return children.any((child) => _textSpanHasColor(child, text, color));
    }
  }
  return false;
}

bool _textSpanHasSyntaxColor(InlineSpan span) {
  if (span is TextSpan) {
    final color = span.style?.color;
    if (color == const Color(0xFF64748B) ||
        color == const Color(0xFF0F766E) ||
        color == const Color(0xFFB45309) ||
        color == const Color(0xFF7C3AED) ||
        color == const Color(0xFF0369A1) ||
        color == const Color(0xFF047857) ||
        color == const Color(0xFF1D4ED8) ||
        color == const Color(0xFFC2410C) ||
        color == const Color(0xFF475569) ||
        color == const Color(0xFFB91C1C)) {
      return true;
    }
    final children = span.children;
    if (children != null) {
      return children.any(_textSpanHasSyntaxColor);
    }
  }
  return false;
}

SovereignMarkdownParseResult _codeOnlyParseResult(
  SovereignFlutterController controller,
) {
  final openerEnd = controller.markdown.indexOf('\n');
  final openerLine = openerEnd < 0
      ? controller.markdown
      : controller.markdown.substring(0, openerEnd);
  final language = openerLine.startsWith('```') && openerLine.length > 3
      ? openerLine.substring(3).trim()
      : '';
  final bodyStart = openerEnd < 0 ? controller.markdown.length : openerEnd + 1;
  final closerStart = controller.markdown.lastIndexOf('```');
  final rawBody = closerStart >= bodyStart
      ? controller.markdown.substring(bodyStart, closerStart)
      : '';
  final closingHiddenStart =
      _containsNonLineBreak(rawBody) && closerStart > bodyStart
      ? closerStart - 1
      : closerStart;
  return SovereignMarkdownParseResult(
    schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: SovereignSourceRange(0, controller.markdown.length),
        attributes: language.isEmpty
            ? const <String, Object?>{}
            : <String, Object?>{'language': language},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: SovereignSourceRange(0, bodyStart),
      ),
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: SovereignSourceRange(
          closingHiddenStart,
          controller.markdown.length,
        ),
      ),
    ],
  );
}

bool _containsNonLineBreak(String value) {
  for (final codeUnit in value.codeUnits) {
    if (codeUnit != 10 && codeUnit != 13) return true;
  }
  return false;
}
