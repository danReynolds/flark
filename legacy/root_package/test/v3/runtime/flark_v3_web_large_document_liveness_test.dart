@TestOn('browser')
library;

import 'dart:async';

import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:test/test.dart';

const int _mib = 1024 * 1024;
const Duration _functionalTimeout = Duration(seconds: 20);
const Duration _ordinaryLineSchedulerRegressionTimeout = Duration(seconds: 3);

String _giantLine(int bytes) => 'x' * bytes;

String _ordinaryLines(int bytes) {
  final line = '${'x' * 79}\n';
  return (line * ((bytes ~/ line.length) + 1)).substring(0, bytes);
}

String _newlineDense(int bytes) =>
    ('x\n' * (bytes ~/ 2 + 1)).substring(0, bytes);

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
  Duration timeout,
) async {
  if (runtime.status.structureCurrent) return runtime.status;
  final status = await runtime.statuses
      .firstWhere(
        (status) =>
            status.structureCurrent ||
            status.state == FlarkV3DocumentRuntimeState.closed,
      )
      .timeout(timeout);
  if (status.structureCurrent) return status;
  await runtime.close();
  throw StateError('The Web runtime closed before structural recertification.');
}

Future<void> _closeIfOpen(FlarkV3DocumentRuntime runtime) async {
  await runtime.close().timeout(const Duration(seconds: 10));
}

