import 'dart:convert';
import 'dart:ffi';
import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flark_spike_ffi/flark_spike_ffi.dart';
import 'package:flark_spike_ffi/projection.dart';

void main() => runApp(const MaterialApp(home: BenchPage()));

class BenchPage extends StatefulWidget { const BenchPage({super.key}); @override State<BenchPage> createState() => _BenchState(); }

class _BenchState extends State<BenchPage> {
  final lines = <String>['starting…'];
  @override void initState() { super.initState(); WidgetsBinding.instance.addPostFrameCallback((_) => run()); }

  void out(String s) { debugPrint('FLARKBENCH $s'); setState(() => lines.add(s)); }

  Future<void> run() async {
    final outCell = flarkAlloc(16);
    final outPtr = outCell.cast<Pointer<Uint8>>();
    final outLen = Pointer<Uint32>.fromAddress(outCell.address + 8);
    await Future<void>.delayed(const Duration(seconds: 3));
    final rng = Random(7);
    var pass = 0;
    for (final size in [25000, 64000, 100000, 25000, 25000, 64000]) {
      pass++;
      var src = gen(size);
      final cap = size * 2;
      final input = flarkAlloc(cap);
      final inputView = input.asTypedList(cap);
      final totals = <double>[], parses = <double>[], projects = <double>[], encodes = <double>[];
      var rowsOut = 0, modelLen = 0;
      for (var k = 0; k < 200; k++) {
        final pos = rng.nextInt(src.length);
        final ch = const ['x', ' ', '*', 'a'][rng.nextInt(4)];
        final sw = Stopwatch()..start();
        src = src.substring(0, pos) + ch + src.substring(pos);
        final bytes = utf8.encode(src);
        inputView.setRange(0, bytes.length, bytes);
        final b = sw.elapsedMicroseconds;
        final rc = flarkParse(input, bytes.length, outPtr, outLen);
        if (rc != 0) { out('parse rc=$rc'); return; }
        final c = sw.elapsedMicroseconds;
        final len = outLen.value;
        final model = Model(outPtr.value.asTypedList(len));
        final rows = project(model, src);
        final d = sw.elapsedMicroseconds;
        flarkFree(outPtr.value, len);
        rowsOut = rows.length; modelLen = len;
        if (k >= 20) { totals.add(d / 1000); encodes.add(b / 1000); parses.add((c - b) / 1000); projects.add((d - c) / 1000); }
        if (k % 50 == 0) await Future<void>.delayed(Duration.zero);
      }
      flarkFree(input, cap);
      if (pass == 1) continue;
      double p(List<double> l, double q) { final s = [...l]..sort(); return s[(s.length * q).floor().clamp(0, s.length - 1)]; }
      String f(List<double> l) => '${p(l, .5).toStringAsFixed(2)}/${p(l, .9).toStringAsFixed(2)}/${p(l, .99).toStringAsFixed(2)}';
      out('${src.length} B total ${f(totals)} | splice+encode ${f(encodes)} parse+extract ${f(parses)} decode+project ${f(projects)} (ms p50/p90/p99; model ${(modelLen / 1024).round()} KiB, $rowsOut rows)');
    }
    out('done');
  }

  @override Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('flark v5 phone bench')),
    body: ListView(padding: const EdgeInsets.all(12), children: [for (final l in lines) SelectableText(l, style: const TextStyle(fontSize: 12, fontFamily: 'Menlo'))]),
  );
}
