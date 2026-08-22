import 'dart:convert';
import 'dart:io';

import 'package:flark_core/src/native/native_document.dart';
import 'package:test/test.dart';

void main() {
  test('Dart ABI negotiation requires the exact current minor', () {
    expect(flarkV4AbiVersionIsCompatible(4, 31), isTrue);
    expect(flarkV4AbiVersionIsCompatible(4, 30), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 28), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 27), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 29), isFalse);
    expect(flarkV4AbiVersionIsCompatible(5, 31), isFalse);
  });

  test('Dart exact-minor expectation agrees with the machine contract', () {
    final manifest =
        jsonDecode(
              File('test/fixtures/v4/runtime_abi_v1.json').readAsStringSync(),
            )
            as Map<String, Object?>;
    final abi = manifest['abi']! as Map<String, Object?>;
    expect(abi['major'], 4);
    expect(abi['minor'], 31);
    expect(
      flarkV4AbiVersionIsCompatible(abi['major']! as int, abi['minor']! as int),
      isTrue,
    );
  });
}
