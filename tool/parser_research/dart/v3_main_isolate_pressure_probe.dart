import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flark/src/v3/source/source.dart';

/// Disposable host-VM pressure probe for the Dart/UI boundary proposed by
/// RFC 023.
///
/// This is deliberately not a Flutter frame benchmark or a physical-device
/// receipt. It can falsify synchronous-work assumptions on the host, compare
/// JIT and AOT behavior, and expose document-size or payload-size scaling.
/// Run one lane per process so RSS readings and GC tails are easier to read:
///
///   dart run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
///     source --size-mib=10
///   dart run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
///     string --size-mib=10
///   dart run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
///     paste --paste-kib=1024 --base-mib=16
///   dart run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
///     transfer --payload-kib=1024
///   dart run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
///     worker-source --size-mib=10
Future<void> main(List<String> arguments) async {
  if (arguments.isEmpty || arguments.first == 'help') {
    stderr.writeln(
      'usage: <source|string|paste|transfer|worker-source> '
      '[--size-mib=N] [--paste-kib=N] [--payload-kib=N] '
      '[--base-mib=N] [--iterations=N]',
    );
    exitCode = 64;
    return;
  }
  final options = _Options(arguments.skip(1));
  _emit('environment', {
    'lane': arguments.first,
    'dart': Platform.version.split('\n').first,
    'os': Platform.operatingSystem,
    'processors': Platform.numberOfProcessors,
    'stopwatch_frequency': _stopwatchFrequency,
    'rss_mib': _rssMiB(),
  });

  switch (arguments.first) {
    case 'source':
      await _runSourceLane(options);
    case 'string':
      await _runWholeStringLane(options);
    case 'paste':
      await _runPasteLane(options);
    case 'transfer':
      await _runTransferLane(options);
    case 'worker-source':
      await _runWorkerSourceLane(options);
    default:
      stderr.writeln('unknown lane: ${arguments.first}');
      exitCode = 64;
  }
  _emit('probe_complete', {'black_hole': _blackHole, 'rss_mib': _rssMiB()});
}

var _blackHole = 0;
final int _stopwatchFrequency = Stopwatch().frequency;

