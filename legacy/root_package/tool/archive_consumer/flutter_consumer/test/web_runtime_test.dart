@TestOn('browser')
library;

import 'package:flark/flark_v3.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'archive assets boot the real Worker and Wasm runtime',
    () async {
      final assetRoot = Uri.base.resolve('/packages/flark_flutter/assets/');
      final runtime = await FlarkV3DocumentRuntime.open(
        '# External archive\n\n**Web** runtime.\n',
        webAssets: FlarkV3WebRuntimeAssets(
          workerUri: assetRoot.resolve('worker/flark_v3_parser_worker.js'),
          wasmUri: assetRoot.resolve('wasm/flark_comrak_bridge.wasm'),
        ),
      );
      addTearDown(() async {
        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runtime.close().timeout(const Duration(seconds: 30));
        }
      });

      await runtime.initialReady.timeout(const Duration(seconds: 30));
      final status = runtime.status.structureCurrent
          ? runtime.status
          : await runtime.statuses
                .firstWhere((candidate) => candidate.structureCurrent)
                .timeout(const Duration(seconds: 30));
      expect(status.structureRevision, runtime.sourceRevision);
      final previousRevision = runtime.sourceRevision;
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: previousRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: 0,
            replacement: '> ',
          ),
        ),
      );
      await runtime.statuses
          .firstWhere(
            (candidate) =>
                candidate.structureCurrent &&
                candidate.structureRevision == runtime.sourceRevision,
          )
          .timeout(const Duration(seconds: 30));
      expect(runtime.sourceRevision, previousRevision + 1);
      expect(runtime.readSourceRange(0, 2), '> ');
      expect(runtime.exportMarkdown(), startsWith('> # External archive'));
      await runtime.close().timeout(const Duration(seconds: 30));
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
