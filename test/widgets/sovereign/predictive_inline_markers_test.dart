import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';

bool _hiddenRangesContainAll(
  List<TextRange> hiddenRanges,
  List<TextRange> expected,
) {
  return expected.every(hiddenRanges.contains);
}

Future<void> _expectHiddenRangesEventually({
  required SovereignController controller,
  required List<TextRange> expected,
  required String reason,
  Duration timeout = const Duration(milliseconds: 40),
}) async {
  final stopwatch = Stopwatch()..start();
  while (stopwatch.elapsed < timeout) {
    if (_hiddenRangesContainAll(controller.decoration.hiddenRanges, expected)) {
      return;
    }
    await Future<void>.delayed(Duration.zero);
  }
  expect(
    controller.decoration.hiddenRanges,
    containsAll(expected),
    reason: reason,
  );
}

void main() {
  group('Sovereign predictive inline markers', () {
    test('Bold markers hide immediately on edit', () {
      final controller = SovereignController(text: 'Hello ');
      addTearDown(controller.dispose);

      const text = 'Hello **Bold** World';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final open = text.indexOf('**');
      final close = text.lastIndexOf('**');
      expect(open, isNot(-1));
      expect(close, isNot(-1));
      expect(close, isNot(open));

      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: open, end: open + 2),
          TextRange(start: close, end: close + 2),
        ]),
        reason: 'Closing ** should not require async parse to hide',
      );
    });

    test('Italic markers hide immediately on edit', () {
      final controller = SovereignController(text: 'Hello ');
      addTearDown(controller.dispose);

      const text = 'Hello _Italic_ World';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final open = text.indexOf('_');
      final close = text.lastIndexOf('_');
      expect(open, isNot(-1));
      expect(close, isNot(-1));
      expect(close, isNot(open));

      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: open, end: open + 1),
          TextRange(start: close, end: close + 1),
        ]),
        reason: 'Closing _ should not require async parse to hide',
      );
    });

    test('Inline code markers hide immediately on edit', () {
      final controller = SovereignController(text: 'Hello ');
      addTearDown(controller.dispose);

      const text = 'Hello `Code` World';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final open = text.indexOf('`');
      final close = text.lastIndexOf('`');
      expect(open, isNot(-1));
      expect(close, isNot(-1));
      expect(close, isNot(open));

      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: open, end: open + 1),
          TextRange(start: close, end: close + 1),
        ]),
        reason: 'Closing ` should not require async parse to hide',
      );
    });

    test('Escaped inline delimiters stay visible during predictive edit', () {
      final engine = _StalePredictEngine();
      final controller = SovereignController(
        text: '',
        syntaxEngine: engine,
      );
      addTearDown(controller.dispose);

      const text = r'\*literal\* \_literal\_ \`code\` &amp;';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final escapedStar = text.indexOf('*');
      final escapedUnderscore = text.indexOf('_');
      final escapedBacktick = text.indexOf('`');
      expect(escapedStar, isNot(-1));
      expect(escapedUnderscore, isNot(-1));
      expect(escapedBacktick, isNot(-1));

      expect(controller.decoration.hiddenRanges, isEmpty);
      controller.selection = TextSelection.collapsed(offset: escapedStar);
      expect(controller.selection.baseOffset, escapedStar);
      controller.selection = TextSelection.collapsed(offset: escapedUnderscore);
      expect(controller.selection.baseOffset, escapedUnderscore);
      controller.selection = TextSelection.collapsed(offset: escapedBacktick);
      expect(controller.selection.baseOffset, escapedBacktick);
      expect(
        engine.hasPendingParse,
        isTrue,
        reason:
            'Escaped delimiters must remain cursor-safe before authoritative parse completes.',
      );
    });

    test('Inline markers are not hidden inside fenced code blocks', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      const text = '```\n**Bold**\n```';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final fenceOpen = text.indexOf('```');
      final fenceClose = text.lastIndexOf('```');
      expect(fenceOpen, 0);
      expect(fenceClose, isNot(0));

      final boldOpen = text.indexOf('**');
      final boldClose = text.lastIndexOf('**');
      expect(boldOpen, isNot(-1));
      expect(boldClose, isNot(-1));

      final hidden = controller.decoration.hiddenRanges;
      expect(
        hidden,
        containsAll(<TextRange>[
          TextRange(start: fenceOpen, end: fenceOpen + 3),
          TextRange(start: fenceClose, end: fenceClose + 3),
        ]),
      );
      expect(
        hidden,
        isNot(contains(TextRange(start: boldOpen, end: boldOpen + 2))),
      );
      expect(
        hidden,
        isNot(contains(TextRange(start: boldClose, end: boldClose + 2))),
      );
    });

    test(
      'Markers near the edit stay responsive when global predictive scan is truncated',
      () async {
        final engine = _StalePredictEngine(ambiguous: true);
        final prefix = List.filled(2000, 'a').join();
        final baseText = '$prefix ';
        final controller = SovereignController(
          text: baseText,
          syntaxEngine: engine,
        );
        addTearDown(controller.dispose);
        controller.setPredictiveScanOverridesForTesting(charLimit: 32);
        controller.resetPredictiveTelemetryForTesting();

        const suffix = '**Bold**';
        final text = '$baseText$suffix';
        controller.value = TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );

        final open = text.indexOf('**', prefix.length);
        final close = text.lastIndexOf('**');
        expect(open, isNot(-1));
        expect(close, isNot(-1));
        expect(close, isNot(open));

        final expectedRanges = <TextRange>[
          TextRange(start: open, end: open + 2),
          TextRange(start: close, end: close + 2),
        ];
        await _expectHiddenRangesEventually(
          controller: controller,
          expected: expectedRanges,
          reason:
              'Local predictive fallback should hide edited-line markers when'
              ' the global budgeted scan does not reach the caret.',
        );
        expect(controller.predictiveBudgetExhaustionCount, greaterThan(0));
        expect(controller.predictiveLocalFallbackCount, greaterThan(0));
      },
    );

    test(
      'Local predictive fallback still ignores inline markers inside fences',
      () {
        final prefix = List.filled(2000, 'p').join();
        final baseText = '$prefix\n```\n';
        final controller = SovereignController(text: baseText);
        addTearDown(controller.dispose);
        controller.setPredictiveScanOverridesForTesting(charLimit: 32);

        const inserted = '**inside**';
        final text = '$baseText$inserted';
        controller.value = TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );

        final fenceOpen = text.indexOf('```');
        final boldOpen = text.indexOf('**', fenceOpen);
        final boldClose = text.lastIndexOf('**');
        expect(fenceOpen, isNot(-1));
        expect(boldOpen, isNot(-1));
        expect(boldClose, isNot(-1));

        final hidden = controller.decoration.hiddenRanges;
        expect(
          hidden,
          contains(TextRange(start: fenceOpen, end: fenceOpen + 3)),
        );
        expect(
          hidden,
          isNot(contains(TextRange(start: boldOpen, end: boldOpen + 2))),
        );
        expect(
          hidden,
          isNot(contains(TextRange(start: boldClose, end: boldClose + 2))),
        );
      },
    );

    test(
      'Stale predictive markers plus ambiguity still do not hide inline markers inside fences',
      () {
        final engine = _AmbiguousFenceExclusionPredictEngine();
        final controller = SovereignController(
          text: 'prefix\n```\n',
          syntaxEngine: engine,
        );
        addTearDown(controller.dispose);
        controller.resetPredictiveTelemetryForTesting();

        const text = 'prefix\n```\n**inside**';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );

        final fenceOpen = text.indexOf('```');
        final boldOpen = text.indexOf('**', fenceOpen);
        final boldClose = text.lastIndexOf('**');
        expect(fenceOpen, isNot(-1));
        expect(boldOpen, isNot(-1));
        expect(boldClose, isNot(-1));

        final hidden = controller.decoration.hiddenRanges;
        expect(
          hidden,
          isNot(contains(TextRange(start: boldOpen, end: boldOpen + 2))),
        );
        expect(
          hidden,
          isNot(contains(TextRange(start: boldClose, end: boldClose + 2))),
        );
        expect(controller.predictiveBudgetExhaustionCount, greaterThan(0));
        expect(
          controller.predictiveLocalFallbackLastScannedChars,
          greaterThan(0),
          reason: 'Local inline fallback should still run near the edit.',
        );
        expect(
          engine.hasPendingParse,
          isTrue,
          reason:
              'This should be verified while authoritative parse is still in flight.',
        );
      },
    );

    test(
      'Truncated predictive scan does not preserve stale shifted inline markers',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const oldText = 'hello\nworld\n**bold**';
        controller.value = const TextEditingValue(
          text: oldText,
          selection: TextSelection.collapsed(offset: oldText.length),
        );

        final oldOpen = oldText.indexOf('**');
        final oldClose = oldText.lastIndexOf('**');
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: oldOpen, end: oldOpen + 2),
            TextRange(start: oldClose, end: oldClose + 2),
          ]),
        );

        controller.setPredictiveScanOverridesForTesting(charLimit: 8);

        const newText = '`hello\nworld\n**bold**';
        controller.value = const TextEditingValue(
          text: newText,
          selection: TextSelection.collapsed(offset: 1),
        );

        final open = newText.indexOf('**');
        final close = newText.lastIndexOf('**');
        expect(open, isNot(-1));
        expect(close, isNot(-1));

        final hidden = controller.decoration.hiddenRanges;
        expect(
          hidden,
          isNot(contains(TextRange(start: open, end: open + 2))),
          reason:
              'Predictive path must not keep shifted inline markers that are'
              ' stale in the new text.',
        );
        expect(
          hidden,
          isNot(contains(TextRange(start: close, end: close + 2))),
          reason:
              'Predictive path must not keep shifted inline markers that are'
              ' stale in the new text.',
        );
      },
    );

    test('Predictive telemetry stays zero when scan completes', () {
      final controller = SovereignController(text: 'hello ');
      addTearDown(controller.dispose);
      controller.clearPredictiveScanOverridesForTesting();
      controller.resetPredictiveTelemetryForTesting();

      const text = 'hello **ok**';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      expect(controller.predictiveBudgetExhaustionCount, 0);
      expect(controller.predictiveLocalFallbackCount, 0);
    });

    test(
      'Fresh inline markers hide even when predictive engine returns stale markers',
      () {
        final engine = _StalePredictEngine();
        final controller = SovereignController(
          text: 'Hello ',
          syntaxEngine: engine,
        );
        addTearDown(controller.dispose);
        final parseCountBeforeEdit = engine.parseCount;

        const text = 'Hello **Bold**';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );

        final open = text.indexOf('**');
        final close = text.lastIndexOf('**');
        expect(open, isNot(-1));
        expect(close, isNot(-1));
        expect(close, isNot(open));

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: open, end: open + 2),
            TextRange(start: close, end: close + 2),
          ]),
          reason:
              'Predictive local inline scan should hide newly typed markers even'
              ' when backend prediction is stale/empty.',
        );
        expect(
          engine.parseCount,
          parseCountBeforeEdit,
          reason: 'Single-flight scheduling keeps the initial parse in flight;'
              ' markers should still hide before any authoritative response.',
        );
        expect(
          engine.hasPendingParse,
          isTrue,
          reason:
              'Markers must hide while authoritative parse is still pending.',
        );
      },
    );

    test(
      'Fresh fence markers hide even when predictive engine returns stale markers',
      () {
        final engine = _StalePredictEngine();
        final controller = SovereignController(
          text: 'before\n',
          syntaxEngine: engine,
        );
        addTearDown(controller.dispose);
        final parseCountBeforeEdit = engine.parseCount;

        const text = 'before\n```\ncode\n```';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );

        final open = text.indexOf('```');
        final close = text.lastIndexOf('```');
        expect(open, isNot(-1));
        expect(close, isNot(-1));
        expect(close, isNot(open));

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: open, end: open + 3),
            TextRange(start: close, end: close + 3),
          ]),
          reason:
              'Predictive reconciliation should hide typed fence markers even'
              ' when backend prediction returns stale marker ranges.',
        );
        expect(
          engine.parseCount,
          parseCountBeforeEdit,
          reason:
              'The in-flight parse should keep authoritative results pending;'
              ' fences should still hide immediately in predictive mode.',
        );
        expect(
          engine.hasPendingParse,
          isTrue,
          reason:
              'Fence markers must hide while authoritative parse is in flight.',
        );
      },
    );

    test(
      'Predictive local fallback scan stays bounded on long single-line edits',
      () {
        final engine = _StalePredictEngine(ambiguous: true);
        final prefix = List.filled(50000, 'a').join();
        final baseText = '$prefix ';
        final controller = SovereignController(
          text: baseText,
          syntaxEngine: engine,
        );
        addTearDown(controller.dispose);
        controller.resetPredictiveTelemetryForTesting();

        const inserted = '**z**';
        final text = '$baseText$inserted';
        final stopwatch = Stopwatch()..start();
        controller.value = TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );
        stopwatch.stop();

        expect(
          controller.predictiveLocalFallbackLastScannedChars,
          lessThanOrEqualTo(controller.predictiveLocalInlineScanCharCap),
        );
        expect(controller.predictiveBudgetExhaustionCount, greaterThan(0));
        expect(
          stopwatch.elapsedMilliseconds,
          lessThan(300),
          reason:
              'Keystroke reconciliation should remain bounded for long lines.',
        );
      },
    );
  });
}