Future<void> _runSourceLane(_Options options) async {
  final sizeMiB = options.integer('size-mib', 10);
  final size = sizeMiB * 1024 * 1024;
  final iterations = options.integer(
    'iterations',
    sizeMiB >= 100 ? 300 : (sizeMiB >= 10 ? 1000 : 2500),
  );

  final generation = Stopwatch()..start();
  final source = _asciiMarkdownOfLength(size);
  generation.stop();
  final rssBeforeBuild = _rssMiB();
  final build = Stopwatch()..start();
  final base = FlarkV3SourceDocument.fromString(source);
  build.stop();
  final diagnostics = base.diagnostics;
  _blackHole ^= base.contentHash32;
  _emit('source_build', {
    'size_mib': sizeMiB,
    'utf16_units': base.utf16Length,
    'utf8_bytes': base.utf8Length,
    'source_generation_ms': _milliseconds(generation),
    'tree_build_ms': _milliseconds(build),
    'leaf_count': diagnostics.leafCount,
    'largest_leaf_utf16': diagnostics.largestLeafUtf16,
    'tree_height': diagnostics.treeHeight,
    'rss_before_build_mib': rssBeforeBuild,
    'rss_after_build_mib': _rssMiB(),
  });

  final positions = <String, int>{
    'start': 0,
    'middle': base.utf16Length ~/ 2,
    'end': base.utf16Length - 1,
  };
  for (final entry in positions.entries) {
    var document = base;
    var toggle = false;
    final samples = _measureSync(
      warmup: math.min(100, iterations ~/ 5),
      iterations: iterations,
      body: () {
        toggle = !toggle;
        final applied = document.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: document.revision,
            operation: FlarkV3SourceEdit(
              startUtf16: entry.value,
              endUtf16: entry.value + 1,
              replacement: toggle ? '\u0001' : '\u0002',
            ),
          ),
        );
        document = applied.document;
        _blackHole ^=
            document.contentHash32 ^ (applied.parserBatch?.wireBytes ?? 0);
      },
    );
    _emit('persistent_local_edit', {
      'size_mib': sizeMiB,
      'position': entry.key,
      'iterations': iterations,
      ...samples.json,
      'final_height': document.diagnostics.treeHeight,
      'final_leaves': document.diagnostics.leafCount,
      'rss_mib': _rssMiB(),
    });
  }

  var coldSeed = 0x2468ACE;
  final coldIterations = math.min(iterations, 1000);
  final coldSamples = _measureSync(
    warmup: 20,
    iterations: coldIterations,
    body: () {
      coldSeed = _next(coldSeed);
      final offset = coldSeed % base.utf16Length;
      final applied = base.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: base.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: offset,
            endUtf16: offset + 1,
            replacement: '\u0007',
          ),
        ),
      );
      _blackHole ^= applied.document.contentHash32;
    },
  );
  _emit('persistent_cold_random_edit', {
    'size_mib': sizeMiB,
    'iterations': coldIterations,
    ...coldSamples.json,
    'rss_mib': _rssMiB(),
  });

  final typingIterations = options.integer('typing-iterations', 8192);
  var typingDocument = base;
  var caret = base.utf16Length ~/ 2;
  var typingToggle = false;
  final typingSamples = _measureSync(
    warmup: 0,
    iterations: typingIterations,
    body: () {
      typingToggle = !typingToggle;
      final applied = typingDocument.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: typingDocument.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: caret,
            endUtf16: caret,
            replacement: typingToggle ? 'x' : 'y',
          ),
        ),
      );
      typingDocument = applied.document;
      caret += 1;
      _blackHole ^= typingDocument.contentHash32;
    },
  );
  _emit('persistent_advancing_typing', {
    'size_mib': sizeMiB,
    'iterations': typingIterations,
    ...typingSamples.json,
    'final_height': typingDocument.diagnostics.treeHeight,
    'final_leaves': typingDocument.diagnostics.leafCount,
    'rss_mib': _rssMiB(),
  });

  final backspaceSamples = _measureSync(
    warmup: 0,
    iterations: typingIterations,
    body: () {
      final applied = typingDocument.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: typingDocument.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: caret - 1,
            endUtf16: caret,
            replacement: '',
          ),
        ),
      );
      typingDocument = applied.document;
      caret -= 1;
      _blackHole ^= typingDocument.contentHash32;
    },
  );
  _emit('persistent_backspace_burst', {
    'size_mib': sizeMiB,
    'iterations': typingIterations,
    ...backspaceSamples.json,
    'final_height': typingDocument.diagnostics.treeHeight,
    'final_leaves': typingDocument.diagnostics.leafCount,
    'rss_mib': _rssMiB(),
  });

  for (final operationCount in const [4, 16, 64]) {
    var document = base;
    var toggle = false;
    final operations = <int>[
      for (var index = 0; index < operationCount; index += 1)
        ((index + 1) * base.utf16Length) ~/ (operationCount + 1),
    ];
    final caseIterations = options.integer(
      'iterations',
      operationCount == 64 ? 100 : math.min(iterations, 500),
    );
    final samples = _measureSync(
      warmup: math.min(20, caseIterations ~/ 5),
      iterations: caseIterations,
      body: () {
        toggle = !toggle;
        final applied = document.apply(
          FlarkV3SourceTransaction(
            baseRevision: document.revision,
            operations: [
              for (final offset in operations)
                FlarkV3SourceEdit(
                  startUtf16: offset,
                  endUtf16: offset + 1,
                  replacement: toggle ? '\u0003' : '\u0004',
                ),
            ],
          ),
        );
        document = applied.document;
        _blackHole ^=
            document.contentHash32 ^ (applied.parserBatch?.wireBytes ?? 0);
      },
    );
    _emit('persistent_multi_edit', {
      'size_mib': sizeMiB,
      'operations': operationCount,
      'iterations': caseIterations,
      ...samples.json,
      'wire_bytes':
          56 + operationCount * (12 + 1), // ASCII replacement payloads.
      'final_height': document.diagnostics.treeHeight,
      'final_leaves': document.diagnostics.leafCount,
      'rss_mib': _rssMiB(),
    });
  }

  var seed = 0x13579BDF;
  final mappingSamples = _measureSync(
    warmup: 50,
    iterations: 400,
    body: () {
      var checksum = 0;
      for (var index = 0; index < 128; index += 1) {
        seed = _next(seed);
        final utf16Offset = seed % (base.utf16Length + 1);
        final utf8Offset = base.utf16ToUtf8(utf16Offset);
        checksum ^= base.utf8ToUtf16(utf8Offset);
      }
      _blackHole ^= checksum;
    },
  );
  _emit('utf16_utf8_mapping', {
    'size_mib': sizeMiB,
    'roundtrips_per_sample': 128,
    'samples': 400,
    ...mappingSamples.json,
    'p50_ns_per_roundtrip': mappingSamples.p50Ns ~/ 128,
    'p99_ns_per_roundtrip': mappingSamples.p99Ns ~/ 128,
    'max_ns_per_roundtrip': mappingSamples.maxNs ~/ 128,
  });

  final unicodeSample = _unicodeOfLength(4096);
  final unicodeStart = (base.utf16Length ~/ 2) & ~4095;
  final unicodeDocument = base
      .apply(
        FlarkV3SourceTransaction.single(
          baseRevision: base.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: unicodeStart,
            endUtf16: unicodeStart + unicodeSample.length,
            replacement: unicodeSample,
          ),
        ),
      )
      .document;
  var unicodeSeed = 0x55AA7711;
  final unicodeMappingSamples = _measureSync(
    warmup: 50,
    iterations: 400,
    body: () {
      var checksum = 0;
      for (var index = 0; index < 128; index += 1) {
        unicodeSeed = _next(unicodeSeed);
        var localOffset = unicodeSeed % (unicodeSample.length + 1);
        while (!_isScalarBoundary(unicodeSample, localOffset)) {
          localOffset += 1;
        }
        final utf16Offset = unicodeStart + localOffset;
        final utf8Offset = unicodeDocument.utf16ToUtf8(utf16Offset);
        checksum ^= unicodeDocument.utf8ToUtf16(utf8Offset);
      }
      _blackHole ^= checksum;
    },
  );
  _emit('utf16_utf8_mapping_unicode_leaf', {
    'size_mib': sizeMiB,
    'unicode_leaf_utf16': unicodeSample.length,
    'roundtrips_per_sample': 128,
    'samples': 400,
    ...unicodeMappingSamples.json,
    'p50_ns_per_roundtrip': unicodeMappingSamples.p50Ns ~/ 128,
    'p99_ns_per_roundtrip': unicodeMappingSamples.p99Ns ~/ 128,
    'max_ns_per_roundtrip': unicodeMappingSamples.maxNs ~/ 128,
  });

  var eventDocument = base;
  final eventLoop = await _blockedTimerReceipt(() {
    final offset = eventDocument.utf16Length ~/ 2;
    eventDocument = eventDocument
        .apply(
          FlarkV3SourceTransaction.single(
            baseRevision: eventDocument.revision,
            operation: FlarkV3SourceEdit(
              startUtf16: offset,
              endUtf16: offset + 1,
              replacement: '\u0005',
            ),
          ),
        )
        .document;
  });
  _emit('persistent_edit_event_loop', {'size_mib': sizeMiB, ...eventLoop});
}

