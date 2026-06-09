import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/projection/projection.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkRenderPlan', () {
    test(
      'builds platform-neutral block and inline ranges from parse output',
      () {
        final parseResult = FlarkMarkdownParseResult(
          schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
          revision: 3,
          sourceTextLength: 9,
          blocks: [
            FlarkMarkdownBlockNode(
              kind: FlarkMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: const FlarkSourceRange(0, 9),
            ),
          ],
          inlineTokens: [
            FlarkMarkdownInlineToken(
              kind: FlarkMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: const FlarkSourceRange(0, 9),
              attributes: const {'marker': '**'},
            ),
          ],
          hiddenRanges: [
            FlarkMarkdownHiddenRange(
              kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: const FlarkSourceRange(0, 2),
              attributes: const {'marker': '**'},
            ),
            FlarkMarkdownHiddenRange(
              kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: const FlarkSourceRange(7, 9),
              attributes: const {'marker': '**'},
            ),
          ],
        );

        final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

        expect(plan.metadata['revision'], 3);
        expect(plan.blocks.single.kind, FlarkMarkdownBlockKind.paragraph);
        expect(plan.blocks.single.sourceRange, const FlarkSourceRange(0, 9));
        expect(plan.blocks.single.displayRange, const FlarkSourceRange(0, 5));
        expect(
          plan.blocks.single.inlineRuns.single.kind,
          FlarkMarkdownInlineKind.strong,
        );
        expect(plan.blocks.single.styleToken, FlarkRenderTextStyleToken.body);
        expect(
          plan.blocks.single.inlineRuns.single.styleToken,
          FlarkRenderTextStyleToken.strong,
        );
        expect(
          plan.blocks.single.inlineRuns.single.displayRange,
          const FlarkSourceRange(0, 5),
        );
      },
    );

    test('preserves unknown render node types for forward compatibility', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: 99,
        revision: 1,
        sourceTextLength: 3,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.unknown,
            type: 'admonition',
            sourceRange: const FlarkSourceRange(0, 3),
          ),
        ],
        inlineTokens: const [],
      );

      final plan = FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: FlarkProjection(textLength: 3),
      );

      expect(plan.blocks.single.kind, FlarkMarkdownBlockKind.unknown);
      expect(plan.blocks.single.type, 'admonition');
    });

    test('maps block and inline display ranges through replacements', () {
      const source = 'A &amp; B';
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 4,
        sourceTextLength: source.length,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: FlarkSourceRange(0, source.length),
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.emphasis,
            type: 'emphasis',
            sourceRange: const FlarkSourceRange(2, 7),
          ),
        ],
        replacementRanges: [
          FlarkMarkdownReplacementRange(
            kind: FlarkMarkdownReplacementRangeKind.htmlEntity,
            type: 'htmlEntity',
            sourceRange: const FlarkSourceRange(2, 7),
            replacementText: '&',
          ),
        ],
      );

      final projection = FlarkProjection.fromParseResult(parseResult);
      final plan = FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: projection,
      );

      expect(projection.projectText(source), 'A & B');
      expect(plan.blocks.single.displayRange, const FlarkSourceRange(0, 5));
      expect(
        plan.blocks.single.inlineRuns.single.displayRange,
        const FlarkSourceRange(2, 3),
      );
    });

    test('applies render-plan extensions in registration order', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 7,
        sourceTextLength: 4,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const FlarkSourceRange(0, 4),
          ),
        ],
        inlineTokens: const [],
      );
      final projection = FlarkProjection(textLength: 4);
      final renderPlan = FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: projection,
      );

      final transformed = applyFlarkRenderPlanExtensions(
        renderPlan: renderPlan,
        parseResult: parseResult,
        projection: projection,
        extensions: FlarkExtensionSet([
          const _MetadataRenderPlanExtension('first'),
          const _MetadataRenderPlanExtension('second'),
        ]),
      );

      expect(transformed.metadata['extensions'], ['first', 'second']);
      expect(transformed.blocks, renderPlan.blocks);
    });

    test('assigns renderer-neutral heading style tokens', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 8,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.heading,
            type: 'heading',
            sourceRange: const FlarkSourceRange(0, 8),
            attributes: const {'level': 2},
          ),
        ],
        inlineTokens: const [],
      );

      final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

      expect(plan.blocks.single.styleToken, FlarkRenderTextStyleToken.heading2);
    });

    test('attaches inline runs to the deepest owning block only', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 10,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.blockquote,
            type: 'blockquote',
            sourceRange: const FlarkSourceRange(0, 10),
            children: [
              FlarkMarkdownBlockNode(
                kind: FlarkMarkdownBlockKind.paragraph,
                type: 'paragraph',
                sourceRange: const FlarkSourceRange(2, 10),
              ),
            ],
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.emphasis,
            type: 'emphasis',
            sourceRange: const FlarkSourceRange(2, 8),
          ),
        ],
      );

      final plan = FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: FlarkProjection(textLength: 10),
      );

      expect(plan.blocks.single.inlineRuns, isEmpty);
      expect(
        plan.blocks.single.children.single.inlineRuns.single.kind,
        FlarkMarkdownInlineKind.emphasis,
      );
    });

    test('exposes typed table descriptors from parser attributes', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 32,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const FlarkSourceRange(0, 32),
            attributes: const {
              'alignments': ['left', 'right', 'future'],
            },
            children: [
              FlarkMarkdownBlockNode(
                kind: FlarkMarkdownBlockKind.tableRow,
                type: 'tableRow',
                sourceRange: const FlarkSourceRange(0, 16),
                attributes: const {'header': true},
                children: [
                  FlarkMarkdownBlockNode(
                    kind: FlarkMarkdownBlockKind.tableCell,
                    type: 'tableCell',
                    sourceRange: const FlarkSourceRange(2, 6),
                  ),
                  FlarkMarkdownBlockNode(
                    kind: FlarkMarkdownBlockKind.tableCell,
                    type: 'tableCell',
                    sourceRange: const FlarkSourceRange(9, 15),
                  ),
                ],
              ),
            ],
          ),
        ],
        inlineTokens: const [],
      );

      final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

      expect(plan.blocks.single.table, isNotNull);
      expect(plan.blocks.single.table!.columnAlignments, const [
        FlarkRenderTableColumnAlignment.left,
        FlarkRenderTableColumnAlignment.right,
        FlarkRenderTableColumnAlignment.unknown,
      ]);
      expect(plan.blocks.single.table!.rows, hasLength(1));
      expect(plan.blocks.single.table!.rows.single.header, isTrue);
      expect(plan.blocks.single.table!.rows.single.cells, hasLength(2));
      expect(
        plan.blocks.single.table!.rows.single.cells.first.sourceRange,
        const FlarkSourceRange(2, 6),
      );
    });

    test('exposes list, task-list, and code-fence descriptors', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 43,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const FlarkSourceRange(0, 10),
            attributes: const {'listKind': 'unordered'},
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const FlarkSourceRange(11, 21),
            attributes: const {'listKind': 'ordered'},
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const FlarkSourceRange(22, 32),
            attributes: const {'checked': true, 'listKind': 'unordered'},
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.codeBlock,
            type: 'codeBlock',
            sourceRange: const FlarkSourceRange(33, 43),
            attributes: const {'language': 'dart'},
          ),
        ],
        inlineTokens: const [],
      );

      final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

      expect(plan.blocks[0].listItem!.kind, FlarkRenderListKind.unordered);
      expect(plan.blocks[1].listItem!.kind, FlarkRenderListKind.ordered);
      expect(plan.blocks[2].listItem!.kind, FlarkRenderListKind.unordered);
      expect(plan.blocks[2].taskListItem!.checked, isTrue);
      expect(plan.blocks.last.codeBlock!.language, 'dart');
    });

    test('exposes link and image action descriptors on inline runs', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 35,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: const FlarkSourceRange(0, 35),
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const FlarkSourceRange(0, 15),
            attributes: const {
              'destination': 'https://example.com',
              'title': 'Example',
              'label': 'link',
            },
          ),
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const FlarkSourceRange(16, 35),
            attributes: const {'src': 'asset://image.png', 'alt': 'Alt text'},
          ),
        ],
      );

      final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

      final link = plan.blocks.single.inlineRuns.first.action!;
      expect(link.kind, FlarkRenderInlineActionKind.link);
      expect(link.destination, 'https://example.com');
      expect(link.title, 'Example');
      expect(link.label, 'link');

      final image = plan.blocks.single.inlineRuns.last.action!;
      expect(image.kind, FlarkRenderInlineActionKind.image);
      expect(image.destination, 'asset://image.png');
      expect(image.label, 'Alt text');
    });

    test(
      'predicts stable semantic block descriptors through content edits',
      () {
        final cases = [
          _PredictionCase(
            id: 'heading',
            block: _block(
              kind: FlarkMarkdownBlockKind.heading,
              type: 'heading',
              styleToken: FlarkRenderTextStyleToken.heading2,
              sourceEnd: 8,
              displayEnd: 6,
              attributes: const {'level': 2},
            ),
            projection: FlarkProjection(
              textLength: 8,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 2),
                  kind: FlarkHiddenRangeKind.markdownMarker,
                ),
              ],
            ),
            verify: (block) {
              expect(block.kind, FlarkMarkdownBlockKind.heading);
              expect(block.styleToken, FlarkRenderTextStyleToken.heading2);
              expect(block.attributes['level'], 2);
            },
          ),
          _PredictionCase(
            id: 'blockquote',
            block: _block(
              kind: FlarkMarkdownBlockKind.blockquote,
              type: 'blockquote',
              sourceEnd: 9,
              displayEnd: 7,
            ),
            projection: FlarkProjection(
              textLength: 9,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 2),
                  kind: FlarkHiddenRangeKind.blockMarker,
                ),
              ],
            ),
            verify: (block) {
              expect(block.kind, FlarkMarkdownBlockKind.blockquote);
            },
          ),
          _PredictionCase(
            id: 'unordered list item',
            block: _block(
              kind: FlarkMarkdownBlockKind.listItem,
              type: 'listItem',
              sourceEnd: 6,
              displayEnd: 4,
              listItem: const FlarkRenderListItemDescriptor(
                kind: FlarkRenderListKind.unordered,
              ),
            ),
            projection: FlarkProjection(
              textLength: 6,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 2),
                  kind: FlarkHiddenRangeKind.markdownMarker,
                ),
              ],
            ),
            verify: (block) {
              expect(block.listItem, isNotNull);
              expect(block.listItem!.kind, FlarkRenderListKind.unordered);
            },
          ),
          _PredictionCase(
            id: 'ordered list item',
            block: _block(
              kind: FlarkMarkdownBlockKind.listItem,
              type: 'listItem',
              sourceEnd: 8,
              displayEnd: 5,
              listItem: const FlarkRenderListItemDescriptor(
                kind: FlarkRenderListKind.ordered,
              ),
            ),
            projection: FlarkProjection(
              textLength: 8,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 3),
                  kind: FlarkHiddenRangeKind.markdownMarker,
                ),
              ],
            ),
            verify: (block) {
              expect(block.listItem, isNotNull);
              expect(block.listItem!.kind, FlarkRenderListKind.ordered);
            },
          ),
          _PredictionCase(
            id: 'task list item',
            block: _block(
              kind: FlarkMarkdownBlockKind.listItem,
              type: 'listItem',
              sourceEnd: 12,
              displayEnd: 6,
              listItem: const FlarkRenderListItemDescriptor(
                kind: FlarkRenderListKind.unordered,
              ),
              taskListItem: const FlarkRenderTaskListItemDescriptor(
                checked: false,
              ),
            ),
            projection: FlarkProjection(
              textLength: 12,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 6),
                  kind: FlarkHiddenRangeKind.blockMarker,
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
              kind: FlarkMarkdownBlockKind.codeBlock,
              type: 'codeBlock',
              sourceEnd: 18,
              displayEnd: 4,
              codeBlock: const FlarkRenderCodeBlockDescriptor(language: 'dart'),
            ),
            projection: FlarkProjection(
              textLength: 18,
              hiddenRanges: const [
                FlarkHiddenRange(
                  range: FlarkSourceRange(0, 8),
                  kind: FlarkHiddenRangeKind.markdownMarker,
                ),
                FlarkHiddenRange(
                  range: FlarkSourceRange(12, 18),
                  kind: FlarkHiddenRangeKind.markdownMarker,
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
              kind: FlarkMarkdownBlockKind.table,
              type: 'table',
              sourceEnd: 40,
              displayEnd: 40,
              table: FlarkRenderTableDescriptor(
                columnAlignments: const [
                  FlarkRenderTableColumnAlignment.left,
                  FlarkRenderTableColumnAlignment.right,
                ],
              ),
            ),
            projection: FlarkProjection(textLength: 40),
            insertOffset: 28,
            verify: (block) {
              expect(block.table, isNotNull);
              expect(block.table!.columnAlignments, const [
                FlarkRenderTableColumnAlignment.left,
                FlarkRenderTableColumnAlignment.right,
              ]);
            },
          ),
        ];

        for (final predictionCase in cases) {
          final transaction = FlarkTransaction.single(
            FlarkSourceOperation.insert(predictionCase.insertOffset, '!'),
          );
          final projectionPrediction = predictionCase.projection.predictAfter(
            transaction,
            textLengthAfter: predictionCase.projection.textLength + 1,
          );
          final predicted =
              FlarkRenderPlan(
                blocks: [predictionCase.block],
                metadata: const {'revision': 1},
              ).predictThroughTransaction(
                transaction: transaction,
                projection: projectionPrediction.projection,
                revision: 2,
                textLengthAfter: predictionCase.projection.textLength + 1,
              );

          expect(
            predicted.fidelity,
            FlarkRenderPlanFidelity.predicted,
            reason: predictionCase.id,
          );
          expect(predicted.metadata['revision'], 2, reason: predictionCase.id);
          expect(predicted.blocks, hasLength(1), reason: predictionCase.id);
          final block = predicted.blocks.single;
          expect(
            block.sourceRange.end,
            predictionCase.block.sourceRange.end + 1,
            reason: predictionCase.id,
          );
          expect(
            block.displayRange.end,
            predictionCase.block.displayRange.end + 1,
            reason: predictionCase.id,
          );
          predictionCase.verify(block);
        }
      },
    );

    test('keeps a block whose content is replaced wholesale', () {
      final block = _block(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceEnd: 5,
        displayEnd: 5,
      );
      final projection = FlarkProjection(textLength: 1);
      // Typing over a fully selected paragraph: replace [0,5) with 'x'.
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: const FlarkSourceRange(0, 5),
          replacementText: 'x',
        ),
      );

      final predicted = block.predictThroughTransaction(
        transaction: transaction,
        projection: projection,
        textLengthAfter: 1,
      );

      expect(predicted, isNotNull, reason: 'a replaced paragraph still exists');
      expect(predicted!.sourceRange, const FlarkSourceRange(0, 1));
    });

    test('drops a block whose content is deleted wholesale', () {
      final block = _block(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceEnd: 5,
        displayEnd: 5,
      );
      final projection = FlarkProjection(textLength: 0);
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.delete(0, 5),
      );

      final predicted = block.predictThroughTransaction(
        transaction: transaction,
        projection: projection,
        textLengthAfter: 0,
      );

      expect(predicted, isNull);
    });

    test('attributes a boundary caret to the block it starts', () {
      FlarkRenderBlock blockAt(int start, int end) {
        return FlarkRenderBlock(
          kind: FlarkMarkdownBlockKind.paragraph,
          type: 'paragraph',
          sourceRange: FlarkSourceRange(start, end),
          displayRange: FlarkSourceRange(start, end),
          styleToken: FlarkRenderTextStyleToken.body,
          inlineRuns: const [],
          children: const [],
        );
      }

      final plan = FlarkRenderPlan(blocks: [blockAt(0, 5), blockAt(5, 10)]);

      // Offset 5 is the end of the first block and the start of the second;
      // the caret visually sits at the start of the second block.
      expect(plan.blockAtDisplayOffset(5)!.displayRange.start, 5);
      expect(plan.blockAtDisplayOffset(4)!.displayRange.start, 0);
      // The final offset of the document still resolves to the last block.
      expect(plan.blockAtDisplayOffset(10)!.displayRange.start, 5);

      // An empty block at the boundary is still reachable when nothing
      // contains the offset strictly.
      final withEmpty = FlarkRenderPlan(blocks: [blockAt(0, 5), blockAt(5, 5)]);
      expect(withEmpty.blockAtDisplayOffset(5)!.displayRange.isCollapsed, true);
    });

    test('predicts inline semantic actions through content edits', () {
      final plan = FlarkRenderPlan(
        blocks: [
          _block(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceEnd: 16,
            displayEnd: 12,
            inlineRuns: [
              FlarkRenderInlineRun(
                kind: FlarkMarkdownInlineKind.link,
                type: 'link',
                sourceRange: const FlarkSourceRange(1, 10),
                displayRange: const FlarkSourceRange(0, 4),
                styleToken: FlarkRenderTextStyleToken.link,
                action: const FlarkRenderInlineActionDescriptor(
                  kind: FlarkRenderInlineActionKind.link,
                  destination: 'https://example.com',
                  label: 'link',
                ),
                attributes: const {'destination': 'https://example.com'},
              ),
            ],
          ),
        ],
      );
      final projection = FlarkProjection(
        textLength: 16,
        hiddenRanges: const [
          FlarkHiddenRange(
            range: FlarkSourceRange(0, 1),
            kind: FlarkHiddenRangeKind.inlineMarker,
          ),
          FlarkHiddenRange(
            range: FlarkSourceRange(5, 10),
            kind: FlarkHiddenRangeKind.linkDestination,
          ),
        ],
      );
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(4, '!'),
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
      expect(run.kind, FlarkMarkdownInlineKind.link);
      expect(run.styleToken, FlarkRenderTextStyleToken.link);
      expect(run.action, isNotNull);
      expect(run.action!.kind, FlarkRenderInlineActionKind.link);
      expect(run.action!.destination, 'https://example.com');
      expect(run.sourceRange, const FlarkSourceRange(1, 11));
      expect(run.displayRange, const FlarkSourceRange(0, 5));
    });

    test('queries overlay-oriented blocks and inline action runs', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 48,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const FlarkSourceRange(0, 20),
            attributes: const {
              'alignments': ['left'],
            },
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.list,
            type: 'list',
            sourceRange: const FlarkSourceRange(21, 48),
            children: [
              FlarkMarkdownBlockNode(
                kind: FlarkMarkdownBlockKind.listItem,
                type: 'listItem',
                sourceRange: const FlarkSourceRange(23, 35),
                attributes: const {'checked': false},
              ),
              FlarkMarkdownBlockNode(
                kind: FlarkMarkdownBlockKind.codeBlock,
                type: 'codeBlock',
                sourceRange: const FlarkSourceRange(36, 48),
                attributes: const {'language': 'dart'},
              ),
            ],
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const FlarkSourceRange(2, 8),
            attributes: const {'destination': 'https://example.com'},
          ),
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const FlarkSourceRange(24, 32),
            attributes: const {'src': 'asset://image.png'},
          ),
        ],
      );

      final plan = FlarkRenderPlan.fromParseResult(parseResult: parseResult);

      expect(plan.allBlocks.length, 4);
      expect(plan.tableBlocks.single.kind, FlarkMarkdownBlockKind.table);
      expect(plan.taskListItemBlocks.single.taskListItem!.checked, isFalse);
      expect(plan.codeBlocks.single.codeBlock!.language, 'dart');
      expect(plan.linkRuns.single.action!.destination, 'https://example.com');
      expect(plan.imageRuns.single.action!.destination, 'asset://image.png');
      expect(
        plan.blockAtDisplayOffset(24)!.kind,
        FlarkMarkdownBlockKind.listItem,
      );
      expect(
        plan.inlineRunAtDisplayOffset(4)!.action!.kind,
        FlarkRenderInlineActionKind.link,
      );
    });

    test('builds overlay targets from render descriptors', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 32,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.table,
            type: 'table',
            sourceRange: const FlarkSourceRange(0, 10),
            attributes: const {
              'alignments': ['left'],
            },
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.listItem,
            type: 'listItem',
            sourceRange: const FlarkSourceRange(11, 20),
            attributes: const {'checked': true},
          ),
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.codeBlock,
            type: 'codeBlock',
            sourceRange: const FlarkSourceRange(21, 32),
            attributes: const {'language': 'dart'},
          ),
        ],
        inlineTokens: [
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.link,
            type: 'link',
            sourceRange: const FlarkSourceRange(1, 5),
            attributes: const {'destination': 'https://example.com'},
          ),
          FlarkMarkdownInlineToken(
            kind: FlarkMarkdownInlineKind.image,
            type: 'image',
            sourceRange: const FlarkSourceRange(12, 18),
            attributes: const {'src': 'asset://image.png'},
          ),
        ],
      );

      final overlayPlan = FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
      ).overlayPlan();

      expect(
        overlayPlan.targets.map((target) => target.kind),
        containsAll([
          FlarkRenderOverlayKind.link,
          FlarkRenderOverlayKind.image,
          FlarkRenderOverlayKind.taskListItem,
          FlarkRenderOverlayKind.table,
          FlarkRenderOverlayKind.codeBlock,
        ]),
      );
      expect(
        overlayPlan
            .ofKind(FlarkRenderOverlayKind.link)
            .single
            .action!
            .destination,
        'https://example.com',
      );
      expect(
        overlayPlan
            .ofKind(FlarkRenderOverlayKind.taskListItem)
            .single
            .taskListItem!
            .checked,
        isTrue,
      );
      expect(
        overlayPlan.ofKind(FlarkRenderOverlayKind.table).single.table,
        isNotNull,
      );
      expect(
        overlayPlan
            .ofKind(FlarkRenderOverlayKind.codeBlock)
            .single
            .codeBlock!
            .language,
        'dart',
      );
    });
  });
}

