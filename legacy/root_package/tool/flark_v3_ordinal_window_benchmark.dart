import 'dart:convert';
import 'dart:io';

import 'package:flark/flark_v3.dart';

/// Product-shaped native benchmark for the synchronous Dart ordinal locator.
///
/// This measures the complete public call, including Dart demand/result
/// handling, FFI, host-tree traversal, and receipt validation. Parser startup
/// is reported separately and excluded from query percentiles.
///
/// Run from the package root after building the native bridge:
///
/// ```sh
/// dart run tool/flark_v3_ordinal_window_benchmark.dart
/// ```
///
/// Optional arguments:
///
/// - `--sizes=4096,50000`
/// - `--samples=2000`
/// - `--warmup=250`
/// - `--burst-samples=500`
/// - `--burst-size=3`
/// - `--window=97`
/// - `--native-library=/absolute/path/to/libflark_comrak_bridge.dylib`
Future<void> main(List<String> arguments) async {
  final options = _Options.parse(arguments);
  final libraryPath = options.nativeLibraryPath ?? _defaultNativeLibraryPath();
  if (libraryPath == null || !File(libraryPath).existsSync()) {
    throw StateError(
      'Build the native bridge first or pass --native-library=<path>.',
    );
  }

  final results = <Map<String, Object?>>[];
  for (final entryCount in options.sizes) {
    results.add(
      await _runCase(
        entryCount: entryCount,
        options: options,
        libraryPath: File(libraryPath).absolute.path,
      ),
    );
  }

  stdout.writeln(
    const JsonEncoder.withIndent('  ').convert(<String, Object?>{
      'benchmark': 'flark_v3_native_structural_ordinal_window',
      'dart': Platform.version,
      'operating_system': Platform.operatingSystem,
      'stopwatch_frequency_hz': Stopwatch().frequency,
      'native_library': File(libraryPath).absolute.path,
      'options': options.toJson(),
      'cases': results,
    }),
  );
}

Future<Map<String, Object?>> _runCase({
  required int entryCount,
  required _Options options,
  required String libraryPath,
}) async {
  final sourceWatch = Stopwatch()..start();
  final markdown = _headingDocument(entryCount);
  sourceWatch.stop();

  final openWatch = Stopwatch()..start();
  final runtime = await FlarkV3DocumentRuntime.open(
    markdown,
    nativeLibraryPath: libraryPath,
  );
  try {
    await runtime.initialReady.timeout(const Duration(minutes: 2));
    if (!runtime.status.structureCurrent) {
      await runtime.statuses
          .firstWhere((status) => status.structureCurrent)
          .timeout(const Duration(minutes: 2));
    }
    openWatch.stop();

    final status = runtime.status;
    final starts = _distantStarts(entryCount, options.windowEntries);
    final demands = <FlarkV3DocumentOrdinalWindowDemand>[
      for (final start in starts)
        FlarkV3DocumentOrdinalWindowDemand(
          sourceRevision: status.sourceRevision,
          structureGeneration: status.structureGeneration,
          startBlockOrdinal: start,
        ),
    ];
    final budget = FlarkV3DocumentOrdinalWindowBudget(
      maximumEntries: options.windowEntries,
    );
    final work = _MaximumWorkReceipt();

    for (var index = 0; index < options.warmupSamples; index += 1) {
      final exact = _queryExact(
        runtime,
        demands[index % demands.length],
        budget,
        expectedTotal: entryCount,
      );
      work.observe(exact);
    }

    final clock = Stopwatch()..start();
    final queryNanoseconds = List<int>.filled(options.samples, 0);
    for (var index = 0; index < options.samples; index += 1) {
      final before = clock.elapsedTicks;
      final exact = _queryExact(
        runtime,
        demands[index % demands.length],
        budget,
        expectedTotal: entryCount,
      );
      final after = clock.elapsedTicks;
      queryNanoseconds[index] = _ticksToNanoseconds(after - before);
      work.observe(exact);
    }

    final burstNanoseconds = List<int>.filled(options.burstSamples, 0);
    for (var sample = 0; sample < options.burstSamples; sample += 1) {
      final before = clock.elapsedTicks;
      for (var query = 0; query < options.burstSize; query += 1) {
        final exact = _queryExact(
          runtime,
          demands[(sample * options.burstSize + query) % demands.length],
          budget,
          expectedTotal: entryCount,
        );
        work.observe(exact);
      }
      final after = clock.elapsedTicks;
      burstNanoseconds[sample] = _ticksToNanoseconds(after - before);
    }

    return <String, Object?>{
      'structural_entries': entryCount,
      'source_utf8_bytes': markdown.length,
      'source_utf16_code_units': runtime.sourceLengthUtf16,
      'source_build_us': sourceWatch.elapsedMicroseconds,
      'open_until_exact_us': openWatch.elapsedMicroseconds,
      'distant_start_ordinals': starts,
      'query_latency': _LatencyStatistics(queryNanoseconds).toJson(),
      'burst_latency': <String, Object?>{
        'queries_per_burst': options.burstSize,
        ..._LatencyStatistics(burstNanoseconds).toJson(),
      },
      'maximum_work_receipt': work.toJson(),
    };
  } finally {
    await runtime.close().timeout(const Duration(seconds: 30));
  }
}

