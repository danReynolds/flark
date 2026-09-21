import 'dart:convert';
import 'package:flark/flark.dart';
import 'package:flark/rendering.dart';

/// Headless construction diagnostic. No host layout/paint or retained-memory
/// comparison is measured here. Run AOT for comparisons, not with a cold JIT.
void main() {
  final backend = createParseBackend();
  final source =
      '# Article\n\n${'A paragraph with **bold** and [a link](https://dart.dev).\n\n' * 80}';
  Object make(bool reader) => reader
      ? FlarkReadDocument(backend, source)
      : FlarkEditor(backend, text: source);
  for (var i = 0; i < 100; i++) {
    make(true);
    make(false);
  }
  final samples = <Map<String, Object>>[];
  for (final count in [1, 100]) {
    final times = {true: <int>[], false: <int>[]};
    for (var round = 0; round < 10; round++) {
      for (final reader in round.isEven ? [true, false] : [false, true]) {
        final watch = Stopwatch()..start();
        final documents = List.generate(count, (_) => make(reader));
        watch.stop();
        // Keep all documents observable until the construction interval ends.
        assert(documents.length == count);
        times[reader]!.add(watch.elapsedMicroseconds);
      }
    }
    for (final reader in [true, false]) {
      final sorted = times[reader]!..sort();
      samples.add({
        'path': reader ? 'reader' : 'engine',
        'documents': count,
        'sourceBytes': utf8.encode(source).length,
        'constructMedianUs': sorted[sorted.length ~/ 2],
        'constructSamplesUs': sorted,
      });
    }
  }
  (backend as dynamic).dispose();
  print(const JsonEncoder.withIndent('  ').convert(samples));
}
