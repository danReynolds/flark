import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    await Future.wait([
      _loadFont('CheckpointArabic', '/System/Library/Fonts/GeezaPro.ttc'),
      _loadFont(
        'CheckpointDevanagari',
        '/System/Library/Fonts/Supplemental/DevanagariMT.ttc',
      ),
      _loadFont(
        'CheckpointThai',
        '/System/Library/Fonts/Supplemental/Thonburi.ttc',
      ),
      _loadFont(
        'CheckpointLatin',
        '/System/Library/Fonts/Supplemental/Times New Roman.ttf',
      ),
    ]);
  });

  test('line-boundary checkpoints differential against monolithic layout', () {
    final cases = _layoutCases();

    final results = <String, _DifferentialResult>{};
    for (final sample in cases) {
      final oracle = _snapshot(sample.text, sample.style, sample.direction);
      final chunked = _chunkedSnapshots(sample);
      final result = _compare(oracle, chunked);
      final contextualChunked = _chunkedSnapshots(sample, contextLines: 2);
      final contextualResult = _compare(oracle, contextualChunked);
      results[sample.label] = result;
      debugPrint(
        'flark_phase0_layout_checkpoint case=${sample.label} '
        'units=${sample.text.length} lines=${oracle.length} '
        'break_mismatches=${result.breakMismatches} '
        'box_mismatches=${result.boxMismatches} '
        'metric_mismatches=${result.metricMismatches} '
        'max_box_delta=${result.maxBoxDelta.toStringAsFixed(4)} '
        'chunks=${result.chunks} chunk_p95_us=${result.chunkP95Micros} '
        'max_chunk_us=${result.maxChunkMicros} context_lines=2 '
        'context_break_mismatches=${contextualResult.breakMismatches} '
        'context_box_mismatches=${contextualResult.boxMismatches} '
        'context_metric_mismatches=${contextualResult.metricMismatches} '
        'context_max_box_delta='
        '${contextualResult.maxBoxDelta.toStringAsFixed(4)} '
        'context_chunks=${contextualResult.chunks} '
        'context_chunk_p95_us=${contextualResult.chunkP95Micros} '
        'context_max_chunk_us=${contextualResult.maxChunkMicros}',
      );

      expect(contextualResult.breakMismatches, 0, reason: sample.label);
      expect(contextualResult.boxMismatches, 0, reason: sample.label);
      expect(contextualResult.metricMismatches, 0, reason: sample.label);
    }

    expect(results['latin_ligatures']!.breakMismatches, 0);
    expect(results['arabic']!.breakMismatches, 0);
    expect(
      results['mixed_bidi']!.boxMismatches,
      greaterThan(0),
      reason: 'small windows without leading context must expose bidi drift',
    );
  });

  test('two-line restart context remains exact after localized edits', () {
    for (final sample in _layoutCases()) {
      final oldLines = _snapshot(sample.text, sample.style, sample.direction);
      final middle = sample.text.length ~/ 2;
      final nearbySpace = sample.text.indexOf(' ', middle);
      final editOffset = nearbySpace < 0 ? middle : nearbySpace + 1;
      const insertion = '[edit] ';
      final after = _LayoutCase(
        label: sample.label,
        text: sample.text.replaceRange(editOffset, editOffset, insertion),
        style: sample.style,
        direction: sample.direction,
      );
      final oracle = _snapshot(after.text, after.style, after.direction);
      final restart = _restartAfterEdit(
        after: after,
        oldLines: oldLines,
        editOffset: editOffset,
        newEditEnd: editOffset + insertion.length,
        delta: insertion.length,
      );
      final result = _compare(oracle, restart.snapshots);
      debugPrint(
        'flark_phase0_layout_edit case=${sample.label} '
        'edit=$editOffset insertion_units=${insertion.length} '
        'break_mismatches=${result.breakMismatches} '
        'box_mismatches=${result.boxMismatches} '
        'metric_mismatches=${result.metricMismatches} '
        'chunks=${result.chunks} chunk_p95_us=${result.chunkP95Micros} '
        'max_chunk_us=${result.maxChunkMicros} '
        'relaid_units=${restart.relaidCodeUnits} '
        'converged=${restart.converged}',
      );
      expect(result.breakMismatches, 0, reason: sample.label);
      expect(result.boxMismatches, 0, reason: sample.label);
      expect(result.metricMismatches, 0, reason: sample.label);
    }
  });

  test('unbounded shaping or bidi state is rejected as checkpoint-safe', () {
    final cases = <_LayoutCase>[
      _LayoutCase(
        label: 'bidi_embedding_span',
        text:
            '\u202Bابتداء ${List<String>.filled(2400, '123 - 456 / ').join()}\u202C',
        style: const TextStyle(fontSize: 21, height: 1.3),
      ),
      _LayoutCase(
        label: 'long_weak_bidi',
        text:
            'עברית ${List<String>.filled(2400, '123 - 456 / ').join()} finish',
        style: const TextStyle(fontSize: 21, height: 1.3),
      ),
      _LayoutCase(
        label: 'arabic_unbroken',
        text: List<String>.filled(4096, 'سلام').join(),
        style: const TextStyle(
          fontFamily: 'CheckpointArabic',
          fontSize: 22,
          height: 1.3,
        ),
        direction: TextDirection.rtl,
      ),
      _LayoutCase(
        label: 'oversized_grapheme',
        text: 'a${List<String>.filled(8192, '\u0301').join()} b',
        style: const TextStyle(fontSize: 21, height: 1.3),
      ),
    ];

    var rejected = 0;
    for (final sample in cases) {
      final oracle = _snapshot(sample.text, sample.style, sample.direction);
      final chunked = _chunkedSnapshots(sample, contextLines: 2);
      final result = _compare(oracle, chunked);
      final unsafe =
          result.breakMismatches > 0 ||
          result.boxMismatches > 0 ||
          result.metricMismatches > 0 ||
          result.maxWindowUnits > 512;
      if (unsafe) rejected += 1;
      debugPrint(
        'flark_phase0_layout_pathological case=${sample.label} '
        'units=${sample.text.length} break_mismatches='
        '${result.breakMismatches} box_mismatches=${result.boxMismatches} '
        'metric_mismatches=${result.metricMismatches} '
        'max_box_delta=${result.maxBoxDelta.toStringAsFixed(4)} '
        'chunk_p95_us=${result.chunkP95Micros} '
        'max_chunk_us=${result.maxChunkMicros} '
        'max_window_units=${result.maxWindowUnits} rejected=$unsafe',
      );
    }
    expect(
      rejected,
      greaterThanOrEqualTo(2),
      reason: 'the fallback classifier needs adversarial positive fixtures',
    );
  });
}

