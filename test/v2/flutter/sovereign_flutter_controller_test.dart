import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/projection/projection.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkFlutterController', () {
    test('starts from markdown with a stale empty render plan', () {
      final controller = FlarkFlutterController.fromMarkdown('hello');

      addTearDown(controller.dispose);
      expect(controller.markdown, 'hello');
      expect(controller.selection, const FlarkSelection.collapsed(5));
      expect(controller.projection.textLength, 5);
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(controller.renderPlan.metadata['stale'], isTrue);
    });

    test(
      'dispatches commands and predicts projection until parse catches up',
      () {
        final runtime = FlarkEditorRuntime.fromMarkdown(
          '**bold**',
          extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
        );
        final controller = FlarkFlutterController(
          runtime: runtime,
          projection: FlarkProjection(
            textLength: 8,
            hiddenRanges: const [
              FlarkHiddenRange(
                range: FlarkSourceRange(0, 2),
                kind: FlarkHiddenRangeKind.inlineMarker,
              ),
              FlarkHiddenRange(
                range: FlarkSourceRange(6, 8),
                kind: FlarkHiddenRangeKind.inlineMarker,
              ),
            ],
          ),
        );
        var notifications = 0;
        controller.addListener(() {
          notifications++;
        });

        addTearDown(controller.dispose);
        final result = controller.dispatch(
          command: FlarkCoreEditingCommands.insertText,
          payload: const FlarkInsertTextPayload('!'),
        );

        expect(result.commandResult.isHandled, isTrue);
        expect(controller.markdown, '**bold**!');
        expect(controller.projection.textLength, 9);
        expect(
          controller.projection.hiddenRanges.last.range,
          const FlarkSourceRange(6, 8),
        );
        expect(controller.lastProjectionPrediction, isNotNull);
        expect(
          controller.lastProjectionPrediction!.touchedProjectionSensitiveRange,
          isFalse,
        );
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
        expect(notifications, 1);
      },
    );

    test('accepts current parse results and rejects stale ones', () {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      var notifications = 0;
      controller.addListener(() {
        notifications++;
      });

      addTearDown(controller.dispose);
      final stale = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 99,
        sourceTextLength: 8,
        blocks: const [],
        inlineTokens: const [],
      );
      final current = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: controller.state.document.length,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const FlarkSourceRange(0, 8),
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.strong,
            type: 'strong',
            sourceRange: const FlarkSourceRange(0, 8),
          ),
        ],
        hiddenRanges: [
          FlarkMarkdownHiddenRange(
            kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: const FlarkSourceRange(0, 2),
          ),
          FlarkMarkdownHiddenRange(
            kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: const FlarkSourceRange(6, 8),
          ),
        ],
      );

      expect(controller.applyParseResult(stale), isFalse);
      expect(controller.applyParseResult(current), isTrue);
      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(controller.projection.projectText(controller.markdown), 'bold');
      expect(
        controller.renderPlan.blocks.single.inlineRuns.single.kind,
        FlarkMarkdownInlineKind.strong,
      );
      expect(notifications, 1);
    });

    test('keeps render plan authoritative across selection-only changes', () {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);

      expect(
        controller.applyParseResult(_strongParseResult(controller)),
        isTrue,
      );
      expect(controller.hasAuthoritativeRenderPlan, isTrue);

      controller.applySelection(
        const FlarkSelection.collapsed(2),
        userEvent: 'test',
      );

      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(
        controller.renderPlan.blocks.single.inlineRuns.single.displayRange,
        const FlarkSourceRange(0, 4),
      );
    });

    test(
      'predicts render plan ranges across text edits until parsing catches up',
      () {
        final controller = FlarkFlutterController(
          runtime: FlarkEditorRuntime(
            state: FlarkEditorState.fromMarkdown(
              '**bold**',
              selection: const FlarkSelection.collapsed(6),
            ),
            extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
          ),
        );
        addTearDown(controller.dispose);

        expect(
          controller.applyParseResult(_strongParseResult(controller)),
          isTrue,
        );

        controller.dispatch(
          command: FlarkCoreEditingCommands.insertText,
          payload: const FlarkInsertTextPayload('!'),
        );

        expect(controller.markdown, '**bold!**');
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
        expect(controller.renderPlan.metadata['predictive'], isTrue);
        expect(controller.projection.projectText(controller.markdown), 'bold!');
        expect(
          controller.renderPlan.blocks.single.sourceRange,
          const FlarkSourceRange(0, 9),
        );
        expect(
          controller.renderPlan.blocks.single.displayRange,
          const FlarkSourceRange(0, 5),
        );
        expect(
          controller.renderPlan.blocks.single.inlineRuns.single.displayRange,
          const FlarkSourceRange(0, 5),
        );
      },
    );

    test(
      'predicts list item descriptors across text edits until parsing catches up',
      () {
        final controller = FlarkFlutterController(
          runtime: FlarkEditorRuntime(
            state: FlarkEditorState.fromMarkdown(
              '* item',
              selection: const FlarkSelection.collapsed(6),
            ),
          ),
        );
        addTearDown(controller.dispose);

        expect(
          controller.applyParseResult(_unorderedListParseResult(controller)),
          isTrue,
        );

        expect(
          controller.applyProjectedTextEdit(
            oldDisplayText: 'item',
            newDisplayText: 'items',
          ),
          isTrue,
        );

        expect(controller.markdown, '* items');
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
        expect(controller.renderPlan.metadata['predictive'], isTrue);
        expect(controller.renderPlan.blocks.single.listItem, isNotNull);
        expect(
          controller.renderPlan.blocks.single.listItem!.kind,
          FlarkRenderListKind.unordered,
        );
        expect(
          controller.renderPlan.blocks.single.displayRange,
          const FlarkSourceRange(0, 5),
        );
      },
    );

    test('emits typed events for runtime and parse changes', () async {
      final controller = FlarkFlutterController.fromMarkdown(
        'hello',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );
      final events = <FlarkControllerEvent>[];
      final subscription = controller.events.listen(events.add);

      addTearDown(subscription.cancel);
      addTearDown(controller.dispose);

      controller.dispatch(
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('!'),
      );
      await pumpEventQueue();
      expect(events.single.kind, FlarkControllerEventKind.projectionPredicted);
      expect(events.single.markdownChanged, isTrue);

      controller.applySelection(const FlarkSelection.collapsed(0));
      await pumpEventQueue();
      expect(events.last.kind, FlarkControllerEventKind.selectionChanged);
      expect(events.last.selectionChanged, isTrue);

      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: controller.state.document.length,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: FlarkSourceRange(0, controller.markdown.length),
          ),
        ],
        inlineTokens: const [],
      );
      expect(controller.applyParseResult(parseResult), isTrue);
      await pumpEventQueue();
      expect(events.last.kind, FlarkControllerEventKind.parseAdopted);
      expect(events.last.markdownChanged, isFalse);

      controller.undo();
      await pumpEventQueue();
      expect(events.last.kind, FlarkControllerEventKind.undo);
    });

    test('applies render-plan extensions from the runtime', () {
      final controller = FlarkFlutterController.fromMarkdown(
        'note',
        extensions: FlarkExtensionSet([const _ControllerRenderPlanExtension()]),
      );
      addTearDown(controller.dispose);

      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: controller.state.document.length,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const FlarkSourceRange(0, 4),
          ),
        ],
        inlineTokens: const [],
      );

      expect(controller.applyParseResult(parseResult), isTrue);
      expect(controller.renderPlan.metadata['controllerExtension'], isTrue);
    });

    test(
      'undo clears stale projections when no inverse transaction is exposed',
      () {
        final controller = FlarkFlutterController.fromMarkdown(
          '',
          extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
        );

        addTearDown(controller.dispose);
        controller.dispatch(
          command: FlarkCoreEditingCommands.insertText,
          payload: const FlarkInsertTextPayload('a'),
        );
        expect(controller.markdown, 'a');

        controller.undo();

        expect(controller.markdown, '');
        expect(controller.projection.textLength, 0);
        expect(controller.lastProjectionPrediction, isNull);
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
      },
    );

    test('applies current text deltas and rejects stale text deltas', () {
      final controller = FlarkFlutterController.fromMarkdown('ab');
      var notifications = 0;
      controller.addListener(() {
        notifications++;
      });

      addTearDown(controller.dispose);
      expect(
        controller.applyTextEditingDelta(
          const TextEditingDeltaInsertion(
            oldText: 'old',
            textInserted: '!',
            insertionOffset: 3,
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange.empty,
          ),
        ),
        isFalse,
      );
      expect(controller.markdown, 'ab');

      expect(
        controller.applyTextEditingDelta(
          const TextEditingDeltaInsertion(
            oldText: 'ab',
            textInserted: 'c',
            insertionOffset: 2,
            selection: TextSelection.collapsed(offset: 3),
            composing: TextRange.empty,
          ),
        ),
        isTrue,
      );
      expect(controller.markdown, 'abc');
      expect(controller.selection, const FlarkSelection.collapsed(3));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(notifications, 1);
    });

    test(
      'applies projected display edits through source selection affinity',
      () {
        final controller = FlarkFlutterController(
          runtime: FlarkEditorRuntime(
            state: FlarkEditorState.fromMarkdown(
              '**bold**',
              selection: const FlarkSelection.collapsed(6),
            ),
          ),
          projection: FlarkProjection(
            textLength: 8,
            hiddenRanges: const [
              FlarkHiddenRange(
                range: FlarkSourceRange(0, 2),
                kind: FlarkHiddenRangeKind.inlineMarker,
              ),
              FlarkHiddenRange(
                range: FlarkSourceRange(6, 8),
                kind: FlarkHiddenRangeKind.inlineMarker,
              ),
            ],
          ),
        );

        addTearDown(controller.dispose);
        expect(
          controller.applyProjectedTextEdit(
            oldDisplayText: 'stale',
            newDisplayText: 'bold!',
          ),
          isFalse,
        );
        expect(
          controller.applyProjectedTextEdit(
            oldDisplayText: 'bold',
            newDisplayText: 'bold!',
          ),
          isTrue,
        );

        expect(controller.markdown, '**bold!**');
        expect(controller.selection, const FlarkSelection.collapsed(7));
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
      },
    );
  });
}

FlarkMarkdownParseResult _strongParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: [
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.strong,
        type: 'strong',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(0, 2),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(
          controller.markdown.length - 2,
          controller.markdown.length,
        ),
      ),
    ],
  );
}

FlarkMarkdownParseResult _unorderedListParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: const {'listKind': 'unordered'},
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

final class _ControllerRenderPlanExtension extends FlarkRenderPlanExtension {
  const _ControllerRenderPlanExtension();

  @override
  String get id => 'controller-render-plan-extension';

  @override
  FlarkRenderPlan transformRenderPlan(FlarkRenderPlanContext context) {
    return FlarkRenderPlan(
      blocks: context.renderPlan.blocks,
      metadata: {...context.renderPlan.metadata, 'controllerExtension': true},
    );
  }
}
