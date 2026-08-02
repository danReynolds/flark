import 'package:flark/flark_v3.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';

void main() {
  final assets = FlarkV3WebRuntimeAssets(
    workerUri: Uri.parse(
      'assets/packages/flark_flutter/lib/assets/worker/'
      'flark_v3_parser_worker.js',
    ),
    wasmUri: Uri.parse(
      'assets/packages/flark_flutter/lib/assets/wasm/'
      'flark_comrak_bridge.wasm',
    ),
  );
  runApp(_ArchiveConsumerApp(assets: assets));
}

final class _ArchiveConsumerApp extends StatelessWidget {
  const _ArchiveConsumerApp({required this.assets});

  final FlarkV3WebRuntimeAssets assets;

  @override
  Widget build(BuildContext context) => MaterialApp(
    home: Scaffold(
      body: Text(
        'flark_flutter ${FlarkMarkdownEditingMode.source.name}\n'
        '${assets.workerUri}',
      ),
    ),
  );
}
