import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';

void main() {
  group('Sovereign syntax sync coordinator regressions', () {
    test(
      'authoritative snapshot adoption updates tree and marker projection',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '# head\n',
          selection: TextSelection.collapsed(offset: 6),
        );

        engine.completeRevision(
          1,
          blocks: const [
            BlockSpan(
              type: BlockType.header,
              start: 0,
              end: 7,
              payload: {'level': 1},
            ),
          ],
          markerRanges: const [TextRange(start: 0, end: 2)],
        );

        await _drainAsyncQueue();
        await _drainAsyncQueue();

        expect(controller.decoration.originRevision, 1);
        expect(controller.decoration.tree.blocks, isNotEmpty);
        expect(controller.decoration.tree.blocks.first.type, BlockType.header);
        expect(
          controller.decoration.hiddenRanges,
          contains(const TextRange(start: 0, end: 2)),
        );
      },
    );

    test(
      'predictive projection keeps stale origin revision until parse completes',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '# a\n',
          selection: TextSelection.collapsed(offset: 4),
        );
        engine.completeRevision(
          1,
          blocks: const [
            BlockSpan(
              type: BlockType.header,
              start: 0,
              end: 4,
              payload: {'level': 1},
            ),
          ],
          markerRanges: const [TextRange(start: 0, end: 2)],
        );
        await _drainAsyncQueue();
        await _drainAsyncQueue();
        expect(controller.decoration.originRevision, 1);

        engine.nextPrediction = SyntaxPrediction(
          revision: 2,
          markerRanges: const [TextRange(start: 0, end: 2)],
          exclusionRanges: const [],
          ambiguityZones: const [TextRange(start: 0, end: 5)],
          cursorMask: HiddenRangeCursorValidationMask(
            textLength: 5,
            hiddenRanges: const [TextRange(start: 0, end: 2)],
          ),
        );
        controller.value = const TextEditingValue(
          text: '# ab\n',
          selection: TextSelection.collapsed(offset: 5),
        );

        // Parse for revision 2 is in flight; decoration should use projected ranges
        // but preserve the authoritative tree revision marker.
        expect(controller.decoration.originRevision, 1);
        expect(controller.decoration.lineIndex.lineCount, 2);
        expect(controller.decoration.hiddenRanges, isNotEmpty);

        engine.completeRevision(
          2,
          blocks: const [
            BlockSpan(
              type: BlockType.header,
              start: 0,
              end: 5,
              payload: {'level': 1},
            ),
          ],
          markerRanges: const [TextRange(start: 0, end: 2)],
        );
        await _drainAsyncQueue();
        await _drainAsyncQueue();

        expect(controller.decoration.originRevision, 2);
      },
    );

    test('composition state suppresses parse scheduling', () async {
      final engine = _ControlledSyntaxEngine();
      final controller = SovereignController(syntaxEngine: engine);
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );

      await _drainAsyncQueue();
      expect(engine.startedRevisions, isEmpty);

      controller.value = const TextEditingValue(
        text: 'ab',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange.empty,
      );

      await _drainAsyncQueue();
      expect(engine.startedRevisions, [2]);
    });

    test(
      'predictive op reconciliation recomputes fresh inline markers before parse settles',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        engine.nextPrediction = const SyntaxPrediction(
          revision: 1,
          markerRanges: <TextRange>[],
          exclusionRanges: <TextRange>[],
          ambiguityZones: <TextRange>[TextRange(start: 0, end: 4)],
          cursorMask: PassthroughCursorValidationMask(textLength: 4),
        );
        controller.value = const TextEditingValue(
          text: '****',
          selection: TextSelection.collapsed(offset: 2),
        );

        expect(
          _containsEitherSplitOrMerged(
            controller.decoration.hiddenRanges,
            split: const <TextRange>[
              TextRange(start: 0, end: 2),
              TextRange(start: 2, end: 4),
            ],
            merged: const TextRange(start: 0, end: 4),
          ),
          isTrue,
          reason:
              'Inline wrapper markers should be recomputed immediately in the '
              'op+override predictive path instead of waiting for parse.',
        );
        expect(
          engine.hasPendingParseFor(1),
          isTrue,
          reason:
              'Assertion must hold while authoritative parse is still in flight.',
        );

        engine.nextPrediction = const SyntaxPrediction(
          revision: 2,
          markerRanges: <TextRange>[],
          exclusionRanges: <TextRange>[],
          ambiguityZones: <TextRange>[TextRange(start: 0, end: 5)],
          cursorMask: PassthroughCursorValidationMask(textLength: 5),
        );
        controller.value = const TextEditingValue(
          text: '**x**',
          selection: TextSelection.collapsed(offset: 3),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(const <TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 3, end: 5),
          ]),
          reason: 'Trailing inline marker should stay hidden after first typed '
              'character inside inserted wrapper.',
        );
      },
    );
  });
}

bool _containsEitherSplitOrMerged(
  List<TextRange> hiddenRanges, {
  required List<TextRange> split,
  required TextRange merged,
}) {
  final hasSplit = split.every(hiddenRanges.contains);
  return hasSplit || hiddenRanges.contains(merged);
}

class _ControlledSyntaxEngine implements SyntaxEngine {
  final List<SyntaxParseRequest> _started = <SyntaxParseRequest>[];
  final List<SyntaxPredictRequest> _predicted = <SyntaxPredictRequest>[];
  final Map<int, Completer<SyntaxSnapshot>> _completers =
      <int, Completer<SyntaxSnapshot>>{};
  final Map<int, int> _textLengthByRevision = <int, int>{};
  SyntaxPrediction? nextPrediction;

  List<int> get startedRevisions =>
      _started.map((request) => request.revision).toList(growable: false);

  bool hasPendingParseFor(int revision) {
    final completer = _completers[revision];
    return completer != null && !completer.isCompleted;
  }

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    _started.add(request);
    _textLengthByRevision[request.revision] = request.text.length;
    return (_completers[request.revision] ??= Completer<SyntaxSnapshot>())
        .future;
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    _predicted.add(request);
    if (nextPrediction != null) {
      final prediction = nextPrediction!;
      nextPrediction = null;
      return prediction;
    }
    return SyntaxPrediction.empty(
      revision: request.revision,
      textLength: request.text.length,
    );
  }

  void completeRevision(
    int revision, {
    int? snapshotRevision,
    List<BlockSpan> blocks = const [],
    List<InlineSpanToken> inlineTokens = const [],
    List<TextRange> markerRanges = const [],
    List<TextRange> exclusionRanges = const [],
    List<TextRange> ambiguityZones = const [],
  }) {
    final completer = _completers[revision];
    if (completer == null || completer.isCompleted) return;
    final textLength = _textLengthByRevision[revision] ?? 0;
    completer.complete(
      SyntaxSnapshot(
        revision: snapshotRevision ?? revision,
        blocks: blocks,
        inlineTokens: inlineTokens,
        markerRanges: markerRanges,
        exclusionRanges: exclusionRanges,
        ambiguityZones: ambiguityZones,
        cursorMask: HiddenRangeCursorValidationMask(
          textLength: textLength,
          hiddenRanges: markerRanges,
        ),
        diagnostics: const [],
      ),
    );
  }
}

Future<void> _drainAsyncQueue() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}
