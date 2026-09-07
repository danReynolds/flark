import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:test/test.dart';

final class _CountingBackend implements FlarkParseBackend {
  _CountingBackend(this.delegate);

  final FlarkParseBackend delegate;
  var calls = 0;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    calls++;
    return delegate.parse(source);
  }
}

final class _SelectiveDeviationBackend implements FlarkParseBackend {
  _SelectiveDeviationBackend(this.delegate);

  final FlarkParseBackend delegate;
  var calls = 0;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    calls++;
    if (source == 'bad') {
      throw const FlarkParseException(
        FlarkParseException.extractionDeviationCode,
        'synthetic extraction deviation',
      );
    }
    return delegate.parse(source);
  }
}

final class _FailsOnParseBackend implements FlarkParseBackend {
  _FailsOnParseBackend(this.delegate);

  final FlarkParseBackend delegate;
  var calls = 0;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    calls++;
    throw StateError('synthetic native fault');
  }
}

void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  test('the conservative default is 16 KiB of UTF-8', () {
    expect(FlarkEditor.defaultSyncLimit, 16 * 1024);
    expect(() => FlarkEditor(backend, syncLimit: -1), throwsArgumentError);
    final counting = _CountingBackend(backend);
    final editor = FlarkEditor(counting, text: 'éé', caret: 2, syncLimit: 4);

    expect(editor.source.length, 2);
    expect(editor.sourceMode, isFalse);
    expect(counting.calls, 1);

    expect(editor.apply(const InsertText('a')), isTrue);
    expect(editor.source, 'ééa');
    expect(editor.source.length, 3);
    expect(editor.sourceMode, isTrue);
    expect(counting.calls, 1, reason: 'five UTF-8 bytes must not be parsed');
  });

  test('an oversized initial source has no document or projection', () {
    final counting = _CountingBackend(backend);
    final editor = FlarkEditor(counting, text: '😀x', caret: 3, syncLimit: 4);

    expect(counting.calls, 0);
    expect(editor.snapshot, isA<FlarkSourceSnapshot>());
    expect(editor.source, '😀x');
    expect(editor.selection, const FlarkSelection.collapsed(3));
    expect(() => editor.document, throwsStateError);
    expect(() => editor.projection, throwsStateError);

    expect(editor.apply(const InsertText('y')), isTrue);
    expect(editor.source, '😀xy');
    expect(counting.calls, 0);
    expect(editor.history.canUndo, isTrue);
    expect(editor.apply(const Undo()), isTrue);
    expect(editor.source, '😀x');
    expect(counting.calls, 0);
  });

  test('paste enters source mode and deletion returns to live atomically', () {
    final counting = _CountingBackend(backend);
    final editor = FlarkEditor(counting, text: 'abc', caret: 3, syncLimit: 4);

    expect(editor.apply(const Paste('😀')), isTrue);
    expect(editor.source, 'abc😀');
    expect(editor.snapshot, isA<FlarkSourceSnapshot>());
    expect(counting.calls, 1, reason: 'the oversized paste must not parse');

    expect(editor.apply(const DeleteBackward()), isTrue);
    expect(editor.source, 'abc');
    expect(editor.snapshot, isA<FlarkLiveSnapshot>());
    expect(editor.projection.rows.single.text, 'abc');
    expect(
      counting.calls,
      2,
      reason: 'crossing back builds one complete live snapshot',
    );

    expect(editor.apply(const Undo()), isTrue);
    expect(editor.source, 'abc😀');
    expect(editor.sourceMode, isTrue);
    expect(
      counting.calls,
      2,
      reason: 'undo to an oversized snapshot must not parse',
    );

    expect(editor.apply(const Redo()), isTrue);
    expect(editor.source, 'abc');
    expect(editor.snapshot, isA<FlarkLiveSnapshot>());
    expect(counting.calls, 3);
  });

  test('near-EOF source navigation stays on local grapheme boundaries', () {
    final prefix = 'a' * (1024 * 1024 - 64);
    const family = '👩‍👩‍👧‍👦';
    final source = '$prefix$family!';
    final counting = _CountingBackend(backend);
    final editor = FlarkEditor(counting, text: source, caret: source.length);

    expect(counting.calls, 0);
    expect(editor.apply(const MoveCaret(MoveDirection.backward)), isTrue);
    expect(editor.selection, FlarkSelection.collapsed(source.length - 1));
    expect(editor.apply(const MoveCaret(MoveDirection.backward)), isTrue);
    expect(editor.selection, FlarkSelection.collapsed(prefix.length));
    expect(editor.apply(const MoveCaret(MoveDirection.forward)), isTrue);
    expect(editor.selection, FlarkSelection.collapsed(source.length - 1));

    expect(
      editor.apply(SetSelection(prefix.length + 2, prefix.length + 2)),
      isTrue,
    );
    expect(editor.selection, FlarkSelection.collapsed(prefix.length));
    expect(counting.calls, 0);
  });

  test('word-forward crosses a CRLF boundary in source mode', () {
    final editor = FlarkEditor(backend, text: 'abc\r\ndef', syncLimit: 3);

    expect(
      editor.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.word)),
      isTrue,
    );
    expect(editor.selection, const FlarkSelection.collapsed(3));
    expect(
      editor.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.word)),
      isTrue,
    );
    expect(editor.selection, const FlarkSelection.collapsed(8));
  });

  test('consecutive source-mode backspaces form one undo group', () {
    final editor = FlarkEditor(backend, text: 'abcdef', caret: 6, syncLimit: 2);

    expect(editor.apply(const DeleteBackward(), at: Duration.zero), isTrue);
    expect(
      editor.apply(
        const DeleteBackward(),
        at: const Duration(milliseconds: 100),
      ),
      isTrue,
    );
    expect(editor.source, 'abcd');
    expect(editor.apply(const Undo()), isTrue);
    expect(editor.source, 'abcdef');
  });

  test('invalid source edits are rejected without changing source history', () {
    final counting = _CountingBackend(backend);
    final editor = FlarkEditor(counting, text: 'abcd', caret: 4, syncLimit: 3);

    expect(editor.apply(const InsertText('\r')), isFalse);
    expect(editor.apply(const InsertText('\uD800')), isFalse);
    expect(editor.source, 'abcd');
    expect(editor.selection, const FlarkSelection.collapsed(4));
    expect(editor.history.canUndo, isFalse);
    expect(counting.calls, 0);
  });

  test('an actual parse fault cannot partially cross back to live mode', () {
    final failing = _FailsOnParseBackend(backend);
    final editor = FlarkEditor(failing, text: 'abcd', caret: 4, syncLimit: 3);

    expect(
      () => editor.apply(const DeleteBackward()),
      throwsA(isA<StateError>()),
    );
    expect(editor.source, 'abcd');
    expect(editor.selection, const FlarkSelection.collapsed(4));
    expect(editor.snapshot, isA<FlarkSourceSnapshot>());
    expect(editor.history.canUndo, isFalse);
    expect(failing.calls, 1);
  });

  test('a source edit remains writable when its in-tier parse deviates', () {
    final selective = _SelectiveDeviationBackend(backend);
    final editor = FlarkEditor(selective, text: 'bad!', caret: 4, syncLimit: 3);

    expect(selective.calls, 0);
    expect(editor.apply(const DeleteBackward()), isTrue);
    expect(editor.source, 'bad');
    expect(editor.sourceMode, isTrue);
    expect(selective.calls, 1);
    expect(editor.history.canUndo, isTrue);
  });

  test('a rich edit that exposes a derived-range deviation is a no-op', () {
    const source = 'ba', at = 2;
    final selective = _SelectiveDeviationBackend(backend);
    final editor = FlarkEditor(selective, text: source, caret: at);
    final rows = editor.projection.rows.map((row) => row.text).toList();

    expect(editor.apply(const InsertText('d')), isFalse);
    expect(editor.source, source);
    expect(editor.selection, FlarkSelection.collapsed(at));
    expect(editor.projection.rows.map((row) => row.text), rows);
    expect(editor.history.canUndo, isFalse);
  });
}
