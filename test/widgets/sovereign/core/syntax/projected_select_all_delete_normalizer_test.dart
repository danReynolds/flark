import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/projected_select_all_delete_normalizer.dart';

void main() {
  group('ProjectedSelectAllDeleteNormalizer', () {
    test('normalizes projected select-all deletion to a full document clear',
        () {
      const oldValue = TextEditingValue(
        text: '# Title',
        selection: TextSelection(baseOffset: 0, extentOffset: 5),
      );
      const platformValue = TextEditingValue(
        text: 'le',
        selection: TextSelection.collapsed(offset: 0),
      );

      final normalized = ProjectedSelectAllDeleteNormalizer.normalize(
        oldValue: oldValue,
        newValue: platformValue,
        projectedHiddenRanges: const <TextRange>[TextRange(start: 0, end: 2)],
      );

      expect(normalized.text, isEmpty);
      expect(normalized.selection, const TextSelection.collapsed(offset: 0));
    });

    test('leaves ordinary deletions unchanged', () {
      const oldValue = TextEditingValue(
        text: '# Title',
        selection: TextSelection(baseOffset: 0, extentOffset: 5),
      );
      const platformValue = TextEditingValue(
        text: 'le',
        selection: TextSelection.collapsed(offset: 0),
      );

      final normalized = ProjectedSelectAllDeleteNormalizer.normalize(
        oldValue: oldValue,
        newValue: platformValue,
        projectedHiddenRanges: const <TextRange>[],
      );

      expect(normalized, platformValue);
    });

    test('does not rewrite composing updates', () {
      const oldValue = TextEditingValue(
        text: '# Title',
        selection: TextSelection(baseOffset: 0, extentOffset: 5),
        composing: TextRange(start: 0, end: 1),
      );
      const platformValue = TextEditingValue(
        text: 'le',
        selection: TextSelection.collapsed(offset: 0),
      );

      final normalized = ProjectedSelectAllDeleteNormalizer.normalize(
        oldValue: oldValue,
        newValue: platformValue,
        projectedHiddenRanges: const <TextRange>[TextRange(start: 0, end: 2)],
      );

      expect(normalized, platformValue);
    });
  });
}
