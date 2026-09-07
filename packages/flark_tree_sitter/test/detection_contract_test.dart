import 'dart:convert';
import 'dart:typed_data';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:test/test.dart';

void main() {
  test('detection rejects invalid transport data before caching it', () {
    final backend = _Backend();
    final code = CodeAnalyzer(backend: backend);
    addTearDown(code.dispose);
    for (final value in [
      'no json',
      '[]',
      '{}',
      '{"version":3,"language":5}',
      '{"version":4,"language":99}',
      '{"version":4,"language":5.0}',
    ]) {
      backend.response = utf8.encode(value);
      expect(() => code.detect('def hello'), throwsA(isA<CodeException>()));
    }
    backend.response = Uint8List.fromList([255]);
    expect(() => code.detect('def hello'), throwsA(isA<CodeException>()));
    backend.response = utf8.encode('{"version":4,"language":5}');
    expect(code.detect('def hello'), CodeLanguage.ruby);
  });

  test('detection clips Unicode safely, bounds cache and owns disposal', () {
    final backend = _Backend();
    final code = CodeAnalyzer(backend: backend);
    final prefix = 'x' * 127;
    code.detect('$prefix😀');
    expect(backend.sources.single, prefix);
    code.detect('$prefix😀 extra');
    expect(backend.sources, hasLength(1));
    for (var i = 0; i < 32; i++) {
      code.detect('source $i');
    }
    code.detect('$prefix😀');
    expect(backend.sources, hasLength(34));
    code.detect('x' * 8193);
    expect(backend.sources, hasLength(34));
    expect(() => code.detect(String.fromCharCode(0xd800)), throwsArgumentError);
    code.dispose();
    code.dispose();
    expect(backend.disposals, 1);
    expect(() => code.detect('x'), throwsStateError);
  });
}

class _Backend implements CodeBackend {
  final sources = <String>[];
  var disposals = 0;
  Uint8List response = utf8.encode('{"version":4,"language":0}');
  @override
  int get version => 4;
  @override
  Uint8List detect(Uint8List source) {
    sources.add(utf8.decode(source));
    return response;
  }

  @override
  Uint8List analyze(Uint8List source, int language) =>
      throw UnimplementedError();
  @override
  Uint8List edit(Uint8List request, int language) => throw UnimplementedError();
  @override
  void dispose() {
    disposals++;
  }
}
