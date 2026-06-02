import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';
import 'package:sovereign_editor/src/v2/projection/projection.dart';
import 'package:sovereign_editor/src/v2/render_plan/render_plan.dart';

void main() {
  group('SovereignRenderPlan', () {
    test('builds platform-neutral block and inline ranges from parse output',
        () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 3,
        sourceTextLength: 9,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const SovereignSourceRange(0, 9),
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.strong,
            type: 'strong',
            sourceRange: const SovereignSourceRange(0, 9),
            attributes: const {'marker': '**'},
          ),
        ],
        hiddenRanges: [
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: const SovereignSourceRange(0, 2),
            attributes: const {'marker': '**'},
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: const SovereignSourceRange(7, 9),
            attributes: const {'marker': '**'},
          ),
        ],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      expect(plan.metadata['revision'], 3);
      expect(plan.blocks.single.kind, SovereignMarkdownBlockKind.paragraph);
      expect(plan.blocks.single.sourceRange, const SovereignSourceRange(0, 9));
      expect(plan.blocks.single.displayRange, const SovereignSourceRange(0, 5));
      expect(plan.blocks.single.inlineRuns.single.kind,
          SovereignMarkdownInlineKind.strong);
      expect(plan.blocks.single.styleToken, SovereignRenderTextStyleToken.body);
      expect(plan.blocks.single.inlineRuns.single.styleToken,
          SovereignRenderTextStyleToken.strong);
      expect(
        plan.blocks.single.inlineRuns.single.displayRange,
        const SovereignSourceRange(0, 5),
      );
    });

    test('preserves unknown render node types for forward compatibility', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: 99,
        revision: 1,
        sourceTextLength: 3,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.unknown,
            type: 'admonition',
            sourceRange: const SovereignSourceRange(0, 3),
          ),
        ],
        inlineTokens: const [],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: SovereignProjection(textLength: 3),
      );

      expect(plan.blocks.single.kind, SovereignMarkdownBlockKind.unknown);
      expect(plan.blocks.single.type, 'admonition');
    });

    test('maps block and inline display ranges through replacements', () {
      const source = 'A &amp; B';
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 4,
        sourceTextLength: source.length,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: SovereignSourceRange(0, source.length),
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.emphasis,
            type: 'emphasis',
            sourceRange: const SovereignSourceRange(2, 7),
          ),
        ],
        replacementRanges: [
          SovereignMarkdownReplacementRange(
            kind: SovereignMarkdownReplacementRangeKind.htmlEntity,
            type: 'htmlEntity',
            sourceRange: const SovereignSourceRange(2, 7),
            replacementText: '&',
          ),
        ],
      );

      final projection = SovereignProjection.fromParseResult(parseResult);
      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: projection,
      );

      expect(projection.projectText(source), 'A & B');
      expect(
        plan.blocks.single.displayRange,
        const SovereignSourceRange(0, 5),
      );
      expect(
        plan.blocks.single.inlineRuns.single.displayRange,
        const SovereignSourceRange(2, 3),
      );
    });

    test('applies render-plan extensions in registration order', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 7,
        sourceTextLength: 4,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const SovereignSourceRange(0, 4),
          ),
        ],
        inlineTokens: const [],
      );
      final projection = SovereignProjection(textLength: 4);
      final renderPlan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: projection,
      );

      final transformed = applySovereignRenderPlanExtensions(
        renderPlan: renderPlan,
        parseResult: parseResult,
        projection: projection,
        extensions: SovereignExtensionSet([
          const _MetadataRenderPlanExtension('first'),
          const _MetadataRenderPlanExtension('second'),
        ]),
      );

      expect(transformed.metadata['extensions'], ['first', 'second']);
      expect(transformed.blocks, renderPlan.blocks);
    });

    test('assigns renderer-neutral heading style tokens', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 8,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.heading,
            type: 'heading',
            sourceRange: const SovereignSourceRange(0, 8),
            attributes: const {'level': 2},
          ),
        ],
        inlineTokens: const [],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      expect(plan.blocks.single.styleToken,
          SovereignRenderTextStyleToken.heading2);
    });

    test('attaches inline runs to the deepest owning block only', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 10,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.blockquote,
            type: 'blockquote',
            sourceRange: const SovereignSourceRange(0, 10),
            children: [
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.paragraph,
                type: 'paragraph',
                sourceRange: const SovereignSourceRange(2, 10),
              ),
            ],
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.emphasis,
            type: 'emphasis',
            sourceRange: const SovereignSourceRange(2, 8),
          ),
        ],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: SovereignProjection(textLength: 10),
      );

      expect(plan.blocks.single.inlineRuns, isEmpty);
      expect(plan.blocks.single.children.single.inlineRuns.single.kind,
          SovereignMarkdownInlineKind.emphasis);
    });

    test('exposes typed table descriptors from parser attributes', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 32,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const SovereignSourceRange(0, 32),
            attributes: const {
              'alignments': ['left', 'right', 'future'],
            },
            children: [
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.tableRow,
                type: 'tableRow',
                sourceRange: const SovereignSourceRange(0, 16),
                attributes: const {'header': true},
                children: [
                  SovereignMarkdownBlockNode(
                    kind: SovereignMarkdownBlockKind.tableCell,
                    type: 'tableCell',
                    sourceRange: const SovereignSourceRange(2, 6),
                  ),
                  SovereignMarkdownBlockNode(
                    kind: SovereignMarkdownBlockKind.tableCell,
                    type: 'tableCell',
                    sourceRange: const SovereignSourceRange(9, 15),
                  ),
                ],
              ),
            ],
          ),
        ],
        inlineTokens: const [],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      expect(plan.blocks.single.table, isNotNull);
      expect(plan.blocks.single.table!.columnAlignments, const [
        SovereignRenderTableColumnAlignment.left,
        SovereignRenderTableColumnAlignment.right,
        SovereignRenderTableColumnAlignment.unknown,
      ]);
      expect(plan.blocks.single.table!.rows, hasLength(1));
      expect(plan.blocks.single.table!.rows.single.header, isTrue);
      expect(plan.blocks.single.table!.rows.single.cells, hasLength(2));
      expect(
        plan.blocks.single.table!.rows.single.cells.first.sourceRange,
        const SovereignSourceRange(2, 6),
      );
    });

    test('exposes list, task-list, and code-fence descriptors', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 43,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const SovereignSourceRange(0, 10),
            attributes: const {'listKind': 'unordered'},
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const SovereignSourceRange(11, 21),
            attributes: const {'listKind': 'ordered'},
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const SovereignSourceRange(22, 32),
            attributes: const {'checked': true, 'listKind': 'unordered'},
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.codeBlock,
            type: 'codeBlock',
            sourceRange: const SovereignSourceRange(33, 43),
            attributes: const {'language': 'dart'},
          ),
        ],
        inlineTokens: const [],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      expect(plan.blocks[0].listItem!.kind, SovereignRenderListKind.unordered);
      expect(plan.blocks[1].listItem!.kind, SovereignRenderListKind.ordered);
      expect(plan.blocks[2].listItem!.kind, SovereignRenderListKind.unordered);
      expect(plan.blocks[2].taskListItem!.checked, isTrue);
      expect(plan.blocks.last.codeBlock!.language, 'dart');
    });

    test('exposes link and image action descriptors on inline runs', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 35,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const SovereignSourceRange(0, 35),
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const SovereignSourceRange(0, 15),
            attributes: const {
              'destination': 'https://example.com',
              'title': 'Example',
              'label': 'link',
            },
          ),
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const SovereignSourceRange(16, 35),
            attributes: const {
              'src': 'asset://image.png',
              'alt': 'Alt text',
            },
          ),
        ],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      final link = plan.blocks.single.inlineRuns.first.action!;
      expect(link.kind, SovereignRenderInlineActionKind.link);
      expect(link.destination, 'https://example.com');
      expect(link.title, 'Example');
      expect(link.label, 'link');

      final image = plan.blocks.single.inlineRuns.last.action!;
      expect(image.kind, SovereignRenderInlineActionKind.image);
      expect(image.destination, 'asset://image.png');
      expect(image.label, 'Alt text');
    });

    test('predicts stable semantic block descriptors through content edits',
        () {
      final cases = [
        _PredictionCase(
          id: 'heading',
          block: _block(
            kind: SovereignMarkdownBlockKind.heading,
            type: 'heading',
            styleToken: SovereignRenderTextStyleToken.heading2,
            sourceEnd: 8,
            displayEnd: 6,
            attributes: const {'level': 2},
          ),
          projection: SovereignProjection(
            textLength: 8,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 2),
                kind: SovereignHiddenRangeKind.markdownMarker,
              ),
            ],
          ),
          verify: (block) {
            expect(block.kind, SovereignMarkdownBlockKind.heading);
            expect(block.styleToken, SovereignRenderTextStyleToken.heading2);
            expect(block.attributes['level'], 2);
          },
        ),
        _PredictionCase(
          id: 'blockquote',
          block: _block(
            kind: SovereignMarkdownBlockKind.blockquote,
            type: 'blockquote',
            sourceEnd: 9,
            displayEnd: 7,
          ),
          projection: SovereignProjection(
            textLength: 9,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 2),
                kind: SovereignHiddenRangeKind.blockMarker,
              ),
            ],
          ),
          verify: (block) {
            expect(block.kind, SovereignMarkdownBlockKind.blockquote);
          },
        ),
        _PredictionCase(
          id: 'unordered list item',
          block: _block(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceEnd: 6,
            displayEnd: 4,
            listItem: const SovereignRenderListItemDescriptor(
              kind: SovereignRenderListKind.unordered,
            ),
          ),
          projection: SovereignProjection(
            textLength: 6,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 2),
                kind: SovereignHiddenRangeKind.markdownMarker,
              ),
            ],
          ),
          verify: (block) {
            expect(block.listItem, isNotNull);
            expect(
              block.listItem!.kind,
              SovereignRenderListKind.unordered,
            );
          },
        ),
        _PredictionCase(
          id: 'ordered list item',
          block: _block(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceEnd: 8,
            displayEnd: 5,
            listItem: const SovereignRenderListItemDescriptor(
              kind: SovereignRenderListKind.ordered,
            ),
          ),
          projection: SovereignProjection(
            textLength: 8,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 3),
                kind: SovereignHiddenRangeKind.markdownMarker,
              ),
            ],
          ),
          verify: (block) {
            expect(block.listItem, isNotNull);
            expect(block.listItem!.kind, SovereignRenderListKind.ordered);
          },
        ),
        _PredictionCase(
          id: 'task list item',
          block: _block(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceEnd: 12,
            displayEnd: 6,
            listItem: const SovereignRenderListItemDescriptor(
              kind: SovereignRenderListKind.unordered,
            ),
            taskListItem: const SovereignRenderTaskListItemDescriptor(
              checked: false,
            ),
          ),
          projection: SovereignProjection(
            textLength: 12,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 6),
                kind: SovereignHiddenRangeKind.blockMarker,
              ),
            ],
          ),
          verify: (block) {
            expect(block.listItem, isNotNull);
            expect(block.taskListItem, isNotNull);
            expect(block.taskListItem!.checked, isFalse);
          },
        ),
        _PredictionCase(
          id: 'code block',
          block: _block(
            kind: SovereignMarkdownBlockKind.codeBlock,
            type: 'codeBlock',
            sourceEnd: 18,
            displayEnd: 4,
            codeBlock: const SovereignRenderCodeBlockDescriptor(
              language: 'dart',
            ),
          ),
          projection: SovereignProjection(
            textLength: 18,
            hiddenRanges: const [
              SovereignHiddenRange(
                range: SovereignSourceRange(0, 8),
                kind: SovereignHiddenRangeKind.markdownMarker,
              ),
              SovereignHiddenRange(
                range: SovereignSourceRange(12, 18),
                kind: SovereignHiddenRangeKind.markdownMarker,
              ),
            ],
          ),
          insertOffset: 12,
          verify: (block) {
            expect(block.codeBlock, isNotNull);
            expect(block.codeBlock!.language, 'dart');
          },
        ),
        _PredictionCase(
          id: 'table',
          block: _block(
            kind: SovereignMarkdownBlockKind.table,
            type: 'table',
            sourceEnd: 40,
            displayEnd: 40,
            table: SovereignRenderTableDescriptor(
              columnAlignments: const [
                SovereignRenderTableColumnAlignment.left,
                SovereignRenderTableColumnAlignment.right,
              ],
            ),
          ),
          projection: SovereignProjection(textLength: 40),
          insertOffset: 28,
          verify: (block) {
            expect(block.table, isNotNull);
            expect(block.table!.columnAlignments, const [
              SovereignRenderTableColumnAlignment.left,
              SovereignRenderTableColumnAlignment.right,
            ]);
          },
        ),
      ];

      for (final predictionCase in cases) {
        final transaction = SovereignTransaction.single(
          SovereignSourceOperation.insert(predictionCase.insertOffset, '!'),
        );
        final projectionPrediction = predictionCase.projection.predictAfter(
          transaction,
          textLengthAfter: predictionCase.projection.textLength + 1,
        );
        final predicted = SovereignRenderPlan(
          blocks: [predictionCase.block],
          metadata: const {'revision': 1},
        ).predictThroughTransaction(
          transaction: transaction,
          projection: projectionPrediction.projection,
          revision: 2,
          textLengthAfter: predictionCase.projection.textLength + 1,
        );

        expect(predicted.metadata['predictive'], isTrue,
            reason: predictionCase.id);
        expect(predicted.metadata['revision'], 2, reason: predictionCase.id);
        expect(predicted.blocks, hasLength(1), reason: predictionCase.id);
        final block = predicted.blocks.single;
        expect(block.sourceRange.end, predictionCase.block.sourceRange.end + 1,
            reason: predictionCase.id);
        expect(
          block.displayRange.end,
          predictionCase.block.displayRange.end + 1,
          reason: predictionCase.id,
        );
        predictionCase.verify(block);
      }
    });

    test('predicts inline semantic actions through content edits', () {
      final plan = SovereignRenderPlan(
        blocks: [
          _block(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceEnd: 16,
            displayEnd: 12,
            inlineRuns: [
              SovereignRenderInlineRun(
                kind: SovereignMarkdownInlineKind.link,
                type: 'link',
                sourceRange: const SovereignSourceRange(1, 10),
                displayRange: const SovereignSourceRange(0, 4),
                styleToken: SovereignRenderTextStyleToken.link,
                action: const SovereignRenderInlineActionDescriptor(
                  kind: SovereignRenderInlineActionKind.link,
                  destination: 'https://example.com',
                  label: 'link',
                ),
                attributes: const {'destination': 'https://example.com'},
              ),
            ],
          ),
        ],
      );
      final projection = SovereignProjection(
        textLength: 16,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 1),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(5, 10),
            kind: SovereignHiddenRangeKind.linkDestination,
          ),
        ],
      );
      final transaction = SovereignTransaction.single(
        SovereignSourceOperation.insert(4, '!'),
      );
      final projectionPrediction = projection.predictAfter(
        transaction,
        textLengthAfter: 17,
      );

      final predicted = plan.predictThroughTransaction(
        transaction: transaction,
        projection: projectionPrediction.projection,
        revision: 2,
        textLengthAfter: 17,
      );

      final run = predicted.blocks.single.inlineRuns.single;
      expect(run.kind, SovereignMarkdownInlineKind.link);
      expect(run.styleToken, SovereignRenderTextStyleToken.link);
      expect(run.action, isNotNull);
      expect(run.action!.kind, SovereignRenderInlineActionKind.link);
      expect(run.action!.destination, 'https://example.com');
      expect(run.sourceRange, const SovereignSourceRange(1, 11));
      expect(run.displayRange, const SovereignSourceRange(0, 5));
    });

    test('queries overlay-oriented blocks and inline action runs', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 48,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const SovereignSourceRange(0, 20),
            attributes: const {
              'alignments': ['left'],
            },
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.list,
            type: 'list',
            sourceRange: const SovereignSourceRange(21, 48),
            children: [
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.listItem,
                type: 'listItem',
                sourceRange: const SovereignSourceRange(23, 35),
                attributes: const {'checked': false},
              ),
              SovereignMarkdownBlockNode(
                kind: SovereignMarkdownBlockKind.codeBlock,
                type: 'codeBlock',
                sourceRange: const SovereignSourceRange(36, 48),
                attributes: const {'language': 'dart'},
              ),
            ],
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const SovereignSourceRange(2, 8),
            attributes: const {'destination': 'https://example.com'},
          ),
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const SovereignSourceRange(24, 32),
            attributes: const {'src': 'asset://image.png'},
          ),
        ],
      );

      final plan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      );

      expect(plan.allBlocks.length, 4);
      expect(plan.tableBlocks.single.kind, SovereignMarkdownBlockKind.table);
      expect(plan.taskListItemBlocks.single.taskListItem!.checked, isFalse);
      expect(plan.codeBlocks.single.codeBlock!.language, 'dart');
      expect(plan.linkRuns.single.action!.destination, 'https://example.com');
      expect(plan.imageRuns.single.action!.destination, 'asset://image.png');
      expect(
        plan.blockAtDisplayOffset(24)!.kind,
        SovereignMarkdownBlockKind.listItem,
      );
      expect(
        plan.inlineRunAtDisplayOffset(4)!.action!.kind,
        SovereignRenderInlineActionKind.link,
      );
    });

    test('builds overlay targets from render descriptors', () {
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 32,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const SovereignSourceRange(0, 10),
            attributes: const {
              'alignments': ['left'],
            },
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const SovereignSourceRange(11, 20),
            attributes: const {'checked': true},
          ),
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.codeBlock,
            type: 'codeBlock',
            sourceRange: const SovereignSourceRange(21, 32),
            attributes: const {'language': 'dart'},
          ),
        ],
        inlineTokens: [
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const SovereignSourceRange(1, 5),
            attributes: const {'destination': 'https://example.com'},
          ),
          SovereignMarkdownInlineToken(
            kind: SovereignMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const SovereignSourceRange(12, 18),
            attributes: const {'src': 'asset://image.png'},
          ),
        ],
      );

      final overlayPlan = SovereignRenderPlan.fromParseResult(
        parseResult: parseResult,
      ).overlayPlan();

      expect(
          overlayPlan.targets.map((target) => target.kind),
          containsAll([
            SovereignRenderOverlayKind.link,
            SovereignRenderOverlayKind.image,
            SovereignRenderOverlayKind.taskListItem,
            SovereignRenderOverlayKind.table,
            SovereignRenderOverlayKind.codeBlock,
          ]));
      expect(
        overlayPlan
            .ofKind(SovereignRenderOverlayKind.link)
            .single
            .action!
            .destination,
        'https://example.com',
      );
      expect(
        overlayPlan
            .ofKind(SovereignRenderOverlayKind.taskListItem)
            .single
            .taskListItem!
            .checked,
        isTrue,
      );
      expect(
        overlayPlan.ofKind(SovereignRenderOverlayKind.table).single.table,
        isNotNull,
      );
      expect(
        overlayPlan
            .ofKind(SovereignRenderOverlayKind.codeBlock)
            .single
            .codeBlock!
            .language,
        'dart',
      );
    });
  });
}