FlarkV3ExactDocumentOrdinalWindow _queryExact(
  FlarkV3DocumentRuntime runtime,
  FlarkV3DocumentOrdinalWindowDemand demand,
  FlarkV3DocumentOrdinalWindowBudget budget, {
  required int expectedTotal,
}) {
  final result = runtime.queryBlockOrdinalWindow(demand, budget: budget);
  if (result is! FlarkV3ExactDocumentOrdinalWindow) {
    throw StateError(
      'Ordinal ${demand.startBlockOrdinal} failed with '
      '${(result as FlarkV3UnavailableDocumentOrdinalWindow).reason}.',
    );
  }
  final expectedNext =
      demand.startBlockOrdinal + budget.maximumEntries > expectedTotal
      ? expectedTotal
      : demand.startBlockOrdinal + budget.maximumEntries;
  if (result.totalBlockCount != expectedTotal ||
      result.startBlockOrdinal != demand.startBlockOrdinal ||
      result.nextBlockOrdinal != expectedNext) {
    throw StateError(
      'Unexpected window '
      '${result.startBlockOrdinal}..${result.nextBlockOrdinal} '
      'of ${result.totalBlockCount}.',
    );
  }
  return result;
}

String _headingDocument(int count) {
  final output = StringBuffer();
  for (var ordinal = 0; ordinal < count; ordinal += 1) {
    if (ordinal != 0) output.write('\n');
    output
      ..write('# heading ')
      ..write(ordinal);
  }
  return output.toString();
}

List<int> _distantStarts(int total, int window) {
  final lastFull = total > window ? total - window : 0;
  return <int>{
        0,
        total ~/ 7,
        total ~/ 4,
        total ~/ 2,
        (total * 3) ~/ 4,
        (total * 6) ~/ 7,
        lastFull,
      }
      .map((value) => value.clamp(0, lastFull))
      .cast<int>()
      .toList(growable: false);
}

int _ticksToNanoseconds(int ticks) =>
    (ticks * Duration.microsecondsPerSecond * 1000) ~/ Stopwatch().frequency;

final class _LatencyStatistics {
  _LatencyStatistics(List<int> samples)
    : _sorted = List<int>.from(samples)..sort();

  final List<int> _sorted;

  int _percentile(int percentile) =>
      _sorted[((_sorted.length - 1) * percentile) ~/ 100];

  Map<String, Object?> toJson() => <String, Object?>{
    'samples': _sorted.length,
    'p50_us': _microseconds(_percentile(50)),
    'p95_us': _microseconds(_percentile(95)),
    'p99_us': _microseconds(_percentile(99)),
    'max_us': _microseconds(_sorted.last),
  };
}

