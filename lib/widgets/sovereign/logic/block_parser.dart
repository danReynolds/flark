import '../models/block_node.dart';
import '../models/block_tree.dart';
import 'fenced_code_scanner.dart';

class BlockParser {
  /// Synchronous scan of the document.
  ///
  /// Implements V1 "Column 0" rules:
  /// - Fenced Code: Starts with ``` at col 0. Ends with ``` at col 0.
  /// - Header: Starts with #+ at col 0.
  /// - Blockquote: Contiguous lines starting with > at col 0.
  /// - List: Contiguous lines starting with `-`, `*`, or `1.` at col 0.
  static BlockTree parse(String text) {
    if (text.isEmpty) return BlockTree.empty();

    final blocks = <BlockNode>[];
    final length = text.length;
    int offset = 0;

    while (offset < length) {
      // 1. Fenced Code (Highest Precedence)
      if (FencedCodeScanner.startsFence(text, offset)) {
        final blockEnd = FencedCodeScanner.blockEnd(text, offset);
        blocks.add(
          BlockNode(type: BlockType.fencedCode, start: offset, end: blockEnd),
        );
        offset = blockEnd;
        continue;
      }

      // 2. Header
      // Check for #{1,6} + space
      final headerLevel = _matchHeader(text, offset);
      if (headerLevel > 0) {
        int lineEnd = _endOfLine(text, offset);
        blocks.add(
          BlockNode(
            type: BlockType.header,
            start: offset,
            end: lineEnd,
            payload: {'level': headerLevel},
          ),
        );
        offset = lineEnd;
        continue;
      }

      // 3. Blockquote (Contiguous)
      if (_startsWith(text, offset, '> ')) {
        int blockEnd = _endOfLine(text, offset);

        // Peek next lines
        while (blockEnd < length && _startsWith(text, blockEnd, '> ')) {
          blockEnd = _endOfLine(text, blockEnd);
        }

        blocks.add(
          BlockNode(type: BlockType.blockquote, start: offset, end: blockEnd),
        );
        offset = blockEnd;
        continue;
      }

      // 4. Unordered List (Contiguous)
      // - space or * space
      if (_startsWith(text, offset, '- ') || _startsWith(text, offset, '* ')) {
        int blockEnd = _endOfLine(text, offset);

        // Peek next lines
        while (blockEnd < length &&
            (_startsWith(text, blockEnd, '- ') ||
                _startsWith(text, blockEnd, '* '))) {
          blockEnd = _endOfLine(text, blockEnd);
        }

        blocks.add(
          BlockNode(
            type: BlockType.unorderedList,
            start: offset,
            end: blockEnd,
          ),
        );
        offset = blockEnd;
        continue;
      }

      // 5. Ordered List (Contiguous)
      // 1. space (simplified for V1, just 1.)
      // Regex check is expensive, maybe just check digit + dot + space?
      if (_matchOrderedList(text, offset)) {
        int blockEnd = _endOfLine(text, offset);

        while (blockEnd < length && _matchOrderedList(text, blockEnd)) {
          blockEnd = _endOfLine(text, blockEnd);
        }

        blocks.add(
          BlockNode(type: BlockType.orderedList, start: offset, end: blockEnd),
        );
        offset = blockEnd;
        continue;
      }

      // No match - advance to next line (implicitly paragraph)
      offset = _endOfLine(text, offset);
    }

    return BlockTree(blocks);
  }

  static int _endOfLine(String text, int start) {
    final idx = text.indexOf('\n', start);
    return idx == -1 ? text.length : idx + 1;
  }

  static bool _startsWith(String text, int offset, String pattern) {
    return text.startsWith(pattern, offset);
  }

  static int _matchHeader(String text, int offset) {
    int i = 0;
    while (offset + i < text.length && text[offset + i] == '#' && i < 6) {
      i++;
    }
    if (i > 0 && offset + i < text.length && text[offset + i] == ' ') {
      return i;
    }
    return 0;
  }

  static bool _matchOrderedList(String text, int offset) {
    // V1: Simple check for "1. " or d+.
    // Manual scan to avoid regex.
    int i = 0;
    while (offset + i < text.length && _isDigit(text.codeUnitAt(offset + i))) {
      i++;
    }
    if (i > 0 &&
        offset + i + 1 < text.length &&
        text[offset + i] == '.' &&
        text[offset + i + 1] == ' ') {
      return true;
    }
    return false;
  }

  static bool _isDigit(int charCode) {
    return charCode >= 48 && charCode <= 57;
  }
}
