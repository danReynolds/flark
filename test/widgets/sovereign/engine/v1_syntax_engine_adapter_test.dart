import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('V1SyntaxEngineAdapter', () {
    test('parse is idempotent for identical input text', () async {
      const text = '''
# Header 😀
> quote line
```dart
print("hello")
```
**bold** _italic_ `code`
''';

      const adapter = V1SyntaxEngineAdapter();
      const request = SyntaxParseRequest(revision: 7, text: text);

      final first = await adapter.parse(request);
      final second = await adapter.parse(request);

      expect(first.stableHash, second.stableHash);
    });

    test('parse emits UTF-16-safe ranges', () async {
      const text = 'Hi 😀 **bold**\n```dart\nprint("x")\n```\n> quote\n';
      const adapter = V1SyntaxEngineAdapter();
      const request = SyntaxParseRequest(revision: 3, text: text);

      final snapshot = await adapter.parse(request);
      final textLength = text.length;

      void expectSafe(TextRange range) {
        expect(range.start, inInclusiveRange(0, textLength));
        expect(range.end, inInclusiveRange(0, textLength));
        expect(range.end, greaterThan(range.start));
      }

      for (final range in snapshot.markerRanges) {
        expectSafe(range);
      }
      for (final range in snapshot.exclusionRanges) {
        expectSafe(range);
      }
      for (final block in snapshot.blocks) {
        expect(block.start, inInclusiveRange(0, textLength));
        expect(block.end, inInclusiveRange(0, textLength));
        expect(block.end, greaterThan(block.start));
      }
      for (final token in snapshot.inlineTokens) {
        expect(token.start, inInclusiveRange(0, textLength));
        expect(token.end, inInclusiveRange(0, textLength));
        expect(token.end, greaterThan(token.start));
      }
    });

    test('predict reports ambiguity zone when scan is truncated', () {
      const text = '**bold**';
      const adapter = V1SyntaxEngineAdapter(predictiveScanSpanBudget: 0);

      final prediction = adapter.predict(
        const SyntaxPredictRequest(revision: 11, text: text),
      );

      expect(prediction.ambiguityZones, isNotEmpty);
      expect(prediction.ambiguityZones.first.start, lessThan(text.length));
      expect(prediction.ambiguityZones.first.end, text.length);
    });

    test('predict has no ambiguity zone when scan completes', () {
      const text = '**bold**';
      const adapter = V1SyntaxEngineAdapter();

      final prediction = adapter.predict(
        const SyntaxPredictRequest(revision: 12, text: text),
      );

      expect(prediction.ambiguityZones, isEmpty);
    });

    test('predict honors request charLimit override', () {
      final prefix = List.filled(300, 'a').join();
      final text = '$prefix **bold**';
      const adapter = V1SyntaxEngineAdapter();

      final full = adapter.predict(
        SyntaxPredictRequest(revision: 13, text: text),
      );

      final truncated = adapter.predict(
        SyntaxPredictRequest(revision: 14, text: text, charLimit: 32),
      );

      expect(full.ambiguityZones, isEmpty);
      expect(truncated.ambiguityZones, isNotEmpty);
      expect(truncated.ambiguityZones.first.end, text.length);
    });

    test('v1 parse includes quoted list marker ranges', () async {
      const text = '> - alpha\n> 2. beta\n';
      const adapter = V1SyntaxEngineAdapter();

      final snapshot = await adapter.parse(
        const SyntaxParseRequest(revision: 15, text: text),
      );

      expect(
        snapshot.markerRanges,
        contains(const TextRange(start: 2, end: 4)),
      );
      expect(
        snapshot.markerRanges,
        contains(const TextRange(start: 12, end: 15)),
      );
    });
  });
}
