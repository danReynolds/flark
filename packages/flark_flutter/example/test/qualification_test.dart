import 'dart:convert';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final shape in profileCycles.keys) {
    test('profile $shape reaches the byte boundary while remaining admitted', () {
      final source = boundedProfileSource(backend, shape, candidateLiveBytes);
      expect(utf8.encode(source).length, candidateLiveBytes - 1);
      final e = FlarkEditor(
        backend,
        text: source,
        caret: source.length,
        syncLimit: candidateLiveBytes,
        liveLimits: candidateLiveLimits,
      );
      final model = backend.parse(source);
      expect(
        e.sourceMode,
        isFalse,
        reason:
            '${model.lineCount} lines, ${model.blockCount} blocks, ${model.runCount} runs',
      );
      final rows = e.projection.rows;
      final largest = rows.reduce(
        (a, b) => a.text.length > b.text.length ? a : b,
      );
      for (final caret in [
        rows.firstWhere((r) => r.text.isNotEmpty).sourceStart,
        largest.sourceForDisplay(largest.text.length ~/ 2),
        source.length,
      ]) {
        e.apply(SetSelection.caret(caret));
        expect(e.apply(const InsertText('x')), isTrue);
        expect(utf8.encode(e.source).length, candidateLiveBytes);
        expect(e.sourceMode, isFalse);
        expect(e.apply(const DeleteBackward()), isTrue);
        expect(e.source, source);
      }
    });
    test('profile $shape start-site rounds apply and restore the source', () {
      final source = boundedProfileSource(
        backend,
        shape,
        candidateLiveBytes - profileStartHeadroom,
      );
      final e = FlarkEditor(
        backend,
        text: source,
        syncLimit: candidateLiveBytes,
        liveLimits: candidateLiveLimits,
      );
      final caret = profileCaret(e.projection, 'start', source);
      e.apply(SetSelection.caret(caret));
      for (var round = 0; round < 3; round++) {
        for (final (name, command) in profileOperations('start')) {
          expect(e.apply(command), isTrue, reason: '$shape $name');
          expect(e.sourceMode, isFalse, reason: '$shape $name');
        }
        expect(e.source, source, reason: '$shape round $round');
        expect(e.selection.extent, caret, reason: '$shape round $round');
      }
    });
  }
}
