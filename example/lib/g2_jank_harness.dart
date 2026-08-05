// RFC 024 gate G2 — the jank harness.
//
// Measures real Flutter FrameTiming during sustained synthetic typing into the
// real v3 document runtime behind the real virtualized editing surface, at
// four document sizes and two document shapes. It also measures cold open
// (open -> first painted document content, open -> structure-current).
//
// The metric is FrameTiming.buildDuration / rasterDuration / totalSpan — NOT
// wall clock around apply(). Wall clock around apply() cannot see raster, and
// cannot see the frames that the edit schedules afterwards.
//
// ---------------------------------------------------------------------------
// How to run
// ---------------------------------------------------------------------------
//
// macOS (what this file was validated on):
//
//   cd example
//   flutter run --profile -d macos -t lib/g2_jank_harness.dart
//
// Android (physical device; profile mode is required for meaningful numbers):
//
//   cd example
//   flutter run --profile -d <android-device-id> -t lib/g2_jank_harness.dart
//
// iOS (physical device):
//
//   cd example
//   flutter run --profile -d <ios-device-id> -t lib/g2_jank_harness.dart
//
// No code change is required for any of the three. Nothing here touches
// dart:io paths, window sizing, desktop-only plugins, or platform channels.
// `exit(0)` at the end is the only platform-sensitive call; pass
// `--dart-define=FLARK_G2_EXIT=false` on a phone if you would rather read the
// table on screen and stop the app yourself.
//
// Optional dart-defines:
//   FLARK_G2_SIZES_KB=5,25,100,1024   document sizes to sweep
//   FLARK_G2_SHAPES=dense,plain       document shapes to sweep
//   FLARK_G2_TYPE_SECONDS=15          measured typing window per configuration
//   FLARK_G2_WARMUP_SECONDS=2         unmeasured settle-in typing per config
//   FLARK_G2_CPS=10                   synthetic characters per second
//   FLARK_G2_EXIT=true                exit(0) once the table is printed
//
// ---------------------------------------------------------------------------
// What is real here and what is synthetic
// ---------------------------------------------------------------------------
//
// Real: FlarkV3DocumentRuntime (FFI on native, Wasm on web), the managed
// Flutter binding, the bounded query path, FlarkV3VirtualizedLiveSurface with
// its single live EditableText, and Flutter's own frame pipeline.
//
// Synthetic: the source of the keystrokes. There is no physical keyboard and
// no platform text-input plugin in the loop. Instead the harness hands a
// TextEditingDeltaInsertion to the live EditableText's own
// `updateEditingValueWithDeltas` — the exact method the macOS/Android/iOS text
// input plugin calls — so everything downstream of the platform channel is the
// production path (delta -> live controller -> bounded source transaction ->
// EditableText value adoption -> scheduled frame). See [_TypingDriver.mode].

import 'dart:async';
import 'dart:io' show exit;
import 'dart:math' as math;


import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const String _sizesKbDefine = String.fromEnvironment(
  'FLARK_G2_SIZES_KB',
  defaultValue: '5,25,100,1024',
);
const String _shapesDefine = String.fromEnvironment(
  'FLARK_G2_SHAPES',
  defaultValue: 'dense,plain',
);
const int _typeSeconds = int.fromEnvironment(
  'FLARK_G2_TYPE_SECONDS',
  defaultValue: 15,
);
const int _warmupSeconds = int.fromEnvironment(
  'FLARK_G2_WARMUP_SECONDS',
  defaultValue: 2,
);
const int _charactersPerSecond = int.fromEnvironment(
  'FLARK_G2_CPS',
  defaultValue: 10,
);
const bool _exitWhenDone = bool.fromEnvironment(
  'FLARK_G2_EXIT',
  defaultValue: true,
);

/// Document shapes under test.
enum G2Shape {
  /// Headings, tight bullet and ordered lists, and paragraphs carrying bold,
  /// emphasis, inline code and inline links.
  dense,

