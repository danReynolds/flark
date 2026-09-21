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
}

void main() {
  test(
    'reader parses once per source, selection reuses projection, bounded fallback',
    () {
      final native = createParseBackend();
      addTearDown(() => (native as dynamic).dispose());
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
    addTearDown(() => (backend as dynamic).dispose());
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
          (backend as dynamic).dispose();
        }),
      );
      await Future<void>.delayed(Duration.zero);
      expect(released, 1);
      expect(notified, 0);
    },
  );
}
