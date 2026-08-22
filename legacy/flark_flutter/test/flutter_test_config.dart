import 'dart:async';

// The tolerant golden comparator subclasses LocalFileComparator and uses its
// dart:io-backed API (basedir, getGoldenBytes, generateFailureOutput). That API
// does not exist in flutter_test's web build, so selecting the implementation by
// platform keeps this config compiling under `flutter test --platform chrome`.
// flark's flaky pixel goldens run on the VM, so web gets a no-op.
import 'golden_tolerance_stub.dart'
    if (dart.library.io) 'golden_tolerance_io.dart';
import 'v2/support/native_test_backend_stub.dart'
    if (dart.library.io) 'v2/support/native_test_backend_io.dart';

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  installTolerantGoldenComparator();
  // `testMain` registers tests and returns before the runner executes them,
  // so this per-isolate override intentionally lives for the full test file.
  installNativeTestBackendOverride();
  await testMain();
}
