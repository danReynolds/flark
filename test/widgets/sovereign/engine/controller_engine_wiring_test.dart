import 'dart:math';
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';

void main() {
  group('SovereignController engine wiring', () {
    test('single-flight parse keeps only latest pending revision', () async {
      final engine = _ControlledSyntaxEngine();
      final controller = SovereignController(syntaxEngine: engine);
      addTearDown(controller.dispose);
      controller.resetParseTelemetryForTesting();

      controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
      );
      controller.value = const TextEditingValue(
        text: 'ab',
        selection: TextSelection.collapsed(offset: 2),
      );
      controller.value = const TextEditingValue(
        text: 'abc',
        selection: TextSelection.collapsed(offset: 3),
      );

      await _eventually(() => engine.startedRevisions.length == 1);
      expect(engine.startedRevisions, [1]);
      expect(controller.parsePendingReplaceCount, 1);

      engine.completeRevision(1);
      await _eventually(
        () =>
            controller.parseStaleDropCount >= 1 &&
            engine.startedRevisions.length >= 2,
      );

      expect(controller.parseStaleDropCount, 1);
      expect(engine.startedRevisions, [1, 3]);

      engine.completeRevision(3);
      await _drainAsyncQueue();

      expect(controller.parseStaleDropCount, 1);
    });

    test('mismatched snapshot revision is dropped as stale', () async {
      final engine = _ControlledSyntaxEngine();
      final controller = SovereignController(syntaxEngine: engine);
      addTearDown(controller.dispose);
      controller.resetParseTelemetryForTesting();

      controller.value = const TextEditingValue(
        text: 'hello',
        selection: TextSelection.collapsed(offset: 5),
      );

      expect(engine.startedRevisions, [1]);
      engine.completeRevision(1, snapshotRevision: 0);
      await _eventually(() => controller.parseStaleDropCount >= 1);

      expect(controller.parseStaleDropCount, 1);
    });

    test(
      'controller forwards configured markdown profile to engine requests',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(
          syntaxEngine: engine,
          markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
        );
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: 'x',
          selection: TextSelection.collapsed(offset: 1),
        );

        await _drainAsyncQueue();

        expect(engine.startedProfiles, [MarkdownSyntaxProfile.commonMarkGfm]);
        expect(
          engine.predictedProfiles,
          contains(MarkdownSyntaxProfile.commonMarkGfm),
        );
      },
    );

    test('predict requests carry the latest authoritative snapshot', () async {
      final engine = _ControlledSyntaxEngine();
      final controller = SovereignController(syntaxEngine: engine);
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: 'hello',
        selection: TextSelection.collapsed(offset: 5),
      );
      engine.completeRevision(1);
      await _eventually(() => controller.decoration.originRevision == 1);

      controller.value = const TextEditingValue(
        text: 'hello!',
        selection: TextSelection.collapsed(offset: 6),
      );

      final withSnapshot = engine.predictedRequests.where(
        (request) => request.previousSnapshot != null,
      );
      expect(withSnapshot, isNotEmpty);
      expect(withSnapshot.last.previousSnapshot!.revision, 1);
    });

    test(
      'snapshot-gap predictive mask snaps selection out of marker interior',
      () {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        engine.nextPrediction = SyntaxPrediction(
          revision: 1,
          markerRanges: const [TextRange(start: 1, end: 3)],
          exclusionRanges: const [],
          ambiguityZones: const [],
          cursorMask: HiddenRangeCursorValidationMask(
            textLength: 4,
            hiddenRanges: const [TextRange(start: 1, end: 3)],
          ),
        );

        controller.value = const TextEditingValue(
          text: 'a**b',
          selection: TextSelection.collapsed(offset: 2),
        );

        expect(controller.selection.baseOffset, anyOf(1, 3));
      },
    );

    test(
      'predictive ambiguity preserves authoritative marker classification',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        const baseText = '**x**';
        controller.value = const TextEditingValue(
          text: baseText,
          selection: TextSelection.collapsed(offset: baseText.length),
        );
        engine.completeRevision(
          1,
          markerRanges: const [
            TextRange(start: 0, end: 2),
            TextRange(start: 3, end: 5),
          ],
        );
        await _drainAsyncQueue();

        engine.nextPrediction = const SyntaxPrediction(
          revision: 2,
          markerRanges: [],
          exclusionRanges: [],
          ambiguityZones: [TextRange(start: 0, end: 6)],
          cursorMask: PassthroughCursorValidationMask(textLength: 6),
        );

        const nextText = '**x**!';
        controller.value = const TextEditingValue(
          text: nextText,
          selection: TextSelection.collapsed(offset: nextText.length),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(const <TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 3, end: 5),
          ]),
        );
      },
    );

    test(
      'predictive edit rescans inline markers immediately before parse completion',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: 'hello',
          selection: TextSelection.collapsed(offset: 5),
        );
        engine.completeRevision(1);
        await _eventually(() => controller.decoration.originRevision == 1);

        // Force the predictive path to rely on local inline fallback scanning.
        engine.nextPrediction = const SyntaxPrediction(
          revision: 2,
          markerRanges: [],
          exclusionRanges: [],
          ambiguityZones: [],
          cursorMask: PassthroughCursorValidationMask(textLength: 11),
        );

        controller.value = const TextEditingValue(
          text: 'hello **x**',
          selection: TextSelection.collapsed(offset: 11),
        );

        // Parse revision 2 is in-flight; marker hiding must already be applied.
        expect(engine.startedRevisions, contains(2));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(const <TextRange>[
            TextRange(start: 6, end: 8),
            TextRange(start: 9, end: 11),
          ]),
        );
      },
    );

    test(
      'delayed parse reconciliation keeps caret snapped outside hidden markers under rapid edits',
      () async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        final random = Random(17);
        final completed = <int>{};

        for (var i = 0; i < 40; i++) {
          final text = _randomMarkerHeavyText(random);
          final nextRevision = controller.state.revision + 1;
          final preview = _previewPredictionForText(
            revision: nextRevision,
            text: text,
          );
          engine.nextPrediction = preview;

          final requestedOffset = _requestedOffsetInsideFirstMarker(
            preview.markerRanges,
            text.length,
          );
          controller.value = TextEditingValue(
            text: text,
            selection: TextSelection.collapsed(offset: requestedOffset),
          );

          _expectSelectionOutsideMarkerInteriors(
            controller.selection.baseOffset,
            controller.decoration.hiddenRanges,
          );

          // Periodically resolve in-flight parses to force stale-drop +
          // reconciliation transitions while continuing to edit.
          if (i % 4 == 3) {
            final outstandingBefore = engine.startedRevisions.where(
              (rev) => !completed.contains(rev),
            );
            if (outstandingBefore.isNotEmpty) {
              final staleRevision = outstandingBefore.first;
              completed.add(staleRevision);
              engine.completeRevision(staleRevision);
              await _drainAsyncQueue();

              _expectSelectionOutsideMarkerInteriors(
                controller.selection.baseOffset,
                controller.decoration.hiddenRanges,
              );

              final outstandingAfter = engine.startedRevisions.where(
                (rev) => !completed.contains(rev),
              );
              if (outstandingAfter.isNotEmpty) {
                final latestRevision = outstandingAfter.last;
                completed.add(latestRevision);
                engine.completeRevision(latestRevision);
                await _drainAsyncQueue();

                _expectSelectionOutsideMarkerInteriors(
                  controller.selection.baseOffset,
                  controller.decoration.hiddenRanges,
                );
              }
            }
          }
        }
      },
    );

    test('controller parses via commonMark backend by default', () async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '  # heading\n  > quote\n',
        selection: TextSelection.collapsed(offset: 20),
      );

      await _eventually(() {
        final blockTypes = controller.decoration.tree.blocks
            .map((block) => block.type)
            .toList(growable: false);
        return blockTypes.contains(BlockType.header) &&
            blockTypes.contains(BlockType.blockquote);
      }, turns: 60);

      final blocks = controller.decoration.tree.blocks;
      final blockTypes =
          blocks.map((block) => block.type).toList(growable: false);
      expect(blockTypes, contains(BlockType.header));
      expect(blockTypes, contains(BlockType.blockquote));
    });

    testWidgets(
      'buildTextSpan uses authoritative inline tokens when available',
      (WidgetTester tester) async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: 'hello',
          selection: TextSelection.collapsed(offset: 5),
        );
        engine.completeRevision(
          1,
          inlineTokens: const [
            InlineSpanToken(style: SovereignStyle.bold, start: 0, end: 5),
          ],
        );
        for (var i = 0;
            i < 20 && controller.decoration.originRevision != 1;
            i++) {
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 1));
        }
        expect(controller.decoration.originRevision, 1);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));

        final span = controller.buildTextSpan(
          context: context,
          style: const TextStyle(fontSize: 12, color: Colors.black),
          withComposing: false,
        );

        final helloSpan = _leafSpans(
          span,
        ).firstWhere((leaf) => (leaf.text ?? '').contains('hello'));
        expect(helloSpan.style?.fontWeight, FontWeight.bold);
      },
    );

    testWidgets(
      'buildTextSpan supplements authoritative inline runs for transient wrapper states',
      (WidgetTester tester) async {
        final engine = _ControlledSyntaxEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '**test **',
          selection: TextSelection.collapsed(offset: 7),
        );
        // Authoritative parser may temporarily return no emphasis token for
        // trailing-whitespace wrapper states.
        engine.completeRevision(1, inlineTokens: const []);
        for (var i = 0;
            i < 20 && controller.decoration.originRevision != 1;
            i++) {
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 1));
        }
        expect(controller.decoration.originRevision, 1);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));

        final span = controller.buildTextSpan(
          context: context,
          style: const TextStyle(fontSize: 12, color: Colors.black),
          withComposing: false,
        );

        final contentSpan = _leafSpans(
          span,
        ).firstWhere((leaf) => (leaf.text ?? '').contains('test'));
        expect(contentSpan.style?.fontWeight, FontWeight.bold);
      },
    );
  });
}