Future<void> _runWholeStringLane(_Options options) async {
  final sizeMiB = options.integer('size-mib', 10);
  final size = sizeMiB * 1024 * 1024;
  final source = _asciiMarkdownOfLength(size);
  _blackHole ^= source.length;
  final iterations = options.integer(
    'iterations',
    sizeMiB >= 100 ? 8 : (sizeMiB >= 10 ? 40 : 200),
  );

  for (final entry in <String, int>{
    'start': 0,
    'middle': source.length ~/ 2,
    'end': source.length - 1,
  }.entries) {
    var toggle = false;
    String? latest;
    final samples = _measureSync(
      warmup: sizeMiB >= 100 ? 0 : 3,
      iterations: iterations,
      body: () {
        toggle = !toggle;
        latest = source.replaceRange(
          entry.value,
          entry.value + 1,
          toggle ? '\u0001' : '\u0002',
        );
        _blackHole ^= latest!.codeUnitAt(entry.value) ^ latest!.length;
      },
    );
    _emit('whole_string_local_replace', {
      'size_mib': sizeMiB,
      'position': entry.key,
      'iterations': iterations,
      ...samples.json,
      'allocated_output_code_units_per_edit': source.length,
      'rss_mib': _rssMiB(),
    });
  }

  final eventLoop = await _blockedTimerReceipt(() {
    final offset = source.length ~/ 2;
    final next = source.replaceRange(offset, offset + 1, '\u0006');
    _blackHole ^= next.codeUnitAt(offset) ^ next.length;
  });
  _emit('whole_string_event_loop', {'size_mib': sizeMiB, ...eventLoop});
}