void main() {
  test('default Web event scheduler preserves task order and zones', () async {
    const scheduler = FlarkV3DartEventTaskScheduler();
    final completed = Completer<void>();
    final observed = <String>[];

    void scheduleFrom(String zoneName) {
      runZoned(
        () => scheduler.schedule(() {
          observed.add(Zone.current[#flarkSchedulerZone] as String);
          if (observed.length == 2) completed.complete();
        }),
        zoneValues: <Object?, Object?>{#flarkSchedulerZone: zoneName},
      );
    }

    scheduleFrom('first');
    scheduleFrom('second');
    expect(observed, isEmpty, reason: 'schedule must not run synchronously');

    await completed.future.timeout(const Duration(seconds: 1));
    expect(observed, <String>['first', 'second']);
  });

  test(
    '1 MiB line shapes converge through the external Worker',
    () async {
      final cases = <({String name, String source, Duration timeout})>[
        (
          name: 'giant line',
          source: _giantLine(_mib),
          timeout: _functionalTimeout,
        ),
        (
          name: '80-byte lines',
          source: _ordinaryLines(_mib),
          // This deliberately coarse ceiling detects a regression to one
          // browser timer per candidate subgrant. It is not a launch latency
          // SLO and excludes test compilation/startup.
          timeout: _ordinaryLineSchedulerRegressionTimeout,
        ),
        (
          name: 'newline-dense paragraph',
          source: _newlineDense(_mib),
          timeout: _functionalTimeout,
        ),
      ];

      for (final testCase in cases) {
        final runtime = await FlarkV3DocumentRuntime.open(
          testCase.source,
          webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
        ).timeout(_functionalTimeout);
        try {
          await runtime.initialReady.timeout(_functionalTimeout);
          final current = await _awaitCurrent(runtime, testCase.timeout);

          expect(current.sourceCurrent, isTrue, reason: testCase.name);
          expect(
            current.structureRevision,
            runtime.sourceRevision,
            reason: testCase.name,
          );
          final query = runtime.queryAtUtf16(0);
          expect(
            query,
            isA<FlarkV3DocumentStructuralQuery>(),
            reason: testCase.name,
          );
          final structure = query as FlarkV3DocumentStructuralQuery;
          expect(
            structure.structure.kind,
            FlarkV3DocumentStructureKind.paragraph,
            reason: testCase.name,
          );
          expect(structure.structure.source.startUtf16, 0);
          expect(structure.structure.source.endUtf16, _mib);

          await runtime.close().timeout(const Duration(seconds: 10));
          expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
        } finally {
          await _closeIfOpen(runtime);
        }
      }
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'active close interrupts a 10 MiB candidate and proves reclamation',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _ordinaryLines(10 * _mib),
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      try {
        final activeCandidate = runtime.statuses.firstWhere(
          (status) => status.sourceCurrent && !status.structureCurrent,
        );
        await runtime.initialReady.timeout(_functionalTimeout);
        await activeCandidate.timeout(_functionalTimeout);

        await runtime.close().timeout(const Duration(seconds: 5));
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
      } finally {
        await _closeIfOpen(runtime);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'mid-parse supersession installs only the latest source revision',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        _ordinaryLines(_mib),
        webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
      ).timeout(_functionalTimeout);
      final currentRevisions = <int>[];
      StreamSubscription<FlarkV3DocumentRuntimeStatus>? subscription;
      try {
        final activeCandidate = runtime.statuses.firstWhere(
          (status) => status.sourceCurrent && !status.structureCurrent,
        );
        await runtime.initialReady.timeout(_functionalTimeout);
        await activeCandidate.timeout(_functionalTimeout);

        subscription = runtime.statuses.listen((status) {
          if (status.structureCurrent) {
            currentRevisions.add(status.structureRevision!);
          }
        });
        for (final replacement in <String>['y', 'z', 'w']) {
          final receipt = runtime.apply(
            FlarkV3SourceTransaction.single(
              baseRevision: runtime.sourceRevision,
              operation: FlarkV3SourceEdit(
                startUtf16: 0,
                endUtf16: 1,
                replacement: replacement,
              ),
            ),
          );
          expect(receipt.changed, isTrue);
        }
        final latestRevision = runtime.sourceRevision;
        final current = await runtime.statuses
            .firstWhere(
              (status) =>
                  status.sourceRevision == latestRevision &&
                  status.structureCurrent,
            )
            .timeout(_functionalTimeout);

        expect(runtime.readSourceRange(0, 1), 'w');
        expect(current.structureRevision, latestRevision);
        expect(currentRevisions, isNotEmpty);
        expect(currentRevisions, everyElement(latestRevision));

        await runtime.close().timeout(const Duration(seconds: 10));
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
      } finally {
        await subscription?.cancel();
        await _closeIfOpen(runtime);
      }
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    '100,000 definitions publish, replace, query, and close without a caller-thread stall',
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
        final openFuture = FlarkV3DocumentRuntime.open(
          source,
          webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
        );
        openCallClock.stop();
        runtime = await openFuture.timeout(const Duration(seconds: 60));
        await runtime.initialReady.timeout(const Duration(seconds: 60));
        final coldElapsed = clock.elapsed - coldStarted;

        final initial = runtime.queryAtUtf16(source.length - 2);
        expect(initial, isA<FlarkV3DocumentStructuralQuery>());
        final initialStructure = initial as FlarkV3DocumentStructuralQuery;
        expect(
          initialStructure.structure.kind,
          FlarkV3DocumentStructureKind.paragraph,
        );
        expect(
          initialStructure.structure.referenceDefinitionCount,
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
        final replacement = await _awaitCurrent(
          runtime,
          const Duration(seconds: 60),
        );
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

        final closeClock = Stopwatch()..start();
        await runtime.close().timeout(const Duration(seconds: 30));
        closeClock.stop();
        expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);

        expect(
          applyClock.elapsed,
          lessThan(const Duration(milliseconds: 50)),
          reason: 'The foreground source edit must remain a small local cut.',
        );
        expect(
          heartbeatCount,
          greaterThan(10),
          reason: 'Publication must yield to the browser event loop.',
        );
        expect(
          maximumHeartbeatGap,
          lessThan(const Duration(seconds: 1)),
          reason:
              'No packet admission or host poll may monopolize the caller '
              'thread for a second.',
        );
        expect(
          closeClock.elapsed,
          lessThan(const Duration(seconds: 2)),
          reason:
              'Truthful document-sized reclamation must use unclamped browser '
              'tasks. Zero-duration timer chaining takes roughly five seconds '
              'for this fixture despite spending only milliseconds in Wasm.',
        );
        // ignore: avoid_print
        print(
          'flark_v3_web_100k_references '
          'source_bytes=${source.length} cold_us=${coldElapsed.inMicroseconds} '
          'open_call_us=${openCallClock.elapsedMicroseconds} '
          'apply_us=${applyClock.elapsedMicroseconds} '
          'replacement_us=${replacementElapsed.inMicroseconds} '
          'close_us=${closeClock.elapsedMicroseconds} '
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