  /// Blank-line separated plain paragraphs and nothing else.
  plain;

  static G2Shape? parse(String raw) {
    for (final shape in G2Shape.values) {
      if (shape.name == raw.trim()) return shape;
    }
    return null;
  }
}

class G2Configuration {
  const G2Configuration({required this.shape, required this.targetBytes});

  final G2Shape shape;
  final int targetBytes;

  String get sizeLabel => targetBytes >= 1024 * 1024
      ? '${(targetBytes / (1024 * 1024)).toStringAsFixed(0)}MB'
      : '${(targetBytes / 1024).toStringAsFixed(0)}KB';

  String get label => '${shape.name}/$sizeLabel';
}

List<G2Configuration> _configurations() {
  final sizes = <int>[];
  for (final raw in _sizesKbDefine.split(',')) {
    final kb = int.tryParse(raw.trim());
    if (kb != null && kb > 0) sizes.add(kb * 1024);
  }
  final shapes = <G2Shape>[];
  for (final raw in _shapesDefine.split(',')) {
    final shape = G2Shape.parse(raw);
    if (shape != null) shapes.add(shape);
  }
  return <G2Configuration>[
    for (final shape in shapes)
      for (final bytes in sizes)
        G2Configuration(shape: shape, targetBytes: bytes),
  ];
}

// ---------------------------------------------------------------------------
// Document generation
// ---------------------------------------------------------------------------

/// A generated document plus the exact source range the caret starts in.
class G2Document {
  const G2Document({
    required this.markdown,
    required this.anchorStartUtf16,
    required this.anchorEndUtf16,
    required this.caretUtf16,
    required this.blockCount,
  });

  final String markdown;

  /// Exact source range of the paragraph the harness types into.
  final int anchorStartUtf16;
  final int anchorEndUtf16;

  /// Initial caret, inside a plain-text run of the anchor paragraph.
  final int caretUtf16;

  final int blockCount;

  int get utf16Length => markdown.length;
}

const List<String> _words = <String>[
  'ledger',
  'bounded',
  'parser',
  'source',
  'frame',
  'budget',
  'window',
  'exact',
  'stale',
  'never',
  'wrong',
  'engine',
  'incremental',
  'document',
  'structure',
  'inline',
  'paragraph',
  'anchor',
  'viewport',
  'certified',
  'authority',
  'projection',
  'canonical',
  'markdown',
  'render',
  'measure',
  'latency',
  'threshold',
  'contract',
  'evidence',
];

/// Deterministic word stream. No Markdown-significant bytes, ever.
String _sentence(math.Random random, int wordCount) {
  final buffer = StringBuffer();
  for (var i = 0; i < wordCount; i += 1) {
    if (i > 0) buffer.write(' ');
    buffer.write(_words[random.nextInt(_words.length)]);
  }
  buffer.write('.');
  return buffer.toString();
}

/// The paragraph the harness types into.
///
/// `ANCHOR` sits inside a plain-text run in both shapes, so the initial caret
/// is never inside a Markdown delimiter or a link destination.
String _anchorBlock(G2Shape shape) => switch (shape) {
  G2Shape.plain =>
    'This anchor paragraph is where the synthetic typist works. '
        'ANCHOR marks the plain text run holding the caret. '
        'The surrounding sentences keep the block a realistic width.',
  G2Shape.dense =>
    'This anchor paragraph carries **bold**, _emphasis_, `inline code` and '
        'a [documented link](https://example.test/anchor) so the inline '
        'projection has real work. ANCHOR marks the plain text run holding '
        'the caret, well away from any delimiter.',
};