class _StalePredictEngine implements SyntaxEngine {
  _StalePredictEngine({this.ambiguous = false});

  final bool ambiguous;
  final _parseCompleter = Completer<SyntaxSnapshot>();
  int parseCount = 0;
  bool get hasPendingParse => !_parseCompleter.isCompleted;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    parseCount++;
    return _parseCompleter.future;
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return SyntaxPrediction(
      revision: request.revision,
      markerRanges: const [],
      exclusionRanges: const [],
      ambiguityZones: ambiguous
          ? <TextRange>[TextRange(start: 0, end: request.text.length)]
          : const [],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
    );
  }
}

class _AmbiguousFenceExclusionPredictEngine implements SyntaxEngine {
  final _parseCompleter = Completer<SyntaxSnapshot>();

  bool get hasPendingParse => !_parseCompleter.isCompleted;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    return _parseCompleter.future;
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    final fenceStart = request.text.indexOf('```');
    final exclusions = fenceStart == -1
        ? const <TextRange>[]
        : <TextRange>[TextRange(start: fenceStart, end: request.text.length)];
    return SyntaxPrediction(
      revision: request.revision,
      markerRanges: const [],
      exclusionRanges: exclusions,
      ambiguityZones: <TextRange>[
        TextRange(start: 0, end: request.text.length),
      ],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
    );
  }
}
