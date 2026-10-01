import 'package:flark/rendering.dart';
import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:flark/session.dart';
import 'package:test/test.dart';

class CountingBackend implements FlarkParseBackend {
  CountingBackend(this.inner);
  final FlarkParseBackend inner;
  int calls = 0;
  @override
  int get schemaVersion => inner.schemaVersion;
  @override
  RenderModel parse(String source) {
    calls++;
    return inner.parse(source);
  }

  @override
  void dispose() => inner.dispose();
}

/// Faults on one source, as a contained native panic would.
class FaultingBackend implements FlarkParseBackend {
  FaultingBackend(this.inner, this.faultOn);
  final FlarkParseBackend inner;
  final String faultOn;
  @override
  int get schemaVersion => inner.schemaVersion;
  @override
  RenderModel parse(String source) => source == faultOn
      ? throw FlarkParseException.fromCode(FlarkParseException.faultCode)
      : inner.parse(source);

  @override
  void dispose() => inner.dispose();
}

void main() {
  test(
    'reader parses once per source, selection reuses projection, bounded fallback',
    () {
      final native = createParseBackend();
      addTearDown(native.dispose);
      final backend = CountingBackend(native);
      final reader = FlarkReadDocument(backend, '# Title\n\n**body**');
      final projection = reader.projection;
      expect(reader.update('# Title\n\n**body**'), isFalse);
      reader.select(const FlarkSelection(0, 5));
      expect(identical(reader.projection, projection), isTrue);
      expect(backend.calls, 1);
      expect(reader.codeEditing, isNull);
      reader.update('new');
      expect(backend.calls, 2);
      final large = 'x' * 1100000;
      reader.update(large);
      expect(reader.sourceMode, isTrue);
      expect(reader.source, large);
      expect(backend.calls, 2);
    },
  );
  test('reader and editor share projection without sharing mutable state', () {
    final backend = createParseBackend();
    addTearDown(backend.dispose);
    const source =
        '# Title\n\n- [x] task\n\n> quote\n\n| a | b |\n| - | - |\n| c | d |';
    final reader = FlarkReadDocument(backend, source);
    final editor = FlarkEditor(backend, text: source);
    expect(
      reader.projection.rows.map((r) => (r.kind, r.text)),
      editor.projection.rows.map((r) => (r.kind, r.text)),
    );
    editor.apply(const InsertText('other'));
    expect(reader.source, source);
  });
  test(
    'disposed reader releases a late parser without publishing content',
    () async {
      final gate = Completer<FlarkBackendLease>();
      final reader = FlarkReader('first', backendLoader: () => gate.future);
      reader.update('latest');
      var notified = 0, released = 0;
      reader.addListener(() => notified++);
      reader.dispose();
      reader.dispose();
      await expectLater(reader.ready, throwsStateError);
      final backend = createParseBackend();
      gate.complete(
        FlarkBackendLease(backend, () {
          released++;
          backend.dispose();
        }),
      );
      await Future<void>.delayed(Duration.zero);
      expect(released, 1);
      expect(notified, 0);
    },
  );

  test(
    'refused text shows as source and the next valid text renders',
    () async {
      final reader = FlarkReader('# Title');
      addTearDown(reader.dispose);
      await reader.ready;
      // A streamed or truncated preview can end inside a surrogate pair, and
      // CommonMark allows bare-CR line endings that the editor does not.
      for (final refused in ['# Title \uD83D', '# Title\rbody']) {
        reader.update(refused);
        expect(reader.status, FlarkStatus.ready);
        expect(reader.document!.sourceMode, isTrue);
        expect(reader.document!.source, refused);
        expect(() => reader.document!.document, throwsStateError);
        reader.update('# Title \u{1F600}');
        expect(reader.status, FlarkStatus.ready);
        expect(reader.document!.sourceMode, isFalse);
        expect(reader.document!.source, '# Title \u{1F600}');
      }
    },
  );

  test('a document the parser refuses opens as source', () async {
    final reader = FlarkReader('# Title\rbody');
    addTearDown(reader.dispose);
    await reader.ready;
    expect(reader.status, FlarkStatus.ready);
    expect(reader.document!.sourceMode, isTrue);

    final backend = createParseBackend();
    addTearDown(backend.dispose);
    final document = FlarkReadDocument(
      FaultingBackend(backend, 'boom'),
      'boom',
    );
    expect(document.sourceMode, isTrue);
    document.update('fine');
    expect(document.sourceMode, isFalse);
  });
}
