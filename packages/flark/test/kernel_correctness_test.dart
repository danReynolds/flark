import 'package:characters/characters.dart';
import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:test/test.dart';

final class _FailAfterInitialParse implements FlarkParseBackend {
  _FailAfterInitialParse(this.delegate);

  final FlarkParseBackend delegate;
  var calls = 0;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    if (calls++ > 0) throw StateError('synthetic parse failure');
    return delegate.parse(source);
  }
}

final class _SwitchableFaultBackend implements FlarkParseBackend {
  _SwitchableFaultBackend(this.delegate);

  final FlarkParseBackend delegate;
  bool fail = false;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    if (fail) throw StateError('synthetic parse failure');
    return delegate.parse(source);
  }
}

void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  group('transactional commits', () {
    test('a parse failure publishes neither source nor history', () {
      final editor = FlarkEditor(
        _FailAfterInitialParse(backend),
        text: 'a',
        caret: 1,
      );

      expect(() => editor.apply(const InsertText('b')), throwsStateError);
      expect(editor.source, 'a');
      expect(editor.selection, const FlarkSelection.collapsed(1));
      expect(editor.history.canUndo, isFalse);
    });

    test('invalid command text is rejected without a partial commit', () {
      final editor = FlarkEditor(backend, text: 'a', caret: 1);

      expect(editor.apply(const Paste('\uD800')), isFalse);
      expect(editor.apply(const Paste('\r')), isFalse);
      expect(editor.source, 'a');
      expect(editor.history.canUndo, isFalse);
    });

    test('a failed undo does not mutate source or history', () {
      final failing = _SwitchableFaultBackend(backend);
      final editor = FlarkEditor(failing, text: 'a', caret: 1);
      expect(editor.apply(const InsertText('b')), isTrue);
      failing.fail = true;

      expect(() => editor.apply(const Undo()), throwsStateError);
      expect(editor.source, 'ab');
      expect(editor.history.canUndo, isTrue);
      expect(editor.history.canRedo, isFalse);
    });

    test('a published live snapshot cannot be mutated by its host', () {
      final editor = FlarkEditor(backend, text: '- item');
      final snapshot = editor.snapshot as FlarkLiveSnapshot;
      final row = snapshot.projection.rows.single;

      expect(
        () => snapshot.document.model.bytes[0] = 0,
        throwsUnsupportedError,
      );
      expect(() => snapshot.projection.rows.clear(), throwsUnsupportedError);
      expect(
        () => snapshot.projection.rowsOnLine(0).clear(),
        throwsUnsupportedError,
      );
      expect(() => row.segments.clear(), throwsUnsupportedError);
      expect(() => row.shells.clear(), throwsUnsupportedError);
      expect(() => row.contentStarts.clear(), throwsUnsupportedError);
      expect(() => row.contentEnds.clear(), throwsUnsupportedError);
      expect(() => row.prefixStarts.clear(), throwsUnsupportedError);
      expect(() => (row as dynamic).index = 99, throwsNoSuchMethodError);
      expect(editor.source, '- item');
      expect(editor.projection.rows.single.text, 'item');
    });
  });

  group('source and caret boundaries', () {
    test('CRLF is preserved exactly while bare CR is rejected', () {
      final editor = FlarkEditor(backend, text: 'a\r\nb', caret: 4);

      expect(editor.source, 'a\r\nb');
      expect(editor.projection.rows.map((row) => row.text), ['a\nb']);
      expect(
        () => FlarkDocument.load('a\rb', backend),
        throwsA(isA<FormatException>()),
      );
    });

    test('selection cannot split scalar, combining, or ZWJ graphemes', () {
      for (final text in ['😀', 'e\u0301', '👨‍👩‍👧‍👦']) {
        final editor = FlarkEditor(backend, text: text, caret: 0);
        final boundaries = <int>{0};
        var boundary = 0;
        for (final grapheme in text.characters) {
          boundary += grapheme.length;
          boundaries.add(boundary);
        }
        for (var offset = 1; offset < text.length; offset++) {
          editor.apply(SetSelection.caret(offset));
          expect(boundaries, contains(editor.selection.extent));
          expect(editor.document.isLegal(editor.selection.extent), isTrue);
        }
      }
    });

    test('entity replacements expose only their source edges', () {
      final document = FlarkDocument.load('&amp;', backend);

      expect(document.isLegal(0), isTrue);
      expect(document.isLegal(5), isTrue);
      for (var offset = 1; offset < 5; offset++) {
        expect(document.isLegal(offset), isFalse);
      }
    });

    test('multi-grapheme replacements move and delete as one atomic unit', () {
      const source = '&fjlig;';
      final editor = FlarkEditor(backend, text: source);

      expect(editor.projection.rows.single.text, 'fj');
      expect(editor.apply(const MoveCaret(MoveDirection.forward)), isTrue);
      expect(editor.selection, const FlarkSelection.collapsed(source.length));
      expect(editor.apply(const MoveCaret(MoveDirection.backward)), isTrue);
      expect(editor.selection, const FlarkSelection.collapsed(0));

      expect(editor.apply(const DeleteForward()), isTrue);
      expect(editor.source, isEmpty);
      expect(editor.apply(const Undo()), isTrue);
      expect(editor.source, source);
      expect(editor.selection, const FlarkSelection.collapsed(0));

      expect(editor.apply(SetSelection.caret(source.length)), isTrue);
      expect(editor.apply(const DeleteBackward()), isTrue);
      expect(editor.source, isEmpty);
    });
  });

  group('projection-safe edits', () {
    test('deleting an owner\'s selected content removes its delimiters', () {
      final editor = FlarkEditor(backend, text: '*t*');
      editor.apply(const SetSelection(1, 2));

      expect(editor.apply(const DeleteBackward()), isTrue);
      expect(editor.source, isEmpty);
      expect(editor.selection, const FlarkSelection.collapsed(0));
    });

    test('empty ReplaceRange removes an emptied owner too', () {
      final editor = FlarkEditor(backend, text: '*t*');

      expect(editor.apply(const ReplaceRange(1, 2, '')), isTrue);
      expect(editor.source, isEmpty);
    });

    test(
      'a style toggle that cannot establish its postcondition is a no-op',
      () {
        final editor = FlarkEditor(backend, text: 'a`b');
        editor.apply(const SetSelection(0, 3));

        expect(editor.apply(const ToggleStyle(Style.code)), isFalse);
        expect(editor.source, 'a`b');
        expect(editor.history.canUndo, isFalse);
      },
    );
  });

  group('bounded logical history', () {
    FlarkDocument document(String source) =>
        FlarkDocument.load(source, backend, caret: source.length);

    test('navigation closes the current typing group', () {
      final editor = FlarkEditor(backend);
      editor.apply(const InsertText('a'), at: Duration.zero);
      editor.apply(
        const MoveCaret(MoveDirection.backward),
        at: const Duration(milliseconds: 100),
      );
      editor.apply(
        const MoveCaret(MoveDirection.forward),
        at: const Duration(milliseconds: 200),
      );
      editor.apply(
        const InsertText('b'),
        at: const Duration(milliseconds: 300),
      );

      expect(editor.apply(const Undo()), isTrue);
      expect(editor.source, 'a');
      expect(editor.apply(const Undo()), isTrue);
      expect(editor.source, isEmpty);
    });

    test('an edit clears the preferred vertical column', () {
      final editor = FlarkEditor(
        backend,
        text: '# abcd\n# x\n# abcd',
        caret: 6,
      );
      expect(
        editor.apply(
          const MoveCaret(MoveDirection.forward, unit: MoveUnit.row),
        ),
        isTrue,
      );
      expect(editor.document.displayOf(editor.selection.extent).offset, 1);
      expect(editor.apply(const InsertText('z')), isTrue);
      expect(
        editor.apply(
          const MoveCaret(MoveDirection.forward, unit: MoveUnit.row),
        ),
        isTrue,
      );
      expect(editor.document.displayOf(editor.selection.extent).offset, 2);
    });

    test('history limits are validated in every build mode', () {
      expect(() => History(maxEntries: 0), throwsArgumentError);
      expect(() => History(maxSourceCodeUnits: -1), throwsArgumentError);
    });

    test('joined typing retains only the group entry needed by undo', () {
      final history = History(maxEntries: 1);
      history.record(
        document(''),
        pending: null,
        typing: true,
        at: Duration.zero,
      );
      history.record(
        document('a'),
        pending: null,
        typing: true,
        at: const Duration(milliseconds: 100),
      );

      expect(history.undo(document('ab'), null)?.source, isEmpty);
      expect(history.canUndo, isFalse);
    });

    test('entry cap evicts the oldest undo state', () {
      final history = History(maxEntries: 2);
      history.record(
        document('0'),
        pending: null,
        typing: false,
        at: Duration.zero,
      );
      history.record(
        document('1'),
        pending: null,
        typing: false,
        at: const Duration(seconds: 1),
      );
      history.record(
        document('2'),
        pending: null,
        typing: false,
        at: const Duration(seconds: 2),
      );

      expect(history.undo(document('3'), null)?.source, '2');
      expect(history.undo(document('2'), null)?.source, '1');
      expect(history.undo(document('1'), null), isNull);
    });

    test('source budget evicts whole snapshots', () {
      final history = History(maxEntries: 10, maxSourceCodeUnits: 5);
      history.record(
        document('0'),
        pending: null,
        typing: false,
        at: Duration.zero,
      );
      history.record(
        document('11'),
        pending: null,
        typing: false,
        at: const Duration(seconds: 1),
      );
      history.record(
        document('222'),
        pending: null,
        typing: false,
        at: const Duration(seconds: 2),
      );

      expect(history.undo(document('3333'), null)?.source, '222');
      expect(history.undo(document('222'), null)?.source, '11');
      expect(history.undo(document('11'), null), isNull);
    });
  });
}
