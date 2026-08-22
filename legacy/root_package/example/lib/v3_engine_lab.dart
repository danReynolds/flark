// Run from example/:
// flutter run --release -d chrome -t lib/v3_engine_lab.dart

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'v3_engine_lab_web_asset_version.dart';

const int v3EngineLabMaximumActiveUtf16 = 4096;
const int v3EngineLabLoadedNeighborhoodUtf16 = 3840;
const int _oneMebibyte = 1024 * 1024;
const String _leadingReferencePrefix = '[flark]: /target\n';
const String _buildModeLabel = kReleaseMode
    ? 'release'
    : kProfileMode
    ? 'profile'
    : 'debug';

const String v3EngineLabEditableTailSource =
    'Edit this bounded input island and watch the engine receipts. '
    '**Bold**, _emphasis_, `code`, ~~strike~~, <https://commonmark.org>, and '
    '<hello@example.com> render live without visible Markdown delimiters. '
    'Parser-certified references &copy; and &ngE; render as cooked text, and '
    'URI <https://e.test/?q=&amp;> cooks the same source token in its label '
    'and destination. '
    'Escaped \\* punctuation also renders with its backslash hidden while '
    'canonical source remains intact.  \n'
    'A parser-certified hard break hides its trailing spaces while keeping '
    'the exact source.';
const String v3EngineLabEditableTailDisplay =
    'Edit this bounded input island and watch the engine receipts. '
    'Bold, emphasis, code, strike, https://commonmark.org, and '
    'hello@example.com render live without visible Markdown delimiters. '
    'Parser-certified references © and ≧̸ render as cooked text, and '
    'URI https://e.test/?q=& cooks the same source token in its label '
    'and destination. '
    'Escaped * punctuation also renders with its backslash hidden while '
    'canonical source remains intact.\n'
    'A parser-certified hard break hides its trailing spaces while keeping '
    'the exact source.';
const String v3EngineLabFencedCodeBody =
    'void main() {\n'
    "  print('**literal Markdown** inside code');\n"
    '}\n';
const String _fencedCodeOpening = '```dart\n';
const String v3EngineLabFencedCodeSource =
    '$_fencedCodeOpening'
    '$v3EngineLabFencedCodeBody'
    '```\n';
final int _fencedCodeBodyStartUtf16 = _fencedCodeOpening.length;
const String v3EngineLabMultiBlockFirstSource =
    '*First* paragraph stays outside the active island.\n';
const String v3EngineLabMultiBlockMiddleSource =
    '**Middle** paragraph with _emphasis_ and `code`.\n';
const String v3EngineLabMultiBlockMiddleDisplay =
    'Middle paragraph with emphasis and code.\n';
const String v3EngineLabMultiBlockTailSource =
    '`Tail` paragraph remains canonical.\n';
const String v3EngineLabMultiBlockSource =
    '$v3EngineLabMultiBlockFirstSource\n'
    '$v3EngineLabMultiBlockMiddleSource\n'
    '$v3EngineLabMultiBlockTailSource';
const String v3EngineLabAtxHeadingSource = '## **β😀** live _heading_ ###\r\n';
const String v3EngineLabAtxHeadingDisplay = 'β😀 live heading';
const String v3EngineLabSetextHeadingSource =
    '**β😀** live _heading_\r\n---\r\n';
const String v3EngineLabSetextHeadingDisplay = 'β😀 live heading';
const String v3EngineLabThematicBreakSource = '  * * * \r\n';
const String v3EngineLabThematicBreakDisplay = '';
const String v3EngineLabIndentedCodeSource =
    "    final message = '**literal Markdown**';\n"
    '      print(message);\n';
const String v3EngineLabIndentedCodeDisplay =
    "final message = '**literal Markdown**';\n"
    '  print(message);\n';
const String v3EngineLabBlockQuoteSource =
    '> **Parser-certified strong text\n'
    '> stays live across physical lines**, with _emphasis_ and `code`.\n'
    '> Canonical quote prefixes remain in exact source.\n';
const String v3EngineLabBlockQuoteDisplay =
    'Parser-certified strong text\n'
    'stays live across physical lines, with emphasis and code.\n'
    'Canonical quote prefixes remain in exact source.\n';
const String v3EngineLabBlockQuoteScope =
    'Checkpoint scope: one depth-one, single-paragraph block quote. '
    'Strong, emphasis, and code styles use parser-certified projected '
    'coordinates; links, images, and references intentionally fail closed.';
const String v3EngineLabBulletListFirstSource = '  - α😀 first item\r\n';
const String v3EngineLabBulletListFirstDisplay = 'α😀 first item\n';
const String v3EngineLabBulletListSecondSource =
    '  - Edit **this** _live_ `list` item.\r\n';
const String v3EngineLabBulletListSecondDisplay = 'Edit this live list item.\n';
const String v3EngineLabBulletListTerminalSource = '-   ';
const String v3EngineLabBulletListTerminalDisplay = '';
const String v3EngineLabBulletListSource =
    '$v3EngineLabBulletListFirstSource'
    '$v3EngineLabBulletListSecondSource'
    '$v3EngineLabBulletListTerminalSource';
const String v3EngineLabBulletListScope =
    'Checkpoint scope: one top-level, depth-one tight bullet list. '
    'The selected item and its parser-certified bold, emphasis, and inline-code '
    'content are marker-free and editable; nested, loose, ordered, and task '
    'lists remain pending.';
const String v3EngineLabOrderedListSource =
    '007) alpha\r\n'
    '9) beta\r\n';
const String v3EngineLabOrderedListDisplay = 'alpha\n';
const String v3EngineLabOrderedListScope =
    'Checkpoint scope: one top-level, depth-one tight ordered list. '
    'The exact parser-authored marker is painted outside the marker-free '
    'editable item; nested, loose, and task list forms remain pending.';
final int _multiBlockMiddleStartUtf16 =
    v3EngineLabMultiBlockFirstSource.length + 1;
final int _multiBlockTailStartUtf16 =
    _multiBlockMiddleStartUtf16 + v3EngineLabMultiBlockMiddleSource.length + 1;
final int _bulletListSecondStartUtf16 = v3EngineLabBulletListFirstSource.length;
final int _bulletListTerminalStartUtf16 =
    _bulletListSecondStartUtf16 + v3EngineLabBulletListSecondSource.length;
const String _smallSeed =
    '$_leadingReferencePrefix'
    '$v3EngineLabEditableTailSource';
const String _largeSeedChunk =
    'flark live source with **bold**, _emphasis_, `code`, and [link](uri) ';

enum V3EngineLabSeed {
  small,
  multiBlockParagraph,
  atxHeading,
  setextHeading,
  thematicBreak,
  fencedCode,
  indentedCode,
  blockQuote,
  bulletList,
  orderedList,
  references4096,
  references100000,
  oneMebibyte,
  tenMebibytes,
}

extension V3EngineLabSeedDescription on V3EngineLabSeed {
  String get label => switch (this) {
    V3EngineLabSeed.small => 'Small · 1 leading ref + live tail',
    V3EngineLabSeed.multiBlockParagraph => 'Multi-block · selected Paragraph',
    V3EngineLabSeed.atxHeading => 'ATX heading · marker-free inline content',
    V3EngineLabSeed.setextHeading =>
      'Setext heading · marker-free inline content',
    V3EngineLabSeed.thematicBreak =>
      'Thematic break · atomic marker-free divider',
    V3EngineLabSeed.fencedCode => 'Fenced code · marker-free body',
    V3EngineLabSeed.indentedCode => 'Indented code · marker-free indentation',
    V3EngineLabSeed.blockQuote => 'Block quote · depth-one single Paragraph',
    V3EngineLabSeed.bulletList => 'Bullet list · marker-free selected item',
    V3EngineLabSeed.orderedList =>
      'Ordered list · exact marker outside selected item',
    V3EngineLabSeed.references4096 => '4,096 leading refs + live tail',
    V3EngineLabSeed.references100000 => '100,000 leading refs + live tail',
    V3EngineLabSeed.oneMebibyte => '1 MiB single paragraph',
    V3EngineLabSeed.tenMebibytes => '10 MiB single paragraph',
  };

  int? get targetBytes => switch (this) {
    V3EngineLabSeed.oneMebibyte => _oneMebibyte,
    V3EngineLabSeed.tenMebibytes => 10 * _oneMebibyte,
    _ => null,
  };

  int? get leadingReferenceCount => switch (this) {
    V3EngineLabSeed.small => 1,
    V3EngineLabSeed.references4096 => 4096,
    V3EngineLabSeed.references100000 => 100000,
    _ => null,
  };

  bool get usesProjectedTailEditor => leadingReferenceCount != null;
  bool get usesManagedEditor =>
      usesProjectedTailEditor ||
      this == V3EngineLabSeed.multiBlockParagraph ||
      this == V3EngineLabSeed.atxHeading ||
      this == V3EngineLabSeed.setextHeading ||
      this == V3EngineLabSeed.thematicBreak ||
      this == V3EngineLabSeed.fencedCode ||
      this == V3EngineLabSeed.indentedCode ||
      this == V3EngineLabSeed.blockQuote ||
      this == V3EngineLabSeed.bulletList ||
      this == V3EngineLabSeed.orderedList;
}

/// One scalar-safe replacement between two bounded input-island snapshots.
final class V3EngineLabEditDelta {
  const V3EngineLabEditDelta({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
  });

  final int startUtf16;
  final int endUtf16;
  final String replacement;
}

/// Finds the smallest single edit while keeping both boundaries scalar-safe.
V3EngineLabEditDelta? computeV3EngineLabEditDelta(String before, String after) {
  if (before == after) return null;

  final sharedLimit = math.min(before.length, after.length);
  var prefix = 0;
  while (prefix < sharedLimit &&
      before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
    prefix += 1;
  }
  while (_splitsScalar(before, prefix) || _splitsScalar(after, prefix)) {
    prefix -= 1;
  }

  var suffix = 0;
  while (suffix < before.length - prefix &&
      suffix < after.length - prefix &&
      before.codeUnitAt(before.length - suffix - 1) ==
          after.codeUnitAt(after.length - suffix - 1)) {
    suffix += 1;
  }
  while (_splitsScalar(before, before.length - suffix) ||
      _splitsScalar(after, after.length - suffix)) {
    suffix -= 1;
  }

  return V3EngineLabEditDelta(
    startUtf16: prefix,
    endUtf16: before.length - suffix,
    replacement: after.substring(prefix, after.length - suffix),
  );
}

bool _splitsScalar(String value, int offset) {
  if (offset <= 0 || offset >= value.length) return false;
  return _isHighSurrogate(value.codeUnitAt(offset - 1)) &&
      _isLowSurrogate(value.codeUnitAt(offset));
}

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xD800 && codeUnit <= 0xDBFF;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xDC00 && codeUnit <= 0xDFFF;

