// ignore_for_file: avoid_print

import 'dart:convert';

/// Disposable dart2js/V8 receipt for the primitives used by the lazy-bulk
/// spike. Node is not a browser or Flutter-web proof; this only falsifies an
/// assumption that compiled-web String.length/codeUnitAt necessarily walk the
/// whole document.
void main() {
  const runtime = String.fromEnvironment(
    'FLARK_WEB_RUNTIME',
    defaultValue: 'compiled_web_on_node_v8',
  );
  for (final sizeMiB in const [1, 10, 100]) {
    final source = _sourceOfLength(sizeMiB * 1024 * 1024);
    _emit('web_string_length_batch', {
      'runtime': runtime,
      'size_mib': sizeMiB,
      ..._measureBatched(
        samples: 100,
        operationsPerSample: 10000,
        body: (index) {
          _blackHole ^= source.length + (index & 1);
        },
      ),
    });

    var seed = 0x13579BDF;
    _emit('web_random_code_unit_batch', {
      'runtime': runtime,
      'size_mib': sizeMiB,
      ..._measureBatched(
        samples: 100,
        operationsPerSample: 1024,
        body: (index) {
          seed = _next(seed + index);
          _blackHole ^= source.codeUnitAt(seed % source.length);
        },
      ),
    });

    final middle = source.length ~/ 2;
    _emit('web_substring_4k_batch', {
      'runtime': runtime,
      'size_mib': sizeMiB,
      ..._measureBatched(
        samples: 100,
        operationsPerSample: 64,
        body: (index) {
          final value = source.substring(middle, middle + 4096);
          _blackHole ^= value.length ^ value.codeUnitAt(index & 4095);
        },
      ),
    });

    _emit('web_forced_owned_slice_4k_batch', {
      'runtime': runtime,
      'size_mib': sizeMiB,
      ..._measureBatched(
        samples: 100,
        operationsPerSample: 8,
        body: (index) {
          final value = String.fromCharCodes(
            source.codeUnits.sublist(middle, middle + 4096),
          );
          _blackHole ^= value.length ^ value.codeUnitAt(index & 4095);
        },
      ),
    });

    _WebBulkHandle? handle;
    _emit('web_lazy_handle_adoption_batch', {
      'runtime': runtime,
      'size_mib': sizeMiB,
      ..._measureBatched(
        samples: 100,
        operationsPerSample: 1024,
        body: (index) {
          handle = _WebBulkHandle(source, index);
          _blackHole ^= handle!.utf16Length ^ handle!.nonce;
        },
      ),
    });
  }
  _emit('web_probe_complete', {'black_hole': _blackHole});
}

final class _WebBulkHandle {
  const _WebBulkHandle(this.source, this.nonce);

  final String source;
  final int nonce;
  int get utf16Length => source.length;
}

Map<String, Object> _measureBatched({
  required int samples,
  required int operationsPerSample,
  required void Function(int index) body,
}) {
  for (var index = 0; index < operationsPerSample; index += 1) {
    body(index);
  }
  final values = <int>[];
  for (var sample = 0; sample < samples; sample += 1) {
    final stopwatch = Stopwatch()..start();
    for (var index = 0; index < operationsPerSample; index += 1) {
      body(index + sample);
    }
    stopwatch.stop();
    values.add(_nanoseconds(stopwatch) ~/ operationsPerSample);
  }
  values.sort();
  return {
    'samples': samples,
    'operations_per_sample': operationsPerSample,
    'p50_ns_per_op': values[(samples - 1) ~/ 2],
    'p99_ns_per_op': values[((samples - 1) * 99) ~/ 100],
    'max_ns_per_op': values.last,
  };
}

String _sourceOfLength(int length) {
  const chunk =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final count = length ~/ chunk.length;
  final remainder = length % chunk.length;
  return '${List<String>.filled(count, chunk).join()}'
      '${chunk.substring(0, remainder)}';
}

var _blackHole = 0;
final int _frequency = Stopwatch().frequency;

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

int _nanoseconds(Stopwatch stopwatch) =>
    (stopwatch.elapsedTicks * 1000000000) ~/ _frequency;

void _emit(String receipt, Map<String, Object?> values) {
  print(jsonEncode({'receipt': receipt, ...values}));
}
