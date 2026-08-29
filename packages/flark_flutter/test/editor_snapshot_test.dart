import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'editor snapshots are complete and immutable across selection',
    () async {
      final controller = await FlarkEditorController.open(
        '**alpha**\n',
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final row = controller.rows.single;
      final first = controller.snapshot;
      expect(identical(controller.value, first), isTrue);
      expect(first.status, FlarkEditorStatus.ready);
      expect(first.lastError, isNull);
      expect(first.sourceByteLength, 10);
      expect(first.sourceUtf16Length, 10);
      expect(first.pendingEdits, 0);
      expect(first.canUndo, isFalse);
      expect(first.canRedo, isFalse);
      final firstRows = first.rows;
      final firstPresentation = firstRows.single.editingPresentations.single;
      FlarkEditorSnapshot? observed;
      controller.addListener(() => observed = controller.value);

      controller.activateRow(row, 5);
      final second = controller.snapshot;

      expect(identical(first, second), isFalse);
      expect(identical(controller.value, second), isTrue);
      expect(identical(observed, second), isTrue);
      expect(second.sequence, greaterThan(first.sequence));
      expect(
        second.interactionGeneration,
        first.interactionGeneration,
        reason:
            'selection paint may republish without invalidating unchanged '
            'layout interaction geometry',
      );
      expect(first.visibleSource, '**alpha**\n');
      expect(
        first.canonicalSelectionExtentUtf16,
        isNot(second.canonicalSelectionExtentUtf16),
      );
      expect(identical(first.rows, firstRows), isTrue);
      expect(firstRows.single.editingPresentations.single, firstPresentation);
      expect(firstPresentation.text, 'alpha');
      expect(() => first.rows.clear(), throwsUnsupportedError);
      expect(
        () => firstRows.single.editingPresentations.clear(),
        throwsUnsupportedError,
      );
      expect(() => firstPresentation.runs.clear(), throwsUnsupportedError);
      expect(
        () => firstPresentation.runs.single.styles.clear(),
        throwsUnsupportedError,
      );
      expect(
        () => firstRows.single.row.inlineFacts?.clear(),
        throwsUnsupportedError,
      );
      expect(
        firstPresentation.selection,
        isNot(second.rows.single.editingPresentations.single.selection),
      );
    },
    skip: libraryPath == null,
  );
}