String _buildLargeSeed(int targetBytes) {
  final output = StringBuffer(_leadingReferencePrefix);
  while (output.length + _largeSeedChunk.length <= targetBytes) {
    output.write(_largeSeedChunk);
  }
  final remainder = targetBytes - output.length;
  if (remainder > 0) {
    output.write(_largeSeedChunk.substring(0, remainder));
  }
  return output.toString();
}

/// Builds an exact reference-prefix stress fixture followed by the same small
/// authoritative editing tail used by every projected Checkpoint C seed.
String buildV3EngineLabLeadingReferenceSeed(int referenceCount) {
  if (referenceCount < 1) {
    throw RangeError.value(
      referenceCount,
      'referenceCount',
      'At least one leading reference definition is required.',
    );
  }
  final output = StringBuffer(_leadingReferencePrefix);
  for (var index = 1; index < referenceCount; index += 1) {
    output
      ..write('[r')
      ..write(index)
      ..write(']: /u\n');
  }
  output.write(v3EngineLabEditableTailSource);
  return output.toString();
}

Future<String> _buildLargeSeedCooperatively(int targetBytes) async {
  const yieldEveryBytes = 256 * 1024;
  final output = StringBuffer(_leadingReferencePrefix);
  var nextYield = yieldEveryBytes;
  while (output.length + _largeSeedChunk.length <= targetBytes) {
    output.write(_largeSeedChunk);
    if (output.length >= nextYield) {
      nextYield += yieldEveryBytes;
      await Future<void>.delayed(Duration.zero);
    }
  }
  final remainder = targetBytes - output.length;
  if (remainder > 0) {
    output.write(_largeSeedChunk.substring(0, remainder));
  }
  return output.toString();
}

Future<String> _buildLeadingReferenceSeedCooperatively(
  int referenceCount,
) async {
  const yieldEveryReferences = 4096;
  final output = StringBuffer(_leadingReferencePrefix);
  for (var index = 1; index < referenceCount; index += 1) {
    output
      ..write('[r')
      ..write(index)
      ..write(']: /u\n');
    if (index % yieldEveryReferences == 0) {
      await Future<void>.delayed(Duration.zero);
    }
  }
  output.write(v3EngineLabEditableTailSource);
  return output.toString();
}

Future<String> _materializeSeed(V3EngineLabSeed seed) {
  if (seed == V3EngineLabSeed.multiBlockParagraph) {
    return SynchronousFuture(v3EngineLabMultiBlockSource);
  }
  if (seed == V3EngineLabSeed.atxHeading) {
    return SynchronousFuture(v3EngineLabAtxHeadingSource);
  }
  if (seed == V3EngineLabSeed.setextHeading) {
    return SynchronousFuture(v3EngineLabSetextHeadingSource);
  }
  if (seed == V3EngineLabSeed.thematicBreak) {
    return SynchronousFuture(v3EngineLabThematicBreakSource);
  }
  if (seed == V3EngineLabSeed.fencedCode) {
    return SynchronousFuture(v3EngineLabFencedCodeSource);
  }
  if (seed == V3EngineLabSeed.indentedCode) {
    return SynchronousFuture(v3EngineLabIndentedCodeSource);
  }
  if (seed == V3EngineLabSeed.blockQuote) {
    return SynchronousFuture(v3EngineLabBlockQuoteSource);
  }
  if (seed == V3EngineLabSeed.bulletList) {
    return SynchronousFuture(v3EngineLabBulletListSource);
  }
  if (seed == V3EngineLabSeed.orderedList) {
    return SynchronousFuture(v3EngineLabOrderedListSource);
  }
  final referenceCount = seed.leadingReferenceCount;
  if (referenceCount != null) {
    if (referenceCount == 1) return SynchronousFuture(_smallSeed);
    // On Web, seed assembly shares the UI event loop. Yield between bounded
    // batches; native Flutter builds the same fixture in a helper isolate.
    if (kIsWeb) {
      return _buildLeadingReferenceSeedCooperatively(referenceCount);
    }
    return compute(
      buildV3EngineLabLeadingReferenceSeed,
      referenceCount,
      debugLabel: 'Flark reference seed builder',
    );
  }
  final targetBytes = seed.targetBytes;
  if (targetBytes == null) {
    throw StateError('Seed ${seed.name} has no materialization strategy.');
  }
  // Flutter Web's compute() callback shares the UI event loop. Yield while
  // assembling fixtures there; native Flutter gets a real helper isolate.
  if (kIsWeb) return _buildLargeSeedCooperatively(targetBytes);
  return compute(
    _buildLargeSeed,
    targetBytes,
    debugLabel: 'Flark seed builder',
  );
}

/// Resolves the explicit `flark_flutter` asset mirrors for the active Flutter
/// Web delivery environment.
///
/// A built Flutter app serves package assets from its asset bundle. The
/// `flutter test --platform chrome` server instead exposes package `lib/`
/// files through `/packages/<package>/...`.
FlarkV3WebRuntimeAssets v3EngineLabWebAssets({
  bool flutterTestPackageServer = const bool.fromEnvironment('FLUTTER_TEST'),
}) {
  final prefix = flutterTestPackageServer
      ? '/packages/flark_flutter/assets'
      : 'assets/packages/flark_flutter/lib/assets';
  Uri versioned(String path) => Uri.base
      .resolve(path)
      .replace(
        queryParameters: const {'flark-build': v3EngineLabWebAssetVersion},
      );
  return FlarkV3WebRuntimeAssets(
    workerUri: versioned('$prefix/worker/flark_v3_parser_worker.js'),
    wasmUri: versioned('$prefix/wasm/flark_comrak_bridge.wasm'),
  );
}

void main() => runApp(const V3EngineLabApp());

class V3EngineLabApp extends StatelessWidget {
  const V3EngineLabApp({super.key, this.openOnStart = true, this.webAssets});

  /// Test seam. The standalone entrypoint always opens the small seed.
  final bool openOnStart;

  /// Test-server asset paths differ from a built Flutter application's bundle.
  final FlarkV3WebRuntimeAssets? webAssets;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Flark v3 engine lab',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF315D78),
          brightness: Brightness.light,
        ),
        scaffoldBackgroundColor: const Color(0xFFF3F5F7),
        useMaterial3: true,
      ),
      home: V3EngineLabPage(openOnStart: openOnStart, webAssets: webAssets),
    );
  }
}

class V3EngineLabPage extends StatefulWidget {
  const V3EngineLabPage({super.key, this.openOnStart = true, this.webAssets});

  final bool openOnStart;
  final FlarkV3WebRuntimeAssets? webAssets;

  @override
  State<V3EngineLabPage> createState() => _V3EngineLabPageState();
}

class _V3EngineLabPageState extends State<V3EngineLabPage> {
  static final FlarkV3WebRuntimeAssets _flutterWebAssets =
      v3EngineLabWebAssets();

  final TextEditingController _editingController = TextEditingController(
    text: v3EngineLabEditableTailSource,
  );
  final Stopwatch _monotonicClock = Stopwatch()..start();
  final Map<int, int> _pendingEditStartMicros = <int, int>{};

  FlarkV3DocumentRuntime? _runtime;
  FlarkV3ManagedFlutterBinding? _managedBinding;
  TextEditingController? _observedManagedEditor;
  FlarkV3FlutterVisibleBlockCoordinator? _observedVisibleBlocks;
  StreamSubscription<FlarkV3DocumentRuntimeStatus>? _statusSubscription;
  FlarkV3DocumentRuntimeStatus? _status;
  FlarkV3DocumentQueryResult? _queryResult;
  V3EngineLabSeed _selectedSeed = V3EngineLabSeed.small;
  String _activeWindowText = v3EngineLabEditableTailSource;
  String _lastManagedDisplayText = v3EngineLabEditableTailSource;
  int _lastManagedSourceRevision = 0;
  int _activeWindowStartUtf16 = _leadingReferencePrefix.length;
  int _queryPositionUtf16 = _leadingReferencePrefix.length;
  bool _lifecycleBusy = false;
  String _lifecycleNote = 'Runtime has not opened yet.';
  String? _error;
  double? _openToCurrentMilliseconds;
  double? _seedPreparationMilliseconds;
  double? _lastEditToCurrentMilliseconds;
  double? _lastForegroundApplyMilliseconds;
  double? _lastCloseMilliseconds;
  Map<String, Object?>? _checkpointBReport;
  String? _checkpointBError;
  double? _checkpointBMilliseconds;
  bool _checkpointBBusy = false;
  int _lifecycleEpoch = 0;

