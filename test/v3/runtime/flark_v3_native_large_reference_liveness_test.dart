@TestOn('vm')
library;

import 'dart:async';

import 'package:flark/flark_v3.dart';
import 'package:test/test.dart';

String _referenceDense(int definitions) {
  final output = StringBuffer();
  for (var ordinal = 0; ordinal < definitions; ordinal += 1) {
    output.writeln('[label-$ordinal]: /u');
  }
  output.write('visible tail\n');
  return output.toString();
}

Future<FlarkV3DocumentRuntimeStatus> _awaitCurrent(
  FlarkV3DocumentRuntime runtime,
) async {
  if (runtime.status.structureCurrent) return runtime.status;
  final status = await runtime.statuses
      .firstWhere(
        (status) =>
            status.structureCurrent ||
            status.state == FlarkV3DocumentRuntimeState.closed,
      )
      .timeout(const Duration(seconds: 60));
  if (status.structureCurrent) return status;
  await runtime.close();
  throw StateError(
    'The native runtime closed before structural recertification.',
  );
}

Future<void> _closeIfOpen(FlarkV3DocumentRuntime runtime) async {
  await runtime.close().timeout(const Duration(seconds: 30));
}

void main() {
  test(
    '100,000 definitions cross the native isolate and FFI host without caller-thread stalls',
    () async {
      const definitions = 100_000;
      const tail = 'visible tail\n';
      final source = _referenceDense(definitions);
      final clock = Stopwatch()..start();
      var previousHeartbeat = Duration.zero;
      var maximumHeartbeatGap = Duration.zero;
      var heartbeatCount = 0;
      final heartbeat = Timer.periodic(const Duration(milliseconds: 8), (_) {
        final now = clock.elapsed;
        final gap = now - previousHeartbeat;
        previousHeartbeat = now;
        heartbeatCount += 1;
        if (gap > maximumHeartbeatGap) maximumHeartbeatGap = gap;
      });

      FlarkV3DocumentRuntime? runtime;
      try {
        final coldStarted = clock.elapsed;
        final openCallClock = Stopwatch()..start();
        final openFuture = FlarkV3DocumentRuntime.open(source);
        openCallClock.stop();
        runtime = await openFuture.timeout(const Duration(seconds: 60));
        await runtime.initialReady.timeout(const Duration(seconds: 60));
        final coldElapsed = clock.elapsed - coldStarted;

        final initial = runtime.queryAtUtf16(source.length - 2);
        expect(initial, isA<FlarkV3DocumentStructuralQuery>());
        expect(
          (initial as FlarkV3DocumentStructuralQuery)
              .structure
              .referenceDefinitionCount,
          definitions,
        );

        final applyClock = Stopwatch()..start();
        final edit = runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: runtime.sourceRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: source.length - tail.length,
              endUtf16: source.length - tail.length + 1,
              replacement: 'V',
            ),
          ),
        );
        applyClock.stop();
        expect(edit.changed, isTrue);
        final replacementStarted = clock.elapsed;
        final replacement = await _awaitCurrent(runtime);
        final replacementElapsed = clock.elapsed - replacementStarted;
        expect(replacement.structureRevision, runtime.sourceRevision);
        final editedStart = source.length - tail.length;
        expect(runtime.readSourceRange(editedStart, editedStart + 1), 'V');

        final replaced = runtime.queryAtUtf16(source.length - 2);
        expect(replaced, isA<FlarkV3DocumentStructuralQuery>());
        expect(
          (replaced as FlarkV3DocumentStructuralQuery)
              .structure
              .referenceDefinitionCount,
          definitions,
        );

        await runtime.close().timeout(const Duration(seconds: 30));
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
        expect(applyClock.elapsed, lessThan(const Duration(milliseconds: 50)));
        expect(heartbeatCount, greaterThan(10));
        expect(
          maximumHeartbeatGap,
          lessThan(const Duration(seconds: 1)),
          reason: 'Native packet admission and host polls must stay bounded.',
        );
        // ignore: avoid_print
        print(
          'flark_v3_native_100k_references '
          'source_bytes=${source.length} cold_us=${coldElapsed.inMicroseconds} '
          'open_call_us=${openCallClock.elapsedMicroseconds} '
          'apply_us=${applyClock.elapsedMicroseconds} '
          'replacement_us=${replacementElapsed.inMicroseconds} '
          'heartbeat_max_us=${maximumHeartbeatGap.inMicroseconds}',
        );
      } finally {
        heartbeat.cancel();
        if (runtime != null) await _closeIfOpen(runtime);
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
