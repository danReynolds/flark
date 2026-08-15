import 'package:flark/src/v3/runtime/web/flark_v3_web_worker_byte_endpoint.dart';

void main() {
  // Keep the concrete Worker adapter reachable so this fixture validates its
  // static interop declarations rather than only the conditional public shell.
  final start = FlarkV3WebWorkerByteEndpoint.start;
  if (start.toString().isEmpty || flarkV3WebMaximumFrameBytes <= 0) {
    throw StateError(
      'The v3 Web Worker endpoint was tree-shaken unexpectedly.',
    );
  }
}
