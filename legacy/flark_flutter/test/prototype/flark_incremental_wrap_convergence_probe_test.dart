@Tags(<String>['benchmark'])
library;

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('bounded wrap windows expose exact reusable line breaks', () {
    final before = _paragraphOfSize(1024 * 1024);

    final bulk = Stopwatch()..start();
    final oldBreaks = _chunkedLayoutBreaks(before);
    bulk.stop();
    final fullBefore = _layoutParagraph(before);
    expect(oldBreaks.length, fullBefore.paragraph.numberOfLines);
    for (final sample in [oldBreaks[100], oldBreaks[oldBreaks.length ~/ 2]]) {
      expect(
        fullBefore.paragraph
            .getLineBoundary(ui.TextPosition(offset: sample - 1))
            .end,
        sample,
      );
    }
    fullBefore.dispose();

    final edits = <_WrapEdit>[
      _WrapEdit.replace(before.length ~/ 2, 1, 'x'),
      _WrapEdit.replace(before.length ~/ 3, 0, 'inserted words '),
      _WrapEdit.replace(before.length ~/ 4, 11, ''),
      _WrapEdit.replace(before.length ~/ 10, 3, 'WIDE'),
      _WrapEdit.replace(before.length - 512, 0, 'tail words '),
      _WrapEdit.replace(oldBreaks[10000] - 1, 1, 'z'),
    ];

    final incrementalMicros = <int>[];
    final oracleMicros = <int>[];
    final relaidCodeUnits = <int>[];
    final maxChunkMicros = <int>[];
    final chunkCounts = <int>[];
    for (final edit in edits) {
      final after = edit.apply(before);
      final incremental = Stopwatch()..start();
      final result = _incrementalBreaks(
        after: after,
        oldBreaks: oldBreaks,
        edit: edit,
      );
      incremental.stop();

      final oracle = Stopwatch()..start();
      final oracleBreaks = _chunkedLayoutBreaks(after);
      oracle.stop();
      expect(result.breaks, oracleBreaks, reason: edit.toString());

      final fullAfter = _layoutParagraph(after);
      expect(oracleBreaks.length, fullAfter.paragraph.numberOfLines);
      expect(
        fullAfter.paragraph
            .getLineBoundary(
              ui.TextPosition(offset: math.max(result.convergence - 1, 0)),
            )
            .end,
        result.convergence,
      );
      fullAfter.dispose();

      incrementalMicros.add(incremental.elapsedMicroseconds);
      oracleMicros.add(oracle.elapsedMicroseconds);
      relaidCodeUnits.add(result.relaidCodeUnits);
      maxChunkMicros.add(result.maxChunkMicros);
      chunkCounts.add(result.chunkCount);
      debugPrint(
        'flark_incremental_wrap_case edit=$edit '
        'relayout_code_units=${result.relaidCodeUnits} '
        'chunks=${result.chunkCount} max_chunk_us=${result.maxChunkMicros} '
        'total_us=${incremental.elapsedMicroseconds}',
      );
    }

    incrementalMicros.sort();
    oracleMicros.sort();
    relaidCodeUnits.sort();
    maxChunkMicros.sort();
    chunkCounts.sort();
    debugPrint(
      'flark_incremental_wrap bytes=${before.length} '
      'visual_lines=${oldBreaks.length} bulk_us=${bulk.elapsedMicroseconds} '
      'cases=${edits.length} '
      'incremental_p50_us=${_percentile(incrementalMicros, 50)} '
      'incremental_p95_us=${_percentile(incrementalMicros, 95)} '
      'incremental_max_us=${incrementalMicros.last} '
      'chunked_oracle_p50_us=${_percentile(oracleMicros, 50)} '
      'chunked_oracle_p95_us=${_percentile(oracleMicros, 95)} '
      'relayout_code_units_p50=${_percentile(relaidCodeUnits, 50)} '
      'relayout_code_units_max=${relaidCodeUnits.last} '
      'chunks_p95=${_percentile(chunkCounts, 95)} '
      'max_chunk_p95_us=${_percentile(maxChunkMicros, 95)} '
      'max_chunk_max_us=${maxChunkMicros.last}',
    );

    expect(
      relaidCodeUnits.last,
      greaterThan(64 * 1024),
      reason: 'the adversarial fixture should demonstrate global propagation',
    );
    expect(
      _percentile(maxChunkMicros, 95),
      lessThan(_percentile(oracleMicros, 50)),
    );
  });
}

const _fontSize = 16.0;
const _height = 1.35;
const _width = 560.0;
const _continuationWindow = 1024;

final class _WrapEdit {
  const _WrapEdit.replace(this.start, this.deletedLength, this.replacement);

  final int start;
  final int deletedLength;
  final String replacement;

  int get oldEnd => start + deletedLength;
  int get newEnd => start + replacement.length;
  int get delta => replacement.length - deletedLength;

  String apply(String source) =>
      source.replaceRange(start, oldEnd, replacement);

  @override
  String toString() =>
      '_WrapEdit(start: $start, deleted: $deletedLength, '
      'replacement: ${replacement.length})';
}

final class _ParagraphLayout {
  const _ParagraphLayout(this.paragraph);

  final ui.Paragraph paragraph;

  void dispose() => paragraph.dispose();
}

final class _IncrementalWrapResult {
  const _IncrementalWrapResult({
    required this.breaks,
    required this.convergence,
    required this.relaidCodeUnits,
    required this.chunkCount,
    required this.maxChunkMicros,
  });

