@Tags(<String>['benchmark'])
library;

import 'dart:io';
import 'dart:math' as math;

import 'package:flark/flark_advanced.dart';
import 'package:test/test.dart';

int _blackHole = 0;

void main() {
  test('persistent rope preserves source and line addressing', () {
    const source = 'zero\none\ntwo\nthree';
    final rope = _PrototypeRope.fromString(source, chunkSize: 3);

    expect(rope.toString(), source);
    expect(rope.length, source.length);
    expect(rope.lineCount, 4);
    expect(
      [for (var i = 0; i < rope.lineCount; i++) rope.lineStart(i)],
      [0, 5, 9, 13],
    );
    for (var offset = 0; offset <= source.length; offset += 1) {
      final expected = '\n'.allMatches(source.substring(0, offset)).length;
      expect(rope.lineAtOffset(offset), expected, reason: 'offset=$offset');
    }

    final edited = rope.replaceRange(6, 8, 'NE\ninserted');
    const expected = 'zero\noNE\ninserted\ntwo\nthree';
    expect(edited.toString(), expected);
    expect(edited.substring(3, 19), expected.substring(3, 19));
    expect(edited.lineCount, '\n'.allMatches(expected).length + 1);
    for (var offset = 0; offset <= expected.length; offset += 1) {
      final expectedLine = '\n'
          .allMatches(expected.substring(0, offset))
          .length;
      expect(
        edited.lineAtOffset(offset),
        expectedLine,
        reason: 'offset=$offset',
      );
    }
  });

  for (final size in const [
    _Size('1MB', 1000000, 12),
    _Size('5MB', 5000000, 5),
    _Size('10MB', 10000000, 3),
  ]) {
    test('localized text substrate scaling at ${size.label}', () {
      final source = _largeText(size.chars);
      final offset = source.length ~/ 2;
      final current = FlarkTextBuffer(source);
      final rope = _PrototypeRope.fromString(source);

      final currentResult = _measure(
        'current_buffer_insert_${size.label}_${source.length}chars',
        iterations: size.currentIterations,
        warmups: 1,
        body: () {
          final next = current.replaceRange(offset, offset, 'x');
          return next.length + next.lineCount;
        },
      );
      final ropeResult = _measure(
        'prototype_rope_insert_${size.label}_${source.length}chars',
        iterations: 100,
        warmups: 10,
        body: () {
          final next = rope.replaceRange(offset, offset, 'x');
          return next.length + next.lineAtOffset(offset) + next.lineCount;
        },
      );
      final snapshotResult = _measure(
        'prototype_rope_materialize_${size.label}_${source.length}chars',
        iterations: size.currentIterations,
        warmups: 1,
        body: () {
          final snapshot = rope.toString();
          return snapshot.length + snapshot.codeUnitAt(offset);
        },
      );

      _report(currentResult);
      _report(ropeResult);
      _report(snapshotResult);

      final edited = rope.replaceRange(offset, offset, 'x');
      expect(edited.length, source.length + 1);
      expect(
        edited.substring(offset - 4, offset + 5),
        '${source.substring(offset - 4, offset)}x${source.substring(offset, offset + 4)}',
      );
      expect(edited.lineCount, current.lineCount);
    });
  }
}

final class _Size {
  const _Size(this.label, this.chars, this.currentIterations);

  final String label;
  final int chars;
  final int currentIterations;
}

final class _PrototypeRope {
  const _PrototypeRope._(this._root);

  factory _PrototypeRope.fromString(String source, {int chunkSize = 4096}) {
    if (source.isEmpty) return const _PrototypeRope._(null);
    final leaves = <_RopeNode>[];
    for (var start = 0; start < source.length; start += chunkSize) {
      final end = math.min(start + chunkSize, source.length);
      leaves.add(_RopeLeaf.fromSource(source, start, end));
    }
    return _PrototypeRope._(_buildBalanced(leaves, 0, leaves.length));
  }

  final _RopeNode? _root;

  int get length => _root?.length ?? 0;

  int get lineCount => (_root?.newlines ?? 0) + 1;

  _PrototypeRope replaceRange(int start, int end, String replacement) {
    _checkRange(start, end);
    final beforeAndRest = _split(_root, start);
    final removedAndAfter = _split(beforeAndRest.right, end - start);
    final inserted = replacement.isEmpty
        ? null
        : _RopeLeaf.fromSource(replacement, 0, replacement.length);
    return _PrototypeRope._(
      _concat(_concat(beforeAndRest.left, inserted), removedAndAfter.right),
    );
  }

