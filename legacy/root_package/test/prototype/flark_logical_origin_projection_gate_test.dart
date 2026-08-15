import 'dart:math' as math;

import 'package:test/test.dart';

void main() {
  test(
    'logical facts compose across stripped prefixes without claiming them',
    () {
      const source = '> - **alpha**\r\n>   beta &amp;';
      const logical = '**alpha**\nbeta &amp;';
      final first = source.indexOf('**alpha**');
      final crlf = source.indexOf('\r\n');
      final second = source.indexOf('beta');
      final entitySource = source.indexOf('&amp;');
      final entityLogical = logical.indexOf('&amp;');

      final projection = _LogicalProjection(
        source: source,
        logical: logical,
        origins: [
          _OriginRun.identity(
            logical: const _Range(0, 9),
            source: _Range(first, first + 9),
          ),
          _OriginRun.atomic(
            logical: const _Range(9, 10),
            source: _Range(crlf, crlf + 2),
          ),
          _OriginRun.identity(
            logical: _Range(10, logical.length),
            source: _Range(second, source.length),
          ),
        ],
        annotations: [
          const _Annotation.hidden(_Range(0, 2)),
          const _Annotation.hidden(_Range(7, 9)),
          _Annotation.replacement(
            _Range(entityLogical, entityLogical + 5),
            '&',
          ),
        ],
      );

      expect(projection.display, 'alpha\nbeta &');
      expect(projection.sourceCoverageIsDisjointAndExact, isTrue);

      // The authoritative strong fact spans both physical content runs, but its
      // physical image is compound. Treating its endpoints as one source range
      // would incorrectly claim CRLF and the second-line quote/list prefix.
      final strongParts = projection.physicalParts(const _Range(0, 9));
      expect(strongParts, [_Range(first, first + 9)]);
      final multilineParts = projection.physicalParts(
        _Range(0, logical.indexOf('&amp;')),
      );
      expect(multilineParts, [
        _Range(first, first + 9),
        _Range(crlf, crlf + 2),
        _Range(second, entitySource),
      ]);
      final naiveEnvelope = _Range(
        multilineParts.first.start,
        multilineParts.last.end,
      );
      expect(naiveEnvelope.length, greaterThan(_sumLengths(multilineParts)));
      expect(
        source.substring(naiveEnvelope.start, naiveEnvelope.end),
        contains('>   '),
      );

      // Source positions inside stripped prefixes snap to the adjacent logical
      // boundary; they can never become content owned by an inline fact.
      expect(projection.sourceToDisplay(0, _Affinity.upstream), 0);
      expect(projection.sourceToDisplay(first - 1, _Affinity.downstream), 0);
      expect(
        projection.sourceToDisplay(second - 1, _Affinity.upstream),
        'alpha\n'.length,
      );
      expect(
        projection.sourceToDisplay(second - 1, _Affinity.downstream),
        'alpha\n'.length,
      );

      // CRLF is one atomic logical newline. Neither of its interior source
      // boundaries becomes a fictitious logical character.
      expect(projection.sourceToLogical(crlf + 1, _Affinity.upstream), 9);
      expect(projection.sourceToLogical(crlf + 1, _Affinity.downstream), 10);
      expect(projection.logicalToSource(9, _Affinity.downstream), crlf);
      expect(projection.logicalToSource(10, _Affinity.upstream), crlf + 2);

      // The entity replacement remains leaf-local. It does not overlap block
      // marker gaps, and both source/display sides round-trip at its edges.
      expect(
        projection.sourceToDisplay(entitySource, _Affinity.downstream),
        11,
      );
      expect(
        projection.sourceToDisplay(source.length, _Affinity.upstream),
        projection.display.length,
      );
      expect(
        projection.displayToSource(11, _Affinity.downstream),
        entitySource,
      );
      expect(
        projection.displayToSource(
          projection.display.length,
          _Affinity.upstream,
        ),
        source.length,
      );
    },
  );

  test(
    'one logical replacement may span several source-origin runs safely',
    () {
      const source = '> ` a\r\n> b `';
      const logical = '` a\nb `';
      final first = source.indexOf('` a');
      final crlf = source.indexOf('\r\n');
      final second = source.indexOf('b `');
      final projection = _LogicalProjection(
        source: source,
        logical: logical,
        origins: [
          _OriginRun.identity(
            logical: const _Range(0, 3),
            source: _Range(first, first + 3),
          ),
          _OriginRun.atomic(
            logical: const _Range(3, 4),
            source: _Range(crlf, crlf + 2),
          ),
          _OriginRun.identity(
            logical: const _Range(4, 7),
            source: _Range(second, second + 3),
          ),
        ],
        annotations: const [
          _Annotation.hidden(_Range(0, 1)),
          _Annotation.replacement(_Range(1, 6), 'a b'),
          _Annotation.hidden(_Range(6, 7)),
        ],
      );

      expect(projection.display, 'a b');
      expect(projection.physicalParts(const _Range(1, 6)), [
        _Range(first + 1, first + 3),
        _Range(crlf, crlf + 2),
        _Range(second, second + 2),
      ]);

      // The replacement has one semantic identity but several physical parts;
      // source positions in the stripped quote prefix map to a boundary rather
      // than overlapping the replacement.
      expect(projection.sourceToDisplay(crlf + 2, _Affinity.upstream), 0);
      expect(projection.sourceToDisplay(crlf + 2, _Affinity.downstream), 3);
      expect(projection.sourceToDisplay(second, _Affinity.upstream), 0);
      expect(projection.sourceToDisplay(second, _Affinity.downstream), 3);
      for (var offset = 0; offset <= projection.display.length; offset += 1) {
        final sourceOffset = projection.displayToSource(
          offset,
          _Affinity.downstream,
        );
        expect(sourceOffset, inInclusiveRange(0, source.length));
      }
    },
  );

  test('tab expansion is an atomic transform, not invented source bytes', () {
    const source = '>\titem';
    const logical = '   item';
    final projection = _LogicalProjection(
      source: source,
      logical: logical,
      origins: [
        _OriginRun.atomic(
          logical: const _Range(0, 3),
          source: const _Range(1, 2),
        ),
        _OriginRun.identity(
          logical: const _Range(3, 7),
          source: const _Range(2, 6),
        ),
      ],
      annotations: const [],
    );

    expect(projection.display, logical);
    expect(projection.logicalToSource(1, _Affinity.upstream), 1);
    expect(projection.logicalToSource(1, _Affinity.downstream), 2);
    expect(projection.sourceToLogical(1, _Affinity.downstream), 0);
    expect(projection.sourceToLogical(2, _Affinity.upstream), 3);
  });
}