  final List<int> breaks;
  final int convergence;
  final int relaidCodeUnits;
  final int chunkCount;
  final int maxChunkMicros;
}

_IncrementalWrapResult _incrementalBreaks({
  required String after,
  required List<int> oldBreaks,
  required _WrapEdit edit,
}) {
  final affectedLine = _lowerBound(oldBreaks, edit.start);
  final restartIndex = math.max(affectedLine - 2, -1);
  final restart = restartIndex < 0 ? 0 : oldBreaks[restartIndex];

  final changedBreaks = <int>[];
  final chunkMicros = <int>[];
  var cursor = restart;
  while (true) {
    final windowEnd = math.min(after.length, cursor + _continuationWindow);
    final chunk = Stopwatch()..start();
    final local = _layoutBreaks(after.substring(cursor, windowEnd));
    chunk.stop();
    chunkMicros.add(chunk.elapsedMicroseconds);
    final usable = windowEnd == after.length
        ? local
        : local.take(math.max(local.length - 1, 0)).toList();
    int? convergence;
    for (final localBreak in usable) {
      final globalBreak = cursor + localBreak;
      changedBreaks.add(globalBreak);
      if (globalBreak <= edit.newEnd) continue;
      final oldBreak = globalBreak - edit.delta;
      if (oldBreak >= edit.oldEnd && _containsSorted(oldBreaks, oldBreak)) {
        convergence = globalBreak;
        break;
      }
    }
    if (convergence != null) {
      final prefix = oldBreaks.takeWhile((value) => value <= restart);
      final suffix = oldBreaks
          .where((value) => value >= edit.oldEnd)
          .map((value) => value + edit.delta)
          .where((value) => value > convergence!);
      return _IncrementalWrapResult(
        breaks: [...prefix, ...changedBreaks, ...suffix],
        convergence: convergence,
        relaidCodeUnits: convergence - restart,
        chunkCount: chunkMicros.length,
        maxChunkMicros: chunkMicros.reduce(math.max),
      );
    }
    if (windowEnd == after.length) {
      return _IncrementalWrapResult(
        breaks: [
          ...oldBreaks.takeWhile((value) => value <= restart),
          ...changedBreaks,
        ],
        convergence: after.length,
        relaidCodeUnits: after.length - restart,
        chunkCount: chunkMicros.length,
        maxChunkMicros: chunkMicros.reduce(math.max),
      );
    }
    if (changedBreaks.isEmpty || changedBreaks.last <= cursor) {
      throw StateError('wrap continuation made no progress');
    }
    cursor = changedBreaks.last;
  }
}

List<int> _chunkedLayoutBreaks(String text) {
  if (text.isEmpty) return const [];
  final breaks = <int>[];
  var start = 0;
  while (start < text.length) {
    var windowEnd = math.min(text.length, start + 4096);
    List<int> usable = const [];
    while (usable.isEmpty) {
      final local = _layoutBreaks(text.substring(start, windowEnd));
      usable = windowEnd == text.length
          ? local
          : local.take(math.max(local.length - 1, 0)).toList();
      if (usable.isEmpty && windowEnd < text.length) {
        windowEnd = math.min(text.length, windowEnd + 4096);
      } else {
        break;
      }
    }
    if (usable.isEmpty) break;
    breaks.addAll(usable.map((value) => start + value));
    final next = breaks.last;
    if (next <= start) throw StateError('wrap chunk made no progress');
    start = next;
  }
  return breaks;
}

_ParagraphLayout _layoutParagraph(String text) {
  final builder = ui.ParagraphBuilder(
    ui.ParagraphStyle(textDirection: ui.TextDirection.ltr),
  )..pushStyle(ui.TextStyle(fontSize: _fontSize, height: _height));
  builder.addText(text);
  final paragraph = builder.build()
    ..layout(const ui.ParagraphConstraints(width: _width));
  return _ParagraphLayout(paragraph);
}

List<int> _layoutBreaks(String text) {
  if (text.isEmpty) return const [];
  final layout = _layoutParagraph(text);
  final breaks = <int>[];
  var previousEnd = 0;
  for (
    var lineNumber = 0;
    lineNumber < layout.paragraph.numberOfLines;
    lineNumber += 1
  ) {
    if (lineNumber == layout.paragraph.numberOfLines - 1) {
      breaks.add(text.length);
      break;
    }
    var low = previousEnd + 1;
    var high = text.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      final middleLine = layout.paragraph.getLineNumberAt(middle);
      if (middleLine != null && middleLine <= lineNumber) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    breaks.add(low);
    previousEnd = low;
  }
  layout.dispose();
  return breaks;
}

int _lowerBound(List<int> values, int target) {
  var low = 0;
  var high = values.length;
  while (low < high) {
    final middle = low + ((high - low) >> 1);
    if (values[middle] < target) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

bool _containsSorted(List<int> values, int target) {
  final index = _lowerBound(values, target);
  return index < values.length && values[index] == target;
}

String _paragraphOfSize(int size) {
  const chunk =
      'alpha beta gamma delta epsilon zeta eta theta iota kappa lambda '
      'bold words and inline code continue through the paragraph ';
  final output = StringBuffer();
  while (output.length < size) {
    output.write(chunk);
  }
  return output.toString().substring(0, size);
}

int _percentile(List<int> values, int percentile) =>
    values[((values.length - 1) * percentile) ~/ 100];
