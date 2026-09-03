import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final controller = File('lib/src/controller.dart').readAsStringSync();
  final coordinator = File(
    '../flark/lib/src/editor_coordinator.dart',
  ).readAsStringSync();

  test('the coordinator owns complete command lifetimes', () {
    for (final retiredOperation in [
      'admitEditingCommand',
      'beginPendingEdit',
      'endPendingEdit',
      'beginHistoryReplay',
      'endHistoryReplay',
      'publishSourceGeneration',
    ]) {
      expect(
        controller,
        isNot(contains(retiredOperation)),
        reason: '$retiredOperation must not return as split lifecycle state',
      );
      expect(
        coordinator,
        isNot(contains('$retiredOperation(')),
        reason: '$retiredOperation must not return as a parallel public path',
      );
    }
    expect(controller, isNot(contains('FlarkEditorStatus.editing')));
    expect(coordinator, contains('FlarkEditorCommandTicket admitCommand('));
    expect(coordinator, contains('void completeCommand('));
    expect(coordinator, contains('void failCommand('));
  });
}