List<_LayoutCase> _layoutCases() => <_LayoutCase>[
  _LayoutCase.repeated(
    label: 'latin_ligatures',
    seed: 'office afflict efficient final typography wraps naturally. ',
    style: const TextStyle(
      fontFamily: 'CheckpointLatin',
      fontSize: 21,
      height: 1.25,
      fontFeatures: [FontFeature.enable('liga')],
    ),
  ),
  _LayoutCase.repeated(
    label: 'arabic',
    seed: 'سلام عليكم كتابة عربية طويلة للاختبار والتفاف السطور. ',
    style: const TextStyle(
      fontFamily: 'CheckpointArabic',
      fontSize: 22,
      height: 1.3,
    ),
    direction: TextDirection.rtl,
  ),
  _LayoutCase.repeated(
    label: 'devanagari',
    seed: 'नमस्ते दुनिया यह देवनागरी पाठ पंक्ति विभाजन जाँचता है। ',
    style: const TextStyle(
      fontFamily: 'CheckpointDevanagari',
      fontSize: 21,
      height: 1.3,
    ),
  ),
  _LayoutCase.repeated(
    label: 'thai',
    seed: 'ภาษาไทยทดสอบการตัดบรรทัดและการจัดรูปอักษรอย่างต่อเนื่อง ',
    style: const TextStyle(
      fontFamily: 'CheckpointThai',
      fontSize: 21,
      height: 1.3,
    ),
  ),
  _LayoutCase.repeated(
    label: 'cjk',
    seed: '实时Markdown编辑器需要快速准确地换行和显示文本。',
    style: const TextStyle(fontSize: 21, height: 1.3),
  ),
  _LayoutCase.repeated(
    label: 'mixed_bidi',
    seed: 'English (123) ثم العربية 456 and עברית [789] together. ',
    style: const TextStyle(fontSize: 21, height: 1.3),
  ),
  _LayoutCase.repeated(
    label: 'emoji_combining',
    seed: 'cafe\u0301 family 👩‍👩‍👧‍👦 flag 🇨🇦 scientist 👩🏽‍🔬 text ',
    style: const TextStyle(fontSize: 21, height: 1.3),
  ),
];

const _width = 360.0;
const _windowUnits = 128;
const _tailContextLines = 3;
const _epsilon = 0.1;

