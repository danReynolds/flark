import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';
import 'package:sovereign_editor/src/v2/projection/projection.dart';
import 'package:sovereign_editor/src/v2/render_plan/render_plan.dart';

void main() {
  group('SovereignFlutterController', () {
    test('starts from markdown with a stale empty render plan', () {
      final controller = SovereignFlutterController.fromMarkdown('hello');

      addTearDown(controller.dispose);
      expect(controller.markdown, 'hello');
      expect(controller.selection, const SovereignSelection.collapsed(5));
      expect(controller.projection.textLength, 5);
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(controller.renderPlan.metadata['stale'], isTrue);
    });

    test('dispatches commands and predicts projection until parse catches up',
        () {
      final runtime = SovereignEditorRuntime.fromMarkdown(
        '**bold**',
        extensions: SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
        ]),
      );
      final controller = SovereignFlutterController(
        runtime: runtime,
        projection: SovereignProjection(
          textLength: 8,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(0, 2),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
            SovereignHiddenRange(
              range: SovereignSourceRange(6, 8),
              kind: SovereignHiddenRangeKind.inlineMarker,
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
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('!'),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**!');
      expect(controller.projection.textLength, 9);
      expect(controller.projection.hiddenRanges.last.range,
          const SovereignSourceRange(6, 8));
      expect(controller.lastProjectionPrediction, isNotNull);
      expect(
        controller.lastProjectionPrediction!.touchedProjectionSensitiveRange,
        isFalse,
      );
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(notifications, 1);
    });

    test('accepts current parse results and rejects stale ones', () {
      final controller = SovereignFlutterController.fromMarkdown('**bold**');
      var notifications = 0;
      controller.addListener(() {
        notifications++;
      });

      addTearDown(controller.dispose);
      final stale = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 99,
        sourceTextLength: 8,
        blocks: const [],
        inlineTokens: const [],
      );
      final current = SovereignMarkdownParseResult(
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
      );

      expect(controller.applyParseResult(stale), isFalse);
      expect(controller.applyParseResult(current), isTrue);
      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(controller.projection.projectText(controller.markdown), 'bold');
      expect(controller.renderPlan.blocks.single.inlineRuns.single.kind,
          SovereignMarkdownInlineKind.strong);
      expect(notifications, 1);
    });

    test('keeps render plan authoritative across selection-only changes', () {
      final controller = SovereignFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);

      expect(
          controller.applyParseResult(_strongParseResult(controller)), isTrue);
      expect(controller.hasAuthoritativeRenderPlan, isTrue);

      controller.applySelection(
        const SovereignSelection.collapsed(2),
        userEvent: 'test',
      );

      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(controller.renderPlan.blocks.single.inlineRuns.single.displayRange,
          const SovereignSourceRange(0, 4));
    });

    test(
        'predicts render plan ranges across text edits until parsing catches up',
        () {
      final controller = SovereignFlutterController(
        runtime: SovereignEditorRuntime(
          state: SovereignEditorState.fromMarkdown(
            '**bold**',
            selection: const SovereignSelection.collapsed(6),
          ),
          extensions: SovereignExtensionSet([
            const SovereignCoreEditingExtension(),
          ]),
        ),
      );
      addTearDown(controller.dispose);

      expect(
          controller.applyParseResult(_strongParseResult(controller)), isTrue);

      controller.dispatch(
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('!'),
      );

      expect(controller.markdown, '**bold!**');
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(controller.renderPlan.metadata['predictive'], isTrue);
      expect(
        controller.projection.projectText(controller.markdown),
        'bold!',
      );
      expect(controller.renderPlan.blocks.single.sourceRange,
          const SovereignSourceRange(0, 9));
      expect(controller.renderPlan.blocks.single.displayRange,
          const SovereignSourceRange(0, 5));
      expect(controller.renderPlan.blocks.single.inlineRuns.single.displayRange,
          const SovereignSourceRange(0, 5));
    });

    test(
        'predicts list item descriptors across text edits until parsing catches up',
        () {
      final controller = SovereignFlutterController(
        runtime: SovereignEditorRuntime(
          state: SovereignEditorState.fromMarkdown(
            '* item',
            selection: const SovereignSelection.collapsed(6),
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
        SovereignRenderListKind.unordered,
      );
      expect(
        controller.renderPlan.blocks.single.displayRange,
        const SovereignSourceRange(0, 5),
      );
    });

    test('emits typed events for runtime and parse changes', () async {
      final controller = SovereignFlutterController.fromMarkdown(
        'hello',
        extensions: SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
        ]),
      );
      final events = <SovereignControllerEvent>[];
      final subscription = controller.events.listen(events.add);

      addTearDown(subscription.cancel);
      addTearDown(controller.dispose);

      controller.dispatch(
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('!'),
      );
      await pumpEventQueue();
      expect(
          events.single.kind, SovereignControllerEventKind.projectionPredicted);
      expect(events.single.markdownChanged, isTrue);

      controller.applySelection(const SovereignSelection.collapsed(0));
      await pumpEventQueue();
      expect(events.last.kind, SovereignControllerEventKind.selectionChanged);
      expect(events.last.selectionChanged, isTrue);

      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: controller.state.document.length,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: SovereignSourceRange(0, controller.markdown.length),
          ),
        ],
        inlineTokens: const [],
      );
      expect(controller.applyParseResult(parseResult), isTrue);
      await pumpEventQueue();
      expect(events.last.kind, SovereignControllerEventKind.parseAdopted);
      expect(events.last.markdownChanged, isFalse);

      controller.undo();
      await pumpEventQueue();
      expect(events.last.kind, SovereignControllerEventKind.undo);
    });

    test('applies render-plan extensions from the runtime', () {
      final controller = SovereignFlutterController.fromMarkdown(
        'note',
        extensions: SovereignExtensionSet([
          const _ControllerRenderPlanExtension(),
        ]),
      );
      addTearDown(controller.dispose);

      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: controller.state.document.length,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const SovereignSourceRange(0, 4),
          ),
        ],
        inlineTokens: const [],
      );

      expect(controller.applyParseResult(parseResult), isTrue);
      expect(controller.renderPlan.metadata['controllerExtension'], isTrue);
    });

    test('undo clears stale projections when no inverse transaction is exposed',
        () {
      final controller = SovereignFlutterController.fromMarkdown(
        '',
        extensions: SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
        ]),
      );

      addTearDown(controller.dispose);
      controller.dispatch(
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('a'),
      );
      expect(controller.markdown, 'a');

      controller.undo();

      expect(controller.markdown, '');
      expect(controller.projection.textLength, 0);
      expect(controller.lastProjectionPrediction, isNull);
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
    });

    test('applies current text deltas and rejects stale text deltas', () {
      final controller = SovereignFlutterController.fromMarkdown('ab');
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
      expect(controller.selection, const SovereignSelection.collapsed(3));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(notifications, 1);
    });

    test('applies projected display edits through source selection affinity',
        () {
      final controller = SovereignFlutterController(
        runtime: SovereignEditorRuntime(
          state: SovereignEditorState.fromMarkdown(
            '**bold**',
            selection: const SovereignSelection.collapsed(6),
          ),
        ),
        projection: SovereignProjection(
          textLength: 8,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(0, 2),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
            SovereignHiddenRange(
              range: SovereignSourceRange(6, 8),
              kind: SovereignHiddenRangeKind.inlineMarker,
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
      expect(controller.selection, const SovereignSelection.collapsed(7));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
    });
  });
}

