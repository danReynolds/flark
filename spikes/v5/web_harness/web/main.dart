// Browser keystroke spike: comrak compiled to wasm32-unknown-unknown, loaded
// through dart:js_interop only (no dart:ui_web), driven from dart2js.
import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:math';
import 'dart:typed_data';
import 'package:web/web.dart' as web;
import 'package:flark_v5_web_harness/projection.dart';

@JS('WebAssembly.instantiateStreaming')
external JSPromise<JSObject> _instantiateStreaming(JSPromise<web.Response> source, JSObject imports);

void log(String s) { web.console.log(s.toJS); final el = web.document.getElementById('out'); if (el != null) { el.textContent = '${el.textContent}$s\n'; } }

Future<void> main() async {
  try { await run(); } catch (e, st) { log('ERROR: $e\n$st'); }
}

Future<void> run() async {
  final t0 = web.window.performance.now();
  final result = await _instantiateStreaming(web.window.fetch('flark_parse_spike.wasm'.toJS), JSObject()).toDart;
  final instance = result.getProperty<JSObject>('instance'.toJS);
  final exports = instance.getProperty<JSObject>('exports'.toJS);
  final memory = exports.getProperty<JSObject>('memory'.toJS);
  final alloc = exports.getProperty<JSFunction>('flark_spike_alloc'.toJS);
  final free = exports.getProperty<JSFunction>('flark_spike_free'.toJS);
  final parse = exports.getProperty<JSFunction>('flark_spike_parse'.toJS);
  final t1 = web.window.performance.now();
  log('wasm instantiated in ${(t1 - t0).toStringAsFixed(1)} ms');

  JSUint8Array heap() => JSUint8Array(memory.getProperty<JSArrayBuffer>('buffer'.toJS));
  int call1(JSFunction f, int a) => (f.callAsFunction(null, a.toJS) as JSNumber).toDartInt;
  int call4(JSFunction f, int a, int b, int c, int d) => (f.callAsFunction(null, a.toJS, b.toJS, c.toJS, d.toJS) as JSNumber).toDartInt;
  void callVoid(JSFunction f, int a, int b) { f.callAsFunction(null, a.toJS, b.toJS); }

  final outCell = call1(alloc, 16);
  final rng = Random(7);
  var pass = 0;
  for (final size in [25000, 25000, 64000, 100000, 25000]) {
    pass++;
    var src = gen(size);
    final cap = size * 2;
    final input = call1(alloc, cap);
    final totals = <double>[], parses = <double>[], projects = <double>[];
    var rowsOut = 0, modelLen = 0;
    for (var k = 0; k < 200; k++) {
      final pos = rng.nextInt(src.length);
      final ch = const ['x', ' ', '*', 'a'][rng.nextInt(4)];
      final a = web.window.performance.now();
      src = src.substring(0, pos) + ch + src.substring(pos);
      final bytes = utf8.encode(src);
      heap().toDart.setRange(input, input + bytes.length, bytes);
      final b = web.window.performance.now();
      final rc = call4(parse, input, bytes.length, outCell, outCell + 8);
      if (rc != 0) { log('parse rc=$rc'); return; }
      final c = web.window.performance.now();
      final h = heap().toDart; // memory may have grown
      final view = ByteData.sublistView(h);
      final outPtr = view.getUint32(outCell, Endian.little), outLen = view.getUint32(outCell + 8, Endian.little);
      final model = Model(Uint8List.sublistView(h, outPtr, outPtr + outLen));
      final rows = project(model, src);
      final d = web.window.performance.now();
      callVoid(free, outPtr, outLen);
      rowsOut = rows.length; modelLen = outLen;
      if (k >= 20) { totals.add(d - a); parses.add(c - b); projects.add(d - c); }
    }
    double p(List<double> l, double q) { final s = [...l]..sort(); return s[(s.length * q).floor().clamp(0, s.length - 1)]; }
    String f(List<double> l) => '${p(l, .5).toStringAsFixed(2)}/${p(l, .9).toStringAsFixed(2)}/${p(l, .99).toStringAsFixed(2)}';
    if (pass == 1) { callVoid(free, input, cap); continue; }
    log('${src.length} B  total ${f(totals)}  parse+extract ${f(parses)}  decode+project ${f(projects)}  (ms p50/p90/p99; model ${(modelLen / 1024).round()} KiB, $rowsOut rows)');
    callVoid(free, input, cap);
  }
  log('done');
}