final V1SyntaxEngineAdapter _predictPreviewDelegate =
    const V1SyntaxEngineAdapter();

SyntaxPrediction _previewPredictionForText({
  required int revision,
  required String text,
}) {
  return _predictPreviewDelegate.predict(
    SyntaxPredictRequest(revision: revision, text: text),
  );
}

int _requestedOffsetInsideFirstMarker(List<TextRange> markers, int textLength) {
  for (final range in markers) {
    if (range.end - range.start > 1) {
      return (range.start + 1).clamp(0, textLength);
    }
  }
  if (markers.isNotEmpty) {
    final range = markers.first;
    return range.start.clamp(0, textLength);
  }
  return textLength;
}

String _randomMarkerHeavyText(Random random) {
  const tokens = <String>[
    '**x**',
    '_x_',
    '`x`',
    '```\ncode\n```',
    '# head',
    '> quote',
    '- item',
    'plain',
  ];
  final prefix = String.fromCharCodes(
    List<int>.generate(random.nextInt(3), (_) => 97 + random.nextInt(3)),
  );
  final suffix = String.fromCharCodes(
    List<int>.generate(random.nextInt(3), (_) => 109 + random.nextInt(3)),
  );
  return '$prefix${tokens[random.nextInt(tokens.length)]}$suffix';
}

