import 'package:flutter_test/flutter_test.dart';

import '../lib/competitor_profile_harness.dart';

void main() {
  test('SuperEditor paste states are exact and reset to the same base', () {
    const base = 'abc';
    const payload = 'XY';
    final pasted = insertExactSource(source: base, payload: payload, offset: 1);

    expect(pasted, 'aXYbc');
    expect(
      exactCanonicalStateProof(
        expectedCanonical: pasted,
        actualPeerSource: pasted,
      ),
      allOf(
        containsPair('classification', 'exact'),
        containsPair('canonicalUtf8Bytes', 5),
      ),
    );
    expect(
      exactCanonicalStateProof(
        expectedCanonical: base,
        actualPeerSource: base,
      )['canonicalSha256'],
      canonicalStateDenominator(base)['sha256'],
    );
    expect(
      () => exactCanonicalStateProof(
        expectedCanonical: base,
        actualPeerSource: '$base$payload',
      ),
      throwsStateError,
    );
  });
}
