/// Browser frame profile for the web byte limit. It mirrors
/// `frame_profile_test.dart`: the editor widget at the candidate limits, fed
/// platform text deltas and commands, with each edit linked to the engine
/// frame that painted it. Run it as a normal release app:
///
/// ```sh
/// flutter build web --wasm -t integration_test/web_frame_profile.dart
/// python3 tool/serve_web.py
/// ```
///
/// Open the served page in a visible browser tab and leave it in front: a
/// hidden tab throttles frames, and a visibility change rejects the run.
/// Query parameters: `bytes` (32768), `shapes` and `sites` (comma lists, all
/// by default), `iterations` (120, of which 20 warm up) and `observe` (1;
/// 0 links frames without the paint observer and its checks). Each workload
/// prints `FLARK_WEB_FRAME_RECEIPT`; `flarkWebFrameReceipts` on the page's
/// global object holds the linked samples. Edits skip DOM input handling,
/// which `test/web_input_test.dart` covers.
library;

import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:ui';
import 'package:flark/session.dart' show flarkDefaultLiveBytes;
import 'package:flark_dogfood/backend.dart';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/foundation.dart' show kIsWasm;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:web/web.dart' as web;

import '../test_driver/frame_gate.dart';

/// The engine's frame clock on the web.
int _nowUs() => (web.window.performance.now() * 1000).toInt();

int _p50(List<int> values) => (values.toList()..sort())[values.length ~/ 2];

/// The WebGL renderer behind the canvas: a software rasterizer is not a
/// receipt for a user's browser.
String _gpu() {
  final canvas = web.document.createElement('canvas') as web.HTMLCanvasElement;
  final gl = canvas.getContext('webgl2') as web.WebGL2RenderingContext?;
  if (gl == null) return 'none';
  final info = gl.getExtension('WEBGL_debug_renderer_info');
  final name = info == null
      ? gl.getParameter(web.WebGLRenderingContext.RENDERER)
      : gl.getParameter(0x9246); // UNMASKED_RENDERER_WEBGL
  return name.dartify().toString();
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final backend = await loadBackend();
  final code = await FlarkTreeSitter.load();
  runApp(_WebFrameProfile(backend, code));
}

class _WebFrameProfile extends StatefulWidget {
  const _WebFrameProfile(this.backend, this.code);
  final FlarkParseBackend backend;
  final FlarkTreeSitter code;

  @override
  State<_WebFrameProfile> createState() => _WebFrameProfileState();
}

