import 'package:flark/flark_v3.dart';

void main() {
  final configured = FlarkV3WebRuntimeAssets(
    workerUri: Uri.parse('/static/flark/flark_v3_parser_worker.js'),
    wasmUri: Uri.parse('/static/flark/flark_comrak_bridge.wasm'),
  );
  if (configured.workerUri == configured.wasmUri) {
    throw StateError(
      'Worker and Wasm URLs must remain independently explicit.',
    );
  }

  final support = FlarkV3DocumentRuntime.platformSupport;
  if (!support.supported) {
    throw StateError(
      'The v3 Web Worker endpoint was not selected: '
      '${support.unavailableReason}',
    );
  }
  if (support.endpoint != 'Web Worker + Wasm') {
    throw StateError('Unexpected web endpoint capability: ${support.endpoint}');
  }
}