/// Builds a blank-line separated document of at least [targetBytes] UTF-8
/// bytes, with the anchor paragraph placed as close to the middle as the block
/// stride allows.
///
/// Deliberately avoids every known engine fault shape: no reference
/// definitions anywhere, no physical line anywhere near 4 KiB, no lazy
/// continuation into an open list item.
G2Document buildDocument({required G2Shape shape, required int targetBytes}) {
  final random = math.Random(0x5EED ^ shape.index ^ targetBytes);
  final blocks = <String>[];
  var bytes = 0;

  String nextBlock(int index) {
    if (shape == G2Shape.plain) {
      return '${_sentence(random, 12)} ${_sentence(random, 14)} '
          '${_sentence(random, 11)}';
    }
    switch (index % 6) {
      case 0:
        return '## Section ${index ~/ 6} ${_words[index % _words.length]}';
      case 1:
        return 'A paragraph with **bold ${_words[index % _words.length]}**, '
            '_emphasis_, `inline code`, and a '
            '[link](https://example.test/${index % 997}) inside it. '
            '${_sentence(random, 12)}';
      case 2:
        return '- ${_sentence(random, 7)}\n'
            '- ${_sentence(random, 6)}\n'
            '- ${_sentence(random, 8)}';
      case 3:
        return '${_sentence(random, 13)} ${_sentence(random, 10)}';
      case 4:
        return '1. ${_sentence(random, 6)}\n'
            '2. ${_sentence(random, 7)}\n'
            '3. ${_sentence(random, 5)}';
      default:
        return 'Another paragraph with `code`, **strong**, and _soft_ '
            'emphasis. ${_sentence(random, 14)}';
    }
  }

  // Two passes: size the body, then splice the anchor into the middle.
  var index = 0;
  while (bytes < targetBytes) {
    final block = nextBlock(index);
    blocks.add(block);
    // +2 for the blank-line separator that follows every block.
    bytes += block.length + 2;
    index += 1;
  }

  final anchor = _anchorBlock(shape);
  final anchorBlockIndex = blocks.length ~/ 2;
  blocks.insert(anchorBlockIndex, anchor);

  final buffer = StringBuffer();
  var anchorStart = -1;
  for (var i = 0; i < blocks.length; i += 1) {
    if (i == anchorBlockIndex) anchorStart = buffer.length;
    buffer.write(blocks[i]);
    if (i != blocks.length - 1) buffer.write('\n\n');
  }
  final markdown = buffer.toString();
  final anchorEnd = anchorStart + anchor.length;
  final anchorMarker = anchor.indexOf('ANCHOR');
  // Land the caret three characters into the plain word 'ANCHOR'.
  final caret = anchorStart + anchorMarker + 3;

  return G2Document(
    markdown: markdown,
    anchorStartUtf16: anchorStart,
    anchorEndUtf16: anchorEnd,
    caretUtf16: caret,
    blockCount: blocks.length,
  );
}

// ---------------------------------------------------------------------------
// Frame timing collection
// ---------------------------------------------------------------------------

class G2DurationStats {
  const G2DurationStats({
    required this.p50,
    required this.p95,
    required this.p99,
    required this.max,
    required this.over8ms,
    required this.over16ms,
    required this.count,
  });

  factory G2DurationStats.from(List<int> microseconds) {
    if (microseconds.isEmpty) {
      return const G2DurationStats(
        p50: 0,
        p95: 0,
        p99: 0,
        max: 0,
        over8ms: 0,
        over16ms: 0,
        count: 0,
      );
    }
    final sorted = List<int>.from(microseconds)..sort();
    int rank(double q) {
      final index = (q * sorted.length).ceil() - 1;
      return sorted[index.clamp(0, sorted.length - 1)];
    }

    return G2DurationStats(
      p50: rank(0.50),
      p95: rank(0.95),
      p99: rank(0.99),
      max: sorted.last,
      over8ms: sorted.where((us) => us > 8000).length,
      over16ms: sorted.where((us) => us > 16000).length,
      count: sorted.length,
    );
  }

  final int p50;
  final int p95;
  final int p99;
  final int max;
  final int over8ms;
  final int over16ms;
  final int count;
}

class _FrameSamples {
  final List<int> build = <int>[];
  final List<int> raster = <int>[];
  final List<int> total = <int>[];