FlarkRenderBlock _block({
  required FlarkMarkdownBlockKind kind,
  required String type,
  required int sourceEnd,
  required int displayEnd,
  FlarkRenderTextStyleToken styleToken = FlarkRenderTextStyleToken.body,
  Iterable<FlarkRenderInlineRun> inlineRuns = const [],
  Iterable<FlarkRenderBlock> children = const [],
  FlarkRenderTableDescriptor? table,
  FlarkRenderListItemDescriptor? listItem,
  FlarkRenderTaskListItemDescriptor? taskListItem,
  FlarkRenderCodeBlockDescriptor? codeBlock,
  Map<String, Object?> attributes = const {},
}) {
  return FlarkRenderBlock(
    kind: kind,
    type: type,
    sourceRange: FlarkSourceRange(0, sourceEnd),
    displayRange: FlarkSourceRange(0, displayEnd),
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
  final FlarkRenderBlock block;
  final FlarkProjection projection;
  final int insertOffset;
  final void Function(FlarkRenderBlock block) verify;
}

final class _MetadataRenderPlanExtension extends FlarkRenderPlanExtension {
  const _MetadataRenderPlanExtension(this.id);

  @override
  final String id;

  @override
  FlarkRenderPlan transformRenderPlan(FlarkRenderPlanContext context) {
    final previous = context.renderPlan.metadata['extensions'];
    return FlarkRenderPlan(
      blocks: context.renderPlan.blocks,
      metadata: {
        ...context.renderPlan.metadata,
        'extensions': [if (previous is List) ...previous, id],
      },
    );
  }
}
