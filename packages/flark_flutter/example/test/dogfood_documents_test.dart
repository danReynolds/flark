import 'package:flark_example/dogfood_documents.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('large dogfood presets have exact declared ASCII byte sizes', () {
    for (final preset in DogfoodDocumentPreset.values) {
      final target = preset.targetBytes;
      if (target == null) continue;
      final source = buildDogfoodDocument(preset);
      expect(source.length, target, reason: preset.label);
      expect(source.codeUnits.every((unit) => unit <= 0x7f), isTrue);
    }
  });

  test('product tour contains the interaction and Unicode cases', () {
    final source = buildDogfoodDocument(DogfoodDocumentPreset.productTour);
    expect(source, contains('| Surface | Authority | State |'));
    expect(source, contains('- [x] A checked task'));
    expect(source, contains('👩‍💻'));
    expect(source, contains('العربية'));
    expect(source, contains('**unfinished'));
  });
}
