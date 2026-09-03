import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('UTF-16 windows exclude rather than split boundary scalars', () {
    const text = 'a🌍b';

    expect(scalarAlignedUtf16Window(text, 2, 4), (start: 3, end: 4));
    expect(scalarAlignedUtf16Window(text, 0, 2), (start: 0, end: 1));
    expect(scalarAlignedUtf16Window(text, 1, 3), (start: 1, end: 3));
  });

  test('UTF-16 window validation fails before slicing', () {
    expect(() => scalarAlignedUtf16Window('abc', -1, 2), throwsRangeError);
    expect(() => scalarAlignedUtf16Window('abc', 2, 1), throwsRangeError);
    expect(() => scalarAlignedUtf16Window('abc', 0, 4), throwsRangeError);
  });
}
