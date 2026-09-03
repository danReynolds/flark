import 'package:flark/flark.dart';
import 'package:flark_flutter/src/text_adaptation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Flutter text state crosses the portable boundary without drift', () {
    const flutter = TextEditingValue(
      text: 'abc',
      selection: TextSelection(
        baseOffset: 3,
        extentOffset: 1,
        affinity: TextAffinity.upstream,
        isDirectional: true,
      ),
      composing: TextRange(start: 1, end: 3),
    );

    final portable = portableEditorInputValue(flutter);

    expect(portable.text, flutter.text);
    expect(portable.selection.baseOffset, flutter.selection.baseOffset);
    expect(portable.selection.extentOffset, flutter.selection.extentOffset);
    expect(portable.selection.affinity, FlarkTextAffinity.upstream);
    expect(portable.selection.isDirectional, isTrue);
    expect((portable.composing.start, portable.composing.end), (1, 3));
    expect(flutterTextSelection(portable.selection), flutter.selection);
  });
}