final class _LayoutCase {
  const _LayoutCase({
    required this.label,
    required this.text,
    required this.style,
    this.direction = TextDirection.ltr,
  });

  factory _LayoutCase.repeated({
    required String label,
    required String seed,
    required TextStyle style,
    TextDirection direction = TextDirection.ltr,
  }) {
    final repeats = (16 * 1024 / seed.length).ceil();
    return _LayoutCase(
      label: label,
      text: List<String>.filled(repeats, seed).join(),
      style: style,
      direction: direction,
    );
  }

  final String label;
  final String text;
  final TextStyle style;
  final TextDirection direction;
}

final class _LineSnapshot {
  const _LineSnapshot({
    required this.start,
    required this.end,
    required this.height,
    required this.baseline,
    required this.boxes,
  });

  final int start;
  final int end;
  final double height;
  final double baseline;
  final List<_BoxSnapshot> boxes;
}

final class _BoxSnapshot {
  const _BoxSnapshot({
    required this.left,
    required this.top,
    required this.right,
    required this.bottom,
    required this.direction,
  });

  final double left;
  final double top;
  final double right;
  final double bottom;
  final TextDirection direction;
}

final class _ChunkedSnapshots {
  const _ChunkedSnapshots({
    required this.lines,
    required this.chunkMicros,
    required this.windowUnits,
  });

  final List<_LineSnapshot> lines;
  final List<int> chunkMicros;
  final List<int> windowUnits;

  int get chunks => chunkMicros.length;
  int get chunkP95Micros => _percentile(chunkMicros, 95);
  int get maxChunkMicros =>
      chunkMicros.isEmpty ? 0 : chunkMicros.reduce(math.max);
  int get maxWindowUnits =>
      windowUnits.isEmpty ? 0 : windowUnits.reduce(math.max);
}

final class _DifferentialResult {
  const _DifferentialResult({
    required this.breakMismatches,
    required this.boxMismatches,
    required this.metricMismatches,
    required this.maxBoxDelta,
    required this.chunks,
    required this.chunkP95Micros,
    required this.maxChunkMicros,
    required this.maxWindowUnits,
  });

  final int breakMismatches;
  final int boxMismatches;
  final int metricMismatches;
  final double maxBoxDelta;
  final int chunks;
  final int chunkP95Micros;
  final int maxChunkMicros;
  final int maxWindowUnits;
}

_ChunkedSnapshots _chunkedSnapshots(
  _LayoutCase sample, {
  int contextLines = 0,
  bool Function(List<_LineSnapshot> lines)? stopWhen,
}) {
  final result = <_LineSnapshot>[];
  final chunkMicros = <int>[];
  final windowUnits = <int>[];
  var emittedEnd = 0;
  while (emittedEnd < sample.text.length) {
    final contextIndex = math.max(result.length - contextLines, 0);
    final layoutStart = result.isEmpty
        ? 0
        : contextLines == 0
        ? emittedEnd
        : result[contextIndex].start;
    var windowEnd = math.min(
      sample.text.length,
      math.max(layoutStart + _windowUnits, emittedEnd + 1),
    );
    List<_LineSnapshot> usable = const [];
    while (usable.isEmpty) {
      final timer = Stopwatch()..start();
      final local = _snapshot(
        sample.text.substring(layoutStart, windowEnd),
        sample.style,
        sample.direction,
      );
      timer.stop();
      chunkMicros.add(timer.elapsedMicroseconds);
      windowUnits.add(windowEnd - layoutStart);
      final complete = windowEnd == sample.text.length
          ? local
          : local.take(math.max(local.length - _tailContextLines, 0)).toList();
      usable = complete
          .map(
            (line) => _LineSnapshot(
              start: layoutStart + line.start,
              end: layoutStart + line.end,
              height: line.height,
              baseline: line.baseline,
              boxes: line.boxes,
            ),
          )
          .where((line) => line.end > emittedEnd)
          .toList();
      if (usable.isEmpty && windowEnd < sample.text.length) {
        windowEnd = math.min(sample.text.length, windowEnd + _windowUnits);
      } else {
        break;
      }
    }
    if (usable.isEmpty) break;
    result.addAll(usable);
    final next = result.last.end;
    if (next <= emittedEnd) {
      throw StateError('checkpoint layout made no progress');
    }
    emittedEnd = next;
    if (stopWhen?.call(result) ?? false) break;
  }
  return _ChunkedSnapshots(
    lines: result,
    chunkMicros: chunkMicros,
    windowUnits: windowUnits,
  );
}