SovereignRenderBlock _block({
  required SovereignMarkdownBlockKind kind,
  required String type,
  required int sourceEnd,
  required int displayEnd,
  SovereignRenderTextStyleToken styleToken = SovereignRenderTextStyleToken.body,
  Iterable<SovereignRenderInlineRun> inlineRuns = const [],
  Iterable<SovereignRenderBlock> children = const [],
  SovereignRenderTableDescriptor? table,
  SovereignRenderListItemDescriptor? listItem,
  SovereignRenderTaskListItemDescriptor? taskListItem,
  SovereignRenderCodeBlockDescriptor? codeBlock,
  Map<String, Object?> attributes = const {},
}) {
  return SovereignRenderBlock(
    kind: kind,
    type: type,
    sourceRange: SovereignSourceRange(0, sourceEnd),
    displayRange: SovereignSourceRange(0, displayEnd),
    styleToken: styleToken,
    inlineRuns: inlineRuns,
    children: children,
    table: table,
    listItem: listItem,
    taskListItem: taskListItem,
    codeBlock: codeBlock,
    attributes: attributes,
  );
}

final class _PredictionCase {
  _PredictionCase({
    required this.id,
    required this.block,
    required this.projection,
    required this.verify,
    int? insertOffset,
  }) : insertOffset = insertOffset ?? block.sourceRange.end;

  final String id;
  final SovereignRenderBlock block;
  final SovereignProjection projection;
  final int insertOffset;
  final void Function(SovereignRenderBlock block) verify;
}

final class _MetadataRenderPlanExtension extends SovereignRenderPlanExtension {
  const _MetadataRenderPlanExtension(this.id);

  @override
  final String id;

  @override
  SovereignRenderPlan transformRenderPlan(
    SovereignRenderPlanContext context,
  ) {
    final previous = context.renderPlan.metadata['extensions'];
    return SovereignRenderPlan(
      blocks: context.renderPlan.blocks,
      metadata: {
        ...context.renderPlan.metadata,
        'extensions': [
          if (previous is List) ...previous,
          id,
        ],
      },
    );
  }
}
