import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('opens the bundled v4 native asset without a library path', () async {
    final document = await FlarkCoreDocument.open('# packaged\n\ntext');
    try {
      expect(await document.readSource(), '# packaged\n\ntext');
      expect(document.revision, 1);
    } finally {
      await document.dispose();
    }
  });
}
