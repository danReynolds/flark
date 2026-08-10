import 'package:flutter_test/flutter_test.dart';

import 'package:flark_peer_benchmark/competitor_profile_harness.dart';

void main() {
  test('Quill canonical paste states tolerate only its proven newline', () {
    const base = 'abc';
    const payload = 'XY';
    final pasted = insertExactSource(source: base, payload: payload, offset: 1);

    expect(pasted, 'aXYbc');
    expect(canonicalStateDenominator(base), {
      'utf8Bytes': 3,
      'sha256': isA<String>(),
    });
    expect(
      quillCanonicalStateProof(
        expectedCanonical: pasted,
        actualPeerSource: '$pasted\n',
      ),
      containsPair('classification', 'peer-appended-terminal-newline'),
    );
    expect(
      () => quillCanonicalStateProof(
        expectedCanonical: pasted,
        actualPeerSource: '$pasted\n\n',
      ),
      throwsStateError,
    );
  });
}
