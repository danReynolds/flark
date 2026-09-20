import 'package:integration_test/integration_test_driver.dart';

import 'profile_receipt.dart';

Future<void> main() => integrationDriver(
  writeResponseOnFailure: true,
  responseDataCallback: (data) async {
    // Preserve failures too; validate before integrationDriver can exit zero.
    await writeResponseData(data);
    validateProfileReceipt(data);
  },
);
