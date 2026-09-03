import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final controller = File('lib/src/controller.dart').readAsStringSync();
  final coordinator = File(
    '../flark/lib/src/editor_coordinator.dart',
  ).readAsStringSync();
  final sourceEditPlanner = File(
    '../flark/lib/src/editor_source_edit_planner.dart',
  ).readAsStringSync();

  test('portable coordinator owns the pending-presentation state slot', () {
    expect(
      RegExp(
        r'FlarkPendingPresentationSnapshot\s+_pendingPresentation\b',
      ).allMatches(coordinator),
      hasLength(1),
    );
    expect(
      RegExp(
        r'FlarkPendingPresentationSnapshot\s+_pendingPresentation\s*=',
      ).allMatches(controller),
      isEmpty,
    );
    expect(
      RegExp(r'_pendingPresentation\s*=(?!=|>)').allMatches(controller),
      isEmpty,
      reason: 'the Flutter adapter must not mutate Core presentation state',
    );
    expect(coordinator, isNot(contains('replacePendingPresentation')));
    for (final operation in [
      'retirePendingPresentation',
      'setPendingDependency',
      'setPendingCaretBoundary',
      'setPendingStructuralSurfaces',
      'setPendingTaskCheck',
      'adoptCommittedPresentation',
    ]) {
      expect(
        coordinator,
        contains('$operation('),
        reason: '$operation must be an explicit coordinator operation',
      );
    }
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

  test('portable source-edit planning consumes the sealed Core binder', () {
    expect(
      RegExp(
        r'bindPendingDependencyAuthority\(',
      ).allMatches(sourceEditPlanner).length,
      greaterThanOrEqualTo(3),
    );
    expect(
      RegExp(r'bindPendingDependencyAuthority\(').allMatches(controller).length,
      1,
      reason: 'Flutter retains only committed-view reconciliation',
    );
    for (final retiredPolicy in [
      'authorizeProjectionEditCell(',
      'authorizeRowProjectionContinuity(',
      'withDependency(null)',
      'withParagraphGap(null)',
      'withStructuralSurfaces(const [])',
    ]) {
      expect(controller, isNot(contains(retiredPolicy)));
    }
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
  });
}