final class _RestartResult {
  const _RestartResult({
    required this.snapshots,
    required this.relaidCodeUnits,
    required this.converged,
  });

  final _ChunkedSnapshots snapshots;
  final int relaidCodeUnits;
  final bool converged;
}

final class _Convergence {
  const _Convergence({required this.newLineIndex, required this.oldLineIndex});

  final int newLineIndex;
  final int oldLineIndex;
}

_RestartResult _restartAfterEdit({
  required _LayoutCase after,
  required List<_LineSnapshot> oldLines,
  required int editOffset,
  required int newEditEnd,
  required int delta,
}) {
  final affectedLine = math.max(
    oldLines.indexWhere((line) => line.end > editOffset),
    0,
  );
  final preservedLine = math.max(affectedLine - 2, 0);
  final preservedEnd = oldLines[preservedLine].start;
  final contextLine = math.max(preservedLine - 2, 0);
  final layoutStart = oldLines[contextLine].start;
  final suffixCase = _LayoutCase(
    label: after.label,
    text: after.text.substring(layoutStart),
    style: after.style,
    direction: after.direction,
  );
  _Convergence? convergence;
  final oldByStart = <int, int>{
    for (var index = 0; index < oldLines.length; index += 1)
      oldLines[index].start: index,
  };
  final suffix = _chunkedSnapshots(
    suffixCase,
    contextLines: 2,
    stopWhen: (lines) {
      const requiredMatches = 3;
      for (var index = 0; index + requiredMatches <= lines.length; index += 1) {
        final globalStart = layoutStart + lines[index].start;
        if (globalStart < newEditEnd) continue;
        final oldIndex = oldByStart[globalStart - delta];
        if (oldIndex == null || oldIndex + requiredMatches > oldLines.length) {
          continue;
        }
        var matches = true;
        for (var offset = 0; offset < requiredMatches; offset += 1) {
          final current = lines[index + offset];
          final old = oldLines[oldIndex + offset];
          if (layoutStart + current.start - delta != old.start ||
              layoutStart + current.end - delta != old.end ||
              !_sameGeometry(current, old)) {
            matches = false;
            break;
          }
        }
        if (matches) {
          convergence = _Convergence(
            newLineIndex: index + requiredMatches - 1,
            oldLineIndex: oldIndex + requiredMatches - 1,
          );
          return true;
        }
      }
      return false;
    },
  );
  final mapped = suffix.lines
      .map(
        (line) => _LineSnapshot(
          start: layoutStart + line.start,
          end: layoutStart + line.end,
          height: line.height,
          baseline: line.baseline,
          boxes: line.boxes,
        ),
      )
      .toList(growable: false);
  if (preservedEnd > layoutStart &&
      !mapped.any((line) => line.end == preservedEnd)) {
    throw StateError('restart context did not converge before the edit');
  }
  final recomputedEndIndex =
      convergence?.newLineIndex ?? suffix.lines.length - 1;
  final recomputed = mapped
      .take(recomputedEndIndex + 1)
      .where((line) => line.end > preservedEnd)
      .toList(growable: false);
  final reused = convergence == null
      ? const <_LineSnapshot>[]
      : oldLines
            .skip(convergence!.oldLineIndex + 1)
            .map((line) => _shiftLine(line, delta))
            .toList(growable: false);
  return _RestartResult(
    snapshots: _ChunkedSnapshots(
      lines: [
        ...oldLines.where((line) => line.end <= preservedEnd),
        ...recomputed,
        ...reused,
      ],
      chunkMicros: suffix.chunkMicros,
      windowUnits: suffix.windowUnits,
    ),
    relaidCodeUnits: recomputed.isEmpty ? 0 : recomputed.last.end - layoutStart,
    converged: convergence != null,
  );
}

_LineSnapshot _shiftLine(_LineSnapshot line, int delta) => _LineSnapshot(
  start: line.start + delta,
  end: line.end + delta,
  height: line.height,
  baseline: line.baseline,
  boxes: line.boxes,
);

bool _sameGeometry(_LineSnapshot left, _LineSnapshot right) {
  if ((left.height - right.height).abs() > _epsilon ||
      (left.baseline - right.baseline).abs() > _epsilon ||
      left.boxes.length != right.boxes.length) {
    return false;
  }
  for (var index = 0; index < left.boxes.length; index += 1) {
    final a = left.boxes[index];
    final b = right.boxes[index];
    if (a.direction != b.direction ||
        (a.left - b.left).abs() > _epsilon ||
        (a.top - b.top).abs() > _epsilon ||
        (a.right - b.right).abs() > _epsilon ||
        (a.bottom - b.bottom).abs() > _epsilon) {
      return false;
    }
  }
  return true;
}

