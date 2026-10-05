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

  @override
  void dispose() => delegate.dispose();
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

  @override
  void dispose() => delegate.dispose();
}

/// A parser that fails on any source holding [marker].
final class _FailOnMarker implements FlarkParseBackend {
  _FailOnMarker(this.delegate, this.marker);

  final FlarkParseBackend delegate;
  final String marker;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) => source.contains(marker)
      ? throw StateError('synthetic parse failure')
      : delegate.parse(source);

  @override
  void dispose() => delegate.dispose();
}

/// Reports an extraction deviation for one source, as the parse crate does
/// for a document whose model it cannot verify against comrak.
final class _DeviatesOn implements FlarkParseBackend {
  _DeviatesOn(this.delegate, this.source);

  final FlarkParseBackend delegate;
  final String source;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String text) {
    if (text == source) {
      throw const FlarkParseException(
        FlarkParseException.extractionDeviationCode,
        'synthetic extraction deviation',
      );
    }
    return delegate.parse(text);
  }

  @override
  void dispose() => delegate.dispose();
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

    test('a host command that fails after ending a composition keeps it', () {
      // applyAfterComposition ends the composition before its command runs.
      // A command that then fails before publishing anything (here the
      // parser fails on its text) fails the call whole: the composition is
      // open again over the text it composed, with no undo step.
      final editor = FlarkEditor(
        _FailOnMarker(backend, '!'),
        text: 'abc',
        caret: 3,
      );
      final revision = editor.revision;
      editor.beginComposition();
      expect(editor.apply(const InsertText('k')), isTrue);

      expect(
        () => editor.applyAfterComposition(const InsertText('!')),
        throwsStateError,
      );
      expect(editor.revision, revision + 1);
      expect(
        (editor.source, editor.composing, editor.history.canUndo),
        ('abck', true, false),
      );
      editor.commitComposition();
      expect((editor.source, editor.history.canUndo), ('abck', true));
    });

    test('a deletion the parser cannot read is refused, not respelled', () {
      // An extraction deviation refuses the edit as asked
      // (EP1-RESULT-PRESENTATION-001). A respelling that would read (a blank
      // line making `b` a paragraph of its own) is no way around it.
      final editor = FlarkEditor(
        _DeviatesOn(backend, 'a\nb'),
        text: 'a\nbc',
        caret: 4,
      );
      expect(editor.apply(const DeleteBackward()), isFalse);
      expect(editor.lastRejection, FlarkRejection.extractionDeviation);
      expect(editor.source, 'a\nbc');
      expect(editor.history.canUndo, isFalse);
    });

    test('a respelling the parser cannot read is passed over', () {
      // At the end of an autolink's text Return breaks the line after the
      // autolink, from the caret's other anchor; a body that already holds
      // a run as long as its fence keeps the fences while the parser reads
      // the block as it was. Both are respellings, passed over when the
      // parser cannot read them: the code edit grows the fences, as asked,
      // and Return, kept nowhere else, is unsupported.
      final autolink = FlarkEditor(
        _DeviatesOn(backend, '<http://a.b>\n'),
        text: '<http://a.b>',
        caret: 11,
      );
      expect(autolink.apply(const Newline()), isFalse);
      expect(autolink.lastRejection, FlarkRejection.unsupportedEdit);
      expect(autolink.source, '<http://a.b>');
      final code = FlarkEditor(
        _DeviatesOn(backend, '```\n    ```\nabx\n```'),
        text: '```\n    ```\nab\n```',
        caret: 14,
      );
      expect(code.apply(const Paste('x')), isTrue);
      expect(code.lastRejection, isNull);
      expect(code.source, '````\n    ```\nabx\n````');
    });

    test('a span continuation the parser cannot read is passed over', () {
      // Moving a span's delimiter past the word typed beside it is a
      // respelling of that word: refused, it is passed over, and the word
      // takes a pair of its own, with no refusal to report.
      for (final (source, caret, deletions, word, deviates, typed) in [
        ('plain ', 6, 0, 'one t', 'plain **one t**', 'plain **one** **t**'),
        ('x **one two**', 7, 3, 'n', 'x **n two**', 'x **n** **two**'),
      ]) {
        final editor = FlarkEditor(
          _DeviatesOn(backend, deviates),
          text: source,
          caret: caret,
        );
        if (deletions == 0) editor.apply(const ToggleStyle(Style.strong));
        for (var i = 0; i < deletions; i++) {
          editor.apply(const DeleteBackward());
        }
        for (final char in word.characters) {
          expect(editor.apply(InsertText(char)), isTrue, reason: deviates);
        }
        expect(editor.lastRejection, isNull, reason: deviates);
        expect(editor.source, typed);
      }
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

    test('typing is grouped by the clock the editor was given', () {
      var now = Duration.zero;
      final editor = FlarkEditor(backend, clock: () => now);
      editor.apply(const InsertText('a'));
      now += const Duration(milliseconds: 500);
      editor.apply(const InsertText('b'));
      now += const Duration(seconds: 2);
      editor.apply(const InsertText('c'));

      expect(editor.apply(const Undo()), isTrue);
      expect(
        editor.source,
        'ab',
        reason: 'a pause longer than the window starts a group',
      );
      expect(editor.apply(const Undo()), isTrue);
      expect(
        editor.source,
        isEmpty,
        reason: 'keystrokes within the window are one group',
      );
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
      // A run of typing longer than the editor's 100-entry cap is still one
      // undo step back to where it began: joined keystrokes add no entries
      // that the cap would evict in place of the run's first state.
      final editor = FlarkEditor(backend);
      for (var i = 0; i < 150; i++) {
        editor.apply(InsertText('a'), at: Duration(milliseconds: i * 10));
      }
      expect(editor.apply(const Undo()), isTrue);
      expect(editor.source, isEmpty);
      expect(editor.history.canUndo, isFalse);
    });

    test('entry cap evicts the oldest undo state', () {
      // 101 steps under a cap of 100 entries: the state before the first
      // is the one evicted.
      final editor = FlarkEditor(backend);
      for (var i = 0; i < 101; i++) {
        editor.apply(const Paste('a'), at: Duration(seconds: i * 2));
      }
      for (var i = 100; i > 0; i--) {
        expect(editor.apply(const Undo()), isTrue);
        expect(editor.source, 'a' * i);
      }
      expect(editor.history.canUndo, isFalse);
    });

    test('source budget evicts whole snapshots', () {
      // Five snapshots of a document of a million characters pass the
      // 4 Mi code-unit budget: the oldest goes whole, the four after it
      // stay whole.
      final start = 'x' * 1000000;
      final editor = FlarkEditor(backend, text: start, caret: start.length);
      for (var i = 0; i < 5; i++) {
        editor.apply(Paste('$i'), at: Duration(seconds: i * 2));
      }
      for (var i = 4; i > 0; i--) {
        expect(editor.apply(const Undo()), isTrue);
        expect(editor.source, '$start${'01234'.substring(0, i)}');
      }
      expect(editor.history.canUndo, isFalse);
    });
  });
}
