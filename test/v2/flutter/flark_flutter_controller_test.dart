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
      expect(controller.renderPlan.fidelity, FlarkRenderPlanFidelity.stale);
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
        expect(
          controller.renderPlan.fidelity,
          FlarkRenderPlanFidelity.predicted,
        );
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
        expect(
          controller.renderPlan.fidelity,
          FlarkRenderPlanFidelity.predicted,
        );
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

    test(
      'promotes parsed fence opener to a code block when newline is typed',
      () {
        final controller = FlarkFlutterController.fromMarkdown('```');
        addTearDown(controller.dispose);

        expect(
          controller.applyParseResult(
            _unclosedCodeFenceOpenerParseResult(controller),
          ),
          isTrue,
        );
        expect(controller.hasAuthoritativeRenderPlan, isTrue);
        expect(controller.renderPlan.blocks.single.codeBlock, isNotNull);

        controller.applyTransaction(
          FlarkTransaction.single(
            FlarkSourceOperation.insert(3, '\n'),
            selectionBefore: const FlarkSelection.collapsed(3),
            selectionAfter: const FlarkSelection.collapsed(4),
          ),
        );

        expect(controller.markdown, '```\n');
        expect(controller.selection, const FlarkSelection.collapsed(4));
        expect(controller.hasAuthoritativeRenderPlan, isFalse);
        expect(
          controller.renderPlan.fidelity,
          FlarkRenderPlanFidelity.predicted,
        );
        expect(controller.renderPlan.blocks.single.codeBlock, isNotNull);
        expect(controller.projection.projectText(controller.markdown), isEmpty);
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

    test('exposes typed markdown and selection change projections', () async {
      final controller = FlarkFlutterController.fromMarkdown(
        'hello',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );
      final markdownChanges = <String>[];
      final selectionChanges = <FlarkSelection>[];
      final markdownSub = controller.markdownChanges.listen(
        markdownChanges.add,
      );
      final selectionSub = controller.selectionChanges.listen(
        selectionChanges.add,
      );

      addTearDown(markdownSub.cancel);
      addTearDown(selectionSub.cancel);
      addTearDown(controller.dispose);

      controller.dispatch(
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('!'),
      );
      await pumpEventQueue();
      expect(markdownChanges, ['hello!']);

      controller.applySelection(const FlarkSelection.collapsed(0));
      await pumpEventQueue();
      // A selection-only change projects to selectionChanges, not markdown.
      expect(markdownChanges, ['hello!']);
      expect(selectionChanges.last, const FlarkSelection.collapsed(0));
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

    test('undo predicts the projection through the inverse transaction', () {
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
      expect(controller.lastProjectionPrediction, isNotNull);
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
    });

    test('undo keeps a predicted render plan instead of an empty reset', () {
      final controller = FlarkFlutterController.fromMarkdown(
        '**bold** text',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );
      addTearDown(controller.dispose);

      // Adopt an authoritative parse so the controller holds a real plan and
      // a projection with hidden inline markers.
      expect(
        controller.applyParseResult(_strongParseResult(controller)),
        isTrue,
      );
      expect(controller.renderPlan.blocks, isNotEmpty);
      expect(controller.projection.hiddenRanges, isNotEmpty);

      // Edit, then undo. Both steps must keep a non-empty predicted plan and
      // mapped hidden ranges; undo previously reset to an empty stale plan,
      // flashing raw source in live-rendered surfaces.
      controller.dispatch(
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('!'),
      );
      expect(controller.renderPlan.blocks, isNotEmpty);

      controller.undo();
      expect(controller.markdown, '**bold** text');
      expect(controller.renderPlan.blocks, isNotEmpty);
      expect(controller.projection.hiddenRanges, isNotEmpty);
      expect(controller.hasAuthoritativeRenderPlan, isFalse);

      controller.redo();
      expect(controller.markdown, '**bold** text!');
      expect(controller.renderPlan.blocks, isNotEmpty);
      expect(controller.projection.hiddenRanges, isNotEmpty);
    });

    test(
      'undo with nothing to undo keeps the render plan and stays silent',
      () {
        final controller = FlarkFlutterController.fromMarkdown('**bold** text');
        addTearDown(controller.dispose);
        expect(
          controller.applyParseResult(_strongParseResult(controller)),
          isTrue,
        );
        var notifications = 0;
        controller.addListener(() => notifications++);

        controller.undo();

        expect(controller.renderPlan.blocks, isNotEmpty);
        expect(controller.hasAuthoritativeRenderPlan, isTrue);
        expect(notifications, 0);
      },
    );

    test(
      'grouped undo maps the projection through every inverse transaction',
      () {
        final controller = FlarkFlutterController.fromMarkdown(
          '**bold** text',
          extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
        );
        addTearDown(controller.dispose);
        expect(
          controller.applyParseResult(_strongParseResult(controller)),
          isTrue,
        );

        // Two grouped edits become one history entry with two inverse
        // transactions; undo must map through both in order.
        controller.applySelection(
          FlarkSelection.collapsed(controller.markdown.length),
        );
        controller.applyTransaction(
          FlarkTransaction.single(
            FlarkSourceOperation.insert(controller.markdown.length, '!'),
            undoGroupId: 7,
          ),
        );
        controller.applyTransaction(
          FlarkTransaction.single(
            FlarkSourceOperation.insert(controller.markdown.length, '?'),
            undoGroupId: 7,
          ),
        );
        expect(controller.markdown, '**bold** text!?');

        final result = controller.undo();
        expect(result.appliedTransactions, hasLength(2));
        expect(controller.markdown, '**bold** text');
        expect(controller.renderPlan.blocks, isNotEmpty);
        expect(controller.projection.hiddenRanges, isNotEmpty);
        expect(controller.projection.textLength, controller.markdown.length);
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

FlarkMarkdownParseResult _unclosedCodeFenceOpenerParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
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