  int get frameCount => build.length;

  void add(FrameTiming timing) {
    build.add(timing.buildDuration.inMicroseconds);
    raster.add(timing.rasterDuration.inMicroseconds);
    total.add(timing.totalSpan.inMicroseconds);
  }

  void clear() {
    build.clear();
    raster.clear();
    total.clear();
  }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

class G2Result {
  G2Result({
    required this.configuration,
    required this.actualBytes,
    required this.blockCount,
  });

  final G2Configuration configuration;
  final int actualBytes;
  final int blockCount;

  double? openCallMs;
  double? firstPaintMs;
  double? structureCurrentMs;
  double? closeMs;

  int keystrokes = 0;
  int rejectedKeystrokes = 0;
  String typingMode = 'unknown';
  String? failure;

  G2DurationStats? build;
  G2DurationStats? raster;
  G2DurationStats? total;
  int frames = 0;
}

// ---------------------------------------------------------------------------
// Typing driver
// ---------------------------------------------------------------------------

/// Feeds one platform-shaped insertion delta at a time into the live editor.
class _TypingDriver {
  _TypingDriver({
    required this.controller,
    required this.editableKey,
  });

  final FlarkV3FlutterLiveController controller;
  final GlobalKey<EditableTextState> editableKey;

  static const String _stream =
      'the quick brown parser jumps over the lazy frame budget while typing ';

  int _cursor = 0;
  int accepted = 0;
  int rejected = 0;

  /// Which input path each keystroke actually took. `editable-delta` is the
  /// production path (identical to what the platform text-input plugin calls).
  /// `controller-delta` skips only `EditableTextState.updateEditingValue`.
  String mode = 'editable-delta';

  bool typeOne() {
    final value = controller.editingController.value;
    final text = value.text;
    if (text.isEmpty) {
      rejected += 1;
      return false;
    }
    var offset = value.selection.isValid && value.selection.isCollapsed
        ? value.selection.extentOffset
        : text.length ~/ 2;
    offset = offset.clamp(0, text.length);
    final character = _stream[_cursor % _stream.length];
    _cursor += 1;

    final delta = TextEditingDeltaInsertion(
      oldText: text,
      textInserted: character,
      insertionOffset: offset,
      selection: TextSelection.collapsed(offset: offset + 1),
      composing: TextRange.empty,
    );

    final state = editableKey.currentState;
    try {
      if (state is DeltaTextInputClient) {
        mode = 'editable-delta';
        (state as DeltaTextInputClient).updateEditingValueWithDeltas(<
          TextEditingDelta
        >[delta]);
      } else {
        mode = 'controller-delta';
        controller.applyTextEditingDeltas(<TextEditingDelta>[delta]);
      }
      accepted += 1;
      return true;
    } catch (error) {
      rejected += 1;
      // A rejected delta means the island moved underneath us. The next tick
      // re-reads the controller value, so this self-heals; it is counted and
      // reported rather than hidden.
      debugPrint('g2| keystroke rejected: $error');
      return false;
    }
  }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

void main() => runApp(const G2JankHarnessApp());

class G2JankHarnessApp extends StatelessWidget {
  const G2JankHarnessApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Flark G2 jank harness',
      theme: ThemeData(useMaterial3: true),
      home: const G2JankHarnessPage(),
    );
  }
}

class G2JankHarnessPage extends StatefulWidget {
  const G2JankHarnessPage({super.key});

  @override
  State<G2JankHarnessPage> createState() => _G2JankHarnessPageState();
}

class _G2JankHarnessPageState extends State<G2JankHarnessPage> {
  final Stopwatch _clock = Stopwatch()..start();
  final _FrameSamples _samples = _FrameSamples();
  final List<G2Result> _results = <G2Result>[];

  final List<String> _report = <String>[];

  bool _collecting = false;
  String _phase = 'starting';