  String substring(int start, [int? end]) {
    final actualEnd = end ?? length;
    _checkRange(start, actualEnd);
    if (start == actualEnd) return '';
    final buffer = StringBuffer();
    _writeRange(_root, start, actualEnd, buffer);
    return buffer.toString();
  }

  int lineAtOffset(int offset) {
    if (offset < 0 || offset > length) {
      throw RangeError.range(offset, 0, length, 'offset');
    }
    return _newlinesBefore(_root, offset);
  }

  int lineStart(int lineIndex) {
    if (lineIndex < 0 || lineIndex >= lineCount) {
      throw RangeError.range(lineIndex, 0, lineCount - 1, 'lineIndex');
    }
    if (lineIndex == 0) return 0;
    return _offsetAfterNthNewline(_root!, lineIndex);
  }

  @override
  String toString() => substring(0, length);

  void _checkRange(int start, int end) {
    if (start < 0 || start > length) {
      throw RangeError.range(start, 0, length, 'start');
    }
    if (end < start || end > length) {
      throw RangeError.range(end, start, length, 'end');
    }
  }
}

sealed class _RopeNode {
  const _RopeNode();

  int get length;
  int get newlines;
  int get height;
}

final class _RopeLeaf extends _RopeNode {
  const _RopeLeaf._({
    required this.source,
    required this.start,
    required this.length,
    required this.newlineOffsets,
  });

  factory _RopeLeaf.fromSource(String source, int start, int end) {
    final newlineOffsets = <int>[];
    for (var offset = start; offset < end; offset += 1) {
      if (source.codeUnitAt(offset) == 0x0A) {
        newlineOffsets.add(offset - start);
      }
    }
    return _RopeLeaf._(
      source: source,
      start: start,
      length: end - start,
      newlineOffsets: List<int>.unmodifiable(newlineOffsets),
    );
  }

  final String source;
  final int start;
  @override
  final int length;
  final List<int> newlineOffsets;

  @override
  int get newlines => newlineOffsets.length;

  @override
  int get height => 1;

  _RopeLeaf slice(int relativeStart, int relativeEnd) {
    return _RopeLeaf.fromSource(
      source,
      start + relativeStart,
      start + relativeEnd,
    );
  }
}

final class _RopeBranch extends _RopeNode {
  _RopeBranch(this.left, this.right)
    : length = left.length + right.length,
      newlines = left.newlines + right.newlines,
      height = math.max(left.height, right.height) + 1;

  final _RopeNode left;
  final _RopeNode right;
  @override
  final int length;
  @override
  final int newlines;
  @override
  final int height;
}

final class _Split {
  const _Split(this.left, this.right);

  final _RopeNode? left;
  final _RopeNode? right;
}

_RopeNode? _buildBalanced(List<_RopeNode> nodes, int start, int end) {
  if (start >= end) return null;
  if (end - start == 1) return nodes[start];
  final middle = start + ((end - start) >> 1);
  return _RopeBranch(
    _buildBalanced(nodes, start, middle)!,
    _buildBalanced(nodes, middle, end)!,
  );
}

_Split _split(_RopeNode? node, int offset) {
  if (node == null) return const _Split(null, null);
  if (offset == 0) return _Split(null, node);
  if (offset == node.length) return _Split(node, null);

  if (node case final _RopeLeaf leaf) {
    return _Split(leaf.slice(0, offset), leaf.slice(offset, leaf.length));
  }

  final branch = node as _RopeBranch;
  if (offset < branch.left.length) {
    final split = _split(branch.left, offset);
    return _Split(split.left, _concat(split.right, branch.right));
  }
  if (offset == branch.left.length) {
    return _Split(branch.left, branch.right);
  }
  final split = _split(branch.right, offset - branch.left.length);
  return _Split(_concat(branch.left, split.left), split.right);
}

_RopeNode? _concat(_RopeNode? left, _RopeNode? right) {
  if (left == null) return right;
  if (right == null) return left;

  if (left.height > right.height + 1) {
    final branch = left as _RopeBranch;
    return _balance(_RopeBranch(branch.left, _concat(branch.right, right)!));
  }
  if (right.height > left.height + 1) {
    final branch = right as _RopeBranch;
    return _balance(_RopeBranch(_concat(left, branch.left)!, branch.right));
  }
  return _RopeBranch(left, right);
}

