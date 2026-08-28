import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'published surface snapshots remain immutable after selection changes',
    () async {
      final controller = await FlarkEditorController.open(
        '**alpha**\n',
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final row = controller.rows.single;
      final first = controller.surfacePublication;
      final firstRows = first.rows;
      final firstPresentation = firstRows.single.editingPresentations.single;

      controller.activateRow(row, 5);
      final second = controller.surfacePublication;

      expect(identical(first, second), isFalse);
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