Future<void> _runPasteLane(_Options options) async {
  final pasteKiB = options.integer('paste-kib', 1024);
  final pasteLength = pasteKiB * 1024;
  final baseMiB = options.integer(
    'base-mib',
    math.max(16, (pasteLength / (1024 * 1024)).ceil() + 2),
  );
  final baseSource = _asciiMarkdownOfLength(baseMiB * 1024 * 1024);
  final base = FlarkV3SourceDocument.fromString(baseSource);

  final payloadBuild = Stopwatch()..start();
  final payload = _payloadOfLength(pasteLength);
  payloadBuild.stop();
  final encode = Stopwatch()..start();
  final encoded = Uint8List.fromList(utf8.encode(payload));
  encode.stop();
  final transferableBuild = Stopwatch()..start();
  final transferable = TransferableTypedData.fromList([encoded]);
  transferableBuild.stop();
  _blackHole ^=
      payload.codeUnitAt(payload.length - 1) ^
      encoded[encoded.length - 1] ^
      transferable.hashCode;

  _emit('paste_payload_prepare', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    'payload_utf16': payload.length,
    'payload_utf8': encoded.length,
    'string_build_ms': _milliseconds(payloadBuild),
    'utf8_encode_ms': _milliseconds(encode),
    'transferable_from_list_ms': _milliseconds(transferableBuild),
    'rss_mib': _rssMiB(),
  });

  final iterations = options.integer(
    'iterations',
    pasteKiB >= 10 * 1024 ? 3 : (pasteKiB >= 1024 ? 10 : 100),
  );
  final warmup = pasteKiB >= 10 * 1024 ? 0 : (pasteKiB >= 1024 ? 1 : 5);
  final insertionOffset = base.utf16Length ~/ 2;
  FlarkV3AppliedSourceTransaction? latestInsertion;
  final persistentInsert = _measureSync(
    warmup: warmup,
    iterations: iterations,
    body: () {
      latestInsertion = base.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: base.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: insertionOffset,
            endUtf16: insertionOffset,
            replacement: payload,
          ),
        ),
      );
      _blackHole ^=
          latestInsertion!.document.contentHash32 ^
          latestInsertion!.parserBatch!.wireBytes;
    },
  );
  _emit('persistent_paste_insert', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    'iterations': iterations,
    ...persistentInsert.json,
    'encoded_bytes_receipt':
        latestInsertion!.sourceWork.replacementUtf8BytesEncoded,
    'encoded_chunks_receipt':
        latestInsertion!.sourceWork.replacementChunksEncoded,
    'parser_wire_bytes': latestInsertion!.parserBatch!.wireBytes,
    'result_leaves': latestInsertion!.document.diagnostics.leafCount,
    'rss_mib': _rssMiB(),
  });

  final replacementLength = math.min(pasteLength, base.utf16Length - 2);
  final replacementStart = (base.utf16Length - replacementLength) ~/ 2;
  final replacementPayload = replacementLength == payload.length
      ? payload
      : payload.substring(0, replacementLength);
  FlarkV3AppliedSourceTransaction? latestReplacement;
  final persistentReplace = _measureSync(
    warmup: warmup,
    iterations: iterations,
    body: () {
      latestReplacement = base.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: base.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: replacementStart,
            endUtf16: replacementStart + replacementLength,
            replacement: replacementPayload,
          ),
        ),
      );
      _blackHole ^=
          latestReplacement!.document.contentHash32 ^
          latestReplacement!.parserBatch!.wireBytes;
    },
  );
  _emit('persistent_paste_replace', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    'replaced_utf16': replacementLength,
    'iterations': iterations,
    ...persistentReplace.json,
    'no_op_compared_utf16': latestReplacement!.sourceWork.noOpComparedUtf16,
    'encoded_bytes_receipt':
        latestReplacement!.sourceWork.replacementUtf8BytesEncoded,
    'parser_wire_bytes': latestReplacement!.parserBatch!.wireBytes,
    'result_leaves': latestReplacement!.document.diagnostics.leafCount,
    'rss_mib': _rssMiB(),
  });

  final identicalPayload = baseSource.substring(
    replacementStart,
    replacementStart + replacementLength,
  );
  FlarkV3AppliedSourceTransaction? latestNoOp;
  final identicalNoOp = _measureSync(
    warmup: warmup,
    iterations: iterations,
    body: () {
      latestNoOp = base.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: base.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: replacementStart,
            endUtf16: replacementStart + replacementLength,
            replacement: identicalPayload,
          ),
        ),
      );
      _blackHole ^=
          latestNoOp!.document.contentHash32 ^
          latestNoOp!.sourceWork.noOpComparedUtf16;
    },
  );
  _emit('persistent_identical_replacement_noop', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    'compared_utf16': latestNoOp!.sourceWork.noOpComparedUtf16,
    'iterations': iterations,
    ...identicalNoOp.json,
    'changed': latestNoOp!.changed,
    'rss_mib': _rssMiB(),
  });

  String? latestString;
  final wholeStringInsert = _measureSync(
    warmup: warmup,
    iterations: iterations,
    body: () {
      latestString = baseSource.replaceRange(
        insertionOffset,
        insertionOffset,
        payload,
      );
      _blackHole ^= latestString!.length;
    },
  );
  _emit('whole_string_paste_insert', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    'iterations': iterations,
    ...wholeStringInsert.json,
    'allocated_output_code_units': baseSource.length + payload.length,
    'rss_mib': _rssMiB(),
  });

  final persistentEventLoop = await _blockedTimerReceipt(() {
    final applied = base.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: base.revision,
        operation: FlarkV3SourceEdit(
          startUtf16: insertionOffset,
          endUtf16: insertionOffset,
          replacement: payload,
        ),
      ),
    );
    _blackHole ^= applied.document.contentHash32;
  });
  _emit('persistent_paste_event_loop', {
    'paste_kib': pasteKiB,
    'base_mib': baseMiB,
    ...persistentEventLoop,
  });
}

