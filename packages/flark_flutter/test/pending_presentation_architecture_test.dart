import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final controller = File('lib/src/controller.dart').readAsStringSync();

  test('controller owns exactly one pending-presentation state slot', () {
    expect(
      RegExp(
        r'FlarkPendingPresentationSnapshot\s+_pendingPresentation\b',
      ).allMatches(controller),
      hasLength(1),
    );
    for (final retiredSlot in [
      '_projectionContinuity',
      '_committedParagraphSplit',
      '_committedStructuralSurfaces',
      '_committedTaskChecks',
    ]) {
      expect(
        controller,
        isNot(contains(retiredSlot)),
        reason: '$retiredSlot must not return as parallel host authority',
      );
    }
  });

  test(
    'controller consumes the sealed Core binder without Markdown policy',
    () {
      expect(
        RegExp(
          r'bindPendingDependencyAuthority\(',
        ).allMatches(controller).length,
        greaterThanOrEqualTo(2),
      );
      expect(controller, isNot(contains('authorizeProjectionEditCell(')));
      expect(controller, isNot(contains('authorizeRowProjectionContinuity(')));
      expect(controller, isNot(contains('withDependency(null)')));
      expect(controller, isNot(contains('withParagraphGap(null)')));
      expect(controller, isNot(contains('withStructuralSurfaces(const [])')));
      expect(controller, contains('.retire('));
      for (final parserPolicy in [
        '_isAsciiAlphanumeric',
        '_isSafeAsciiProsePunctuation',
        'FlarkLiteralEditClass.',
        'FlarkProjectionEditMatcher.',
      ]) {
        expect(
          controller,
          isNot(contains(parserPolicy)),
          reason: '$parserPolicy belongs behind the Core/parser boundary',
        );
      }
    },
  );
}
