import 'dart:async';
import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('Snapshot-gap cursor safety', () {
    test(
      'requested caret inside marker snaps to safe boundary while parse is in flight',
      () {
        final engine = _InFlightParsePredictEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '**bold**',
          selection: TextSelection.collapsed(offset: 1),
        );
        _expectSelectionOutsideMarkerInteriors(
          controller.selection.baseOffset,
          engine.lastPrediction.markerRanges,
        );

        controller.value = const TextEditingValue(
          text: '```\ncode\n```',
          selection: TextSelection.collapsed(offset: 1),
        );
        _expectSelectionOutsideMarkerInteriors(
          controller.selection.baseOffset,
          engine.lastPrediction.markerRanges,
        );
      },
    );

    test(
      'randomized snapshot-gap edits keep caret outside marker interiors',
      () {
        final engine = _InFlightParsePredictEngine();
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        final random = Random(13);
        const tokens = <String>[
          '**x**',
          '_x_',
          '`x`',
          '```\ncode\n```',
          '# heading',
          '> quote',
          '- item',
          'plain',
        ];

        for (var i = 0; i < 120; i++) {
          final prefix = String.fromCharCodes(
            List<int>.generate(
              random.nextInt(3),
              (_) => 97 + random.nextInt(3),
            ),
          );
          final token = tokens[random.nextInt(tokens.length)];
          final suffix = String.fromCharCodes(
            List<int>.generate(
              random.nextInt(3),
              (_) => 109 + random.nextInt(3),
            ),
          );
          final text = '$prefix$token$suffix';

          final preview = engine.previewPrediction(text);
          final targetOffset = _requestedOffsetInsideFirstMarker(preview, text);

          controller.value = TextEditingValue(
            text: text,
            selection: TextSelection.collapsed(offset: targetOffset),
          );

          _expectSelectionOutsideMarkerInteriors(
            controller.selection.baseOffset,
            engine.lastPrediction.markerRanges,
          );
        }
      },
    );
  });
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

int _requestedOffsetInsideFirstMarker(SyntaxPrediction preview, String text) {
  for (final range in preview.markerRanges) {
    if (range.end - range.start > 1) {
      return (range.start + 1).clamp(0, text.length);
    }
  }
  return text.length;
}

class _InFlightParsePredictEngine implements SyntaxEngine {
  final V1SyntaxEngineAdapter _delegate = const V1SyntaxEngineAdapter();
  SyntaxPrediction _lastPrediction = const SyntaxPrediction(
    revision: 0,
    markerRanges: [],
    exclusionRanges: [],
    ambiguityZones: [],
    cursorMask: PassthroughCursorValidationMask(textLength: 0),
  );

  SyntaxPrediction get lastPrediction => _lastPrediction;

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    final completer = Completer<SyntaxSnapshot>();
    return completer.future;
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    _lastPrediction = _delegate.predict(request);
    return _lastPrediction;
  }

  SyntaxPrediction previewPrediction(String text) {
    return _delegate.predict(SyntaxPredictRequest(revision: 0, text: text));
  }
}
