import 'dart:io';

import 'flark_wasm_buildinfo.dart';

/// Regenerates `lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo`, the source
/// manifest that pins the committed WASM binary to the Rust sources it was
/// built from. Invoked by `scripts/build_comrak_wasm.sh` right after a WASM
/// build so the manifest always travels with a fresh binary.
void main() {
  final packageRoot = File.fromUri(Platform.script).parent.parent;
  final crateDir = Directory('${packageRoot.path}/native/comrak_bridge');
  if (!crateDir.existsSync()) {
    stderr.writeln('Cannot find crate at ${crateDir.path}');
    exitCode = 1;
    return;
  }

  final manifest = flarkWasmBuildInfo(crateDir);
  final output = File(
    '${packageRoot.path}/lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo',
  );
  output.parent.createSync(recursive: true);
  output.writeAsStringSync(manifest);

  final inputCount = manifest.split('\n').where((l) => l.isNotEmpty).length;
  stdout.writeln('Wrote ${output.path} ($inputCount inputs).');
}
