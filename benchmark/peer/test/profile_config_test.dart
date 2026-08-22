import 'package:flark_peer_benchmark/profile_config.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('nonclaim smoke config accepts a small exact-byte fixture', () {
    final config = ProfileConfig.fromEnvironment(const {
      'COMPETITOR_SCENARIO': 'sustained-typing',
      'COMPETITOR_TARGET_BYTES': '1024',
      'COMPETITOR_NONCLAIM_RUN': '1',
      'COMPETITOR_TYPING_WARMUPS': '1',
      'COMPETITOR_TYPING_SAMPLES': '2',
    });
    expect(config.sizeTierId, 'nonclaim-1024b');
    expect(config.completionEnvelopeConfigurationEligible, isFalse);
    expect(config.typingWarmups, 1);
    expect(config.typingSamples, 2);
  });

  test('unknown scenarios fail closed', () {
    expect(
      () => ProfileConfig.fromEnvironment(const {
        'COMPETITOR_SCENARIO': 'invented',
        'COMPETITOR_TARGET_BYTES': '1024',
        'COMPETITOR_NONCLAIM_RUN': '1',
      }),
      throwsFormatException,
    );
  });
}