class _WebFrameProfileState extends State<_WebFrameProfile> {
  final _paints = <FlarkPaintObservation>[];
  final _frames = <int, FrameTiming>{};
  final _log = <String>[];
  FlarkController? _controller;
  bool _observe = true;
  bool _hidden = web.document.visibilityState != 'visible';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addTimingsCallback(_timings);
    web.document.addEventListener(
      'visibilitychange',
      (web.Event _) {
        if (web.document.visibilityState != 'visible') _hidden = true;
      }.toJS,
    );
    WidgetsBinding.instance.addPostFrameCallback((_) => _run());
  }

  void _timings(List<FrameTiming> batch) {
    for (final f in batch) {
      _frames[f.frameNumber] = f;
    }
  }

  void _say(String line) {
    // ignore: avoid_print
    print(line);
    setState(() => _log.add(line));
  }

  Future<void> _frame() {
    WidgetsBinding.instance.scheduleFrame();
    return WidgetsBinding.instance.endOfFrame;
  }

  /// Timings arrive in batches when a frame renders; keep frames coming.
  Future<void> _collect(Iterable<int> numbers) async {
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    while (numbers.any((n) => !_frames.containsKey(n)) &&
        DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
      await _frame();
    }
  }

  /// The engine reports 60 Hz on the web whatever the display does, so the
  /// next-frame gate uses the measured vsync interval.
  Future<double> _refreshRate() async {
    final numbers = <int>[];
    for (var i = 0; i < 61; i++) {
      await _frame();
      numbers.add(PlatformDispatcher.instance.frameData.frameNumber);
    }
    await _collect(numbers);
    final vsyncs = [
      for (final n in numbers)
        if (_frames[n] case final f?)
          f.timestampInMicroseconds(FramePhase.vsyncStart),
    ];
    return 1e6 /
        _p50([
          for (var i = 1; i < vsyncs.length; i++) vsyncs[i] - vsyncs[i - 1],
        ]);
  }

  Future<void> _run() async {
    final params = Uri.base.queryParameters;
    final bytes = int.parse(params['bytes'] ?? '$candidateLiveBytes');
    final iterations = int.parse(params['iterations'] ?? '120');
    final shapes = params['shapes']?.split(',') ?? profileCycles.keys.toList();
    final sites =
        params['sites']?.split(',') ?? const ['start', 'largest block', 'end'];
    _observe = params['observe'] != '0';
    final backend = widget.backend;
    final view = PlatformDispatcher.instance.views.first;
    if (_hidden || view.physicalSize.isEmpty) {
      _say('FLARK_WEB_FRAME_DONE ["the page is hidden or has no size"]');
      return;
    }
    final gpu = _gpu();
    final hz = await _refreshRate();
    _say('FLARK_WEB_FRAME_PROGRESS measured ${hz.toStringAsFixed(1)} Hz');
    final receipts = <Map<String, Object>>[];
    final failures = <String>[];
    for (final shape in shapes) {
      for (final site in sites) {
        final text = boundedProfileSource(
          backend,
          shape,
          site == 'start' ? bytes - profileStartHeadroom : bytes,
        );
        final c = FlarkController(
          FlarkEditor(
            backend,
            codeEditing: widget.code,
            text: text,
            caret: text.length,
            syncLimit: bytes,
            liveLimits: candidateLiveLimits,
          ),
        );
        if (c.editor.sourceMode) {
          failures.add('$shape $site admission');
          c.dispose();
          continue;
        }
        if (site != 'end') {
          final caret = profileCaret(c.editor.projection, site, text);
          c.command(SetSelection(caret, caret));
        }
        final caret = c.editor.selection.extent;
        final insertedSource = text.replaceRange(caret, caret, 'x');
        final kernelUs = <int>[], parseUs = <int>[], projectUs = <int>[];
        for (var k = 0; k < 100; k++) {
          var start = _nowUs();
          final model = backend.parse(text);
          parseUs.add(_nowUs() - start);
          start = _nowUs();
          Projection.of(model, text);
          projectUs.add(_nowUs() - start);
          start = _nowUs();
          c.command(const InsertText('x'));
          kernelUs.add(_nowUs() - start);
          c.command(const DeleteBackward());
        }
        setState(() => _controller = c);
        for (var i = 0; i < 6; i++) {
          await _frame();
        }
        final samples = <Map<String, Object>>[];
        void fail(String what) => failures.add('$shape $site $what');
        for (var i = 0; i < iterations; i++) {
          for (final (operation, command) in profileOperations(site)) {
            _paints.clear();
            final start = _nowUs();
            final bool applied;
            if (operation == 'insert') {
              final value = c.value;
              applied = c.receiveDeltas([
                TextEditingDeltaInsertion(
                  oldText: value.text,
                  textInserted: 'x',
                  insertionOffset: value.selection.extentOffset,
                  selection: TextSelection.collapsed(
                    offset: value.selection.extentOffset + 1,
                  ),
                  composing: TextRange.empty,
                ),
              ]);
            } else {
              applied = c.command(command);
            }
            final commandUs = _nowUs() - start;
            if (!applied) fail('$operation not applied');
            if (c.editor.sourceMode) fail('$operation left live rendering');
            if (operation == 'insert' && c.text != insertedSource) {
              fail('insert source');
            }
            if ((operation == 'delete' || operation.startsWith('undo')) &&
                c.text != text) {
              fail('$operation source');
            }
            final revision = c.editor.revision, expected = c.text;
            await WidgetsBinding.instance.endOfFrame;
            if (_observe) {
              if (_paints.isEmpty) fail('$operation without a paint');
              for (final p in _paints) {
                if (p.revision != revision ||
                    p.caret == null ||
                    p.caretSource != c.editor.selection.extent ||
                    p.snapshot.source != expected) {
                  fail('$operation painted a stale frame');
                }
              }
            }
            if (i >= 20) {
              samples.add({
                'operation': operation,
                'startUs': start,
                'frameNumber': _observe && _paints.isNotEmpty
                    ? _paints.last.frameNumber
                    : PlatformDispatcher.instance.frameData.frameNumber,
                'commandUs': commandUs,
              });
            }
            await Future<void>.delayed(const Duration(milliseconds: 20));
          }
        }
        await _collect([for (final s in samples) s['frameNumber'] as int]);
        final linked = <Map<String, Object>>[];
        for (final s in samples) {
          final frame = _frames[s['frameNumber']];
          if (frame == null) {
            fail('frame ${s['frameNumber']} without timings');
            continue;
          }
          int at(FramePhase phase) => frame.timestampInMicroseconds(phase);
          final start = s['startUs'] as int;
          linked.add({
            ...s,
            'vsyncStartUs': at(FramePhase.vsyncStart),
            'buildStartUs': at(FramePhase.buildStart),
            'rasterStartUs': at(FramePhase.rasterStart),
            'rasterFinishUs': at(FramePhase.rasterFinish),
            'buildUs': frame.buildDuration.inMicroseconds,
            'rasterUs': frame.rasterDuration.inMicroseconds,
            'latencyUs': at(FramePhase.rasterFinish) - start,
            // Build and raster share the page's thread unless the renderer
            // rasterizes in a worker: command plus the whole frame.
            'mainThreadUs':
                (s['commandUs'] as int) +
                at(FramePhase.rasterFinish) -
                at(FramePhase.buildStart),
          });
        }
        final operations = {for (final s in linked) s['operation'] as String};
        final gates = {
          for (final operation in operations)
            operation: FrameGateResult(
              [
                for (final s in linked)
                  if (s['operation'] == operation) s,
              ],
              hz,
              budgetUs: frameBudgetUs,
            ),
        };
        for (final MapEntry(:key, :value) in gates.entries) {
          failures.addAll(value.failures('$shape $site $key', nextFrame: true));
        }
        List<int> field(String name) => [
          for (final s in linked) s[name] as int,
        ];
        final receipt = <String, Object>{
          'shape': shape,
          'site': site,
          'bytes': utf8.encode(text).length,
          'blocks': c.editor.document.model.blockCount,
          'runs': c.editor.document.model.runCount,
          'samples': linked.length,
          'observed': _observe,
          'parseP50Us': _p50(parseUs.skip(20).toList()),
          'projectionP50Us': _p50(projectUs.skip(20).toList()),
          'kernelOnlyP50Us': _p50(kernelUs.skip(20).toList()),
          'kernelOnlyP99Us': nearestRankP99(kernelUs.skip(20).toList()),
          if (linked.isNotEmpty) ...{
            'commandP50Us': _p50(field('commandUs')),
            'commandP99Us': nearestRankP99(field('commandUs')),
            'buildP50Us': _p50(field('buildUs')),
            'buildP99Us': nearestRankP99(field('buildUs')),
            'rasterP50Us': _p50(field('rasterUs')),
            'rasterP99Us': nearestRankP99(field('rasterUs')),
            'mainThreadP50Us': _p50(field('mainThreadUs')),
            'mainThreadP99Us': nearestRankP99(field('mainThreadUs')),
          },
          'gates': {
            for (final MapEntry(:key, :value) in gates.entries)
              key: value.toJson(),
          },
          'measuredHz': hz,
          'defaultLiveBytes': flarkDefaultLiveBytes,
          'wasm': kIsWasm,
          'crossOriginIsolated': web.window.crossOriginIsolated,
          'userAgent': web.window.navigator.userAgent,
          'gpu': gpu,
          'hiddenDuringRun': _hidden,
          'devicePixelRatio': view.devicePixelRatio,
          'logicalViewport': {
            'width': view.physicalSize.width / view.devicePixelRatio,
            'height': view.physicalSize.height / view.devicePixelRatio,
          },
        };
        receipts.add({...receipt, 'raw': linked});
        _say('FLARK_WEB_FRAME_RECEIPT ${jsonEncode(receipt)}');
        setState(() => _controller = null);
        await _frame();
        c.dispose();
      }
    }
    if (_hidden || web.document.visibilityState != 'visible') {
      failures.add('the page was hidden during the run');
    }
    if (view.physicalSize.isEmpty) failures.add('the view has no size');
    globalContext['flarkWebFrameReceipts'] = jsonEncode(receipts).toJS;
    _say('FLARK_WEB_FRAME_DONE ${jsonEncode(failures)}');
  }

  @override
  Widget build(BuildContext context) {
    final c = _controller;
    return MaterialApp(
      home: Scaffold(
        body: Column(
          children: [
            Expanded(
              child: c == null
                  ? const SizedBox()
                  : FlarkEditorWidget(
                      controller: c,
                      autofocus: true,
                      onPaint: _observe ? _paints.add : null,
                    ),
            ),
            SizedBox(
              height: 96,
              child: ListView(
                reverse: true,
                children: [
                  for (final line in _log.reversed)
                    Text(line, maxLines: 1, overflow: TextOverflow.ellipsis),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
