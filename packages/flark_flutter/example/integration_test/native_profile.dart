import 'package:flutter/widgets.dart';

/// Establish the OS-owned semantics handle before testWidgets records its
/// baseline. Forcing a framework tree first can send updates before the macOS
/// accessibility bridge exists; acquiring the OS handle inside the test also
/// makes the widget runner report it as a leaked test handle.
/// Frame delivery is checked inside the test: the live binding intentionally
/// disables frames outside a running test, including during setUpAll.
Future<void> waitForNativeProfile(
  WidgetsBinding binding, {
  Duration timeout = const Duration(seconds: 60),
}) async {
  bool ready() =>
      binding.lifecycleState == AppLifecycleState.resumed &&
      binding.platformDispatcher.semanticsEnabled;
  final deadline = DateTime.now().add(timeout);
  while (!ready() && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  if (!ready()) {
    throw StateError(
      'Activate the native app and inspect its accessibility tree before '
      'profiling: lifecycle=${binding.lifecycleState}, '
      'frames=${binding.framesEnabled}, '
      'nativeSemantics=${binding.platformDispatcher.semanticsEnabled}',
    );
  }
}
