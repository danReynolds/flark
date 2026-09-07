import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:test/test.dart';
import '../tool/catalog_cases.dart';

void main() {
  late CodeAnalyzer code;
  setUp(() => code = CodeAnalyzer());
  tearDown(() => code.dispose());
  test('every selectable grammar has highlight and detection examples', () {
    expect(catalogCases.keys.toSet(), CodeLanguage.values.skip(1).toSet());
    expect(
      codeLanguages.keys.where((n) => n != 'text').toSet(),
      CodeLanguage.values.skip(1).map((l) => l.name).toSet(),
    );
  });
  for (final c in catalogCases.entries) {
    test('${c.key.name} highlights upstream syntax with exact ranges', () {
      final result = code.analyze(c.value, language: c.key);
      expect(
        result.spans.map((s) => c.value.substring(s.start, s.end)).join(),
        c.value,
      );
      expect(result.spans.where((s) => s.scopes.isNotEmpty), isNotEmpty);
      expect(result.spans.length, greaterThan(1));
    });
  }
  for (final c in detectionCases.entries) {
    test('Automatic ${c.value.name}: ${c.key}', () {
      expect(code.detect(c.key), c.value);
      code.detect('another document');
      expect(code.detect(c.key), c.value);
    });
  }
  test('aliases and manual choices bypass Automatic', () {
    for (final pair in [
      ('js', 'javascript'),
      ('rb', 'ruby'),
      ('html', 'html'),
      ('xml', 'xml'),
      ('sh', 'bash'),
      ('yml', 'yaml'),
    ]) {
      expect(codeLanguageName(pair.$1), pair.$2);
    }
    expect(code.detect('x' * 8193), CodeLanguage.plain);
  });
}