_RopeNode _balance(_RopeBranch node) {
  final balance = node.left.height - node.right.height;
  if (balance > 1) {
    final left = node.left as _RopeBranch;
    if (left.left.height < left.right.height) {
      final pivot = left.right as _RopeBranch;
      final rotatedLeft = _RopeBranch(left.left, pivot.left);
      return _RopeBranch(rotatedLeft, _RopeBranch(pivot.right, node.right));
    }
    return _RopeBranch(left.left, _RopeBranch(left.right, node.right));
  }
  if (balance < -1) {
    final right = node.right as _RopeBranch;
    if (right.right.height < right.left.height) {
      final pivot = right.left as _RopeBranch;
      final rotatedRight = _RopeBranch(pivot.right, right.right);
      return _RopeBranch(_RopeBranch(node.left, pivot.left), rotatedRight);
    }
    return _RopeBranch(_RopeBranch(node.left, right.left), right.right);
  }
  return node;
}

void _writeRange(_RopeNode? node, int start, int end, StringBuffer buffer) {
  if (node == null || start >= end) return;
  if (node case final _RopeLeaf leaf) {
    buffer.write(leaf.source.substring(leaf.start + start, leaf.start + end));
    return;
  }
  final branch = node as _RopeBranch;
  if (start < branch.left.length) {
    _writeRange(branch.left, start, math.min(end, branch.left.length), buffer);
  }
  if (end > branch.left.length) {
    _writeRange(
      branch.right,
      math.max(0, start - branch.left.length),
      end - branch.left.length,
      buffer,
    );
  }
}

int _newlinesBefore(_RopeNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.length) return node.newlines;
  if (node case final _RopeLeaf leaf) {
    var low = 0;
    var high = leaf.newlineOffsets.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (leaf.newlineOffsets[middle] < offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  }
  final branch = node as _RopeBranch;
  if (offset <= branch.left.length) {
    return _newlinesBefore(branch.left, offset);
  }
  return branch.left.newlines +
      _newlinesBefore(branch.right, offset - branch.left.length);
}

int _offsetAfterNthNewline(_RopeNode node, int count) {
  if (node case final _RopeLeaf leaf) {
    return leaf.newlineOffsets[count - 1] + 1;
  }
  final branch = node as _RopeBranch;
  if (count <= branch.left.newlines) {
    return _offsetAfterNthNewline(branch.left, count);
  }
  return branch.left.length +
      _offsetAfterNthNewline(branch.right, count - branch.left.newlines);
}

String _largeText(int targetChars) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetChars) {
    buffer
      ..write('paragraph ')
      ..write(index)
      ..write(' with **inline** markdown and [a link][shared]. ')
      ..writeln('The line is intentionally ordinary.');
    index += 1;
  }
  buffer.writeln('\n[shared]: https://example.com');
  return buffer.toString();
}

_BenchmarkResult _measure(
  String name, {
  required int iterations,
  required int warmups,
  required int Function() body,
}) {
  for (var i = 0; i < warmups; i += 1) {
    _consume(body());
  }
  final samples = <Duration>[];
  for (var i = 0; i < iterations; i += 1) {
    final stopwatch = Stopwatch()..start();
    _consume(body());
    stopwatch.stop();
    samples.add(stopwatch.elapsed);
  }
  return _BenchmarkResult(name, samples);
}

void _consume(int value) {
  _blackHole = (_blackHole + value) & 0x3fffffff;
}

void _report(_BenchmarkResult result) {
  stdout.writeln('flark_prototype ${result.summary}');
}

final class _BenchmarkResult {
  _BenchmarkResult(this.name, Iterable<Duration> samples)
    : samples = List<Duration>.unmodifiable(
        [...samples]..sort((left, right) => left.compareTo(right)),
      );

  final String name;
  final List<Duration> samples;

  Duration get median => samples[samples.length ~/ 2];
  Duration get p95 => samples[((samples.length - 1) * 0.95).ceil()];

  String get summary =>
      '$name iterations=${samples.length} median=${_fmt(median)} '
      'p95=${_fmt(p95)}';
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
