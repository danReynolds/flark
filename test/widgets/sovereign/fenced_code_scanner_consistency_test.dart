import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/block_parser.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_geometry_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  test('FencedCodeScanner matches BlockParser and GeometryScanner ranges', () {
    const text = 'a\n```\ncode\n```\nmore\n```\nunclosed';

    final scanned = FencedCodeScanner.scan(text);

    final tree = BlockParser.parse(text);
    final parsedFences = tree.blocks
        .where((b) => b.type == BlockType.fencedCode)
        .toList(growable: false);

    expect(parsedFences.length, scanned.length);
    for (var i = 0; i < scanned.length; i++) {
      expect(parsedFences[i].start, scanned[i].start);
      expect(parsedFences[i].end, scanned[i].end);
    }

    final lineIndex = LineIndex.fromText(text);
    final geometry = const SovereignGeometryScanner().scan(text, lineIndex);

    expect(geometry.codeBlocks.length, scanned.length);
    for (var i = 0; i < scanned.length; i++) {
      expect(geometry.codeBlocks[i].startOffset, scanned[i].start);
      expect(geometry.codeBlocks[i].endOffset, scanned[i].end);
    }
  });

  test('FencedCodeScanner recognizes immediate closing fence line', () {
    const text = '```\n```\nnext';
    final scanned = FencedCodeScanner.scan(text);

    expect(scanned.length, 1);
    expect(scanned.first.start, 0);
    expect(scanned.first.end, 8);

    final tree = BlockParser.parse(text);
    final parsedFences = tree.blocks
        .where((b) => b.type == BlockType.fencedCode)
        .toList(growable: false);
    expect(parsedFences.length, 1);
    expect(parsedFences.first.start, 0);
    expect(parsedFences.first.end, 8);
  });
}