void _expectSelectionOutsideMarkerInteriors(
  int selectionOffset,
  List<TextRange> markers,
) {
  for (final range in markers) {
    if (range.end <= range.start) continue;
    final inside = selectionOffset > range.start && selectionOffset < range.end;
    expect(
      inside,
      isFalse,
      reason: 'Caret landed inside hidden marker interior at $selectionOffset '
          'for range [${range.start}, ${range.end})',
    );
  }
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
  List<MarkdownSyntaxProfile> get startedProfiles =>
      _started.map((request) => request.profile).toList(growable: false);
  List<SyntaxPredictRequest> get predictedRequests =>
      List<SyntaxPredictRequest>.unmodifiable(_predicted);
  List<MarkdownSyntaxProfile> get predictedProfiles =>
      _predicted.map((request) => request.profile).toList(growable: false);

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
        blocks: const [],
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

List<TextSpan> _leafSpans(InlineSpan root) {
  final leaves = <TextSpan>[];
  void walk(InlineSpan span) {
    if (span is! TextSpan) return;
    if (span.text != null && span.text!.isNotEmpty) {
      leaves.add(span);
    }
    final children = span.children;
    if (children == null) return;
    for (final child in children) {
      walk(child);
    }
  }

  walk(root);
  return leaves;
}

Future<void> _drainAsyncQueue() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

Future<void> _eventually(bool Function() predicate, {int turns = 80}) async {
  for (var i = 0; i < turns; i++) {
    if (predicate()) return;
    await _drainAsyncQueue();
    if (i % 5 == 4) {
      await Future<void>.delayed(const Duration(milliseconds: 1));
    }
  }
  expect(predicate(), isTrue);
}
