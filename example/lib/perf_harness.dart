// Profile-mode frame-timing harness for live-rendered editing.
//
// Unlike the widget-test benchmarks (debug VM, `pump()` wall-clock), this runs
// the real engine and captures FrameTiming.buildDuration / rasterDuration via
// addTimingsCallback while scripting one edit per frame. Run it in PROFILE mode
// so the numbers reflect AOT + real raster, not debug:
//
//   cd example
//   flutter run --profile -d macos -t lib/perf_harness.dart \
//     --dart-define=FLARK_PROFILE_BLOCKS=40
//
// It prints a `flark_profile ...` line and exits. Vary FLARK_PROFILE_BLOCKS
// (10/20/40/80) to see scaling. This is the on-device validation the
// debug benchmarks cannot provide (see doc/architecture/live_rendered_rebuild_isolation.md).

import 'dart:io';
import 'dart:ui' show FrameTiming;

import 'package:flark/flark.dart';
import 'package:flutter/material.dart';

const _blockCount = int.fromEnvironment(
  'FLARK_PROFILE_BLOCKS',
  defaultValue: 40,
);
// 'end' (default) inserts near the document end — realistic top-down typing,
// where blocks before the cursor are reused. 'start' inserts at the top — the
// worst case, where every later block's offsets shift and it must rebuild.
const _editPosition = String.fromEnvironment(
  'FLARK_PROFILE_EDIT',
  defaultValue: 'end',
);
const _warmupEdits = 20;
const _measuredEdits = 120;

void main() {
  runApp(const _PerfHarnessApp());
}

class _PerfHarnessApp extends StatelessWidget {
  const _PerfHarnessApp();

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      home: Scaffold(body: _PerfHarness(blockCount: _blockCount)),
    );
  }
}

class _PerfHarness extends StatefulWidget {
  const _PerfHarness({required this.blockCount});

  final int blockCount;

  @override
  State<_PerfHarness> createState() => _PerfHarnessState();
}

class _PerfHarnessState extends State<_PerfHarness> {
  late final FlarkFlutterController _controller;
  final _buildMicros = <int>[];
  final _rasterMicros = <int>[];
  int _editCount = 0;
  bool _started = false;

  @override
  void initState() {
    super.initState();
    _controller = FlarkFlutterController.fromMarkdown(
      _taskListMarkdown(widget.blockCount),
      parseDebounce: Duration.zero,
    );
    WidgetsBinding.instance.addTimingsCallback(_onFrameTimings);
    WidgetsBinding.instance.addPostFrameCallback((_) => _waitForPlan());
  }

  void _onFrameTimings(List<FrameTiming> timings) {
    if (!_started) return;
    for (final timing in timings) {
      _buildMicros.add(timing.buildDuration.inMicroseconds);
      _rasterMicros.add(timing.rasterDuration.inMicroseconds);
    }
  }

  // Wait until the first authoritative parse lands so block widgets exist.
  void _waitForPlan() {
    if (!mounted) return;
    if (_controller.hasAuthoritativeRenderPlan) {
      _started = true;
      _editLoop();
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _waitForPlan());
  }

  // One edit per frame: apply a source insert (marks the tree dirty → schedules
  // a frame), then queue the next edit from the post-frame callback.
  void _editLoop() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_editCount >= _warmupEdits + _measuredEdits) {
        _report();
        return;
      }
      if (_editCount == _warmupEdits) {
        // Discard warmup frames; measure from here.
        _buildMicros.clear();
        _rasterMicros.clear();
      }
      final length = _controller.state.document.length;
      final offset = _editPosition == 'start'
          ? 5
          : (length - 3).clamp(0, length);
      _controller.applyTransaction(
        FlarkTransaction.single(
          FlarkSourceOperation.insert(offset, 'x'),
          metadata: const FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.input,
            userEvent: 'profile.insert',
          ),
        ),
      );
      _editCount += 1;
      _editLoop();
    });
  }

  void _report() {
    final build = _summary(_buildMicros);
    final raster = _summary(_rasterMicros);
    // ignore: avoid_print
    print(
      'flark_profile blocks=${widget.blockCount} edit=$_editPosition '
      'frames=${_buildMicros.length} '
      'build_median=${build.median} build_p95=${build.p95} '
      'raster_median=${raster.median} raster_p95=${raster.p95}',
    );
    WidgetsBinding.instance.addPostFrameCallback((_) => exit(0));
    setState(() {});
  }

  ({String median, String p95}) _summary(List<int> micros) {
    if (micros.isEmpty) return (median: 'n/a', p95: 'n/a');
    final sorted = [...micros]..sort();
    String fmt(int us) =>
        us < 1000 ? '${us}us' : '${(us / 1000).toStringAsFixed(2)}ms';
    return (
      median: fmt(sorted[sorted.length ~/ 2]),
      p95: fmt(sorted[((sorted.length - 1) * 0.95).ceil()]),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeTimingsCallback(_onFrameTimings);
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FlarkMarkdownEditor(
      controller: _controller,
      editingMode: FlarkMarkdownEditingMode.liveRendered,
      style: const TextStyle(fontSize: 14),
    );
  }
}

String _taskListMarkdown(int count) {
  final buffer = StringBuffer();
  for (var i = 0; i < count; i += 1) {
    buffer.writeln('- [ ] task item number $i with a little inline text');
  }
  return buffer.toString();
}
