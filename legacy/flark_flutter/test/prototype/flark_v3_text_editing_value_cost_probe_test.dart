import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Opt-in host-JIT receipt for the Flutter text-model cost that surrounds the
/// v3 source tree. This test is skipped in ordinary suites.
///
/// Run with:
///
///   flutter test \
///     --dart-define=FLARK_RUN_TEXT_COST_PROBE=true \
///     --dart-define=FLARK_TEXT_COST_SIZE_MIB=10 \
///     test/prototype/flark_v3_text_editing_value_cost_probe_test.dart
const _enabled = bool.fromEnvironment('FLARK_RUN_TEXT_COST_PROBE');
const _sizeMiB = int.fromEnvironment(
  'FLARK_TEXT_COST_SIZE_MIB',
  defaultValue: 10,
);

void main() {
  test(
    'separates TextEditingValue wrapper work from whole-String mutation',
    () {
      final source = _sourceOfLength(_sizeMiB * 1024 * 1024);
      final middle = source.length ~/ 2;
      final base = TextEditingValue(
        text: source,
        selection: TextSelection.collapsed(offset: middle),
      );
      var blackHole = 0;

      final wrapperIterations = 100000;
      var wrapper = base;
      final wrapperWatch = Stopwatch()..start();
      for (var index = 0; index < wrapperIterations; index += 1) {
        wrapper = wrapper.copyWith(
          selection: TextSelection.collapsed(offset: middle + (index & 1)),
        );
        blackHole ^= wrapper.selection.extentOffset;
      }
      wrapperWatch.stop();
      _emit('text_editing_value_wrapper', {
        'size_mib': _sizeMiB,
        'iterations': wrapperIterations,
        'total_us': wrapperWatch.elapsedMicroseconds,
        'ns_per_copy':
            wrapperWatch.elapsedMicroseconds * 1000 / wrapperIterations,
      });

      final iterations = switch (_sizeMiB) {
        >= 100 => 8,
        >= 10 => 40,
        _ => 200,
      };
      final direct = _samples(iterations, () {
        final next = source.replaceRange(middle, middle + 1, '\u0001');
        blackHole ^= next.length ^ next.codeUnitAt(middle);
      });
      _emit('dart_string_replace_range', {
        'size_mib': _sizeMiB,
        'iterations': iterations,
        ...direct,
        'allocated_output_code_units_per_edit': source.length,
      });

      final replaced = _samples(iterations, () {
        final next = base.replaced(
          TextRange(start: middle, end: middle + 1),
          '\u0002',
        );
        blackHole ^= next.text.length ^ next.text.codeUnitAt(middle);
      });
      _emit('text_editing_value_replaced', {
        'size_mib': _sizeMiB,
        'iterations': iterations,
        ...replaced,
        'allocated_output_code_units_per_edit': source.length,
      });

      final delta = TextEditingDeltaInsertion(
        oldText: source,
        textInserted: 'x',
        insertionOffset: middle,
        selection: TextSelection.collapsed(offset: middle + 1),
        composing: TextRange.empty,
      );
      final deltaApplied = _samples(iterations, () {
        final next = delta.apply(base);
        blackHole ^= next.text.length ^ next.text.codeUnitAt(middle);
      });
      _emit('flutter_delta_insertion_apply', {
        'size_mib': _sizeMiB,
        'iterations': iterations,
        ...deltaApplied,
        'allocated_output_code_units_per_edit': source.length + 1,
      });
      _emit('text_cost_probe_complete', {
        'black_hole': blackHole,
        'rss_mib': (ProcessInfo.currentRss / (1024 * 1024)).round(),
      });
    },
    skip: !_enabled,
  );
}

Map<String, Object> _samples(int iterations, void Function() body) {
  final values = <int>[];
  for (var index = 0; index < iterations; index += 1) {
    final stopwatch = Stopwatch()..start();
    body();
    stopwatch.stop();
    values.add(stopwatch.elapsedMicroseconds);
  }
  values.sort();
  return {
    'p50_us': values[((values.length - 1) * 50) ~/ 100],
    'p95_us': values[((values.length - 1) * 95) ~/ 100],
    'p99_us': values[((values.length - 1) * 99) ~/ 100],
    'max_us': values.last,
  };
}

String _sourceOfLength(int length) {
  const chunk =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final fullChunks = length ~/ chunk.length;
  final remainder = length % chunk.length;
  return '${List<String>.filled(fullChunks, chunk).join()}'
      '${chunk.substring(0, remainder)}';
}

void _emit(String receipt, Map<String, Object?> values) {
  stdout.writeln(jsonEncode({'receipt': receipt, ...values}));
}