enum _Affinity { upstream, downstream }

final class _Range {
  const _Range(this.start, this.end) : assert(start <= end);

  final int start;
  final int end;

  int get length => end - start;

  bool containsInterior(int offset) => start < offset && offset < end;

  _Range? intersection(_Range other) {
    final intersectionStart = math.max(start, other.start);
    final intersectionEnd = math.min(end, other.end);
    return intersectionStart < intersectionEnd
        ? _Range(intersectionStart, intersectionEnd)
        : null;
  }

  @override
  bool operator ==(Object other) =>
      other is _Range && start == other.start && end == other.end;

  @override
  int get hashCode => Object.hash(start, end);

  @override
  String toString() => '[$start,$end)';
}

enum _OriginKind { identity, atomic }

final class _OriginRun {
  const _OriginRun.identity({required this.logical, required this.source})
    : kind = _OriginKind.identity,
      assert(logical.length == source.length);

  const _OriginRun.atomic({required this.logical, required this.source})
    : kind = _OriginKind.atomic;

  final _Range logical;
  final _Range source;
  final _OriginKind kind;
}

enum _AnnotationKind { hidden, replacement }

final class _Annotation {
  const _Annotation.hidden(this.logical)
    : kind = _AnnotationKind.hidden,
      replacement = '';

  const _Annotation.replacement(this.logical, this.replacement)
    : kind = _AnnotationKind.replacement;

  final _Range logical;
  final _AnnotationKind kind;
  final String replacement;
}

final class _LogicalProjection {
  _LogicalProjection({
    required this.source,
    required this.logical,
    required List<_OriginRun> origins,
    required List<_Annotation> annotations,
  }) : origins = List.unmodifiable(origins),
       annotations = List.unmodifiable(
         List<_Annotation>.of(annotations)..sort(_compareAnnotations),
       ) {
    _validate();
    final buffer = StringBuffer();
    var cursor = 0;
    for (final annotation in this.annotations) {
      buffer.write(logical.substring(cursor, annotation.logical.start));
      if (annotation.kind == _AnnotationKind.replacement) {
        buffer.write(annotation.replacement);
      }
      cursor = annotation.logical.end;
    }
    buffer.write(logical.substring(cursor));
    display = buffer.toString();
  }

  final String source;
  final String logical;
  final List<_OriginRun> origins;
  final List<_Annotation> annotations;
  late final String display;

  bool get sourceCoverageIsDisjointAndExact {
    var previousLogical = 0;
    var previousSource = 0;
    for (final run in origins) {
      if (run.logical.start != previousLogical ||
          run.source.start < previousSource) {
        return false;
      }
      previousLogical = run.logical.end;
      previousSource = run.source.end;
    }
    return previousLogical == logical.length && previousSource <= source.length;
  }

