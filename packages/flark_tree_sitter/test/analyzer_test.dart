import 'dart:convert';
import 'dart:typed_data';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:test/test.dart';

void main() {
  late CodeAnalyzer analyzer;
  setUp(() => analyzer = CodeAnalyzer());
  tearDown(() => analyzer.dispose());

  for (final language in CodeLanguage.values) {
    test(
      '${language.name}: exact Unicode/CRLF source after editing earlier context',
      () {
        const source = '😀 café 中文\r\n\tvalue = "hello";\n';
        final first = analyzer.analyze(source, language: language);
        analyzer.analyze('/* unfinished', language: language);
        final second = analyzer.analyze(source, language: language);
        expect(
          second.spans.map((s) => source.substring(s.start, s.end)).join(),
          source,
        );
        expect(second.spans.last.endByte, utf8.encode(source).length);
        expect(
          second.spans.map((s) => s.scopes),
          first.spans.map((s) => s.scopes),
        );
        expect(() => second.spans.clear(), throwsUnsupportedError);
        expect(
          () => second.spans.first.scopes.add('fake'),
          throwsUnsupportedError,
        );
      },
    );
  }

  test('typed Dart loop highlights through every incomplete prefix', () {
    const source = 'for (final x in [1, 2, 3]) {\n  print("😀");\n}';
    for (var end = 0; end <= source.length; end++) {
      if (end < source.length &&
          source.codeUnitAt(end) >= 0xdc00 &&
          source.codeUnitAt(end) <= 0xdfff) {
        continue;
      }
      final prefix = source.substring(0, end);
      final result = analyzer.analyze(prefix, language: CodeLanguage.dart);
      expect(
        result.spans.map((s) => prefix.substring(s.start, s.end)).join(),
        prefix,
      );
    }
    final full = analyzer.analyze(source, language: CodeLanguage.dart);
    expect(full.spans.expand((s) => s.scopes), contains('string'));
    expect(full.spans.first.scopes, contains('keyword'));
  });

  test('Ruby method, keyword and string scopes survive incomplete typing', () {
    const source = "def hello\n  print '😀 café'\nend";
    for (var end = 0; end <= source.length; end++) {
      if (end < source.length &&
          source.codeUnitAt(end) >= 0xdc00 &&
          source.codeUnitAt(end) <= 0xdfff) {
        continue;
      }
      final prefix = source.substring(0, end);
      final result = analyzer.analyze(prefix, language: CodeLanguage.ruby);
      expect(
        result.spans.map((s) => prefix.substring(s.start, s.end)).join(),
        prefix,
      );
    }
    final full = analyzer.analyze(source, language: CodeLanguage.ruby);
    Set<String> scopes(String text) => full.spans
        .where(
          (s) =>
              s.start <= source.indexOf(text) && source.indexOf(text) < s.end,
        )
        .expand((s) => s.scopes)
        .toSet();
    expect(scopes('def'), contains('keyword'));
    expect(scopes('end'), contains('keyword'));
    expect(scopes('hello'), contains('function.method'));
    expect(scopes('😀'), contains('string'));
  });

  test('oversized input stays exact and reports the limit', () {
    final source = '😀' * 4097;
    final result = analyzer.analyze(source, language: CodeLanguage.python);
    expect(result.source, source);
    expect(result.status, CodeAnalysisStatus.limit);
    expect(result.spans.single.end, source.length);
  });

  test('rejects invalid Unicode before crossing the bridge', () {
    expect(
      () => analyzer.analyze(
        String.fromCharCode(0xd800),
        language: CodeLanguage.dart,
      ),
      throwsArgumentError,
    );
  });

  test('dispose is idempotent and forbids subsequent calls', () {
    analyzer.dispose();
    analyzer.dispose();
    expect(
      () => analyzer.analyze('', language: CodeLanguage.dart),
      throwsStateError,
    );
  });

  test('version mismatch disposes the supplied backend', () {
    final backend = _Reply(version: 99);
    expect(() => CodeAnalyzer(backend: backend), throwsA(isA<CodeException>()));
    expect(backend.disposed, isTrue);
  });

  for (final fault in [
    'version',
    'language',
    'empty_range',
    'end',
    'end_byte',
    'scopes',
    'missing',
    'surrogate',
    'scope_index',
    'tuple',
    'scope_type',
  ]) {
    test('rejects corrupt bridge response: $fault', () {
      final source = fault == 'surrogate' ? '😀' : 'x';
      final span = <Object>[source.length, utf8.encode(source).length, 0];
      final value = <String, Object>{
        'version': 4,
        'language': 1,
        'status': 'highlighted',
        'spans': [span],
        'scope_sets': [<String>[]],
      };
      switch (fault) {
        case 'version':
          value['version'] = 2;
        case 'language':
          value['language'] = 2;
        case 'empty_range':
          span[0] = 0;
        case 'end':
          span[0] = 9;
        case 'end_byte':
          span[1] = 7;
        case 'scopes':
          value['scope_sets'] = [
            [''],
          ];
        case 'missing':
          value['spans'] = [];
        case 'surrogate':
          span[0] = 1;
        case 'scope_index':
          span[2] = 1;
        case 'tuple':
          span.add(0);
        case 'scope_type':
          value['scope_sets'] = [
            [42],
          ];
      }
      final service = CodeAnalyzer(
        backend: _Reply(reply: utf8.encode(jsonEncode(value))),
      );
      addTearDown(service.dispose);
      expect(
        () => service.analyze(source, language: CodeLanguage.dart),
        throwsA(isA<CodeException>()),
      );
    });
  }
}

final class _Reply implements CodeBackend {
  _Reply({this.version = 4, Uint8List? reply}) : reply = reply ?? Uint8List(0);
  @override
  Uint8List detect(Uint8List source) => throw UnimplementedError();
  @override
  final int version;
  final Uint8List reply;
  bool disposed = false;
  @override
  Uint8List analyze(Uint8List source, int language) => reply;
  @override
  Uint8List edit(Uint8List request, int language) => reply;
  @override
  void dispose() => disposed = true;
}
