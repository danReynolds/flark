@TestOn('browser')
library;

import 'dart:async';

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final assets = FlarkV3WebRuntimeAssets(
    workerUri: _versionedPackageAsset(
      '/packages/flark_flutter/assets/worker/flark_v3_parser_worker.js',
    ),
    wasmUri: _versionedPackageAsset(
      '/packages/flark_flutter/assets/wasm/flark_comrak_bridge.wasm',
    ),
  );

  testWidgets('widget-owned web runtime closes before the next test', (
    tester,
  ) async {
    await _openAndCloseWidgetRuntime(
      tester,
      assets: assets,
      source: 'first exact paragraph',
    );
  });

  testWidgets('same-URI web runtime reopens in the next widget test', (
    tester,
  ) async {
    await _openAndCloseWidgetRuntime(
      tester,
      assets: assets,
      source: 'second exact paragraph',
    );
  });
}

Future<void> _openAndCloseWidgetRuntime(
  WidgetTester tester, {
  required FlarkV3WebRuntimeAssets assets,
  required String source,
}) async {
  await tester.pumpWidget(_RuntimeOwner(assets: assets, source: source));
  await tester.tap(find.byKey(const Key('open-runtime')));
  await tester.pump();
  await _pumpUntil(tester, () => find.text('open').evaluate().isNotEmpty);

  await tester.tap(find.byKey(const Key('close-runtime')));
  await tester.pump();
  await _pumpUntil(tester, () => find.text('closed').evaluate().isNotEmpty);
  await tester.pumpWidget(const SizedBox.shrink());
}

final class _RuntimeOwner extends StatefulWidget {
  const _RuntimeOwner({required this.assets, required this.source});

  final FlarkV3WebRuntimeAssets assets;
  final String source;

  @override
  State<_RuntimeOwner> createState() => _RuntimeOwnerState();
}

final class _RuntimeOwnerState extends State<_RuntimeOwner> {
  FlarkV3DocumentRuntime? _runtime;
  String _phase = 'idle';

  void _open() {
    if (_phase != 'idle') return;
    setState(() => _phase = 'opening');
    unawaited(_openRuntime());
  }

  Future<void> _openRuntime() async {
    final runtime = await FlarkV3DocumentRuntime.open(
      widget.source,
      webAssets: widget.assets,
    );
    await runtime.initialReady;
    if (!mounted) {
      await runtime.close();
      return;
    }
    _runtime = runtime;
    setState(() => _phase = 'open');
  }

  void _close() {
    if (_phase != 'open') return;
    setState(() => _phase = 'closing');
    unawaited(_closeRuntime());
  }

  Future<void> _closeRuntime() async {
    final runtime = _runtime;
    if (runtime == null) return;
    await runtime.close();
    if (!mounted) return;
    _runtime = null;
    setState(() => _phase = 'closed');
  }

  @override
  void dispose() {
    final runtime = _runtime;
    if (runtime != null) unawaited(runtime.close());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    home: Column(
      children: [
        ElevatedButton(
          key: const Key('open-runtime'),
          onPressed: _open,
          child: const Text('Open'),
        ),
        ElevatedButton(
          key: const Key('close-runtime'),
          onPressed: _close,
          child: const Text('Close'),
        ),
        Text(_phase),
      ],
    ),
  );
}

Future<void> _pumpUntil(WidgetTester tester, bool Function() condition) async {
  final watch = Stopwatch()..start();
  while (!condition()) {
    if (watch.elapsed >= const Duration(seconds: 15)) {
      throw TestFailure('Timed out waiting for widget runtime lifecycle.');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    await tester.pump(const Duration(milliseconds: 1));
  }
}

Uri _versionedPackageAsset(String path) => Uri.base
    .resolve(path)
    .replace(
      queryParameters: const {'flark-test-build': 'reopen-lifecycle-v1'},
    );
