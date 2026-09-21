import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../test_driver/profile_receipt.dart';

void main() {
  Map<String, dynamic> receipt(String name) =>
      jsonDecode(
            utf8.decode(
              gzip.decode(
                File(
                  '../../../docs/architecture/v5/receipts/'
                  'native-attended-2026-09-20/$name-driver.json.gz',
                ).readAsBytesSync(),
              ),
            ),
          )
          as Map<String, dynamic>;

  test('accepts complete captured native runs', () {
    validateProfileReceipt(receipt('frames'));
    validateProfileReceipt(receipt('workbench-corrected'));
  });
  test('rejects the missing data from failed suite setup', () {
    expect(() => validateProfileReceipt(null), throwsStateError);
    expect(() => validateProfileReceipt({}), throwsStateError);
  });
  test('rejects partial and repeated frame cases', () {
    final partial = receipt('frames');
    (partial['receipts'] as List).removeLast();
    expect(() => validateProfileReceipt(partial), throwsStateError);
    final duplicate = receipt('frames');
    final cases = duplicate['receipts'] as List;
    cases[23] = cases[0];
    expect(() => validateProfileReceipt(duplicate), throwsStateError);
  });
  test('rejects truncated sustained workload and failed limits', () {
    final partial = receipt('workbench-corrected');
    (partial['samples'] as List).removeLast();
    expect(() => validateProfileReceipt(partial), throwsStateError);
    final missingEdit = receipt('workbench-corrected');
    final samples = missingEdit['samples'] as List;
    samples.removeAt(samples.indexWhere((s) => s['operation'] == 'delete'));
    expect(() => validateProfileReceipt(missingEdit), throwsStateError);
    final failed = receipt('workbench-corrected');
    failed['failures'] = ['peak RSS'];
    expect(() => validateProfileReceipt(failed), throwsStateError);
  });
  test('rejects framework-only frame receipts and frame timing misses', () {
    final noNative = receipt('frames');
    noNative['receipts'][0]['nativeSemantics'] = false;
    expect(() => validateProfileReceipt(noNative), throwsStateError);
    final missed = receipt('frames');
    missed['receipts'][0]['insertP99Us'] = 16667;
    expect(() => validateProfileReceipt(missed), throwsStateError);
  });
}