double _microseconds(int nanoseconds) =>
    (nanoseconds / 1000 * 1000).round() / 1000;

final class _MaximumWorkReceipt {
  int storagePagesVisited = 0;
  int treeNodesVisited = 0;
  int packedEntriesInspected = 0;
  int summaryNodesSkipped = 0;

  void observe(FlarkV3ExactDocumentOrdinalWindow window) {
    if (window.storagePagesVisited > storagePagesVisited) {
      storagePagesVisited = window.storagePagesVisited;
    }
    if (window.treeNodesVisited > treeNodesVisited) {
      treeNodesVisited = window.treeNodesVisited;
    }
    if (window.packedEntriesInspected > packedEntriesInspected) {
      packedEntriesInspected = window.packedEntriesInspected;
    }
    if (window.summaryNodesSkipped > summaryNodesSkipped) {
      summaryNodesSkipped = window.summaryNodesSkipped;
    }
  }

  Map<String, Object?> toJson() => <String, Object?>{
    'storage_pages_visited': storagePagesVisited,
    'tree_nodes_visited': treeNodesVisited,
    'packed_entries_inspected': packedEntriesInspected,
    'summary_nodes_skipped': summaryNodesSkipped,
  };
}

final class _Options {
  const _Options({
    required this.sizes,
    required this.samples,
    required this.warmupSamples,
    required this.burstSamples,
    required this.burstSize,
    required this.windowEntries,
    required this.nativeLibraryPath,
  });

  factory _Options.parse(List<String> arguments) {
    var sizes = const <int>[4096, 50000];
    var samples = 2000;
    var warmupSamples = 250;
    var burstSamples = 500;
    var burstSize = 3;
    var windowEntries = 97;
    String? nativeLibraryPath;

    for (final argument in arguments) {
      final parts = argument.split('=');
      if (parts.length != 2 || !parts.first.startsWith('--')) {
        throw FormatException('Expected --name=value, received $argument.');
      }
      switch (parts.first) {
        case '--sizes':
          sizes = parts.last.split(',').map(int.parse).toList(growable: false);
        case '--samples':
          samples = int.parse(parts.last);
        case '--warmup':
          warmupSamples = int.parse(parts.last);
        case '--burst-samples':
          burstSamples = int.parse(parts.last);
        case '--burst-size':
          burstSize = int.parse(parts.last);
        case '--window':
          windowEntries = int.parse(parts.last);
        case '--native-library':
          nativeLibraryPath = parts.last;
        default:
          throw FormatException('Unknown argument ${parts.first}.');
      }
    }
    if (sizes.isEmpty ||
        sizes.any((size) => size <= 0) ||
        samples <= 0 ||
        warmupSamples < 0 ||
        burstSamples <= 0 ||
        burstSize <= 0 ||
        windowEntries <= 0 ||
        windowEntries > 4096) {
      throw RangeError('Benchmark counts and sizes must be positive.');
    }
    return _Options(
      sizes: sizes,
      samples: samples,
      warmupSamples: warmupSamples,
      burstSamples: burstSamples,
      burstSize: burstSize,
      windowEntries: windowEntries,
      nativeLibraryPath: nativeLibraryPath,
    );
  }

  final List<int> sizes;
  final int samples;
  final int warmupSamples;
  final int burstSamples;
  final int burstSize;
  final int windowEntries;
  final String? nativeLibraryPath;

  Map<String, Object?> toJson() => <String, Object?>{
    'sizes': sizes,
    'samples': samples,
    'warmup_samples': warmupSamples,
    'burst_samples': burstSamples,
    'burst_size': burstSize,
    'window_entries': windowEntries,
  };
}

String? _defaultNativeLibraryPath() {
  final candidates = switch (Platform.operatingSystem) {
    'macos' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
    ],
    'linux' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
    ],
    'windows' => const <String>[
      'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
    ],
    _ => const <String>[],
  };
  for (final candidate in candidates) {
    if (File(candidate).existsSync()) return candidate;
  }
  return null;
}
