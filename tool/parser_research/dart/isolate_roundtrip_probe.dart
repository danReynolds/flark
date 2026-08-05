import 'dart:async';
import 'dart:io';
import 'dart:isolate';

/// Measures only the scheduling/message floor of a long-lived parser isolate.
/// No parsing or payload decoding is included.
Future<void> main() async {
  final replies = ReceivePort();
  final worker = await Isolate.spawn(_workerMain, replies.sendPort);
  final iterator = StreamIterator<Object?>(replies);
  await iterator.moveNext();
  final send = iterator.current! as SendPort;

  const warmup = 1000;
  const cases = 10000;
  final samples = <int>[];
  for (var index = 0; index < warmup + cases; index += 1) {
    final stopwatch = Stopwatch()..start();
    send.send((index, replies.sendPort));
    await iterator.moveNext();
    stopwatch.stop();
    if (index >= warmup) samples.add(stopwatch.elapsedMicroseconds);
  }

  samples.sort();
  stdout.writeln(
    'isolate_roundtrip cases=$cases '
    'p50_us=${_percentile(samples, 50)} '
    'p95_us=${_percentile(samples, 95)} '
    'p99_us=${_percentile(samples, 99)} '
    'p999_us=${_percentile(samples, 999, scale: 1000)} '
    'max_us=${samples.last}',
  );

  send.send(null);
  await iterator.cancel();
  replies.close();
  worker.kill();
}

void _workerMain(SendPort ready) {
  final requests = ReceivePort();
  ready.send(requests.sendPort);
  requests.listen((message) {
    if (message == null) {
      requests.close();
      return;
    }
    final request = message as (int, SendPort);
    request.$2.send(request.$1);
  });
}

int _percentile(List<int> values, int percentile, {int scale = 100}) =>
    values[((values.length - 1) * percentile) ~/ scale];