Future<void> _runTransferLane(_Options options) async {
  final payloadKiB = options.integer('payload-kib', 1024);
  final payload = _payloadOfLength(payloadKiB * 1024);
  final bytes = Uint8List.fromList(utf8.encode(payload));
  final worker = await _ProbeWorker.start();
  final iterations = options.integer(
    'iterations',
    payloadKiB >= 10 * 1024 ? 5 : (payloadKiB >= 1024 ? 20 : 500),
  );
  final warmup = payloadKiB >= 1024 ? 1 : 20;

  for (final kind in const [
    'bytes',
    'transferable',
    'string',
    'worker-encode',
  ]) {
    final sendSamples = <int>[];
    final prepareSamples = <int>[];
    final roundTripSamples = <int>[];
    for (var index = -warmup; index < iterations; index += 1) {
      final roundTrip = Stopwatch()..start();
      Object messagePayload;
      final prepare = Stopwatch()..start();
      if (kind == 'transferable') {
        messagePayload = TransferableTypedData.fromList([bytes]);
      } else {
        messagePayload = kind == 'bytes' ? bytes : payload;
      }
      prepare.stop();
      final send = Stopwatch()..start();
      final reply = worker.request(kind, messagePayload);
      send.stop();
      final result = await reply;
      roundTrip.stop();
      _blackHole ^= result;
      if (index >= 0) {
        prepareSamples.add(_nanoseconds(prepare));
        sendSamples.add(_nanoseconds(send));
        roundTripSamples.add(_nanoseconds(roundTrip));
      }
    }
    final prepareStats = _Samples(prepareSamples);
    final sendStats = _Samples(sendSamples);
    final roundTripStats = _Samples(roundTripSamples);
    _emit('isolate_payload_roundtrip', {
      'kind': kind,
      'payload_kib': payloadKiB,
      'iterations': iterations,
      'prepare_p50_us': prepareStats.p50Ns / 1000,
      'prepare_p99_us': prepareStats.p99Ns / 1000,
      'prepare_max_us': prepareStats.maxNs / 1000,
      'send_call_p50_us': sendStats.p50Ns / 1000,
      'send_call_p99_us': sendStats.p99Ns / 1000,
      'send_call_max_us': sendStats.maxNs / 1000,
      'roundtrip_p50_us': roundTripStats.p50Ns / 1000,
      'roundtrip_p99_us': roundTripStats.p99Ns / 1000,
      'roundtrip_max_us': roundTripStats.maxNs / 1000,
      'rss_mib': _rssMiB(),
    });
  }

  final heartbeat = _Heartbeat()..start();
  await Future<void>.delayed(const Duration(milliseconds: 20));
  heartbeat.resetMaximum();
  final transferable = TransferableTypedData.fromList([bytes]);
  await worker.request('transferable', transferable);
  await Future<void>.delayed(const Duration(milliseconds: 10));
  heartbeat.stop();
  _emit('isolate_transfer_heartbeat', {
    'payload_kib': payloadKiB,
    'period_us': heartbeat.period.inMicroseconds,
    'max_tick_gap_us': heartbeat.maximumGap.inMicroseconds,
    'ticks': heartbeat.ticks,
  });

  await worker.close();
}

