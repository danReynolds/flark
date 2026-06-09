import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flark_live_block_reconciler.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkLiveBlockReconciler', () {
    test('keeps ids stable when an early edit shifts later block offsets', () {
      final reconciler = FlarkLiveBlockReconciler();
      final before = _doc([
        ('paragraph', 'alpha'),
        ('paragraph', 'bravo'),
        ('paragraph', 'charlie'),
      ]);
      final idsBefore = reconciler.assignIds(before.blocks, before.text);

      // Edit "alpha" -> "alphaX": block 0's content changes; blocks 1 and 2 are
      // unchanged but their absolute offsets shift by +1.
      final after = _doc([
        ('paragraph', 'alphaX'),
        ('paragraph', 'bravo'),
        ('paragraph', 'charlie'),
      ]);
      final idsAfter = reconciler.assignIds(after.blocks, after.text);

      // Edited block keeps its id (pass 2: same type, changed content).
      expect(idsAfter[0], idsBefore[0]);
      // Unchanged-but-shifted blocks keep their ids (pass 1: content match).
      expect(idsAfter[1], idsBefore[1]);
      expect(idsAfter[2], idsBefore[2]);
    });

    test('edited middle block keeps its id', () {
      final reconciler = FlarkLiveBlockReconciler();
      final before = _doc([
        ('paragraph', 'one'),
        ('paragraph', 'two'),
        ('paragraph', 'three'),
      ]);
      final idsBefore = reconciler.assignIds(before.blocks, before.text);

      final after = _doc([
        ('paragraph', 'one'),
        ('paragraph', 'twoX'),
        ('paragraph', 'three'),
      ]);
      final idsAfter = reconciler.assignIds(after.blocks, after.text);

      expect(idsAfter, idsBefore);
    });

    test('is idempotent for an unchanged document', () {
      final reconciler = FlarkLiveBlockReconciler();
      final doc = _doc([
        ('paragraph', 'a'),
        ('heading', 'b'),
        ('paragraph', 'c'),
      ]);
      final first = reconciler.assignIds(doc.blocks, doc.text);
      final second = reconciler.assignIds(doc.blocks, doc.text);
      expect(second, first);
    });

    test('inserted block gets a fresh id; existing blocks keep theirs', () {
      final reconciler = FlarkLiveBlockReconciler();
      final before = _doc([('paragraph', 'a'), ('paragraph', 'b')]);
      final idsBefore = reconciler.assignIds(before.blocks, before.text);

      final after = _doc([
        ('paragraph', 'a'),
        ('paragraph', 'inserted'),
        ('paragraph', 'b'),
      ]);
      final idsAfter = reconciler.assignIds(after.blocks, after.text);

      expect(idsAfter[0], idsBefore[0]);
      expect(idsAfter[2], idsBefore[1]);
      expect(idsAfter[1], isNot(anyOf(idsBefore[0], idsBefore[1])));
    });

    test('deleted block drops out; survivors keep ids', () {
      final reconciler = FlarkLiveBlockReconciler();
      final before = _doc([
        ('paragraph', 'a'),
        ('paragraph', 'b'),
        ('paragraph', 'c'),
      ]);
      final idsBefore = reconciler.assignIds(before.blocks, before.text);

      final after = _doc([('paragraph', 'a'), ('paragraph', 'c')]);
      final idsAfter = reconciler.assignIds(after.blocks, after.text);

      expect(idsAfter[0], idsBefore[0]);
      expect(idsAfter[1], idsBefore[2]);
    });

    test('synthetic stableId blocks bypass reconciliation', () {
      final reconciler = FlarkLiveBlockReconciler();
      final blocks = [
        _block('paragraph', 0, 5),
        _block('paragraph', 6, 6, stableId: 'terminalAppendHost:6'),
      ];
      final ids = reconciler.assignIds(blocks, 'alpha\n');
      expect(ids[1], 'live-block:terminalAppendHost:6');
    });

    test('same-text blocks with different descriptor state keep distinct '
        'identities across deletes', () {
      // Two task items with identical visible text differ only in checked
      // state. After deleting the unchecked one, the survivor must keep
      // *its own* id — a text-only key would hand it the deleted block's id.
      final reconciler = FlarkLiveBlockReconciler();
      const text = 'todo\ntodo\n';
      final unchecked = _taskBlock(0, 4, checked: false);
      final checked = _taskBlock(5, 9, checked: true);

      final idsBefore = reconciler.assignIds([unchecked, checked], text);
      expect(idsBefore[0], isNot(idsBefore[1]));

      final survivor = _taskBlock(0, 4, checked: true);
      final idsAfter = reconciler.assignIds([survivor], 'todo\n');
      expect(idsAfter.single, idsBefore[1]);
    });

    test('same-body code fences with different languages keep distinct '
        'identities', () {
      final reconciler = FlarkLiveBlockReconciler();
      const text = 'foo\nfoo\n';
      final dart = _codeBlock(0, 3, language: 'dart');
      final rust = _codeBlock(4, 7, language: 'rust');

      final idsBefore = reconciler.assignIds([dart, rust], text);
      expect(idsBefore[0], isNot(idsBefore[1]));

      final survivor = _codeBlock(0, 3, language: 'rust');
      final idsAfter = reconciler.assignIds([survivor], 'foo\n');
      expect(idsAfter.single, idsBefore[1]);
    });
  });
}

FlarkRenderBlock _taskBlock(int start, int end, {required bool checked}) {
  return FlarkRenderBlock(
    kind: FlarkMarkdownBlockKind.listItem,
    type: 'listItem',
    sourceRange: FlarkSourceRange(start, end),
    displayRange: FlarkSourceRange(start, end),
    styleToken: FlarkRenderTextStyleToken.body,
    inlineRuns: const [],
    children: const [],
    listItem: const FlarkRenderListItemDescriptor(
      kind: FlarkRenderListKind.unordered,
    ),
    taskListItem: FlarkRenderTaskListItemDescriptor(checked: checked),
  );
}

FlarkRenderBlock _codeBlock(int start, int end, {required String language}) {
  return FlarkRenderBlock(
    kind: FlarkMarkdownBlockKind.codeBlock,
    type: 'codeBlock',
    sourceRange: FlarkSourceRange(start, end),
    displayRange: FlarkSourceRange(start, end),
    styleToken: FlarkRenderTextStyleToken.body,
    inlineRuns: const [],
    children: const [],
    codeBlock: FlarkRenderCodeBlockDescriptor(language: language),
  );
}

({String text, List<FlarkRenderBlock> blocks}) _doc(
  List<(String type, String text)> lines,
) {
  final buffer = StringBuffer();
  final blocks = <FlarkRenderBlock>[];
  for (final (type, text) in lines) {
    final start = buffer.length;
    buffer.write(text);
    final end = buffer.length;
    buffer.write('\n');
    blocks.add(_block(type, start, end));
  }
  return (text: buffer.toString(), blocks: blocks);
}

FlarkRenderBlock _block(String type, int start, int end, {String? stableId}) {
  return FlarkRenderBlock(
    kind: FlarkMarkdownBlockKind.paragraph,
    type: type,
    sourceRange: FlarkSourceRange(start, end),
    displayRange: FlarkSourceRange(start, end),
    styleToken: FlarkRenderTextStyleToken.body,
    inlineRuns: const [],
    children: const [],
    attributes: stableId == null ? const {} : {'stableId': stableId},
  );
}
