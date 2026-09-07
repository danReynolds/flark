import 'dart:convert';
import 'dart:typed_data';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:test/test.dart';
import '../tool/edit_cases.dart';

void main() {
  late CodeAnalyzer analyzer;
  setUp(() => analyzer = CodeAnalyzer());
  tearDown(() => analyzer.dispose());
  for (final c in editCases) {
    test(c.name, () => runEditCase(analyzer, c));
  }
  test('rejects stale source when applying a proposal', () {
    final e = analyzer.proposeEdit(
      'x',
      language: d,
      base: 1,
      extent: 1,
      action: enter,
    )!;
    expect(() => e.applyTo('y'), throwsStateError);
  });
  test('declines edits outside the snippet budget', () {
    expect(
      analyzer.proposeEdit(
        'x' * 8193,
        language: d,
        base: 8193,
        extent: 8193,
        action: enter,
      ),
      isNull,
    );
    expect(
      analyzer.proposeEdit(
        'x' * 8192,
        language: d,
        base: 8192,
        extent: 8192,
        action: type,
        text: 'x',
      ),
      isNull,
    );
  });
  test('rejects split Unicode and CRLF selections', () {
    for (final s in ['😀', '\r\n']) {
      expect(
        () => analyzer.proposeEdit(
          s,
          language: d,
          base: 1,
          extent: 1,
          action: enter,
        ),
        throwsArgumentError,
      );
    }
  });
  test('rejects invalid options and disposed use', () {
    expect(
      () => analyzer.proposeEdit(
        '',
        language: d,
        base: 0,
        extent: 0,
        action: enter,
        indentUnit: 'x',
      ),
      throwsArgumentError,
    );
    expect(
      () => analyzer.proposeEdit(
        '',
        language: d,
        base: 0,
        extent: 0,
        action: enter,
        newline: '\r',
      ),
      throwsArgumentError,
    );
    analyzer.dispose();
    expect(
      () => analyzer.proposeEdit(
        '',
        language: d,
        base: 0,
        extent: 0,
        action: enter,
      ),
      throwsStateError,
    );
  });
  for (final fault in [
    'version',
    'language',
    'range',
    'surrogate',
    'selection',
    'crlf',
    'unicode',
  ]) {
    test('rejects malformed edit response: $fault', () {
      final reply = <String, Object>{
        'version': 4,
        'language': 1,
        'start': 0,
        'end': 0,
        'text': 'x',
        'base': 1,
        'extent': 1,
      };
      switch (fault) {
        case 'version':
          reply['version'] = 1;
        case 'language':
          reply['language'] = 3;
        case 'range':
          reply['start'] = -1;
        case 'surrogate':
          reply['end'] = 1;
        case 'selection':
          reply['base'] = 2;
        case 'crlf':
          reply['text'] = '\r\n';
        case 'unicode':
          reply['text'] = String.fromCharCode(0xd800);
      }
      final service = CodeAnalyzer(
        backend: _EditReply(utf8.encode(jsonEncode(reply))),
      );
      addTearDown(service.dispose);
      expect(
        () => service.proposeEdit(
          '😀',
          language: d,
          base: 0,
          extent: 0,
          action: type,
          text: 'x',
        ),
        throwsA(isA<CodeException>()),
      );
    });
  }
}

final class _EditReply implements CodeBackend {
  _EditReply(this.reply);
  final Uint8List reply;
  @override
  Uint8List detect(Uint8List source) => throw UnimplementedError();
  @override
  int get version => 4;
  @override
  Uint8List analyze(Uint8List source, int language) =>
      throw UnimplementedError();
  @override
  Uint8List edit(Uint8List request, int language) => reply;
  @override
  void dispose() {}
}
