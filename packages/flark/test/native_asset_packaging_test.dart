import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('opens the bundled flark_core native asset transitively', () async {
    final controller = await FlarkEditorController.open('# packaged\n\ntext');
    try {
      expect(controller.sourceUtf16Length, '# packaged\n\ntext'.length);
      expect(controller.status, isNot(FlarkEditorStatus.faulted));
    } finally {
      await controller.close();
    }
  });
}
