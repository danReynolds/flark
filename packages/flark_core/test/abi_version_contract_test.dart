import 'package:flark_core/src/native/native_document.dart';
import 'package:test/test.dart';

void main() {
  test('Dart ABI negotiation requires the exact current minor', () {
    expect(flarkV4AbiVersionIsCompatible(4, 27), isTrue);
    expect(flarkV4AbiVersionIsCompatible(4, 26), isFalse);
    expect(flarkV4AbiVersionIsCompatible(4, 28), isFalse);
    expect(flarkV4AbiVersionIsCompatible(5, 27), isFalse);
  });
}