Future<void> _runWorkerSourceLane(_Options options) async {
  final sizeMiB = options.integer('size-mib', 10);
  String? source = _asciiMarkdownOfLength(sizeMiB * 1024 * 1024);
  final worker = await _ProbeWorker.start();
  // Remove first-message serializer/port initialization from the document
  // handoff receipt.
  await worker.request('string', 'warm');
  final heartbeat = _Heartbeat()..start();
  await Future<void>.delayed(const Duration(milliseconds: 20));
  heartbeat.resetMaximum();
  final send = Stopwatch()..start();
  final initialized = worker.request('source-init', source);
  send.stop();
  final initRoundTrip = Stopwatch()..start();
  final workerBuildMicros = await initialized;
  initRoundTrip.stop();
  await Future<void>.delayed(const Duration(milliseconds: 10));
  _emit('worker_source_build', {
    'size_mib': sizeMiB,
    'send_call_us': _microseconds(send),
    'roundtrip_ms': _milliseconds(initRoundTrip),
    'worker_reported_build_ms': workerBuildMicros / 1000,
    'main_max_tick_gap_us': heartbeat.maximumGap.inMicroseconds,
    'rss_mib': _rssMiB(),
  });
  source = null;

  final iterations = options.integer(
    'iterations',
    sizeMiB >= 100 ? 200 : (sizeMiB >= 10 ? 500 : 1000),
  );
  final sendSamples = <int>[];
  final roundTripSamples = <int>[];
  heartbeat.resetMaximum();
  for (var index = -50; index < iterations; index += 1) {
    final roundTrip = Stopwatch()..start();
    final sendOne = Stopwatch()..start();
    final reply = worker.request('source-edit', [
      index.isEven ? '\u0001' : '\u0002',
      1,
      2,
    ]);
    sendOne.stop();
    final result = await reply;
    roundTrip.stop();
    _blackHole ^= result;
    if (index >= 0) {
      sendSamples.add(_nanoseconds(sendOne));
      roundTripSamples.add(_nanoseconds(roundTrip));
    }
  }
  await Future<void>.delayed(const Duration(milliseconds: 10));
  heartbeat.stop();
  final sendStats = _Samples(sendSamples);
  final roundTripStats = _Samples(roundTripSamples);
  _emit('worker_source_local_edit', {
    'size_mib': sizeMiB,
    'iterations': iterations,
    'send_p50_us': sendStats.p50Ns / 1000,
    'send_p99_us': sendStats.p99Ns / 1000,
    'send_max_us': sendStats.maxNs / 1000,
    'roundtrip_p50_us': roundTripStats.p50Ns / 1000,
    'roundtrip_p99_us': roundTripStats.p99Ns / 1000,
    'roundtrip_max_us': roundTripStats.maxNs / 1000,
    'main_max_tick_gap_us': heartbeat.maximumGap.inMicroseconds,
    'rss_mib': _rssMiB(),
  });
  await worker.close();
}

