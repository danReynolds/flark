import 'native_comrak_ffi.dart';

NativeComrakBridge createNativeComrakBridge({String? overrideLibraryPath}) {
  throw NativeComrakBridgeLoadException(
    kind: NativeComrakBridgeLoadFailureKind.unsupportedFfi,
    message: 'Native comrak bridge is unavailable without dart:ffi support.',
    remediationSteps: const [
      'Run on a platform/runtime with dart:ffi support (macOS, Linux, iOS, or Android).',
    ],
  );
}

NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
}) {
  return const NativeComrakBridgePreflightResult.unavailable(
    NativeComrakBridgeLoadException(
      kind: NativeComrakBridgeLoadFailureKind.unsupportedFfi,
      message: 'Native comrak bridge is unavailable without dart:ffi support.',
      remediationSteps: [
        'Run on a platform/runtime with dart:ffi support (macOS, Linux, iOS, or Android).',
      ],
    ),
  );
}
