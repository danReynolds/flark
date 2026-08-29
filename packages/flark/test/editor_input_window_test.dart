import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('bounded activation preserves exact represented endpoints', () {
    final window = FlarkEditorInputWindowPlanner.activate(
      text: '01234😀567890abcdefghij',
      sourceStart: 100,
      caret: 109,
      selectionExtent: 112,
      ordinal: 7,
      affinity: FlarkTextAffinity.upstream,
      maximumCodeUnits: 8,
    );

    expect(window.selectionRepresented, isTrue);
    expect(window.text.length, lessThanOrEqualTo(8));
    expect(window.activeOrdinal, 7);
    expect(window.canonicalSelectionBaseUtf16, 109);
    expect(window.canonicalSelectionExtentUtf16, 112);
    expect(window.crossRowSelection, isTrue);
    expect(
      window.globalUtf16Start + window.selection.baseOffset,
      window.canonicalSelectionBaseUtf16,
    );
    expect(
      window.globalUtf16Start + window.selection.extentOffset,
      window.canonicalSelectionExtentUtf16,
    );
    expect(_startsWithLowSurrogate(window.text), isFalse);
    expect(_endsWithHighSurrogate(window.text), isFalse);
  });

  test('unrepresentable selection retains canonical endpoints', () {
    final window = FlarkEditorInputWindowPlanner.activate(
      text: '0123456789abcdefghij',
      sourceStart: 100,
      caret: 102,
      selectionExtent: 118,
      ordinal: 3,
      affinity: FlarkTextAffinity.downstream,
      maximumCodeUnits: 8,
    );

    expect(window.selectionRepresented, isFalse);
    expect(window.selection.isCollapsed, isTrue);
    expect(window.canonicalSelectionBaseUtf16, 102);
    expect(window.canonicalSelectionExtentUtf16, 118);
    expect(window.text.length, lessThanOrEqualTo(8));
  });

  test('collapsed window is scalar aligned and globally exact', () {
    final window = FlarkEditorInputWindowPlanner.collapsed(
      text: '01234😀567890abcdefghij',
      sourceStart: 40,
      caret: 51,
      ordinal: 4,
      maximumCodeUnits: 7,
    );

    expect(window.canonicalSelectionBaseUtf16, 51);
    expect(window.canonicalSelectionExtentUtf16, 51);
    expect(window.text.length, lessThanOrEqualTo(7));
    expect(_startsWithLowSurrogate(window.text), isFalse);
    expect(_endsWithHighSurrogate(window.text), isFalse);
  });

  test('window planning rejects nonpositive capacities', () {
    expect(
      () => FlarkEditorInputWindowPlanner.activate(
        text: 'a',
        sourceStart: 0,
        caret: 0,
        ordinal: 0,
        affinity: FlarkTextAffinity.downstream,
        maximumCodeUnits: 0,
      ),
      throwsArgumentError,
    );
    expect(
      () => FlarkEditorInputWindowPlanner.collapsed(
        text: 'a',
        sourceStart: 0,
        caret: 0,
        ordinal: 0,
        maximumCodeUnits: -1,
      ),
      throwsArgumentError,
    );
  });
}

bool _startsWithLowSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(0) >= 0xDC00 &&
    value.codeUnitAt(0) <= 0xDFFF;

bool _endsWithHighSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(value.length - 1) >= 0xD800 &&
    value.codeUnitAt(value.length - 1) <= 0xDBFF;
