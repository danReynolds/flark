import 'package:flark_flutter/flark_flutter_advanced.dart';

void installNativeTestBackendOverride() {}

FlarkNativeComrakParseBackend flarkTestNativeBackend() {
  return FlarkNativeComrakParseBackend.withNativeBridge(
    wasmSource: NativeComrakWasmUriSource(
      Uri.base.resolve(
        '/packages/flark_flutter/assets/wasm/flark_comrak_bridge.wasm',
      ),
    ),
  );
}