  List<_Range> physicalParts(_Range logicalRange) {
    final parts = <_Range>[];
    for (final run in origins) {
      final overlap = run.logical.intersection(logicalRange);
      if (overlap == null) continue;
      if (run.kind == _OriginKind.identity) {
        final start = run.source.start + overlap.start - run.logical.start;
        parts.add(_Range(start, start + overlap.length));
      } else if (overlap == run.logical) {
        parts.add(run.source);
      } else {
        throw StateError('an inline fact split an atomic origin run');
      }
    }
    return parts;
  }

  int sourceToDisplay(int offset, _Affinity affinity) =>
      logicalToDisplay(sourceToLogical(offset, affinity), affinity);

  int displayToSource(int offset, _Affinity affinity) =>
      logicalToSource(displayToLogical(offset, affinity), affinity);

  int sourceToLogical(int offset, _Affinity affinity) {
    RangeError.checkValueInInterval(offset, 0, source.length, 'offset');
    _OriginRun? previous;
    for (final run in origins) {
      if (offset < run.source.start) {
        return affinity == _Affinity.upstream
            ? previous?.logical.end ?? run.logical.start
            : run.logical.start;
      }
      if (offset == run.source.start) return run.logical.start;
      if (offset < run.source.end) {
        if (run.kind == _OriginKind.identity) {
          return run.logical.start + offset - run.source.start;
        }
        return affinity == _Affinity.upstream
            ? run.logical.start
            : run.logical.end;
      }
      if (offset == run.source.end) return run.logical.end;
      previous = run;
    }
    return logical.length;
  }

  int logicalToSource(int offset, _Affinity affinity) {
    RangeError.checkValueInInterval(offset, 0, logical.length, 'offset');
    for (final run in origins) {
      if (offset < run.logical.start) return run.source.start;
      if (offset == run.logical.start) return run.source.start;
      if (offset < run.logical.end) {
        if (run.kind == _OriginKind.identity) {
          return run.source.start + offset - run.logical.start;
        }
        return affinity == _Affinity.upstream
            ? run.source.start
            : run.source.end;
      }
      if (offset == run.logical.end) return run.source.end;
    }
    return source.length;
  }

  int logicalToDisplay(int offset, _Affinity affinity) {
    RangeError.checkValueInInterval(offset, 0, logical.length, 'offset');
    var removed = 0;
    var added = 0;
    for (final annotation in annotations) {
      if (offset < annotation.logical.start) break;
      final displayStart = annotation.logical.start - removed + added;
      if (offset == annotation.logical.start) return displayStart;
      if (offset <= annotation.logical.end) {
        if (offset == annotation.logical.end) {
          return displayStart + annotation.replacement.length;
        }
        return affinity == _Affinity.upstream
            ? displayStart
            : displayStart + annotation.replacement.length;
      }
      removed += annotation.logical.length;
      added += annotation.replacement.length;
    }
    return offset - removed + added;
  }

  int displayToLogical(int offset, _Affinity affinity) {
    RangeError.checkValueInInterval(offset, 0, display.length, 'offset');
    var logicalCursor = 0;
    var displayCursor = 0;
    for (final annotation in annotations) {
      final plainLength = annotation.logical.start - logicalCursor;
      if (offset <= displayCursor + plainLength) {
        return logicalCursor + offset - displayCursor;
      }
      displayCursor += plainLength;
      final replacementEnd = displayCursor + annotation.replacement.length;
      if (annotation.replacement.isEmpty && offset == displayCursor) {
        return affinity == _Affinity.upstream
            ? annotation.logical.start
            : annotation.logical.end;
      }
      if (offset == displayCursor) return annotation.logical.start;
      if (offset == replacementEnd) return annotation.logical.end;
      if (offset <= replacementEnd) {
        return affinity == _Affinity.upstream
            ? annotation.logical.start
            : annotation.logical.end;
      }
      displayCursor = replacementEnd;
      logicalCursor = annotation.logical.end;
    }
    return logicalCursor + offset - displayCursor;
  }

  void _validate() {
    if (!sourceCoverageIsDisjointAndExact) {
      throw StateError('origin runs must exactly partition logical text');
    }
    var annotationEnd = 0;
    for (final annotation in annotations) {
      if (annotation.logical.start < annotationEnd ||
          annotation.logical.end > logical.length) {
        throw StateError('projection annotations overlap or leave the leaf');
      }
      annotationEnd = annotation.logical.end;
    }
    for (final run in origins) {
      if (run.kind == _OriginKind.identity &&
          logical.substring(run.logical.start, run.logical.end) !=
              source.substring(run.source.start, run.source.end)) {
        throw StateError('identity origin bytes differ');
      }
    }
  }
}

int _compareAnnotations(_Annotation left, _Annotation right) =>
    left.logical.start.compareTo(right.logical.start);

int _sumLengths(Iterable<_Range> ranges) =>
    ranges.fold(0, (sum, range) => sum + range.length);
