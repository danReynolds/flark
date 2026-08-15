import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter_test/flutter_test.dart';

Future<FlarkV3DocumentRuntime> openManagedRuntimeForTest(String markdown) =>
    FlarkV3DocumentRuntime.open(
      markdown,
      webAssets: FlarkV3WebRuntimeAssets(
        workerUri: _versionedPackageAsset(
          '/packages/flark_flutter/assets/worker/'
          'flark_v3_parser_worker.js',
        ),
        wasmUri: _versionedPackageAsset(
          '/packages/flark_flutter/assets/wasm/flark_comrak_bridge.wasm',
        ),
      ),
    );

Future<T> runManagedRuntimeAsyncForTest<T>(
  WidgetTester tester,
  Future<T> Function() work,
) async {
  final result = await tester.runAsync(work);
  return result as T;
}

Uri _versionedPackageAsset(String path) => Uri.base
    .resolve(path)
    .replace(queryParameters: const {'flark-test-build': 'atx-heading-v4'});