final class _ProbeWorker {
  _ProbeWorker._(this._isolate, this._send, this._replies, this._iterator);

  final Isolate _isolate;
  final SendPort _send;
  final ReceivePort _replies;
  final StreamIterator<Object?> _iterator;
  var _nextId = 0;

  static Future<_ProbeWorker> start() async {
    final replies = ReceivePort();
    final isolate = await Isolate.spawn(_workerMain, replies.sendPort);
    final iterator = StreamIterator<Object?>(replies);
    await iterator.moveNext();
    final send = iterator.current! as SendPort;
    return _ProbeWorker._(isolate, send, replies, iterator);
  }

  Future<int> request(String kind, Object? payload) async {
    final id = _nextId++;
    _send.send([id, kind, payload, _replies.sendPort]);
    await _iterator.moveNext();
    final reply = _iterator.current! as List<Object?>;
    if (reply[0] != id) {
      throw StateError('out-of-order isolate reply: ${reply[0]} != $id');
    }
    return reply[1]! as int;
  }

  Future<void> close() async {
    _send.send(null);
    await _iterator.cancel();
    _replies.close();
    _isolate.kill();
  }
}

void _workerMain(SendPort ready) {
  final requests = ReceivePort();
  FlarkV3SourceDocument? document;
  ready.send(requests.sendPort);
  requests.listen((message) {
    if (message == null) {
      requests.close();
      return;
    }
    final request = message as List<Object?>;
    final id = request[0]! as int;
    final kind = request[1]! as String;
    final payload = request[2];
    final reply = request[3]! as SendPort;
    var result = 0;
    switch (kind) {
      case 'bytes':
        final bytes = payload! as Uint8List;
        result = bytes.length ^ bytes.first ^ bytes.last;
      case 'transferable':
        final bytes = (payload! as TransferableTypedData)
            .materialize()
            .asUint8List();
        result = bytes.length ^ bytes.first ^ bytes.last;
      case 'string':
        final source = payload! as String;
        result =
            source.length ^
            source.codeUnitAt(0) ^
            source.codeUnitAt(source.length - 1);
      case 'worker-encode':
        final source = payload! as String;
        final bytes = utf8.encode(source);
        result = bytes.length ^ bytes.first ^ bytes.last;
      case 'source-init':
        final stopwatch = Stopwatch()..start();
        document = FlarkV3SourceDocument.fromString(payload! as String);
        stopwatch.stop();
        result = stopwatch.elapsedMicroseconds;
      case 'source-edit':
        final edit = payload! as List<Object?>;
        final current = document!;
        // Keep this at the middle without sending a document-sized coordinate
        // or text snapshot back to the main isolate.
        final offset = current.utf16Length ~/ 2;
        final applied = current.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: current.revision,
            operation: FlarkV3SourceEdit(
              startUtf16: offset,
              endUtf16: offset + (edit[1]! as int),
              replacement: edit[0]! as String,
            ),
          ),
        );
        document = applied.document;
        result = document!.contentHash32 ^ applied.parserBatch!.wireBytes;
      default:
        throw StateError('unknown worker request $kind');
    }
    reply.send([id, result]);
  });
}

final class _Heartbeat {
  final period = const Duration(milliseconds: 1);
  Timer? _timer;
  int _lastTicks = 0;
  int _maximumGapTicks = 0;
  int ticks = 0;

  Duration get maximumGap => Duration(
    microseconds: (_maximumGapTicks * 1000000) ~/ _stopwatchFrequency,
  );

  void start() {
    final stopwatch = Stopwatch()..start();
    _lastTicks = stopwatch.elapsedTicks;
    _timer = Timer.periodic(period, (_) {
      final now = stopwatch.elapsedTicks;
      final gap = now - _lastTicks;
      if (gap > _maximumGapTicks) _maximumGapTicks = gap;
      _lastTicks = now;
      ticks += 1;
    });
  }

  void resetMaximum() => _maximumGapTicks = 0;

  void stop() => _timer?.cancel();
}

