import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/block_parser.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';

void main() {
  group('BlockParser Level 1 Tests (Correctness + Invariants)', () {
    // -------------------------------------------------------------------------
    // INVARIANT HELPERS
    // -------------------------------------------------------------------------
    void assertInvariants(String text, List<BlockNode> blocks) {
      if (blocks.isEmpty) return;

      int lastEnd = -1;
      for (int i = 0; i < blocks.length; i++) {
        final block = blocks[i];

        // 1. Valid Offsets
        expect(block.start, greaterThanOrEqualTo(0), reason: 'Start < 0');
        expect(block.end, lessThanOrEqualTo(text.length), reason: 'End > Len');
        expect(
          block.start,
          lessThan(block.end),
          reason: 'Start >= End (Empty/Neg)',
        );

        // 2. Sorted & Non-Overlapping
        if (lastEnd != -1) {
          expect(
            block.start,
            greaterThanOrEqualTo(lastEnd),
            reason: 'Overlapping or unsorted block at index $i',
          );
        }
        lastEnd = block.end;
      }
    }

    // -------------------------------------------------------------------------
    // STANDARD BLOCKS
    // -------------------------------------------------------------------------

    test('Header (Levels 1-6)', () {
      final corpus = '''
# H1
## H2
### H3
#### H4
##### H5
###### H6
####### Not H7
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 6);
      expect(tree.blocks[0].type, BlockType.header);
      expect(tree.blocks[0].payload?['level'], 1);
      expect(tree.blocks[5].type, BlockType.header);
      expect(tree.blocks[5].payload?['level'], 6);
    });

    test('Fenced Code (Backticks)', () {
      final corpus = '''
```
code
```
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 1);
      final block = tree.blocks.first;
      expect(block.type, BlockType.fencedCode);
      // Content check: includes fences
      expect(corpus.substring(block.start, block.end), contains('code'));
    });

    test('Blockquote (Contiguous)', () {
      final corpus = '''
> Line 1
> Line 2

> Line 3
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 2);
      expect(tree.blocks[0].type, BlockType.blockquote);
      // Should include Line 1 and Line 2
      final qs1 = corpus.substring(tree.blocks[0].start, tree.blocks[0].end);
      expect(qs1, contains('Line 1'));
      expect(qs1, contains('Line 2'));

      expect(tree.blocks[1].type, BlockType.blockquote);
      expect(
        corpus.substring(tree.blocks[1].start, tree.blocks[1].end),
        contains('Line 3'),
      );
    });

    test('Lists (Unordered & Ordered)', () {
      final corpus = '''
- Item 1

* Item 2

1. Item 3
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 3);
      expect(tree.blocks[0].type, BlockType.unorderedList);
      expect(tree.blocks[1].type, BlockType.unorderedList);
      expect(tree.blocks[2].type, BlockType.orderedList);
    });

    test('Unordered List Merging', () {
      final corpus = '''
- Item 1
* Item 2
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 1);
      expect(tree.blocks[0].type, BlockType.unorderedList);
      final content = corpus.substring(
        tree.blocks[0].start,
        tree.blocks[0].end,
      );
      expect(content, contains('Item 1'));
      expect(content, contains('Item 2'));
    });

    // -------------------------------------------------------------------------
    // EDGE CASES (ROBUSTNESS)
    // -------------------------------------------------------------------------

    test('Unclosed Fenced Code (Extends to EOF)', () {
      final corpus = '''
Header
```
Unclosed code block...
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 1);
      expect(tree.blocks.first.type, BlockType.fencedCode);
      expect(tree.blocks.first.end, corpus.length);
    });

    test('Nested Markers (Ignored in V1)', () {
      // V1 handles top-level blocks. Nested stuff inside should purely be content.
      // E.g. a header inside a blockquote is just text if Column 0 rule applies strict.
      // If "> # Header" -> The "> " is matched first.
      final corpus = '> # Header inside quote';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 1);
      expect(tree.blocks.first.type, BlockType.blockquote);
    });

    test('Mixed Content (Gap Handling)', () {
      final corpus = '''
# Header

Paragraph text (ignored)

- List Item
''';
      final tree = BlockParser.parse(corpus);
      assertInvariants(corpus, tree.blocks);

      expect(tree.blocks.length, 2);
      expect(tree.blocks[0].type, BlockType.header);
      expect(tree.blocks[1].type, BlockType.unorderedList);
      // Paragraph is implicitly the gap between end of header and start of list
    });

    test('Empty Input', () {
      final tree = BlockParser.parse('');
      expect(tree.blocks, isEmpty);
    });

    test('Whitespace Only', () {
      final corpus = '   \n  ';
      final tree = BlockParser.parse(corpus);
      // Should find nothing as per Column 0 rules (indentation breaks match)
      expect(tree.blocks, isEmpty);
    });
  });
}
