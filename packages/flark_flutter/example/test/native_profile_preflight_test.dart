import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../integration_test/native_profile.dart';

void main() {
  testWidgets('profile preflight needs native semantics and foreground', (
    tester,
  ) async {
    final binding = tester.binding;
    binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    expect(binding.framesEnabled, isTrue);
    expect(binding.semanticsEnabled, isTrue);
    expect(binding.platformDispatcher.semanticsEnabled, isFalse);
    await expectLater(
      waitForNativeProfile(binding, timeout: Duration.zero),
      throwsStateError,
    );

    // Simulate the OS request only to exercise the guard. This unit test is not
    // evidence that the native bridge has been activated on a real device.
    binding.platformDispatcher.semanticsEnabledTestValue = true;
    try {
      await waitForNativeProfile(binding, timeout: Duration.zero);
      binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      await expectLater(
        waitForNativeProfile(binding, timeout: Duration.zero),
        throwsStateError,
      );
    } finally {
      binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      binding.platformDispatcher.clearSemanticsEnabledTestValue();
    }
  });
}
