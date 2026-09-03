import 'dart:convert';
import 'dart:io';

import 'package:flark/src/native/native_document.dart';
import 'package:test/test.dart';

void main() {
  test('Dart ABI negotiation requires the exact current minor', () {
    expect(flarkV4AbiVersionIsCompatible(4, 38), isTrue);
    expect(flarkV4AbiVersionIsCompatible(4, 37), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 36), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 35), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 34), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 33), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 32), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 31), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 30), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 28), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 27), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 29), isFalse);
    expect(flarkV4AbiVersionIsCompatible(5, 38), isFalse);
  });

  test('Dart exact-minor expectation agrees with the machine contract', () {
    final manifest =
        jsonDecode(
              File('test/fixtures/v4/runtime_abi_v1.json').readAsStringSync(),
            )
            as Map<String, Object?>;
    final abi = manifest['abi']! as Map<String, Object?>;
    expect(abi['major'], 4);
    expect(abi['minor'], 38);
    expect(
      flarkV4AbiVersionIsCompatible(abi['major']! as int, abi['minor']! as int),
      isTrue,
    );
  });

  test('ABI 4.38 continuation metadata rejects every reserved high bit', () {
    const lengths = 1 | (2 << 16) | (1 << 32);
    const stablePolicy = 1 | (1 << 16);
    expect(
      flarkV4InlineContinuationMetadataIsCanonical(
        hasInlineContinuation: true,
        packedLengths: lengths,
        packedPolicy: stablePolicy,
      ),
      isTrue,
    );
    expect(
      flarkV4InlineContinuationMetadataIsCanonical(
        hasInlineContinuation: true,
        packedLengths: lengths | (1 << 48),
        packedPolicy: stablePolicy,
      ),
      isFalse,
      reason: 'reserved[0] bits 48..63 must fail closed',
    );
    expect(
      flarkV4InlineContinuationMetadataIsCanonical(
        hasInlineContinuation: true,
        packedLengths: lengths,
        packedPolicy: stablePolicy | (1 << 32),
      ),
      isFalse,
      reason: 'reserved[1] bits 32..63 must fail closed',
    );
    expect(
      flarkV4InlineContinuationMetadataIsCanonical(
        hasInlineContinuation: false,
        packedLengths: lengths,
        packedPolicy: 0,
      ),
      isFalse,
      reason: 'an absent flag cannot smuggle recipe metadata',
    );
  });
}