  /// Every FrameTiming this process has ever received, and every frame the
  /// scheduler has actually run. If these disagree with the per-configuration
  /// sample counts, the timings pipeline — not the engine — is the problem.
  int _timingsEverReceived = 0;
  int _framesEverRun = 0;
  int _framesInWindow = 0;

  // Per-configuration live state.
  FlarkV3DocumentRuntime? _runtime;
  FlarkV3ManagedFlutterBinding? _binding;
  FlarkV3ManagedViewportPresentationSource? _presentation;
  FlarkV3VirtualizedLiveSurfaceController? _surfaceController;
  GlobalKey<EditableTextState>? _editableKey;
  FocusNode? _focusNode;
  StreamSubscription<FlarkV3DocumentRuntimeStatus>? _statusSubscription;

  @override
  void initState() {
    super.initState();
    SchedulerBinding.instance.addTimingsCallback(_onFrameTimings);
    SchedulerBinding.instance.addPersistentFrameCallback((_) {
      _framesEverRun += 1;
      if (_collecting) _framesInWindow += 1;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(_runAll()));
  }

  @override
  void dispose() {
    SchedulerBinding.instance.removeTimingsCallback(_onFrameTimings);
    unawaited(_teardownConfiguration());
    super.dispose();
  }

  void _onFrameTimings(List<FrameTiming> timings) {
    _timingsEverReceived += timings.length;
    if (!_collecting) return;
    for (final timing in timings) {
      _samples.add(timing);
    }
  }

  double _nowMs() =>
      _clock.elapsedMicroseconds / Duration.microsecondsPerMillisecond;

  Future<void> _runAll() async {
    final configurations = _configurations();
    _print(
      'g2| harness start  configurations=${configurations.length} '
      'cps=$_charactersPerSecond warmup=${_warmupSeconds}s '
      'measured=${_typeSeconds}s',
    );

    // One discarded configuration first. The very first open in a process pays
    // dylib load, endpoint startup and JIT/AOT page-in; charging that to the
    // 5 KB row would misreport every cold-open number after it.
    _print('g2| process warm-up (discarded)…');
    await _runConfiguration(
      const G2Configuration(shape: G2Shape.plain, targetBytes: 5 * 1024),
      warmUpOnly: true,
    );
    await _teardownConfiguration(recordClose: false);
    await Future<void>.delayed(const Duration(milliseconds: 500));

    for (final configuration in configurations) {
      final result = await _runConfiguration(configuration);
      _results.add(result);
      _printResultLine(result);
      await _teardownConfiguration();
      // Let the previous document's memory settle before sizing the next one.
      await Future<void>.delayed(const Duration(milliseconds: 750));
    }
    _printTables();
    await _writeReportFile();
    if (!mounted) return;
    setState(() => _phase = 'done');
    if (_exitWhenDone) {
      await Future<void>.delayed(const Duration(milliseconds: 400));
      exit(0);
    }
  }

  Future<void> _writeReportFile() async {
    try {
      final file = File(
        '${Directory.systemTemp.path}/flark_g2_jank_report.txt',
      );
      await file.writeAsString(_report.join('\n'));
      _print('g2| report written to ${file.path}');
    } catch (error) {
      _print('g2| report file write failed: $error');
    }
  }

  Future<G2Result> _runConfiguration(
    G2Configuration configuration, {
    bool warmUpOnly = false,
  }) async {
    final document = buildDocument(
      shape: configuration.shape,
      targetBytes: configuration.targetBytes,
    );
    final result = G2Result(
      configuration: configuration,
      actualBytes: document.markdown.length,
      blockCount: document.blockCount,
    );

    if (mounted) {
      setState(() => _phase = 'opening ${configuration.label}');
    }
    // Give the placeholder frame a chance to land before the open begins, so
    // the open measurement is not polluted by an unrelated rebuild.
    await _nextFrame();

    final openStartMs = _nowMs();
    final firstPaintCompleter = Completer<double>();
    final structureCompleter = Completer<double>();

    FlarkV3DocumentRuntime runtime;
    try {
      runtime = await FlarkV3DocumentRuntime.open(document.markdown);
    } catch (error) {
      result.failure = 'open failed: $error';
      return result;
    }
    result.openCallMs = _nowMs() - openStartMs;
    _runtime = runtime;

    final editableKey = GlobalKey<EditableTextState>(
      debugLabel: 'g2-${configuration.label}',
    );
    final focusNode = FocusNode(debugLabel: 'g2-${configuration.label}');
    _editableKey = editableKey;
    _focusNode = focusNode;
    _surfaceController = FlarkV3VirtualizedLiveSurfaceController();

    FlarkV3ManagedFlutterBinding binding;
    try {
      binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: document.anchorStartUtf16,
          maximumUtf16: 8192,
          value: TextEditingValue(
            text: runtime.readSourceRange(
              document.anchorStartUtf16,
              document.anchorEndUtf16,
            ),
            selection: TextSelection.collapsed(
              offset: document.caretUtf16 - document.anchorStartUtf16,
            ),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 64 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
    } catch (error) {
      result.failure = 'binding attach failed: $error';
      return result;
    }
    _binding = binding;

    final presentation = binding.attachViewportPresentationAroundSourcePoint(
      sourcePointUtf16: document.caretUtf16,
    );
    _presentation = presentation;

    void handlePresentation() {
      if (firstPaintCompleter.isCompleted) return;
      final snapshot = presentation.snapshot;
      if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot) return;
      if (snapshot.blocks.isEmpty) return;
      // The exact snapshot exists as of this notification; the frame that
      // paints it ends at the following post-frame callback.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!firstPaintCompleter.isCompleted) {
          firstPaintCompleter.complete(_nowMs() - openStartMs);
        }
      });
      WidgetsBinding.instance.scheduleFrame();
    }

    void handleStatus(FlarkV3DocumentRuntimeStatus status) {
      if (structureCompleter.isCompleted) return;
      if (status.structureCurrent &&
          status.structureRevision == status.sourceRevision) {
        structureCompleter.complete(_nowMs() - openStartMs);
      }
    }

    presentation.addListener(handlePresentation);
    _statusSubscription = runtime.statuses.listen(handleStatus);

    if (mounted) {
      setState(() => _phase = 'mounting ${configuration.label}');
    }
    await _nextFrame();
    // A runtime can already be structure-current before the first status event
    // is delivered; sample it directly rather than waiting for an edge.
    handleStatus(runtime.status);
    handlePresentation();

    result.firstPaintMs = await firstPaintCompleter.future
        .timeout(const Duration(seconds: 180), onTimeout: () => double.nan);
    result.structureCurrentMs = await structureCompleter.future
        .timeout(const Duration(seconds: 180), onTimeout: () => double.nan);
    _print(
      'g2| ${configuration.label} open=${_ms(result.openCallMs)}ms '
      'paint=${_ms(result.firstPaintMs)}ms '
      'structure=${_ms(result.structureCurrentMs)}ms',
    );

    focusNode.requestFocus();
    await _nextFrame();

    final driver = _TypingDriver(
      controller: binding.controller,
      editableKey: editableKey,
    );

    if (mounted) {
      setState(() => _phase = 'warmup ${configuration.label}');
    }
    await _type(driver, seconds: _warmupSeconds);
    if (warmUpOnly) return result;

    _samples.clear();
    _framesInWindow = 0;
    _collecting = true;
    if (mounted) {
      setState(() => _phase = 'typing ${configuration.label}');
    }
    await _type(driver, seconds: _typeSeconds);
    // Frame timings are delivered late and in batches; drain the tail before
    // closing the window. The extra frames are cheap idle frames, so they can
    // only make the numbers look better, never worse.
    for (var i = 0; i < 12; i += 1) {
      await _nextFrame();
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    _collecting = false;

    result.framesRun = _framesInWindow;
    result.keystrokes = driver.accepted;
    result.rejectedKeystrokes = driver.rejected;
    result.typingMode = driver.mode;
    result.frames = _samples.frameCount;
    result.build = G2DurationStats.from(_samples.build);
    result.raster = G2DurationStats.from(_samples.raster);
    result.total = G2DurationStats.from(_samples.total);
    return result;
  }

  Future<void> _type(_TypingDriver driver, {required int seconds}) async {
    if (seconds <= 0) return;
    final period = Duration(
      microseconds: Duration.microsecondsPerSecond ~/ _charactersPerSecond,
    );
    final deadline = _nowMs() + seconds * 1000;
    while (_nowMs() < deadline) {
      final tickStart = _nowMs();
      driver.typeOne();
      final elapsed = _nowMs() - tickStart;
      final remaining =
          period.inMicroseconds - (elapsed * 1000).round();
      if (remaining > 0) {
        await Future<void>.delayed(Duration(microseconds: remaining));
      } else {
        // The keystroke itself outran the typing period. Yield once so the
        // frame pipeline is not starved, and keep going.
        await Future<void>.delayed(Duration.zero);
      }
    }
  }

  Future<void> _nextFrame() {
    final completer = Completer<void>();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!completer.isCompleted) completer.complete();
    });
    WidgetsBinding.instance.scheduleFrame();
    return completer.future;
  }

  Future<void> _teardownConfiguration() async {
    final subscription = _statusSubscription;
    _statusSubscription = null;
    if (subscription != null) await subscription.cancel();

    _presentation = null;
    final binding = _binding;
    _binding = null;
    binding?.dispose();

    _surfaceController = null;
    _editableKey = null;
    final focusNode = _focusNode;
    _focusNode = null;
    focusNode?.dispose();

    final runtime = _runtime;
    _runtime = null;
    if (runtime != null) {
      final closeStart = _nowMs();
      try {
        await runtime.close().timeout(const Duration(seconds: 30));
      } catch (error) {
        _print('g2| close failed/timed out: $error');
      }
      if (_results.isNotEmpty && _results.last.closeMs == null) {
        _results.last.closeMs = _nowMs() - closeStart;
      }
    }
    if (mounted) setState(() {});
  }

  // -------------------------------------------------------------------------
  // Reporting
  // -------------------------------------------------------------------------

  void _print(String line) {
    // ignore: avoid_print
    print(line);
  }

  static String _ms(double? value) {
    if (value == null) return 'n/a';
    if (value.isNaN) return 'timeout';
    return value.toStringAsFixed(1);
  }

  static String _us(int microseconds) =>
      (microseconds / 1000).toStringAsFixed(2);

  static String _pad(String value, int width) => value.padRight(width);

  static String _padLeft(String value, int width) => value.padLeft(width);

  void _printResultLine(G2Result result) {
    if (result.failure != null) {
      _print('g2| ${result.configuration.label} FAILED: ${result.failure}');
      return;
    }
    _print(
      'g2| ${result.configuration.label} done '
      'frames=${result.frames} keystrokes=${result.keystrokes} '
      'mode=${result.typingMode}',
    );
  }

  void _printTables() {
    _print('');
    _print('g2| ===== RFC 024 G2 jank harness =====');
    _print('g2| typing: $_charactersPerSecond char/s, '
        '${_warmupSeconds}s warmup (discarded) + ${_typeSeconds}s measured');
    _print('g2| surface: FlarkV3VirtualizedLiveSurface + '
        'FlarkV3ManagedFlutterBinding + FlarkV3DocumentRuntime');
    _print('');

    _print('g2| --- cold open (ms) ---');
    _print(
      'g2| ${_pad('config', 14)}${_padLeft('bytes', 9)}'
      '${_padLeft('blocks', 8)}${_padLeft('open()', 10)}'
      '${_padLeft('1st paint', 11)}${_padLeft('struct-cur', 12)}'
      '${_padLeft('close()', 10)}',
    );
    for (final result in _results) {
      if (result.failure != null) {
        _print('g2| ${_pad(result.configuration.label, 14)}'
            'FAILED ${result.failure}');
        continue;
      }
      _print(
        'g2| ${_pad(result.configuration.label, 14)}'
        '${_padLeft('${result.actualBytes}', 9)}'
        '${_padLeft('${result.blockCount}', 8)}'
        '${_padLeft(_ms(result.openCallMs), 10)}'
        '${_padLeft(_ms(result.firstPaintMs), 11)}'
        '${_padLeft(_ms(result.structureCurrentMs), 12)}'
        '${_padLeft(_ms(result.closeMs), 10)}',
      );
    }

    _print('');
    _print('g2| --- frame timings during sustained typing (ms) ---');
    _print(
      'g2| ${_pad('config', 14)}${_pad('metric', 8)}'
      '${_padLeft('frames', 8)}${_padLeft('p50', 8)}${_padLeft('p95', 8)}'
      '${_padLeft('p99', 8)}${_padLeft('max', 9)}'
      '${_padLeft('>8ms', 7)}${_padLeft('>16ms', 7)}',
    );
    for (final result in _results) {
      if (result.failure != null) continue;
      void row(String metric, G2DurationStats? stats) {
        if (stats == null) return;
        _print(
          'g2| ${_pad(result.configuration.label, 14)}${_pad(metric, 8)}'
          '${_padLeft('${stats.count}', 8)}'
          '${_padLeft(_us(stats.p50), 8)}'
          '${_padLeft(_us(stats.p95), 8)}'
          '${_padLeft(_us(stats.p99), 8)}'
          '${_padLeft(_us(stats.max), 9)}'
          '${_padLeft('${stats.over8ms}', 7)}'
          '${_padLeft('${stats.over16ms}', 7)}',
        );
      }

      row('build', result.build);
      row('raster', result.raster);
      row('total', result.total);
    }

    _print('');
    _print('g2| --- keystroke accounting ---');
    _print(
      'g2| ${_pad('config', 14)}${_padLeft('accepted', 10)}'
      '${_padLeft('rejected', 10)}  input-path',
    );
    for (final result in _results) {
      if (result.failure != null) continue;
      _print(
        'g2| ${_pad(result.configuration.label, 14)}'
        '${_padLeft('${result.keystrokes}', 10)}'
        '${_padLeft('${result.rejectedKeystrokes}', 10)}  '
        '${result.typingMode}',
      );
    }
    _print('g2| ===== end =====');
  }

  // -------------------------------------------------------------------------
  // UI
  // -------------------------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final presentation = _presentation;
    final binding = _binding;
    final editableKey = _editableKey;
    final focusNode = _focusNode;
    final surfaceController = _surfaceController;

    return Scaffold(
      appBar: AppBar(title: Text('G2 jank harness — $_phase')),
      body: presentation == null ||
              binding == null ||
              editableKey == null ||
              focusNode == null ||
              surfaceController == null
          ? Center(child: Text(_phase))
          : FlarkV3VirtualizedLiveSurface(
              key: ValueKey<GlobalKey<EditableTextState>>(editableKey),
              liveController: binding.controller,
              visibleBlockCoordinator: binding.visibleBlocks,
              presentationSource: presentation,
              controller: surfaceController,
              focusNode: focusNode,
              editableKey: editableKey,
              windowBlockCount: 64,
              horizontalPadding: 24,
              blockSpacing: 16,
              style: const TextStyle(fontSize: 16, height: 1.5),
              codeStyle: const TextStyle(
                fontFamily: 'monospace',
                fontSize: 14,
                height: 1.5,
              ),
              paintLayerBuilder: (context, state) => const SizedBox.shrink(),
              sourceGapBuilder: (context, snapshot) =>
                  const Center(child: Text('certifying…')),
            ),
    );
  }
}
