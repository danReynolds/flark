import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:flark_tree_sitter/src/highlight_queue.dart';
import 'package:flark_tree_sitter/src/highlight_transport.dart';
import 'package:test/test.dart';

void main() {
  test('coalesces rapid edits and never returns superseded colors', () async {
    final transport = _Controlled();
    final queue = HighlightQueue(transport);
    addTearDown(queue.dispose);
    final first = queue.analyze('old', CodeLanguage.dart);
    final intermediate = queue.analyze('middle', CodeLanguage.python);
    final last = queue.analyze('最新😀', CodeLanguage.yaml);
    expect(await first, isNull);
    expect(await intermediate, isNull);
    expect(transport.calls, ['old']);
    // Even invalid stale output is discarded before decoding/adoption.
    transport.pending.removeAt(0).complete(Uint8List(0));
    await Future<void>.delayed(Duration.zero);
    expect(transport.calls, ['old', '最新😀']);
    transport.pending.removeAt(0).complete(_reply('最新😀', CodeLanguage.yaml));
    final result = await last;
    expect(result!.source, '最新😀');
    expect(result.language, CodeLanguage.yaml);
    expect(result.spans.single.endByte, utf8.encode('最新😀').length);
  });

  test('plain, limit and language changes invalidate pending work', () async {
    final transport = _Controlled();
    final queue = HighlightQueue(transport);
    addTearDown(queue.dispose);
    final old = queue.analyze('x', CodeLanguage.dart);
    final plain = await queue.analyze('x', CodeLanguage.plain);
    expect(await old, isNull);
    expect(plain!.status, CodeAnalysisStatus.plain);
    final limit = await queue.analyze('😀' * 4097, CodeLanguage.python);
    expect(limit!.status, CodeAnalysisStatus.limit);
    expect(transport.calls, ['x']);
    transport.pending.removeAt(0).complete(_reply('x', CodeLanguage.dart));
  });

  test('disposal resolves active and queued work and closes once', () async {
    final transport = _Controlled();
    final queue = HighlightQueue(transport);
    final first = queue.analyze('x', CodeLanguage.dart);
    final last = queue.analyze('y', CodeLanguage.dart);
    queue.dispose();
    queue.dispose();
    expect(await first, isNull);
    expect(await last, isNull);
    expect(transport.disposals, 1);
    expect(() => queue.analyze('x', CodeLanguage.dart), throwsStateError);
  });

  test(
    'current worker failure surfaces and stale failures do not leak',
    () async {
      final transport = _Controlled();
      final queue = HighlightQueue(transport);
      addTearDown(queue.dispose);
      final stale = queue.analyze('old', CodeLanguage.dart);
      final current = queue.analyze('new', CodeLanguage.dart);
      transport.pending.removeAt(0).completeError(StateError('stale failure'));
      expect(await stale, isNull);
      await Future<void>.delayed(Duration.zero);
      final check = expectLater(current, throwsA(isA<CodeException>()));
      transport.pending.removeAt(0).complete(Uint8List(0));
      await check;
    },
  );

  test('invalid input cannot cancel a valid pending request', () async {
    final transport = _Controlled();
    final queue = HighlightQueue(transport);
    addTearDown(queue.dispose);
    final valid = queue.analyze('ok', CodeLanguage.dart);
    expect(
      () => queue.analyze(String.fromCharCode(0xd800), CodeLanguage.dart),
      throwsArgumentError,
    );
    transport.pending.removeAt(0).complete(_reply('ok', CodeLanguage.dart));
    expect((await valid)!.source, 'ok');
  });

  test(
    'real native isolate matches direct analysis across languages',
    () async {
      final worker = await CodeHighlightWorker.start();
      final direct = CodeAnalyzer();
      addTearDown(worker.dispose);
      addTearDown(direct.dispose);
      for (final language in CodeLanguage.values) {
        const source = 'for (final x in [1, 2]) {\r\n  print("😀");\r\n}';
        final actual = await worker.analyze(source, language: language);
        final expected = direct.analyze(source, language: language);
        expect(
          actual!.spans.map(
            (s) => [s.start, s.end, s.startByte, s.endByte, s.scopes],
          ),
          expected.spans.map(
            (s) => [s.start, s.end, s.startByte, s.endByte, s.scopes],
          ),
        );
      }
      final pending = worker.analyze(
        'void f() {}\n' * 500,
        language: CodeLanguage.dart,
      );
      worker.dispose();
      expect(await pending, isNull);
    },
  );
}

Uint8List _reply(String source, CodeLanguage language) => utf8.encode(
  jsonEncode({
    'version': 4,
    'language': language.index,
    'status': 'highlighted',
    'scope_sets': [<String>[]],
    'spans': [
      [source.length, utf8.encode(source).length, 0],
    ],
  }),
);

final class _Controlled implements HighlightTransport {
  final calls = <String>[];
  final pending = <Completer<Uint8List>>[];
  int disposals = 0;
  @override
  Future<Uint8List> analyze(String source, int language) {
    calls.add(source);
    final result = Completer<Uint8List>();
    pending.add(result);
    return result.future;
  }

  @override
  void dispose() {
    disposals++;
    for (final call in pending) {
      call.completeError(StateError('Closed'));
    }
    pending.clear();
  }
}
