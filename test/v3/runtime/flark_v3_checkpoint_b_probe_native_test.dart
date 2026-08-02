import 'dart:convert';

import 'package:flark/src/v3/runtime/native/flark_v3_native_endpoint_bindings.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_checkpoint_b_probe_native.dart';
import 'package:test/test.dart';

const _expectedParityDigest =
    '2233974cc604304e5e2e8bc28f0f36ea25cf51f00ef8470338aad783baab11d4';

void main() {
  test(
    'Checkpoint B native probe returns bounded passing JSON from an isolate',
    () async {
      final encoded = await runCheckpointBProbe();
      final receipt = jsonDecode(encoded) as Map<String, Object?>;

      expect(
        utf8.encode(encoded).length,
        lessThanOrEqualTo(flarkV3NativeCheckpointBMaximumJsonBytes),
      );
      expect(receipt['schema'], 1);
      expect(receipt['platform'], 'native');
      expect(receipt['allChecksPassed'], isTrue);
      expect(receipt['parityDigest'], _expectedParityDigest);
      expect(receipt['steps'], hasLength(4));
      expect(receipt['lifecycle'], containsPair('closedToZero', true));
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}