Future<Map<String, Object>> _blockedTimerReceipt(void Function() body) async {
  final completer = Completer<int>();
  final timer = Stopwatch()..start();
  Timer.run(() {
    timer.stop();
    completer.complete(_microseconds(timer));
  });
  final synchronous = Stopwatch()..start();
  body();
  synchronous.stop();
  return {
    'synchronous_work_us': _microseconds(synchronous),
    'timer_observed_delay_us': await completer.future,
  };
}

_Samples _measureSync({
  required int warmup,
  required int iterations,
  required void Function() body,
}) {
  for (var index = 0; index < warmup; index += 1) {
    body();
  }
  final samples = <int>[];
  for (var index = 0; index < iterations; index += 1) {
    final stopwatch = Stopwatch()..start();
    body();
    stopwatch.stop();
    samples.add(_nanoseconds(stopwatch));
  }
  return _Samples(samples);
}

final class _Samples {
  _Samples(List<int> values) : _values = [...values]..sort() {
    if (_values.isEmpty) throw ArgumentError.value(values, 'values');
  }

  final List<int> _values;

  int get p50Ns => _percentile(50);
  int get p95Ns => _percentile(95);
  int get p99Ns => _percentile(99);
  int get maxNs => _values.last;

  int _percentile(int percentile) =>
      _values[((_values.length - 1) * percentile) ~/ 100];

  Map<String, Object> get json => {
    'p50_us': p50Ns / 1000,
    'p95_us': p95Ns / 1000,
    'p99_us': p99Ns / 1000,
    'max_us': maxNs / 1000,
  };
}

final class _Options {
  _Options(Iterable<String> arguments) {
    for (final argument in arguments) {
      if (!argument.startsWith('--') || !argument.contains('=')) {
        throw FormatException('invalid option $argument');
      }
      final separator = argument.indexOf('=');
      _values[argument.substring(2, separator)] = argument.substring(
        separator + 1,
      );
    }
  }

  final Map<String, String> _values = {};

  int integer(String name, int fallback) =>
      _values[name] == null ? fallback : int.parse(_values[name]!);
}

String _asciiMarkdownOfLength(int length) {
  const line =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final chunk = StringBuffer();
  while (chunk.length < 64 * 1024) {
    chunk.write(line);
  }
  final chunkText = chunk.toString();
  final fullChunks = length ~/ chunkText.length;
  final remainder = length % chunkText.length;
  return '${List<String>.filled(fullChunks, chunkText).join()}'
      '${chunkText.substring(0, remainder)}';
}

String _payloadOfLength(int length) {
  const chunk =
      'PASTE **bold** and `code` with a unicode-free payload for exact bytes.\n';
  final fullChunks = length ~/ chunk.length;
  final remainder = length % chunk.length;
  return '${List<String>.filled(fullChunks, chunk).join()}'
      '${chunk.substring(0, remainder)}';
}

String _unicodeOfLength(int length) {
  const chunk = 'a😀éβ𝄞z e\u0301 🇨🇦\n';
  final output = StringBuffer();
  while (output.length + chunk.length <= length) {
    output.write(chunk);
  }
  while (output.length < length) {
    output.write('a');
  }
  return output.toString();
}

bool _isScalarBoundary(String source, int offset) {
  if (offset <= 0 || offset >= source.length) return true;
  final previous = source.codeUnitAt(offset - 1);
  final current = source.codeUnitAt(offset);
  return !(previous >= 0xD800 &&
      previous <= 0xDBFF &&
      current >= 0xDC00 &&
      current <= 0xDFFF);
}

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

int _nanoseconds(Stopwatch stopwatch) =>
    (stopwatch.elapsedTicks * 1000000000) ~/ _stopwatchFrequency;

int _microseconds(Stopwatch stopwatch) =>
    (stopwatch.elapsedTicks * 1000000) ~/ _stopwatchFrequency;

double _milliseconds(Stopwatch stopwatch) =>
    stopwatch.elapsedTicks * 1000 / _stopwatchFrequency;

int _rssMiB() => (ProcessInfo.currentRss / (1024 * 1024)).round();

void _emit(String receipt, Map<String, Object?> values) {
  stdout.writeln(jsonEncode({'receipt': receipt, ...values}));
}
