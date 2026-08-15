@Tags(<String>['benchmark'])
library;

import 'dart:io';
import 'dart:math' as math;

import 'package:flark/flark_advanced.dart';
import 'package:test/test.dart';

int _blackHole = 0;

void main() {
  for (final segmentCount in const [5000, 50000]) {
    test('projection delta scaling at $segmentCount segments', () {
      const stride = 24;
      final source = List<String>.filled(
        segmentCount,
        '**${'a' * (stride - 2)}',
      ).join();
      final projection = FlarkProjection(
        textLength: source.length,
        hiddenRanges: [
          for (var index = 0; index < segmentCount; index += 1)
            FlarkHiddenRange(
              range: FlarkSourceRange(index * stride, index * stride + 2),
              kind: FlarkHiddenRangeKind.inlineMarker,
            ),
        ],
      );
      final editSegment = segmentCount ~/ 2;
      final editOffset = editSegment * stride + 12;
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(editOffset, 'x'),
      );
      final segmented = _SegmentTree.uniform(
        segmentCount,
        sourceLength: stride,
        displayLength: stride - 2,
      );

      final currentPredict = _measure(
        'current_projection_predict_${segmentCount}segments',
        iterations: segmentCount == 5000 ? 20 : 5,
        warmups: 2,
        body: () {
          final next = projection.predictAfter(
            transaction,
            textLengthAfter: source.length + 1,
          );
          return next.projection.hiddenRanges.length +
              next.projection.displayLength;
        },
      );
      final segmentedUpdate = _measure(
        'segmented_projection_update_${segmentCount}segments',
        iterations: 500,
        warmups: 20,
        body: () {
          final next = segmented.replace(
            editSegment,
            sourceLength: stride + 1,
            displayLength: stride - 1,
          );
          return next.sourceToDisplay(source.length - 1) + next.displayLength;
        },
      );
      final currentMaterialize = _measure(
        'current_projection_materialize_${segmentCount}segments',
        iterations: segmentCount == 5000 ? 20 : 5,
        warmups: 2,
        body: () {
          final display = projection.projectText(source);
          return display.length + display.codeUnitAt(display.length - 1);
        },
      );
      final localMaterialize = _measure(
        'segmented_projection_materialize_one_of_$segmentCount',
        iterations: 500,
        warmups: 20,
        body: () {
          final start = editSegment * stride;
          final local = source.substring(start + 2, start + stride);
          return local.length + local.codeUnitAt(local.length - 1);
        },
      );

      _report(currentPredict);
      _report(segmentedUpdate);
      _report(currentMaterialize);
      _report(localMaterialize);

      final updated = segmented.replace(
        editSegment,
        sourceLength: stride + 1,
        displayLength: stride - 1,
      );
      expect(updated.sourceLength, source.length + 1);
      expect(updated.displayLength, projection.displayLength + 1);
      expect(
        updated.sourceToDisplay(editOffset),
        projection.sourceToDisplayOffset(editOffset),
      );
    });
  }
}

final class _SegmentTree {
  const _SegmentTree._(this._root);

  factory _SegmentTree.uniform(
    int count, {
    required int sourceLength,
    required int displayLength,
  }) {
    return _SegmentTree._(
      _buildUniform(
        0,
        count,
        sourceLength: sourceLength,
        displayLength: displayLength,
      ),
    );
  }

  final _SegmentNode _root;

  int get sourceLength => _root.sourceLength;
  int get displayLength => _root.displayLength;

  _SegmentTree replace(
    int index, {
    required int sourceLength,
    required int displayLength,
  }) {
    if (index < 0 || index >= _root.count) {
      throw RangeError.range(index, 0, _root.count - 1, 'index');
    }
    return _SegmentTree._(
      _replaceLeaf(
        _root,
        index,
        sourceLength: sourceLength,
        displayLength: displayLength,
      ),
    );
  }

  int sourceToDisplay(int sourceOffset) {
    if (sourceOffset < 0 || sourceOffset > sourceLength) {
      throw RangeError.range(sourceOffset, 0, sourceLength, 'sourceOffset');
    }
    return _sourceToDisplay(_root, sourceOffset);
  }
}

sealed class _SegmentNode {
  const _SegmentNode();

  int get count;
  int get sourceLength;
  int get displayLength;
}

final class _SegmentLeaf extends _SegmentNode {
  const _SegmentLeaf({required this.sourceLength, required this.displayLength});

  @override
  int get count => 1;
  @override
  final int sourceLength;
  @override
  final int displayLength;
}

final class _SegmentBranch extends _SegmentNode {
  _SegmentBranch(this.left, this.right)
    : count = left.count + right.count,
      sourceLength = left.sourceLength + right.sourceLength,
      displayLength = left.displayLength + right.displayLength;

  final _SegmentNode left;
  final _SegmentNode right;
  @override
  final int count;
  @override
  final int sourceLength;
  @override
  final int displayLength;
}

_SegmentNode _buildUniform(
  int start,
  int end, {
  required int sourceLength,
  required int displayLength,
}) {
  if (end - start == 1) {
    return _SegmentLeaf(
      sourceLength: sourceLength,
      displayLength: displayLength,
    );
  }
  final middle = start + ((end - start) >> 1);
  return _SegmentBranch(
    _buildUniform(
      start,
      middle,
      sourceLength: sourceLength,
      displayLength: displayLength,
    ),
    _buildUniform(
      middle,
      end,
      sourceLength: sourceLength,
      displayLength: displayLength,
    ),
  );
}

_SegmentNode _replaceLeaf(
  _SegmentNode node,
  int index, {
  required int sourceLength,
  required int displayLength,
}) {
  if (node case _SegmentLeaf()) {
    return _SegmentLeaf(
      sourceLength: sourceLength,
      displayLength: displayLength,
    );
  }
  final branch = node as _SegmentBranch;
  if (index < branch.left.count) {
    return _SegmentBranch(
      _replaceLeaf(
        branch.left,
        index,
        sourceLength: sourceLength,
        displayLength: displayLength,
      ),
      branch.right,
    );
  }
  return _SegmentBranch(
    branch.left,
    _replaceLeaf(
      branch.right,
      index - branch.left.count,
      sourceLength: sourceLength,
      displayLength: displayLength,
    ),
  );
}

int _sourceToDisplay(_SegmentNode node, int sourceOffset) {
  if (node case final _SegmentLeaf leaf) {
    // This prototype models one two-code-unit hidden opener at the start of
    // every segment. A production leaf would carry its local projection map.
    return math.max(0, sourceOffset - (leaf.sourceLength - leaf.displayLength));
  }
  final branch = node as _SegmentBranch;
  if (sourceOffset <= branch.left.sourceLength) {
    return _sourceToDisplay(branch.left, sourceOffset);
  }
  return branch.left.displayLength +
      _sourceToDisplay(branch.right, sourceOffset - branch.left.sourceLength);
}

_BenchmarkResult _measure(
  String name, {
  required int iterations,
  required int warmups,
  required int Function() body,
}) {
  for (var index = 0; index < warmups; index += 1) {
    _consume(body());
  }
  final samples = <Duration>[];
  for (var index = 0; index < iterations; index += 1) {
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