SovereignMarkdownParseResult _strongParseResult(
  SovereignFlutterController controller,
) {
  return SovereignMarkdownParseResult(
    schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: SovereignSourceRange(0, controller.markdown.length),
      ),
    ],
    inlineTokens: [
      SovereignMarkdownInlineToken(
        kind: SovereignMarkdownInlineKind.strong,
        type: 'strong',
        sourceRange: SovereignSourceRange(0, controller.markdown.length),
      ),
    ],
    hiddenRanges: [
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: SovereignSourceRange(0, 2),
      ),
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: SovereignSourceRange(
          controller.markdown.length - 2,
          controller.markdown.length,
        ),
      ),
    ],
  );
}

SovereignMarkdownParseResult _unorderedListParseResult(
  SovereignFlutterController controller,
) {
  return SovereignMarkdownParseResult(
    schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: SovereignSourceRange(0, controller.markdown.length),
        attributes: const {'listKind': 'unordered'},
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

final class _ControllerRenderPlanExtension
    extends SovereignRenderPlanExtension {
  const _ControllerRenderPlanExtension();

  @override
  String get id => 'controller-render-plan-extension';

  @override
  SovereignRenderPlan transformRenderPlan(
    SovereignRenderPlanContext context,
  ) {
    return SovereignRenderPlan(
      blocks: context.renderPlan.blocks,
      metadata: {
        ...context.renderPlan.metadata,
        'controllerExtension': true,
      },
    );
  }
}
