import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  const fact = FlarkInlineFact(
    kind: FlarkInlineFactKind.strong,
    flags: 1 << 7,
    sourceBytes: FlarkSourceRange(0, 8),
    sourceUtf16: FlarkSourceRange(0, 8),
    contentBytes: FlarkSourceRange(2, 6),
    contentUtf16: FlarkSourceRange(2, 6),
  );

  test('parser policy binds continuity to one exact transaction', () {
    final receipt = authorizeInlineProjectionContinuity(
      revision: 4,
      facts: const [fact],
      startUtf16: 4,
      endUtf16: 4,
      replacement: 'er',
    );

    expect(receipt, isNotNull);
    expect(receipt!.baseRevision, 4);
    expect(receipt.resultRevision, 5);
    expect(receipt.editStartUtf16, 4);
    expect(receipt.editEndUtf16, 4);
    expect(receipt.replacement, 'er');
    expect(receipt.authorizedContentUtf16.start, 2);
    expect(receipt.authorizedContentUtf16.end, 8);

    final chained = receipt.continueWith(
      startUtf16: 5,
      endUtf16: 5,
      replacement: 'x',
    );
    expect(chained, isNotNull);
    expect(chained!.baseRevision, 5);
    expect(chained.resultRevision, 6);
    expect(chained.authorizedContentUtf16.start, 2);
    expect(chained.authorizedContentUtf16.end, 9);
  });

  test('markers and syntax-shaped text fail closed', () {
    expect(
      authorizeInlineProjectionContinuity(
        revision: 1,
        facts: const [fact],
        startUtf16: 1,
        endUtf16: 1,
        replacement: 'x',
      ),
      isNull,
    );
    expect(
      authorizeInlineProjectionContinuity(
        revision: 1,
        facts: const [fact],
        startUtf16: 4,
        endUtf16: 4,
        replacement: '*',
      ),
      isNull,
    );
    expect(
      authorizeInlineProjectionContinuity(
        revision: 1,
        facts: const [fact],
        startUtf16: 2,
        endUtf16: 2,
        replacement: 'x',
      ),
      isNotNull,
    );
  });
}
