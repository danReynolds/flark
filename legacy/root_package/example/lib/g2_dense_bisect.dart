// Bisects the G2 dense-fixture fault (parserFailure: 4).
//
//   cd example && dart run lib/g2_dense_bisect.dart
//
// The G2 harness found that a 5 KB markdown-DENSE document reaches structure,
// never paints, then faults with `state: faulted, parserFailure: 4` and rejects
// every keystroke -- while a plain document of the same size is fine. Lines are
// well under 4 KiB, so this is not the known over-window defect.
//
// This isolates which construct is responsible: each case opens a small
// document made of one construct, waits for structure, applies one edit, and
// reports the resulting runtime state.

import 'dart:async';
import 'dart:io';

import 'package:flark/flark_v3.dart';

/// One block of each construct the dense fixture emits.
const Map<String, String> _constructs = <String, String>{
  'paragraph': 'A plain paragraph with nothing special in it at all.',
  'heading': '## Section one heading',
  'bold': 'A paragraph with **bold words** inside it.',
  'emphasis': 'A paragraph with _emphasised words_ inside it.',
  'inline-code': 'A paragraph with `inline code` inside it.',
  'link': 'A paragraph with a [link](https://example.test/1) inside it.',
  'bullet-list': '- first item\n- second item\n- third item',
  'ordered-list': '1. first item\n2. second item\n3. third item',
  'all-inline':
      'A paragraph with **bold**, _emphasis_, `inline code`, and a '
      '[link](https://example.test/1) inside it.',
};

String _repeat(String block, int count) =>
    List<String>.filled(count, block).join('\n\n');

Future<void> _probe(String label, String markdown) async {
  final stopwatch = Stopwatch()..start();
  FlarkV3DocumentRuntime? runtime;
  var verdict = '?';
  try {
    runtime = await FlarkV3DocumentRuntime.open(markdown);
    try {
      await runtime.initialReady.timeout(const Duration(seconds: 30));
    } catch (error) {
      verdict = 'OPEN-FAULT ${_short(error)}';
      return;
    }

    // One edit at the very end of the document — never inside a delimiter.
    final end = runtime.sourceLengthUtf16;
    try {
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: end,
            endUtf16: end,
            replacement: 'x',
          ),
        ),
      );
    } catch (error) {
      verdict = 'EDIT-REJECTED state=${runtime.status.state.name} '
          '${_short(error)}';
      return;
    }

    try {
      await runtime.statuses
          .firstWhere(
            (status) =>
                status.structureCurrent ||
                status.state != FlarkV3DocumentRuntimeState.open,
          )
          .timeout(const Duration(seconds: 30));
    } catch (_) {
      // fall through to the state report
    }

    final status = runtime.status;
    verdict =
        status.structureCurrent &&
            status.state == FlarkV3DocumentRuntimeState.open
        ? 'OK ${stopwatch.elapsedMilliseconds}ms'
        : 'BAD state=${status.state.name} '
            'structureCurrent=${status.structureCurrent} '
            'structureRevision=${status.structureRevision}';
  } catch (error) {
    verdict = 'THREW ${_short(error)}';
  } finally {
    stdout.writeln(
      'bisect ${label.padRight(26)} bytes=${markdown.length.toString().padLeft(6)} :: $verdict',
    );
    try {
      await runtime?.close().timeout(const Duration(seconds: 10));
    } catch (_) {}
  }
}

String _short(Object error) {
  final text = error.toString().replaceAll('\n', ' ');
  return text.length > 130 ? '${text.substring(0, 130)}…' : text;
}

Future<void> main() async {
  // 1. Each construct alone, small.
  for (final entry in _constructs.entries) {
    await _probe('${entry.key}/x1', entry.value);
  }

  // 2. Each construct repeated to ~5 KB — the size G2 failed at.
  for (final entry in _constructs.entries) {
    final count = (5 * 1024 / (entry.value.length + 2)).ceil();
    await _probe('${entry.key}/5KB', _repeat(entry.value, count));
  }

  // 3. The dense mixture, growing, to find the size threshold.
  final mixture = _constructs.values.join('\n\n');
  for (final target in <int>[1, 2, 5, 10, 25]) {
    final count = (target * 1024 / (mixture.length + 2)).ceil();
    await _probe('mixture/${target}KB', _repeat(mixture, count));
  }

  exit(0);
}