  @override
  void initState() {
    super.initState();
    if (widget.openOnStart) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) unawaited(_openSeed(V3EngineLabSeed.small));
      });
    }
  }

  @override
  void dispose() {
    _lifecycleEpoch += 1;
    _detachManagedBinding();
    final subscription = _statusSubscription;
    if (subscription != null) unawaited(subscription.cancel());
    final runtime = _runtime;
    if (runtime != null) unawaited(runtime.close().catchError((_) {}));
    _editingController.dispose();
    super.dispose();
  }

  Future<void> _openSeed(V3EngineLabSeed seed) async {
    if (_lifecycleBusy) return;
    final epoch = ++_lifecycleEpoch;
    setState(() {
      _lifecycleBusy = true;
      _selectedSeed = seed;
      _error = null;
      _seedPreparationMilliseconds = null;
      _openToCurrentMilliseconds = null;
      _lifecycleNote = 'Preparing ${seed.label} exact source…';
    });

    await _retireCurrentRuntime();
    if (!mounted || epoch != _lifecycleEpoch) return;

    final String markdown;
    final seedWatch = Stopwatch()..start();
    try {
      markdown = await _materializeSeed(seed);
    } catch (error) {
      if (!mounted || epoch != _lifecycleEpoch) return;
      setState(() {
        _lifecycleBusy = false;
        _error = 'Seed construction failed: $error';
      });
      return;
    }
    seedWatch.stop();
    _seedPreparationMilliseconds =
        seedWatch.elapsedMicroseconds / Duration.microsecondsPerMillisecond;
    if (!mounted || epoch != _lifecycleEpoch) return;

    setState(() => _lifecycleNote = 'Opening ${seed.label} runtime…');
    final readyWatch = Stopwatch()..start();
    late final FlarkV3DocumentRuntime runtime;
    try {
      runtime = await FlarkV3DocumentRuntime.open(
        markdown,
        webAssets: widget.webAssets ?? _flutterWebAssets,
      );
    } catch (error) {
      if (!mounted || epoch != _lifecycleEpoch) return;
      setState(() {
        _lifecycleBusy = false;
        _error = 'Runtime open failed: $error';
        _lifecycleNote = 'No runtime is owned.';
      });
      return;
    }
    if (!mounted || epoch != _lifecycleEpoch) {
      await runtime.close();
      return;
    }

    _runtime = runtime;
    _status = runtime.status;
    _pendingEditStartMicros.clear();
    _openToCurrentMilliseconds = null;
    _lastEditToCurrentMilliseconds = null;
    _lastForegroundApplyMilliseconds = null;
    if (seed.usesManagedEditor) {
      late final int islandStart;
      late final String islandText;
      late final int caretUtf16;
      if (seed.usesProjectedTailEditor) {
        islandStart =
            runtime.sourceLengthUtf16 - v3EngineLabEditableTailSource.length;
        islandText = runtime.readSourceRange(
          islandStart,
          runtime.sourceLengthUtf16,
        );
        if (islandText != v3EngineLabEditableTailSource) {
          await runtime.close();
          _runtime = null;
          if (!mounted || epoch != _lifecycleEpoch) return;
          setState(() {
            _lifecycleBusy = false;
            _error = 'Projected seed did not end in the shared editable tail.';
            _lifecycleNote = 'No runtime is owned.';
          });
          return;
        }
        caretUtf16 = runtime.sourceLengthUtf16;
      } else if (seed == V3EngineLabSeed.fencedCode) {
        islandStart = 0;
        islandText = runtime.readSourceRange(0, runtime.sourceLengthUtf16);
        caretUtf16 =
            _fencedCodeBodyStartUtf16 +
            v3EngineLabFencedCodeBody.indexOf('literal');
      } else if (seed == V3EngineLabSeed.multiBlockParagraph) {
        islandStart = _multiBlockMiddleStartUtf16;
        islandText = v3EngineLabMultiBlockMiddleSource;
        caretUtf16 =
            islandStart +
            v3EngineLabMultiBlockMiddleSource.indexOf('Middle') +
            2;
      } else if (seed == V3EngineLabSeed.atxHeading) {
        islandStart = 0;
        islandText = v3EngineLabAtxHeadingSource;
        caretUtf16 = v3EngineLabAtxHeadingSource.indexOf('live') + 2;
      } else if (seed == V3EngineLabSeed.setextHeading) {
        islandStart = 0;
        islandText = v3EngineLabSetextHeadingSource;
        caretUtf16 = v3EngineLabSetextHeadingSource.indexOf('live') + 2;
      } else if (seed == V3EngineLabSeed.thematicBreak) {
        islandStart = 0;
        islandText = v3EngineLabThematicBreakSource;
        caretUtf16 = 0;
      } else if (seed == V3EngineLabSeed.indentedCode) {
        islandStart = 0;
        islandText = v3EngineLabIndentedCodeSource;
        caretUtf16 = v3EngineLabIndentedCodeSource.indexOf('message') + 2;
      } else if (seed == V3EngineLabSeed.blockQuote) {
        islandStart = 0;
        islandText = v3EngineLabBlockQuoteSource;
        caretUtf16 = v3EngineLabBlockQuoteSource.indexOf('strong text') + 2;
      } else if (seed == V3EngineLabSeed.bulletList) {
        islandStart = 0;
        islandText = v3EngineLabBulletListSource;
        caretUtf16 =
            _bulletListSecondStartUtf16 +
            v3EngineLabBulletListSecondSource.indexOf('Edit') +
            2;
      } else if (seed == V3EngineLabSeed.orderedList) {
        islandStart = 0;
        islandText = v3EngineLabOrderedListSource;
        caretUtf16 = v3EngineLabOrderedListSource.indexOf('alpha') + 2;
      } else {
        throw StateError('Managed seed ${seed.name} has no input island.');
      }
      _activeWindowStartUtf16 = islandStart;
      _activeWindowText = islandText;
      _queryPositionUtf16 = caretUtf16;
      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: islandStart,
          maximumUtf16: v3EngineLabMaximumActiveUtf16,
          value: TextEditingValue(
            text: islandText,
            selection: TextSelection.collapsed(
              offset: caretUtf16 - islandStart,
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
      _attachManagedBinding(binding);
    } else {
      _queryPositionUtf16 = 0;
      _adoptWindow(runtime, startUtf16: 0);
    }
    _queryResult = _query(runtime);
    _statusSubscription = runtime.statuses.listen(
      (status) => _acceptStatus(runtime, status),
      onError: (Object error, StackTrace stackTrace) {
        if (!mounted || !identical(_runtime, runtime)) return;
        setState(() => _error = 'Status stream failed: $error');
      },
    );
    setState(() {
      _lifecycleBusy = false;
      _lifecycleNote = 'Awaiting exact-current startup…';
    });

    try {
      await runtime.initialReady;
      if (!runtime.status.structureCurrent) {
        if (mounted &&
            epoch == _lifecycleEpoch &&
            identical(_runtime, runtime)) {
          setState(() {
            _lifecycleNote =
                'Source is synchronized; awaiting exact structural install…';
          });
        }
        await runtime.statuses.firstWhere((status) => status.structureCurrent);
      }
      readyWatch.stop();
      if (!mounted ||
          epoch != _lifecycleEpoch ||
          !identical(_runtime, runtime)) {
        return;
      }
      setState(() {
        _openToCurrentMilliseconds =
            readyWatch.elapsedMicroseconds /
            Duration.microsecondsPerMillisecond;
        _lifecycleNote = 'Initial exact source is certified and synchronized.';
      });
    } catch (error) {
      if (!mounted ||
          epoch != _lifecycleEpoch ||
          !identical(_runtime, runtime)) {
        return;
      }
      setState(() {
        _error = 'Initial readiness failed: $error';
        _lifecycleNote = 'Runtime did not reach initial readiness.';
      });
    }
  }

  Future<void> _closeFromUi() async {
    if (_lifecycleBusy || _runtime == null) return;
    final epoch = ++_lifecycleEpoch;
    setState(() {
      _lifecycleBusy = true;
      _error = null;
      _lifecycleNote = 'Awaiting runtime close…';
    });
    final closeError = await _retireCurrentRuntime();
    if (!mounted || epoch != _lifecycleEpoch) return;
    setState(() {
      _lifecycleBusy = false;
      _lifecycleNote = closeError == null
          ? 'Close completed with the endpoint slot released.'
          : 'Runtime ownership ended; endpoint release was not proven.';
    });
  }

  Future<Object?> _retireCurrentRuntime() async {
    final runtime = _runtime;
    final subscription = _statusSubscription;
    if (runtime == null) return null;

    _detachManagedBinding();
    final watch = Stopwatch()..start();
    Object? closeError;
    try {
      await runtime.close();
    } catch (error) {
      closeError = error;
    }
    watch.stop();
    await subscription?.cancel();
    if (!mounted || !identical(_runtime, runtime)) return closeError;

    _runtime = null;
    _statusSubscription = null;
    _status = runtime.status;
    _queryResult = null;
    _pendingEditStartMicros.clear();
    _lastCloseMilliseconds =
        watch.elapsedMicroseconds / Duration.microsecondsPerMillisecond;
    if (closeError != null) {
      _error =
          'Runtime close failed; endpoint release was not proven: $closeError';
    }
    return closeError;
  }

  void _attachManagedBinding(FlarkV3ManagedFlutterBinding binding) {
    _managedBinding = binding;
    final editor = binding.controller.editingController;
    _observedManagedEditor = editor;
    _lastManagedDisplayText = editor.text;
    _lastManagedSourceRevision = binding.runtime.sourceRevision;
    editor.addListener(_handleManagedEditorValue);
    _observedVisibleBlocks = binding.visibleBlocks
      ..addListener(_handleVisibleBlocksChange);
    _requestManagedVisibleBlocks();
  }

  void _detachManagedBinding() {
    _observedManagedEditor?.removeListener(_handleManagedEditorValue);
    _observedManagedEditor = null;
    _observedVisibleBlocks?.removeListener(_handleVisibleBlocksChange);
    _observedVisibleBlocks = null;
    _managedBinding?.dispose();
    _managedBinding = null;
  }

  void _handleVisibleBlocksChange() {
    if (mounted) setState(() {});
  }

  void _requestManagedVisibleBlocks() {
    final binding = _managedBinding;
    if (binding == null) return;
    final controller = binding.controller;
    binding.visibleBlocks.requestVisibleSourceRange(
      TextRange(
        start: controller.inputIslandGlobalStartUtf16,
        end: controller.inputIslandGlobalEndUtf16,
      ),
    );
  }

  void _handleManagedEditorValue() {
    final editor = _observedManagedEditor;
    final runtime = _runtime;
    if (editor == null || runtime == null) return;
    final displayText = editor.text;
    if (displayText == _lastManagedDisplayText) return;

    _lastManagedDisplayText = displayText;
    final revision = runtime.sourceRevision;
    if (revision > _lastManagedSourceRevision) {
      _lastManagedSourceRevision = revision;
      // This timestamp is deliberately after the managed binding has
      // synchronously accepted the source edit and updated the visible
      // projection. It measures visible-tail update → exact current, not the
      // binding's unexposed foreground apply duration.
      _pendingEditStartMicros[revision] = _monotonicClock.elapsedMicroseconds;
    }
    _requestManagedVisibleBlocks();
  }

  void _acceptStatus(
    FlarkV3DocumentRuntime runtime,
    FlarkV3DocumentRuntimeStatus status,
  ) {
    if (!mounted || !identical(_runtime, runtime)) return;

    final previousRevision = _status?.sourceRevision;
    if (previousRevision != null &&
        status.sourceRevision > previousRevision &&
        !_pendingEditStartMicros.containsKey(status.sourceRevision)) {
      // When no editor listener observed the accepted revision first, this
      // public source-status edge is the earliest timestamp exposed to the
      // example.
      _pendingEditStartMicros[status.sourceRevision] =
          _monotonicClock.elapsedMicroseconds;
    }
    if (status.structureCurrent &&
        status.structureRevision == status.sourceRevision) {
      final startedAt = _pendingEditStartMicros[status.sourceRevision];
      if (startedAt != null) {
        _lastEditToCurrentMilliseconds =
            (_monotonicClock.elapsedMicroseconds - startedAt) /
            Duration.microsecondsPerMillisecond;
      }
      _pendingEditStartMicros.removeWhere(
        (revision, _) => revision <= status.sourceRevision,
      );
    }

    _status = status;
    final managedBinding = _managedBinding;
    if (managedBinding != null) {
      _activeWindowText = runtime.readSourceRange(
        managedBinding.controller.inputIslandGlobalStartUtf16,
        managedBinding.controller.inputIslandGlobalEndUtf16,
      );
      if (previousRevision != status.sourceRevision) {
        _requestManagedVisibleBlocks();
      }
    }
    _queryPositionUtf16 = _normalizeScalarBoundary(
      runtime,
      math.min(_queryPositionUtf16, runtime.sourceLengthUtf16),
    );
    _queryResult = _query(runtime);
    setState(() {});
  }

  void _onEditorChanged(String nextText) {
    final runtime = _runtime;
    final status = _status;
    if (runtime == null ||
        status == null ||
        (status.state != FlarkV3DocumentRuntimeState.open &&
            status.state != FlarkV3DocumentRuntimeState.opening)) {
      _replaceEditorText(_activeWindowText);
      return;
    }

    final delta = computeV3EngineLabEditDelta(_activeWindowText, nextText);
    if (delta == null) return;
    final startedAt = _monotonicClock.elapsedMicroseconds;
    final watch = Stopwatch()..start();
    try {
      final receipt = runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: _activeWindowStartUtf16 + delta.startUtf16,
            endUtf16: _activeWindowStartUtf16 + delta.endUtf16,
            replacement: delta.replacement,
          ),
        ),
      );
      watch.stop();
      _lastForegroundApplyMilliseconds =
          watch.elapsedMicroseconds / Duration.microsecondsPerMillisecond;
      _activeWindowText = nextText;
      if (receipt.changed) {
        _pendingEditStartMicros[receipt.sourceRevision] = startedAt;
      }
      _status = runtime.status;
      _queryPositionUtf16 = math.min(
        _queryPositionUtf16,
        runtime.sourceLengthUtf16,
      );
      _queryResult = _query(runtime);
      setState(() => _error = null);
    } catch (error) {
      watch.stop();
      _replaceEditorText(_activeWindowText);
      setState(() => _error = 'Exact source edit was rejected: $error');
    }
  }

  void _recoverFaultedRuntime() {
    final runtime = _runtime;
    if (runtime == null ||
        _status?.state != FlarkV3DocumentRuntimeState.faulted) {
      return;
    }
    try {
      runtime.recover();
      setState(() {
        _status = runtime.status;
        _lifecycleNote = 'Recovery started; exact source is being reseeded.';
        _error = null;
      });
    } catch (error) {
      setState(() => _error = 'Fault recovery failed: $error');
    }
  }

  void _selectQueryPosition(double value) {
    final runtime = _runtime;
    if (runtime == null) return;
    final position = _normalizeScalarBoundary(runtime, value.round());
    setState(() {
      _queryPositionUtf16 = position;
      _queryResult = _query(runtime);
    });
  }

  void _jumpQueryTo(int position) {
    final runtime = _runtime;
    if (runtime == null) return;
    _selectQueryPosition(
      math.min(position, runtime.sourceLengthUtf16).toDouble(),
    );
  }

  void _selectManagedLeafAt(
    int positionUtf16, {
    required V3EngineLabSeed seed,
  }) {
    final runtime = _runtime;
    final binding = _managedBinding;
    if (runtime == null || binding == null || _selectedSeed != seed) {
      return;
    }
    try {
      binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: positionUtf16),
          composing: TextRange.empty,
        ),
      );
      _activeWindowStartUtf16 = binding.controller.inputIslandGlobalStartUtf16;
      _activeWindowText = runtime.readSourceRange(
        _activeWindowStartUtf16,
        binding.controller.inputIslandGlobalEndUtf16,
      );
      _queryPositionUtf16 = positionUtf16;
      _queryResult = _query(runtime);
      _requestManagedVisibleBlocks();
      setState(() => _error = null);
    } catch (error) {
      setState(() => _error = 'Managed selection was rejected: $error');
    }
  }

  void _loadQueryNeighborhood() {
    final runtime = _runtime;
    if (runtime == null) return;
    var start = math.max(
      0,
      _queryPositionUtf16 - v3EngineLabMaximumActiveUtf16 ~/ 2,
    );
    start = _normalizeScalarBoundary(runtime, start);
    _adoptWindow(runtime, startUtf16: start);
    setState(() {});
  }

  void _adoptWindow(FlarkV3DocumentRuntime runtime, {required int startUtf16}) {
    final sourceLength = runtime.sourceLengthUtf16;
    var start = math.min(startUtf16, sourceLength);
    // A full formatter-sized island would reject the reviewer's first normal
    // insertion. Keep a small, explicit editing reserve while still bounding
    // every TextEditingController value independently of document size.
    var end = math.min(
      sourceLength,
      start + v3EngineLabLoadedNeighborhoodUtf16,
    );
    start = _normalizeScalarBoundary(runtime, start);
    if (end < sourceLength && _boundarySplitsRuntimeScalar(runtime, end)) {
      end -= 1;
    }
    _activeWindowStartUtf16 = start;
    _activeWindowText = runtime.readSourceRange(start, end);
    _replaceEditorText(_activeWindowText);
  }

  int _normalizeScalarBoundary(FlarkV3DocumentRuntime runtime, int position) {
    final bounded = position.clamp(0, runtime.sourceLengthUtf16);
    return _boundarySplitsRuntimeScalar(runtime, bounded)
        ? bounded - 1
        : bounded;
  }

  bool _boundarySplitsRuntimeScalar(
    FlarkV3DocumentRuntime runtime,
    int position,
  ) {
    if (position <= 0 || position >= runtime.sourceLengthUtf16) return false;
    final pair = runtime.readSourceRange(position - 1, position + 1);
    return _isHighSurrogate(pair.codeUnitAt(0)) &&
        _isLowSurrogate(pair.codeUnitAt(1));
  }

  FlarkV3DocumentQueryResult? _query(FlarkV3DocumentRuntime runtime) {
    final state = runtime.status.state;
    if (state == FlarkV3DocumentRuntimeState.closing ||
        state == FlarkV3DocumentRuntimeState.closed ||
        state == FlarkV3DocumentRuntimeState.faulted) {
      return null;
    }
    try {
      return runtime.queryAtUtf16(_queryPositionUtf16);
    } catch (error) {
      _error = 'Bounded point query failed: $error';
      return null;
    }
  }

  void _replaceEditorText(String text) {
    _editingController.value = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: text.length),
    );
  }

  Future<void> _runCheckpointB() async {
    if (_checkpointBBusy) return;
    setState(() {
      _checkpointBBusy = true;
      _checkpointBError = null;
      _checkpointBReport = null;
      _checkpointBMilliseconds = null;
    });
    final watch = Stopwatch()..start();
    try {
      final encoded = await runFlarkV3CheckpointBProbeJson(
        webAssets: widget.webAssets ?? _flutterWebAssets,
      );
      final decoded = jsonDecode(encoded);
      if (decoded is! Map<String, Object?> ||
          decoded['schema'] != 1 ||
          decoded['allChecksPassed'] != true) {
        throw const FormatException(
          'Checkpoint B did not return a passing schema-1 receipt.',
        );
      }
      watch.stop();
      if (!mounted) return;
      setState(() {
        _checkpointBBusy = false;
        _checkpointBReport = decoded;
        _checkpointBMilliseconds =
            watch.elapsedMicroseconds / Duration.microsecondsPerMillisecond;
      });
    } catch (error) {
      watch.stop();
      if (!mounted) return;
      setState(() {
        _checkpointBBusy = false;
        _checkpointBError = '$error';
        _checkpointBMilliseconds =
            watch.elapsedMicroseconds / Duration.microsecondsPerMillisecond;
      });
    }
  }

  Widget _buildActiveEditor(bool writable) {
    final binding = _managedBinding;
    if (_selectedSeed.usesManagedEditor && binding != null) {
      return IgnorePointer(
        ignoring: !writable,
        child: Opacity(
          opacity: writable ? 1 : 0.65,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color:
                  _selectedSeed == V3EngineLabSeed.fencedCode ||
                      _selectedSeed == V3EngineLabSeed.indentedCode
                  ? Theme.of(context).colorScheme.surfaceContainerHigh
                  : Theme.of(context).colorScheme.surface,
              border: Border.all(
                color: Theme.of(context).colorScheme.outlineVariant,
              ),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: FlarkV3LiveEditorPrototype(
                key: const Key('v3-engine-lab-live-editor'),
                editableKey: const Key('v3-engine-lab-editor'),
                controller: binding.controller,
                style: const TextStyle(
                  fontFamily: 'Inter',
                  fontSize: 16,
                  height: 1.5,
                  color: Color(0xFF17242D),
                ),
                paintLayerBuilder: (context, paint) {
                  final certified =
                      binding.controller.hasCertifiedInlinePresentation;
                  final thematicBreak =
                      paint.atomicBlockLease?.kind ==
                      FlarkV3FlutterAtomicBlockKind.thematicBreak;
                  final fence = switch (paint.documentQuery) {
                    FlarkV3DocumentStructuralQuery(
                      structure: FlarkV3DocumentStructure(
                        kind: FlarkV3DocumentStructureKind.fencedCode,
                        fencedCode: final fence?,
                      ),
                    ) =>
                      fence,
                    _ => null,
                  };
                  final fencedCodeBody =
                      fence != null &&
                      binding.controller.inputIslandGlobalStartUtf16 >=
                          fence.bodySource.startUtf16 &&
                      binding.controller.inputIslandGlobalEndUtf16 <=
                          fence.bodySource.endUtf16;
                  final indentedCode =
                      paint.blockStyleLease?.kind ==
                      FlarkV3FlutterBlockStyleKind.indentedCode;
                  final blockQuote =
                      paint.blockStyleLease?.kind ==
                      FlarkV3FlutterBlockStyleKind.blockQuote;
                  final blockQuoteInline = switch (paint.documentQuery) {
                    FlarkV3RecursiveGreenPointQuery(
                      projectedInlineFacts: FlarkV3ProjectedInlineFacts(
                        disposition: FlarkV3ProjectedInlineFactsDisposition
                            .authoritative,
                      ),
                    ) =>
                      true,
                    _ => false,
                  };
                  final tightListItem =
                      paint.blockStyleLease?.kind ==
                      FlarkV3FlutterBlockStyleKind.tightListItem;
                  final orderedListItem =
                      tightListItem &&
                      _selectedSeed == V3EngineLabSeed.orderedList;
                  final bulletListItem =
                      tightListItem &&
                      _selectedSeed == V3EngineLabSeed.bulletList;
                  final heading = switch (paint.documentQuery) {
                    FlarkV3DocumentStructuralQuery(
                      structure: FlarkV3DocumentStructure(
                        kind: FlarkV3DocumentStructureKind.heading,
                        heading: final heading?,
                      ),
                    ) =>
                      heading,
                    _ => null,
                  };
                  final headingSyntax = switch (heading) {
                    FlarkV3AtxHeadingFacts() => 'ATX',
                    FlarkV3SetextHeadingFacts() => 'Setext',
                    _ => null,
                  };
                  return Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: Text(
                      thematicBreak
                          ? 'Parser-certified atomic marker-free thematic break active'
                          : indentedCode
                          ? 'Parser-certified marker-free indented code active'
                          : blockQuote && blockQuoteInline
                          ? 'Parser-certified marker-free block quote + inline styles active'
                          : blockQuote
                          ? 'Parser-certified marker-free block quote active'
                          : orderedListItem
                          ? 'Parser-certified marker-free ordered-list item active'
                          : bulletListItem
                          ? 'Parser-certified marker-free bullet-list item active'
                          : fencedCodeBody
                          ? 'Parser-certified fenced code body active'
                          : headingSyntax != null && certified
                          ? 'Parser-certified marker-free $headingSyntax heading active'
                          : certified
                          ? 'Parser-certified hidden inline rendering active'
                          : binding.controller.hasProjectedInlinePresentation
                          ? 'Hidden projection retained while authority catches up'
                          : 'Literal source fallback',
                      key: const Key('v3-engine-lab-inline-status'),
                      style: Theme.of(context).textTheme.labelMedium?.copyWith(
                        color:
                            thematicBreak ||
                                certified ||
                                fencedCodeBody ||
                                indentedCode ||
                                blockQuote ||
                                tightListItem
                            ? Theme.of(context).colorScheme.primary
                            : Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      );
    }

    // Giant-paragraph structural seeds deliberately retain the bounded
    // literal TextField lane. Their controller never holds more than one
    // capped source neighborhood.
    return TextField(
      key: const Key('v3-engine-lab-editor'),
      controller: _editingController,
      enabled: writable,
      minLines: 6,
      maxLines: 12,
      keyboardType: TextInputType.multiline,
      inputFormatters: const [
        _Utf16LengthLimitingFormatter(v3EngineLabMaximumActiveUtf16),
      ],
      onChanged: _onEditorChanged,
      style: const TextStyle(
        fontFamily: 'monospace',
        fontSize: 14,
        height: 1.45,
      ),
      decoration: const InputDecoration(
        border: OutlineInputBorder(),
        hintText: 'Open a seed to edit exact source.',
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final runtime = _runtime;
    final status = _status;
    final sourceLengthUtf16 = runtime?.sourceLengthUtf16 ?? 0;
    final structureRevision = status?.structureRevision;
    final certifiedLag = status == null
        ? null
        : math.max(0, status.sourceRevision - status.certifiedSourceRevision);
    final structureLag = status == null || structureRevision == null
        ? null
        : math.max(0, status.sourceRevision - structureRevision);
    final canonicalFenceSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.fencedCode
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalMultiBlockSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.multiBlockParagraph
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalAtxHeadingSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.atxHeading
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalSetextHeadingSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.setextHeading
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalThematicBreakSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.thematicBreak
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalIndentedCodeSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.indentedCode
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalBlockQuoteSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.blockQuote
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalBulletListSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.bulletList
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final canonicalOrderedListSource =
        runtime != null && _selectedSeed == V3EngineLabSeed.orderedList
        ? runtime.readSourceRange(0, runtime.sourceLengthUtf16)
        : null;
    final managedController = _managedBinding?.controller;
    final activeInputStartUtf16 =
        managedController?.inputIslandGlobalStartUtf16 ??
        _activeWindowStartUtf16;
    final activeInputEndUtf16 =
        managedController?.inputIslandGlobalEndUtf16 ??
        (_activeWindowStartUtf16 + _activeWindowText.length);
    final activeInputSource = runtime != null && managedController != null
        ? runtime.readSourceRange(activeInputStartUtf16, activeInputEndUtf16)
        : _activeWindowText;
    final writable =
        runtime != null &&
        status != null &&
        (status.state == FlarkV3DocumentRuntimeState.open ||
            status.state == FlarkV3DocumentRuntimeState.opening) &&
        !_lifecycleBusy;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Flark v3 · Feedback Checkpoints A + B + C'),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 20),
            child: Center(
              child: Text(
                FlarkV3DocumentRuntime.platformSupport.endpoint,
                style: Theme.of(context).textTheme.labelMedium,
              ),
            ),
          ),
        ],
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(20),
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const _MilestoneBanner(),
                const SizedBox(height: 16),
                _CheckpointBCard(
                  busy: _checkpointBBusy,
                  report: _checkpointBReport,
                  error: _checkpointBError,
                  elapsedMilliseconds: _checkpointBMilliseconds,
                  onRun: _runCheckpointB,
                ),
                const SizedBox(height: 16),
                _CheckpointCCard(
                  selectedSeed: _selectedSeed,
                  status: status,
                  visibleBlocks: _managedBinding?.visibleBlocks,
                  seedPreparationMilliseconds: _seedPreparationMilliseconds,
                  openToCurrentMilliseconds: _openToCurrentMilliseconds,
                  latestEditToCurrentMilliseconds:
                      _lastEditToCurrentMilliseconds,
                  foregroundApplyMilliseconds: _lastForegroundApplyMilliseconds,
                ),
                const SizedBox(height: 16),
                _LifecycleControls(
                  selectedSeed: _selectedSeed,
                  busy: _lifecycleBusy,
                  hasRuntime: runtime != null,
                  canRecover: status?.recoveryAvailable ?? false,
                  onOpenSeed: _openSeed,
                  onReopen: () => _openSeed(_selectedSeed),
                  onClose: _closeFromUi,
                  onRecover: _recoverFaultedRuntime,
                ),
                const SizedBox(height: 8),
                Text(_lifecycleNote),
                if (_error != null) ...[
                  const SizedBox(height: 12),
                  _ErrorPanel(message: _error!),
                ],
                const SizedBox(height: 16),
                _SectionCard(
                  title: 'Runtime state',
                  subtitle:
                      'Public snapshots only. “Pending edits” is this lab’s '
                      'receipt count, not an internal parser queue.',
                  child: Wrap(
                    spacing: 10,
                    runSpacing: 10,
                    children: [
                      _MetricTile(
                        key: const Key('v3-engine-lab-build-mode'),
                        label: 'Build mode',
                        value: _buildModeLabel,
                      ),
                      _MetricTile(
                        label: 'State',
                        value: status?.state.name ?? 'not open',
                      ),
                      _MetricTile(
                        key: const Key('v3-engine-lab-source-length'),
                        label: 'Exact source length',
                        value: _formatUtf16(sourceLengthUtf16),
                      ),
                      _MetricTile(
                        key: const Key('v3-engine-lab-source-revision'),
                        label: 'Source revision',
                        value: status?.sourceRevision.toString() ?? '—',
                      ),
                      _MetricTile(
                        label: 'Certified revision',
                        value:
                            status?.certifiedSourceRevision.toString() ?? '—',
                      ),
                      _MetricTile(
                        label: 'Structure revision',
                        value: structureRevision?.toString() ?? 'none',
                      ),
                      _MetricTile(
                        label: 'Source current',
                        value: _yesNo(status?.sourceCurrent),
                      ),
                      _MetricTile(
                        key: const Key('v3-engine-lab-structure-current'),
                        label: 'Structure current',
                        value: _yesNo(status?.structureCurrent),
                      ),
                      _MetricTile(
                        key: const Key(
                          'v3-engine-lab-inline-presentation-generation',
                        ),
                        label: 'Inline presentation generation',
                        value:
                            status?.inlinePresentationGeneration.toString() ??
                            '—',
                      ),
                      _MetricTile(
                        label: 'Certification lag',
                        value: _formatLag(certifiedLag),
                      ),
                      _MetricTile(
                        label: 'Structure lag',
                        value: _formatLag(structureLag),
                      ),
                      _MetricTile(
                        label: 'Recovery available',
                        value: _yesNo(status?.recoveryAvailable),
                      ),
                      _MetricTile(
                        label: 'Pending edit receipts',
                        value: _pendingEditStartMicros.length.toString(),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                _SectionCard(
                  title: 'Measured liveness',
                  subtitle:
                      'Wall-clock observations from this Flutter process; '
                      'they do not claim hidden parser-stage timing. For a '
                      'managed island, the edit clock begins after the managed '
                      'binding has accepted the source edit and updated the '
                      'visible presentation; its synchronous apply duration is '
                      'not exposed by this example seam.',
                  child: Wrap(
                    spacing: 10,
                    runSpacing: 10,
                    children: [
                      _MetricTile(
                        label: 'Seed preparation',
                        value: _formatDuration(_seedPreparationMilliseconds),
                      ),
                      _MetricTile(
                        key: const Key('v3-engine-lab-cold-open-current'),
                        label: 'Cold full open → exact',
                        value: _formatDuration(_openToCurrentMilliseconds),
                      ),
                      _MetricTile(
                        key: const Key('v3-engine-lab-latest-edit-current'),
                        label: _selectedSeed.usesManagedEditor
                            ? 'Visible island → exact'
                            : 'Latest edit → exact',
                        value: _formatDuration(_lastEditToCurrentMilliseconds),
                      ),
                      _MetricTile(
                        label: 'Foreground apply()',
                        value: _formatDuration(
                          _lastForegroundApplyMilliseconds,
                        ),
                      ),
                      _MetricTile(
                        label: 'Last truthful close',
                        value: _formatDuration(_lastCloseMilliseconds),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                _SectionCard(
                  title: 'Bounded active input island',
                  subtitle:
                      'At most $v3EngineLabMaximumActiveUtf16 UTF-16 units '
                      'enter TextEditingController, including for a '
                      '100,000-reference or 10 MiB document. Loaded '
                      'neighborhoods reserve '
                      '${v3EngineLabMaximumActiveUtf16 - v3EngineLabLoadedNeighborhoodUtf16} '
                      'units for normal insertion. Current range: '
                      '[$activeInputStartUtf16, $activeInputEndUtf16).',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _buildActiveEditor(writable),
                      if (_selectedSeed == V3EngineLabSeed.blockQuote) ...[
                        const SizedBox(height: 10),
                        const Text(
                          v3EngineLabBlockQuoteScope,
                          key: Key('v3-engine-lab-block-quote-scope'),
                        ),
                      ],
                      if (_selectedSeed == V3EngineLabSeed.bulletList) ...[
                        const SizedBox(height: 10),
                        const Text(
                          v3EngineLabBulletListScope,
                          key: Key('v3-engine-lab-bullet-list-scope'),
                        ),
                        const SizedBox(height: 12),
                        Wrap(
                          spacing: 8,
                          runSpacing: 8,
                          children: [
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-first-list-item',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      v3EngineLabBulletListFirstSource.indexOf(
                                            'α',
                                          ) +
                                          1,
                                      seed: V3EngineLabSeed.bulletList,
                                    )
                                  : null,
                              child: const Text('Select first item'),
                            ),
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-second-list-item',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      _bulletListSecondStartUtf16 +
                                          v3EngineLabBulletListSecondSource
                                              .indexOf('Edit') +
                                          2,
                                      seed: V3EngineLabSeed.bulletList,
                                    )
                                  : null,
                              child: const Text('Select second item'),
                            ),
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-empty-list-item',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      _bulletListTerminalStartUtf16 + 1,
                                      seed: V3EngineLabSeed.bulletList,
                                    )
                                  : null,
                              child: const Text('Select empty exit item'),
                            ),
                          ],
                        ),
                      ],
                      if (_selectedSeed == V3EngineLabSeed.orderedList) ...[
                        const SizedBox(height: 10),
                        const Text(
                          v3EngineLabOrderedListScope,
                          key: Key('v3-engine-lab-ordered-list-scope'),
                        ),
                      ],
                      if (_selectedSeed ==
                          V3EngineLabSeed.multiBlockParagraph) ...[
                        const SizedBox(height: 12),
                        Wrap(
                          spacing: 8,
                          runSpacing: 8,
                          children: [
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-first-paragraph',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      v3EngineLabMultiBlockFirstSource.indexOf(
                                            'First',
                                          ) +
                                          2,
                                      seed: V3EngineLabSeed.multiBlockParagraph,
                                    )
                                  : null,
                              child: const Text('Select first Paragraph'),
                            ),
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-middle-paragraph',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      _multiBlockMiddleStartUtf16 +
                                          v3EngineLabMultiBlockMiddleSource
                                              .indexOf('Middle') +
                                          2,
                                      seed: V3EngineLabSeed.multiBlockParagraph,
                                    )
                                  : null,
                              child: const Text('Select middle Paragraph'),
                            ),
                            OutlinedButton(
                              key: const Key(
                                'v3-engine-lab-select-tail-paragraph',
                              ),
                              onPressed: writable
                                  ? () => _selectManagedLeafAt(
                                      _multiBlockTailStartUtf16 +
                                          v3EngineLabMultiBlockTailSource
                                              .indexOf('Tail') +
                                          2,
                                      seed: V3EngineLabSeed.multiBlockParagraph,
                                    )
                                  : null,
                              child: const Text('Select tail Paragraph'),
                            ),
                          ],
                        ),
                      ],
                      const SizedBox(height: 12),
                      Text(
                        'Exact source · active island',
                        style: Theme.of(context).textTheme.labelMedium,
                      ),
                      const SizedBox(height: 4),
                      SelectableText(
                        activeInputSource,
                        key: const Key('v3-engine-lab-exact-source'),
                        style: const TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          height: 1.4,
                        ),
                      ),
                      if (canonicalFenceSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalFenceSource,
                          key: const Key(
                            'v3-engine-lab-canonical-fence-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalMultiBlockSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical multi-block Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalMultiBlockSource,
                          key: const Key(
                            'v3-engine-lab-canonical-multi-block-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalAtxHeadingSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical ATX Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalAtxHeadingSource,
                          key: const Key(
                            'v3-engine-lab-canonical-atx-heading-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalSetextHeadingSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical Setext Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalSetextHeadingSource,
                          key: const Key(
                            'v3-engine-lab-canonical-setext-heading-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalThematicBreakSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical thematic-break Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalThematicBreakSource,
                          key: const Key(
                            'v3-engine-lab-canonical-thematic-break-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalIndentedCodeSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical indented-code Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalIndentedCodeSource,
                          key: const Key(
                            'v3-engine-lab-canonical-indented-code-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalBlockQuoteSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical block-quote Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalBlockQuoteSource,
                          key: const Key(
                            'v3-engine-lab-canonical-block-quote-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalBulletListSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical bullet-list Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalBulletListSource,
                          key: const Key(
                            'v3-engine-lab-canonical-bullet-list-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                      if (canonicalOrderedListSource != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          'Canonical ordered-list Markdown · instrumentation only',
                          style: Theme.of(context).textTheme.labelMedium,
                        ),
                        const SizedBox(height: 4),
                        SelectableText(
                          canonicalOrderedListSource,
                          key: const Key(
                            'v3-engine-lab-canonical-ordered-list-source',
                          ),
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            height: 1.4,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                _SectionCard(
                  title: 'Bounded point query',
                  subtitle:
                      'Choose any source position. Loading its neighborhood '
                      'replaces only the bounded input island.',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                        'Position $_queryPositionUtf16 of $sourceLengthUtf16 UTF-16',
                      ),
                      Slider(
                        key: const Key('v3-engine-lab-query-slider'),
                        value: math
                            .min(_queryPositionUtf16, sourceLengthUtf16)
                            .toDouble(),
                        min: 0,
                        max: math.max(1, sourceLengthUtf16).toDouble(),
                        onChanged: runtime == null
                            ? null
                            : _selectQueryPosition,
                      ),
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        children: [
                          TextButton(
                            onPressed: runtime == null
                                ? null
                                : () => _jumpQueryTo(0),
                            child: const Text('Start'),
                          ),
                          TextButton(
                            onPressed: runtime == null
                                ? null
                                : () => _jumpQueryTo(sourceLengthUtf16 ~/ 2),
                            child: const Text('Middle'),
                          ),
                          TextButton(
                            onPressed: runtime == null
                                ? null
                                : () => _jumpQueryTo(sourceLengthUtf16),
                            child: const Text('End'),
                          ),
                          OutlinedButton.icon(
                            onPressed: runtime == null
                                ? null
                                : _loadQueryNeighborhood,
                            icon: const Icon(Icons.center_focus_strong),
                            label: const Text('Load neighborhood for editing'),
                          ),
                        ],
                      ),
                      const SizedBox(height: 12),
                      _QueryResultPanel(result: _queryResult),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                _SectionCard(
                  title: 'Explicit Flutter Web assets',
                  subtitle:
                      'Native ignores this Web-only configuration; both '
                      'platforms otherwise exercise the same Dart runtime API.',
                  child: SelectableText(
                    'Worker: ${(widget.webAssets ?? _flutterWebAssets).workerUri}\n'
                    'Wasm:   ${(widget.webAssets ?? _flutterWebAssets).wasmUri}',
                    style: const TextStyle(fontFamily: 'monospace'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Utf16LengthLimitingFormatter extends TextInputFormatter {
  const _Utf16LengthLimitingFormatter(this.maximumUtf16);

  final int maximumUtf16;

  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    return newValue.text.length <= maximumUtf16 ? newValue : oldValue;
  }
}

class _MilestoneBanner extends StatelessWidget {
  const _MilestoneBanner();

  @override
  Widget build(BuildContext context) {
    return Card(
      color: const Color(0xFFFFE7B3),
      child: const Padding(
        padding: EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'M1.1 GRAMMAR LIMIT — ENGINE OBSERVABILITY, NOT A RENDERER',
              style: TextStyle(fontWeight: FontWeight.w800),
            ),
            SizedBox(height: 6),
            Text(
              'The current structural milestone exactly partitions supported '
              'blank-separated Paragraph, Blank, DefinitionsOnly, top-level '
              'FencedCode, IndentedCode, ATX Heading, Setext Heading, and '
              'ThematicBreak leaves, plus depth-one single-Paragraph block '
              'quotes and top-level depth-one tight bullet and ordered lists. '
              'Nested, loose, and task lists; nested or multi-child quotes; HTML; '
              'tables; and other unsupported block openers deliberately fail '
              'closed as typed Unknown/source gaps. The small, ATX, and Setext '
              'seeds paint '
              'parser-certified strong, emphasis, inline-code, and '
              'strikethrough facts; the small seed also paints single- and '
              'two-scalar parser-certified character references plus '
              'exact-target URI and email angle autolinks. Character '
              'references inside its URI autolink cook both the visible label '
              'and destination from the same parser-authored value. All of their '
              'certified delimiters are hidden; both heading seeds also hide their '
              'block markers and apply parser-authored typography. The '
              'fenced-code seed hides opener, info, and closer syntax while '
              'keeping its parser-certified body literal and editable. The '
              'indented-code seed hides the parser-certified four-column '
              'prefix while keeping deeper indentation and literal body text. '
              'The block-quote seed hides parser-certified quote prefixes and '
              'paints a quote rail; parser-certified strong, emphasis, and '
              'inline-code styles compose through its projected coordinates. '
              'The bullet-list seed hides parser-certified '
              'item prefixes, paints a semantic gutter, and keeps selected-item '
              'editing on the same input client. The ordered-list seed paints '
              'the exact parser-authored marker outside marker-free item '
              'content and continues its number on Enter. The '
              'thematic-break seed paints one parser-certified atomic divider; '
              'its marker bytes remain canonical source and never enter '
              'EditableText. The '
              '4,096- and 100,000-reference fixtures attach that exact same '
              'small display-space input lease at the document tail, mapping '
              'edits and IME composition back to canonical Markdown without '
              'remounting. Cold full-document open and visible-tail edit '
              'convergence are reported separately. The 1 MiB and 10 MiB '
              'seeds remain deliberate '
              'single-line paragraph witnesses for the exact-clean path, not '
              'representative full-grammar files.',
            ),
          ],
        ),
      ),
    );
  }
}

class _CheckpointBCard extends StatelessWidget {
  const _CheckpointBCard({
    required this.busy,
    required this.report,
    required this.error,
    required this.elapsedMilliseconds,
    required this.onRun,
  });

  final bool busy;
  final Map<String, Object?>? report;
  final String? error;
  final double? elapsedMilliseconds;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final report = this.report;
    final steps = _jsonObjectList(report?['steps']);
    final lifecycle = _jsonObject(report?['lifecycle']);
    return _SectionCard(
      title: 'Feedback Checkpoint B · persistent incremental SourceFacts',
      subtitle:
          'Runs a fixed multi-page edit battery off the UI context through '
          'the native isolate or Web Worker. It proves exact crop/splice '
          'equality, retained page identity, cancellation, fallback, and '
          'reclamation. Checkpoint C below continues that authority chain '
          'through exact-base structural publication and the live UI loop.',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: FilledButton.icon(
              key: const Key('v3-engine-lab-checkpoint-b-run'),
              onPressed: busy ? null : onRun,
              icon: busy
                  ? const SizedBox.square(
                      dimension: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.science_outlined),
              label: Text(
                busy ? 'Running production proof…' : 'Run Checkpoint B proof',
              ),
            ),
          ),
          if (error != null) ...[
            const SizedBox(height: 12),
            _ErrorPanel(message: 'Checkpoint B failed: $error'),
          ],
          if (report != null) ...[
            const SizedBox(height: 14),
            Container(
              key: const Key('v3-engine-lab-checkpoint-b-pass'),
              padding: const EdgeInsets.all(14),
              decoration: BoxDecoration(
                color: const Color(0xFFDDF4E4),
                borderRadius: BorderRadius.circular(10),
              ),
              child: const Text(
                'PASS · all clean-equality, identity, lifecycle, and '
                'zero-residency checks succeeded',
                style: TextStyle(fontWeight: FontWeight.w800),
              ),
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: [
                _MetricTile(
                  label: 'Execution',
                  value: '${report['platform'] ?? 'unknown'} off-caller',
                ),
                _MetricTile(
                  label: 'Fixed fixture',
                  value: '${report['fixtureBytes'] ?? '—'} bytes',
                ),
                _MetricTile(label: 'Edit shapes', value: '${steps.length}'),
                _MetricTile(
                  label: 'Whole proof',
                  value: _formatDuration(elapsedMilliseconds),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text(
              'Cross-platform parity digest',
              style: Theme.of(context).textTheme.labelSmall,
            ),
            SelectableText(
              '${report['parityDigest'] ?? 'missing'}',
              key: const Key('v3-engine-lab-checkpoint-b-parity'),
              style: const TextStyle(
                fontFamily: 'monospace',
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 10),
            for (final step in steps) _CheckpointBStepPanel(step: step),
            if (lifecycle != null) ...[
              const SizedBox(height: 8),
              Text(
                'Lifecycle adversaries',
                style: Theme.of(context).textTheme.titleSmall,
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _CheckpointBCheckChip(
                    label: 'cancel reached promotion',
                    passed: lifecycle['cancellationReachedPromotion'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'base restored',
                    passed: lifecycle['baseRestoredAfterCancellation'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'cancelled build reclaimed',
                    passed: lifecycle['cancelledBuildReclaimed'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label:
                        'rapid edit '
                        '(${lifecycle['nearbyRapidEditLineageTransitions'] ?? '—'} lineages)',
                    passed: lifecycle['nearbyRapidEditMatchesClean'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'distant reuse rejected',
                    passed: lifecycle['distantEditReuseRejected'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'failed reuse preserved base',
                    passed: lifecycle['failedReusePreservedBase'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'clean fallback current',
                    passed: lifecycle['cleanFallbackReachedTarget'] == true,
                  ),
                  _CheckpointBCheckChip(
                    label: 'close reached zero',
                    passed: lifecycle['closedToZero'] == true,
                  ),
                ],
              ),
            ],
          ],
        ],
      ),
    );
  }
}

class _CheckpointCCard extends StatelessWidget {
  const _CheckpointCCard({
    required this.selectedSeed,
    required this.status,
    required this.visibleBlocks,
    required this.seedPreparationMilliseconds,
    required this.openToCurrentMilliseconds,
    required this.latestEditToCurrentMilliseconds,
    required this.foregroundApplyMilliseconds,
  });

  final V3EngineLabSeed selectedSeed;
  final FlarkV3DocumentRuntimeStatus? status;
  final FlarkV3FlutterVisibleBlockCoordinator? visibleBlocks;
  final double? seedPreparationMilliseconds;
  final double? openToCurrentMilliseconds;
  final double? latestEditToCurrentMilliseconds;
  final double? foregroundApplyMilliseconds;

  @override
  Widget build(BuildContext context) {
    final status = this.status;
    final exactCurrent =
        status?.structureCurrent == true &&
        status?.structureRevision == status?.sourceRevision;
    final referenceCount = selectedSeed.leadingReferenceCount;
    final projectedTail = selectedSeed.usesProjectedTailEditor;
    final multiBlock = selectedSeed == V3EngineLabSeed.multiBlockParagraph;
    final atxHeading = selectedSeed == V3EngineLabSeed.atxHeading;
    final setextHeading = selectedSeed == V3EngineLabSeed.setextHeading;
    final thematicBreak = selectedSeed == V3EngineLabSeed.thematicBreak;
    final fencedCode = selectedSeed == V3EngineLabSeed.fencedCode;
    final indentedCode = selectedSeed == V3EngineLabSeed.indentedCode;
    final blockQuote = selectedSeed == V3EngineLabSeed.blockQuote;
    final bulletList = selectedSeed == V3EngineLabSeed.bulletList;
    final orderedList = selectedSeed == V3EngineLabSeed.orderedList;
    final managedEditor = selectedSeed.usesManagedEditor;
    final visibleBlocks = this.visibleBlocks;
    final exactVisible = visibleBlocks?.exactValue;
    final visiblePhase = visibleBlocks?.phase.name ?? 'not attached';
    final visibleBlockCount = exactVisible?.blocks.length;
    final visibleCoverage = exactVisible?.coveredSource;
    return _SectionCard(
      title: 'Feedback Checkpoint C · exact-base live loop',
      subtitle: multiBlock
          ? 'This fixture selects a real nonzero Paragraph from a three-block '
                'document. Parser-authored inline facts hide only that leaf’s '
                'certified markers; the buttons move the same bounded '
                'EditableText and platform input client between first, '
                'middle, and tail Paragraphs while canonical Markdown remains '
                'document-owned.'
          : atxHeading
          ? 'This fixture exercises the first inline-bearing block other than '
                'Paragraph: parser-authored geometry hides the ATX opener and '
                'accepted closing hashes, while the same inline service hides '
                'strong/emphasis delimiters inside the projected heading '
                'content. Canonical Markdown remains document-owned.'
          : setextHeading
          ? 'This fixture proves the multiline heading form uses the same '
                'heading and inline presentation contracts: parser-authored '
                'geometry hides the Setext underline while the bounded input '
                'island edits only its inline content. Canonical Markdown, '
                'including CRLF, remains document-owned.'
          : thematicBreak
          ? 'This fixture renders a parser-certified thematic break as one '
                'atomic divider. Its marker line remains canonical document '
                'source and contributes no fake EditableText characters; '
                'Backspace or Delete removes the whole source atom on the '
                'same input client.'
          : fencedCode
          ? 'This fixture edits only the parser-certified code body. Its '
                'opener, info string, and closer remain canonical source '
                'outside the bounded EditableText; Markdown-looking code stays '
                'literal and the same input client survives open/closed fence '
                'transitions.'
          : indentedCode
          ? 'This fixture hides exactly the parser-certified four-column '
                'indentation from every code line while preserving deeper '
                'indentation and literal Markdown-looking body text. Enter '
                'writes canonical indentation back into document source.'
          : blockQuote
          ? 'This fixture hides parser-certified quote prefixes, paints one '
                'quote rail, and maps Enter to canonical `> ` continuation '
                'without replacing the input client. It proves one depth-one '
                'single-Paragraph quote with parser-certified strong, emphasis, '
                'and inline-code composition across physical lines; nested and '
                'multi-child quotes remain pending.'
          : bulletList
          ? 'This fixture selects one item from a parser-certified depth-one '
                'tight bullet list. The exact source marker stays document-owned '
                'while Flutter paints a gutter and edits marker-free item '
                'content on the same input client. Parser-certified bold, '
                'emphasis, and inline-code delimiters stay in canonical source '
                'while their content renders live. The item buttons exercise '
                'handoff, and the terminal empty item exercises canonical list '
                'exit. Ordered lists use their separate exact-marker '
                'checkpoint; nested, loose, and task list forms remain pending.'
          : orderedList
          ? 'This fixture selects the first item from one parser-certified '
                'top-level, depth-one tight ordered list. Its exact `007)` '
                'marker is paint-only outside marker-free editable content. '
                'Enter uses parser-authored `008) ` continuation and preserves '
                'the canonical CRLF without replacing the input client. '
                'Nested, loose, and task list forms remain pending.'
          : projectedTail
          ? 'This fixture cold-opens the entire canonical document before '
                'attaching the shared bounded tail editor. That full-open '
                'latency is reported separately from subsequent tail-edit '
                'convergence; this checkpoint does not claim incremental cold '
                'startup. The production Worker/isolate authenticates each '
                'exact-base update while one stable EditableText retains its '
                'marker-free projection.'
          : 'This giant single-paragraph fixture is a structural/liveness '
                'witness with a bounded literal source neighborhood. Choose a '
                'managed fixture to test marker-free live editing.',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            key: const Key('v3-engine-lab-checkpoint-c-state'),
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: exactCurrent
                  ? const Color(0xFFDDF4E4)
                  : const Color(0xFFFFE7B3),
              borderRadius: BorderRadius.circular(10),
            ),
            child: Text(
              exactCurrent
                  ? 'EXACT CURRENT · source and structural authority match'
                  : 'STABLE/WAITING · exact-current authority is pending',
              style: const TextStyle(fontWeight: FontWeight.w800),
            ),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              _MetricTile(
                key: const Key('v3-engine-lab-checkpoint-c-fixture'),
                label: 'Selected fixture',
                value: selectedSeed.label,
              ),
              _MetricTile(
                label: 'Leading references',
                value: referenceCount == null
                    ? 'not a ref-tail seed'
                    : _formatInteger(referenceCount),
              ),
              _MetricTile(
                label: multiBlock
                    ? 'Selected Paragraph'
                    : atxHeading || setextHeading
                    ? 'Certified heading content'
                    : thematicBreak
                    ? 'Atomic source projection'
                    : fencedCode || indentedCode
                    ? 'Certified code body'
                    : bulletList || orderedList
                    ? 'Selected list item'
                    : 'Shared source tail',
                value: multiBlock
                    ? '${v3EngineLabMultiBlockMiddleSource.length} u16'
                    : atxHeading
                    ? '${v3EngineLabAtxHeadingDisplay.length} u16'
                    : setextHeading
                    ? '${v3EngineLabSetextHeadingDisplay.length} u16'
                    : thematicBreak
                    ? '${v3EngineLabThematicBreakDisplay.length} u16 display · '
                          '${v3EngineLabThematicBreakSource.length} u16 source'
                    : fencedCode
                    ? '${v3EngineLabFencedCodeBody.length} u16'
                    : indentedCode
                    ? '${v3EngineLabIndentedCodeDisplay.length} u16 display · '
                          '${v3EngineLabIndentedCodeSource.length} u16 source'
                    : bulletList
                    ? '${v3EngineLabBulletListSecondDisplay.length} u16 display · '
                          '${v3EngineLabBulletListSource.length} u16 source'
                    : orderedList
                    ? '${v3EngineLabOrderedListDisplay.length} u16 display · '
                          '${v3EngineLabOrderedListSource.length} u16 source'
                    : projectedTail
                    ? '${v3EngineLabEditableTailSource.length} u16'
                    : 'not attached',
              ),
              _MetricTile(
                label: 'Fixture preparation',
                value: _formatDuration(seedPreparationMilliseconds),
              ),
              _MetricTile(
                label: 'Cold full open → exact',
                value: _formatDuration(openToCurrentMilliseconds),
              ),
              _MetricTile(
                label: 'Source → structure',
                value:
                    '${status?.sourceRevision ?? '—'} → '
                    '${status?.structureRevision ?? '—'}',
              ),
              _MetricTile(
                label: 'Foreground edit',
                value: managedEditor
                    ? 'not exposed'
                    : _formatDuration(foregroundApplyMilliseconds),
              ),
              _MetricTile(
                label: managedEditor
                    ? 'Visible island → exact'
                    : 'Edit → exact current',
                value: _formatDuration(latestEditToCurrentMilliseconds),
              ),
              _MetricTile(
                key: const Key('v3-engine-lab-checkpoint-c-visible-range'),
                label: 'Visible block range',
                value: visibleBlockCount == null
                    ? visiblePhase
                    : '$visiblePhase · $visibleBlockCount block'
                          '${visibleBlockCount == 1 ? '' : 's'}',
              ),
              _MetricTile(
                key: const Key('v3-engine-lab-checkpoint-c-visible-work'),
                label: 'Visible range work',
                value: visibleBlocks == null
                    ? 'not attached'
                    : '${visibleBlocks.boundedAdvanceCount} bounded '
                          '${visibleBlocks.boundedAdvanceCount == 1 ? 'quantum' : 'quanta'}'
                          '${visibleCoverage == null ? '' : ' · '
                                    '${visibleCoverage.startUtf16}–'
                                    '${visibleCoverage.endUtf16} u16'}',
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _CheckpointBStepPanel extends StatelessWidget {
  const _CheckpointBStepPanel({required this.step});

  final Map<String, Object?> step;

  @override
  Widget build(BuildContext context) {
    final work = _jsonObject(step['work']) ?? const <String, Object?>{};
    final pages = _jsonObjectList(step['afterPages']);
    final retained = pages
        .where((page) => '${page['classification']}'.startsWith('retained'))
        .length;
    final rescanned = pages.length - retained;
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      clipBehavior: Clip.antiAlias,
      child: ExpansionTile(
        title: Text(
          '${step['label'] ?? 'unnamed edit'}',
          style: const TextStyle(fontWeight: FontWeight.w700),
        ),
        subtitle: Text(
          '${step['cropBytes'] ?? '—'} bytes rescanned · '
          '$retained retained pages · $rescanned replacement pages',
        ),
        childrenPadding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        expandedCrossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              _MetricTile(
                label: 'Base → target pages',
                value:
                    '${step['basePages'] ?? '—'} → '
                    '${step['targetPages'] ?? '—'}',
              ),
              _MetricTile(
                label: 'Target byte crop',
                value:
                    '[${step['cropByteStart'] ?? '—'}, '
                    '${step['cropByteEnd'] ?? '—'})',
              ),
              _MetricTile(
                label: 'Base page crop',
                value:
                    '[${step['basePageStart'] ?? '—'}, '
                    '${step['basePageEnd'] ?? '—'})',
              ),
              _MetricTile(
                label: 'Scanned bytes',
                value: '${work['scannedBytes'] ?? '—'}',
              ),
              _MetricTile(
                label: 'Leaves reused',
                value: '${work['leavesReused'] ?? '—'}',
              ),
              _MetricTile(
                label: 'Branches allocated',
                value: '${work['branchesAllocated'] ?? '—'}',
              ),
              _MetricTile(
                label: 'Nodes visited',
                value: '${work['nodesVisited'] ?? '—'}',
              ),
              _MetricTile(
                label: 'Max atomic height',
                value: '${work['maximumAtomicHeight'] ?? '—'}',
              ),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _CheckpointBCheckChip(
                label: 'matches clean oracle',
                passed: step['summaryMatchesClean'] == true,
              ),
              _CheckpointBCheckChip(
                label: 'absolute terminal exact',
                passed: step['absoluteTerminalMatchesRoot'] == true,
              ),
              _CheckpointBCheckChip(
                label: '${step['lineageTransitions'] ?? '—'} lineage(s)',
                passed: true,
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text(
            'After-page identity map',
            style: Theme.of(context).textTheme.labelMedium,
          ),
          const SizedBox(height: 6),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final page in pages) _CheckpointBPageChip(page: page),
            ],
          ),
        ],
      ),
    );
  }
}

class _CheckpointBPageChip extends StatelessWidget {
  const _CheckpointBPageChip({required this.page});

  final Map<String, Object?> page;

  @override
  Widget build(BuildContext context) {
    final classification = '${page['classification'] ?? 'unknown'}';
    final retained = classification.startsWith('retained');
    final ordinal = page['ordinal'] ?? '—';
    final baseOrdinal = page['baseOrdinal'];
    final id = '${page['id'] ?? 'missing'}';
    final digest = '${page['digest'] ?? 'missing'}';
    return Tooltip(
      message:
          '$classification\nArenaId $id\ncanonical digest $digest\n'
          'checkpoints ${page['checkpointCount'] ?? '—'}',
      child: Chip(
        avatar: Icon(
          retained ? Icons.link : Icons.autorenew,
          size: 16,
          color: retained
              ? const Color(0xFF1D6B3B)
              : Theme.of(context).colorScheme.primary,
        ),
        label: Text(
          baseOrdinal == null ? 'p$ordinal new' : 'p$ordinal ← p$baseOrdinal',
          style: const TextStyle(fontFamily: 'monospace'),
        ),
      ),
    );
  }
}

class _CheckpointBCheckChip extends StatelessWidget {
  const _CheckpointBCheckChip({required this.label, required this.passed});

  final String label;
  final bool passed;

  @override
  Widget build(BuildContext context) {
    return Chip(
      avatar: Icon(
        passed ? Icons.check_circle : Icons.cancel,
        size: 17,
        color: passed
            ? const Color(0xFF1D6B3B)
            : Theme.of(context).colorScheme.error,
      ),
      label: Text(label),
    );
  }
}

class _LifecycleControls extends StatelessWidget {
  const _LifecycleControls({
    required this.selectedSeed,
    required this.busy,
    required this.hasRuntime,
    required this.canRecover,
    required this.onOpenSeed,
    required this.onReopen,
    required this.onClose,
    required this.onRecover,
  });

  final V3EngineLabSeed selectedSeed;
  final bool busy;
  final bool hasRuntime;
  final bool canRecover;
  final ValueChanged<V3EngineLabSeed> onOpenSeed;
  final VoidCallback onReopen;
  final VoidCallback onClose;
  final VoidCallback onRecover;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final seed in V3EngineLabSeed.values)
          OutlinedButton(
            onPressed: busy ? null : () => onOpenSeed(seed),
            child: Text('Open ${seed.label}'),
          ),
        FilledButton.icon(
          onPressed: busy ? null : onReopen,
          icon: const Icon(Icons.refresh),
          label: Text('Reopen ${selectedSeed.label}'),
        ),
        OutlinedButton.icon(
          onPressed: busy || !hasRuntime ? null : onClose,
          icon: const Icon(Icons.power_settings_new),
          label: const Text('Close and await receipt'),
        ),
        Tooltip(
          message: canRecover
              ? 'Recover parsing authority from the retained exact source.'
              : 'Recovery is available only after a recoverable parser fault.',
          child: OutlinedButton.icon(
            onPressed: busy || !canRecover ? null : onRecover,
            icon: const Icon(Icons.restart_alt),
            label: const Text('Recover faulted runtime'),
          ),
        ),
      ],
    );
  }
}

class _SectionCard extends StatelessWidget {
  const _SectionCard({
    required this.title,
    required this.subtitle,
    required this.child,
  });

  final String title;
  final String subtitle;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(title, style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 3),
            Text(subtitle, style: Theme.of(context).textTheme.bodySmall),
            const SizedBox(height: 14),
            child,
          ],
        ),
      ),
    );
  }
}

class _MetricTile extends StatelessWidget {
  const _MetricTile({super.key, required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 166,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: Theme.of(context).textTheme.labelSmall),
          const SizedBox(height: 3),
          Text(
            value,
            style: const TextStyle(
              fontFamily: 'monospace',
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}

class _QueryResultPanel extends StatelessWidget {
  const _QueryResultPanel({required this.result});

  final FlarkV3DocumentQueryResult? result;

  @override
  Widget build(BuildContext context) {
    final result = this.result;
    if (result == null) {
      return const Text('No query result is currently available.');
    }

    final (String, String) description = switch (result) {
      FlarkV3DocumentPendingQuery pending => (
        'PENDING · ${pending.reason.name}',
        'Source revision ${pending.sourceRevision}; stable paint-only '
            'structure ${pending.stableStructureRevision ?? 'none'}.',
      ),
      FlarkV3DocumentSourceGapQuery gap => (
        'SOURCE GAP · ${gap.reason.name}',
        'Source revision ${gap.sourceRevision}; structure '
            '${gap.structureRevision}; range ${_formatSpan(gap.range)}.',
      ),
      FlarkV3RecursiveGreenPointQuery recursive => (
        'EXACT RECURSIVE GREEN · '
            '${recursive.owner.kind?.name ?? 'kind-${recursive.owner.kindId}'}',
        'Source/structure ${recursive.sourceRevision}/'
            '${recursive.structureRevision}; '
            'atom ${_formatSpan(recursive.source)}; '
            '${recursive.coveragePart.name}/'
            '${recursive.logicalAtom.kind.name}; ancestry '
            '${recursive.ancestry.length}; work '
            '${recursive.work.eventsScanned} events across '
            '${recursive.work.storagePagesVisited} pages.',
      ),
      FlarkV3DocumentStructuralQuery structural => (
        'EXACT STRUCTURE · ${structural.structure.kind.name}',
        'Source/structure ${structural.sourceRevision}/'
            '${structural.structureRevision}; '
            'source ${_formatSpan(structural.structure.source)}; visible '
            '${_formatSpan(structural.structure.visibleSource)}; projection '
            'runs ${structural.projection.runCount}; references '
            '${structural.structure.referenceDefinitionCount}; '
            '${_formatUnknown(structural.structure.unknownReason)}.',
      ),
    };

    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        border: Border.all(color: Theme.of(context).colorScheme.outlineVariant),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            description.$1,
            style: const TextStyle(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 5),
          SelectableText(
            description.$2,
            style: const TextStyle(fontFamily: 'monospace'),
          ),
        ],
      ),
    );
  }
}

class _ErrorPanel extends StatelessWidget {
  const _ErrorPanel({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Card(
      color: Theme.of(context).colorScheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Text(
          message,
          style: TextStyle(
            color: Theme.of(context).colorScheme.onErrorContainer,
          ),
        ),
      ),
    );
  }
}

Map<String, Object?>? _jsonObject(Object? value) {
  if (value is Map<String, Object?>) return value;
  if (value is! Map) return null;
  final result = <String, Object?>{};
  for (final entry in value.entries) {
    if (entry.key case final String key) result[key] = entry.value;
  }
  return result;
}

List<Map<String, Object?>> _jsonObjectList(Object? value) {
  if (value is! List) return const <Map<String, Object?>>[];
  return [for (final item in value) ?_jsonObject(item)];
}

String _formatSpan(FlarkV3SourceSpan span) =>
    'u16[${span.startUtf16},${span.endUtf16}) '
    'u8[${span.startUtf8},${span.endUtf8})';

String _formatUnknown(FlarkV3DocumentUnknownReason? reason) {
  if (reason == null) return 'no Unknown fallback';
  return switch (reason) {
    FlarkV3DocumentUnknownReason.blankBoundary => 'Unknown: blank boundary',
    FlarkV3DocumentUnknownReason.unsupportedSource =>
      'Unknown: unsupported source',
  };
}

String _yesNo(bool? value) => value == null
    ? '—'
    : value
    ? 'yes'
    : 'no';

String _formatLag(int? value) => value == null ? '—' : '$value rev';

String _formatDuration(double? milliseconds) => milliseconds == null
    ? 'not observed'
    : '${milliseconds.toStringAsFixed(2)} ms';

String _formatInteger(int value) {
  final digits = '$value';
  final output = StringBuffer();
  for (var index = 0; index < digits.length; index += 1) {
    if (index > 0 && (digits.length - index) % 3 == 0) output.write(',');
    output.write(digits[index]);
  }
  return output.toString();
}

String _formatUtf16(int value) {
  if (value >= _oneMebibyte) {
    return '$value u16 (${(value / _oneMebibyte).toStringAsFixed(2)} Mi)';
  }
  if (value >= 1024) {
    return '$value u16 (${(value / 1024).toStringAsFixed(1)} Ki)';
  }
  return '$value u16';
}
