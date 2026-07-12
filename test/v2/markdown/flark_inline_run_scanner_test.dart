import 'package:flark/src/v2/markdown/inline/flark_inline_run_scanner.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('validEnclosingRun', () {
    test('finds a flanking-valid strong run around the caret', () {
      final run = FlarkInlineRunScanner.validEnclosingRun('**foo**', 3, '**');
      expect(run, isNotNull);
      expect(run!.openStart, 0);
      expect(run.contentStart, 2);
      expect(run.closeStart, 5);
      expect(run.closeEnd, 7);
    });

    test('caret at close start and content start count as inside', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo**', 5, '**'),
        isNotNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo**', 2, '**'),
        isNotNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo**', 1, '**'),
        isNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo**', 6, '**'),
        isNull,
      );
    });

    test('rejects a close delimiter preceded by whitespace', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo **', 3, '**'),
        isNull,
      );
    });

    test('rejects an open delimiter followed by whitespace', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('** foo**', 4, '**'),
        isNull,
      );
    });

    test('rejects blank content', () {
      expect(FlarkInlineRunScanner.validEnclosingRun('** **', 3, '**'), isNull);
    });

    test('rejects escaped delimiters', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun(r'\**foo**', 4, '**'),
        isNull,
      );
    });

    test('a ** probe never matches inside a *** cluster', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('***foo***', 4, '**'),
        isNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('***foo***', 4, '***'),
        isNotNull,
      );
    });

    test('underscore refuses to open intraword', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('foo_bar_', 5, '_'),
        isNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('foo _bar_', 6, '_'),
        isNotNull,
      );
    });

    test('does not pair a close with a later open across plain text', () {
      // Caret between two real runs must not report a phantom run built from
      // the first run's closer and the second run's opener.
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**a** b **c**', 6, '**'),
        isNull,
      );
    });

    test('never crosses a blank line', () {
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo\n\nbar**', 3, '**'),
        isNull,
      );
      expect(
        FlarkInlineRunScanner.validEnclosingRun('**foo\nbar**', 3, '**'),
        isNotNull,
      );
    });
  });

  group('runClosingAt / runOpeningAt', () {
    test('reports the run only exactly at its edges', () {
      expect(FlarkInlineRunScanner.runClosingAt('**hello**', 7), isNotNull);
      expect(FlarkInlineRunScanner.runClosingAt('**hello**', 6), isNull);
      expect(FlarkInlineRunScanner.runOpeningAt('**hello**', 2), isNotNull);
      expect(FlarkInlineRunScanner.runOpeningAt('**hello**', 3), isNull);
    });

    test('prefers the full *** cluster at a stacked run edge', () {
      final run = FlarkInlineRunScanner.runClosingAt('***hello***', 8);
      expect(run, isNotNull);
      expect(run!.marker, '***');
    });
  });

  group('reentryGapAt', () {
    test('finds the gap after a run followed by horizontal whitespace', () {
      final gap = FlarkInlineRunScanner.reentryGapAt('**hello** ', 10);
      expect(gap, isNotNull);
      expect(gap!.run.marker, '**');
      expect(gap.whitespace, ' ');
    });

    test('spans multiple spaces and tabs', () {
      final gap = FlarkInlineRunScanner.reentryGapAt('**hello** \t ', 12);
      expect(gap, isNotNull);
      expect(gap!.whitespace, ' \t ');
    });

    test('requires whitespace, a valid run, and no newline crossing', () {
      expect(FlarkInlineRunScanner.reentryGapAt('**hello**', 9), isNull);
      expect(FlarkInlineRunScanner.reentryGapAt('**hello **  ', 11), isNull);
      expect(FlarkInlineRunScanner.reentryGapAt('**hello**\n', 10), isNull);
    });
  });
}
