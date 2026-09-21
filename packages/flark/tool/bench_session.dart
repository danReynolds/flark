import 'dart:convert';
import 'package:flark/flark.dart';
import 'package:flark/session.dart';

/// Diagnostic of publication overhead, not host-frame or device qualification.
Future<void> main() async {
  final backend = createParseBackend();
  const paragraph =
      'A paragraph with **bold** and [a link](https://dart.dev).\n\n';
  final results = <Map<String, Object>>[];
  for (final repeats in [80, 250]) {
    final source = paragraph * repeats;
    final editor = FlarkEditor(backend, text: source);
    editor.apply(SetSelection(source.length, source.length));
    final session = FlarkSession(
      markdown: source,
      backendLoader: () async => FlarkBackendLease(backend, () {}),
    );
    await session.ready;
    session.command(SetSelection(source.length, source.length));
    final timings = {true: <int>[], false: <int>[]};
    for (var i = 0; i < 250; i++) {
      for (final public in i.isEven ? [true, false] : [false, true]) {
        final watch = Stopwatch()..start();
        final accepted = public
            ? session.command(const InsertText('x')).accepted
            : editor.apply(const InsertText('x'));
        watch.stop();
        if (!accepted) throw StateError('Benchmark edit rejected');
        if (i >= 50) timings[public]!.add(watch.elapsedMicroseconds);
        if (public) {
          session.command(const Undo());
        } else {
          editor.apply(const Undo());
        }
      }
    }
    for (final public in [false, true]) {
      final sorted = timings[public]!..sort();
      results.add({
        'path': public ? 'session' : 'engine',
        'sourceBytes': utf8.encode(source).length,
        'samples': sorted.length,
        'p50Us': sorted[sorted.length ~/ 2],
        'p99Us': sorted[(sorted.length * .99).floor()],
        'maxUs': sorted.last,
      });
    }
    session.dispose();
  }
  (backend as dynamic).dispose();
  print(const JsonEncoder.withIndent('  ').convert(results));
}
