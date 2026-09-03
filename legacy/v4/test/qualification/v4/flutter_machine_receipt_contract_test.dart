import 'package:test/test.dart';

import '../../../scripts/verify_flutter_machine_test.dart';

void main() {
  const requiredName = 'required native canary';

  test('accepts one successful non-skipped named test', () {
    final receipt = verifyFlutterMachineTest(const [
      '{"test":{"id":3,"name":"required native canary"},"type":"testStart"}',
      '{"testID":3,"result":"success","skipped":false,"type":"testDone"}',
      '{"success":true,"type":"done"}',
    ], expectedName: requiredName);

    expect(receipt.passed, isTrue);
  });

  test('rejects the silent-skip false green', () {
    expect(
      () => verifyFlutterMachineTest(const [
        '{"test":{"id":3,"name":"required native canary"},"type":"testStart"}',
        '{"testID":3,"result":"success","skipped":true,"type":"testDone"}',
        '{"success":true,"type":"done"}',
      ], expectedName: requiredName),
      throwsStateError,
    );
  });

  test('rejects missing, duplicate, incomplete, and failed receipts', () {
    expect(
      () => verifyFlutterMachineTest(const [
        '{"success":true,"type":"done"}',
      ], expectedName: requiredName),
      throwsFormatException,
    );
    expect(
      () => verifyFlutterMachineTest(const [
        '{"test":{"id":3,"name":"required native canary"},"type":"testStart"}',
        '{"test":{"id":4,"name":"required native canary"},"type":"testStart"}',
        '{"success":true,"type":"done"}',
      ], expectedName: requiredName),
      throwsFormatException,
    );
    expect(
      () => verifyFlutterMachineTest(const [
        '{"test":{"id":3,"name":"required native canary"},"type":"testStart"}',
        '{"success":true,"type":"done"}',
      ], expectedName: requiredName),
      throwsFormatException,
    );
    expect(
      () => verifyFlutterMachineTest(const [
        '{"test":{"id":3,"name":"required native canary"},"type":"testStart"}',
        '{"testID":3,"result":"failure","skipped":false,"type":"testDone"}',
        '{"success":false,"type":"done"}',
      ], expectedName: requiredName),
      throwsStateError,
    );
  });
}