List<_LineSnapshot> _snapshot(
  String text,
  TextStyle style,
  TextDirection direction,
) {
  if (text.isEmpty) return const [];
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout(maxWidth: _width);
  final metrics = painter.computeLineMetrics();
  final breaks = _lineBreaks(painter, text.length, metrics.length);
  final result = <_LineSnapshot>[];
  var start = 0;
  for (var index = 0; index < breaks.length; index += 1) {
    final end = breaks[index];
    final metric = metrics[index];
    final boxes = painter
        .getBoxesForSelection(
          TextSelection(baseOffset: start, extentOffset: end),
        )
        .map(
          (box) => _BoxSnapshot(
            left: box.left,
            top: box.top - metric.baseline + metric.ascent,
            right: box.right,
            bottom: box.bottom - metric.baseline + metric.ascent,
            direction: box.direction,
          ),
        )
        .toList(growable: false);
    result.add(
      _LineSnapshot(
        start: start,
        end: end,
        height: metric.height,
        baseline: metric.ascent,
        boxes: boxes,
      ),
    );
    start = end;
  }
  painter.dispose();
  return result;
}

List<int> _lineBreaks(TextPainter painter, int textLength, int lineCount) {
  final breaks = <int>[];
  var previousEnd = 0;
  for (var line = 0; line < lineCount; line += 1) {
    if (line == lineCount - 1) {
      breaks.add(textLength);
      break;
    }
    var low = previousEnd + 1;
    var high = textLength;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      final boundary = painter.getLineBoundary(TextPosition(offset: middle));
      if (boundary.end <= previousEnd) {
        low = middle + 1;
      } else if (boundary.start <= previousEnd) {
        high = boundary.end;
        low = boundary.end;
      } else {
        high = middle;
      }
    }
    final boundary = painter.getLineBoundary(
      TextPosition(offset: math.min(low, textLength)),
    );
    final end = boundary.start <= previousEnd ? boundary.end : low;
    breaks.add(end);
    previousEnd = end;
  }
  return breaks;
}

_DifferentialResult _compare(
  List<_LineSnapshot> oracle,
  _ChunkedSnapshots chunked,
) {
  var breakMismatches = (oracle.length - chunked.lines.length).abs();
  var boxMismatches = 0;
  var metricMismatches = 0;
  var maxBoxDelta = 0.0;
  final count = math.min(oracle.length, chunked.lines.length);
  for (var index = 0; index < count; index += 1) {
    final expected = oracle[index];
    final actual = chunked.lines[index];
    if (expected.start != actual.start || expected.end != actual.end) {
      breakMismatches += 1;
      continue;
    }
    if ((expected.height - actual.height).abs() > _epsilon ||
        (expected.baseline - actual.baseline).abs() > _epsilon) {
      metricMismatches += 1;
    }
    if (expected.boxes.length != actual.boxes.length) {
      boxMismatches += 1;
      continue;
    }
    for (var boxIndex = 0; boxIndex < expected.boxes.length; boxIndex += 1) {
      final left = expected.boxes[boxIndex];
      final right = actual.boxes[boxIndex];
      final delta = [
        (left.left - right.left).abs(),
        (left.top - right.top).abs(),
        (left.right - right.right).abs(),
        (left.bottom - right.bottom).abs(),
      ].reduce(math.max);
      maxBoxDelta = math.max(maxBoxDelta, delta);
      if (delta > _epsilon || left.direction != right.direction) {
        boxMismatches += 1;
        break;
      }
    }
  }
  return _DifferentialResult(
    breakMismatches: breakMismatches,
    boxMismatches: boxMismatches,
    metricMismatches: metricMismatches,
    maxBoxDelta: maxBoxDelta,
    chunks: chunked.chunks,
    chunkP95Micros: chunked.chunkP95Micros,
    maxChunkMicros: chunked.maxChunkMicros,
    maxWindowUnits: chunked.maxWindowUnits,
  );
}

int _percentile(List<int> values, int percentile) {
  if (values.isEmpty) return 0;
  final sorted = [...values]..sort();
  final index = ((sorted.length - 1) * percentile / 100).round();
  return sorted[index];
}

Future<void> _loadFont(String family, String path) async {
  final bytes = await File(path).readAsBytes();
  final loader = FontLoader(family)
    ..addFont(Future<ByteData>.value(ByteData.sublistView(bytes)));
  await loader.load();
}
