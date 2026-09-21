import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  const styles = [
    (Style.strong, '**'),
    (Style.emphasis, '*'),
    (Style.strikethrough, '~~'),
    (Style.code, '`'),
  ];
  for (final (style, marker) in styles) {
    test('$marker pending state, idempotence, input and undo', () {
      final e = FlarkEditor(backend);
      var notifications = 0;
      e.addListener(() => notifications++);
      expect(e.styleState(style).isOn, isFalse);
      expect(e.styleState(style).canToggle, isTrue);
      expect(e.apply(SetStyle(style, enabled: true)), isTrue);
      expect(e.styleState(style).isOn, isTrue);
      expect(e.apply(SetStyle(style, enabled: true)), isFalse);
      expect(e.lastRejection, isNull);
      expect(notifications, 1);
      expect(e.history.canUndo, isFalse);
      expect(e.apply(SetStyle(style, enabled: false)), isTrue);
      expect(e.styleState(style).isOn, isFalse);
      expect(e.apply(SetStyle(style, enabled: false)), isFalse);
      expect(e.lastRejection, isNull);
      expect(notifications, 2);
      e.apply(SetStyle(style, enabled: true));
      e.apply(const InsertText('hello'));
      expect(e.source, '${marker}hello$marker');
      expect(e.styleState(style).isOn, isTrue);
      e.apply(const Undo());
      expect(e.source, '');
      expect(e.styleState(style).isOn, isTrue);
    });

    for (final reverse in [false, true]) {
      for (final enabled in [false, true]) {
        test('$marker mixed selection set $enabled reverse=$reverse', () {
          final source = 'one ${marker}two$marker three';
          final e = FlarkEditor(backend, text: source);
          final selection = reverse
              ? FlarkSelection(source.length, 0)
              : FlarkSelection(0, source.length);
          e.apply(SetSelection(selection.base, selection.extent));
          expect(e.styleState(style).value, FlarkStyleValue.mixed);
          expect(e.styleState(style).canToggle, isTrue);
          expect(e.apply(SetStyle(style, enabled: enabled)), isTrue);
          expect(
            e.source,
            enabled ? '${marker}one two three$marker' : 'one two three',
          );
          expect(e.styleState(style).isOn, enabled);
          expect(e.selection.base > e.selection.extent, reverse);
          final snapshot = e.snapshot, revision = e.revision;
          expect(e.apply(SetStyle(style, enabled: enabled)), isFalse);
          expect(e.snapshot, same(snapshot));
          expect(e.revision, revision);
          expect(e.lastRejection, isNull);
          e.apply(const Undo());
          expect(e.source, source);
          expect(e.selection, selection);
          e.apply(const Redo());
          expect(e.styleState(style).isOn, enabled);
          e.apply(const InsertText('new'));
          expect(e.projection.rows.single.text, 'new');
          expect(e.typingContext & style != 0, enabled);
        });
      }
    }
  }

  test(
    'mixed toggle enables and preserves nested italic and link metadata',
    () {
      const source = '**one** and *two* [three](https://example.com)';
      final e = FlarkEditor(backend, text: source);
      e.apply(const SelectAll());
      expect(e.styleState(Style.strong).isMixed, isTrue);
      expect(e.apply(const ToggleStyle(Style.strong)), isTrue);
      expect(e.source, '**one and *two* [three](https://example.com)**');
      expect(e.apply(const ToggleStyle(Style.strong)), isTrue);
      expect(e.source, 'one and *two* [three](https://example.com)');
    },
  );

  test('selection state ignores its direction and outer delimiter anchors', () {
    final e = FlarkEditor(backend, text: '**one** plain');
    for (final range in [const SetSelection(0, 7), const SetSelection(7, 0)]) {
      e.apply(range);
      expect(e.styleState(Style.strong).isOn, isTrue);
    }
    e.apply(const SelectAll());
    expect(e.styleState(Style.strong).isMixed, isTrue);
    e.apply(const SetSelection.caret(2));
    expect(e.styleState(Style.strong).isOn, isTrue);
    e.apply(const SetStyle(Style.strong, enabled: false));
    expect(e.styleState(Style.strong).isOn, isFalse);
    expect(e.source, '**one** plain'); // Existing caret-edge behavior.
  });

  test('unsupported ranges report disabled without rewriting Markdown', () {
    for (final source in ['**one** plain', 'one\n\ntwo', '```\none\n```']) {
      final e = FlarkEditor(backend, text: source);
      e.apply(
        source.startsWith('**') ? const SetSelection(3, 5) : const SelectAll(),
      );
      expect(e.styleState(Style.strong).canToggle, isFalse, reason: source);
      final before = e.snapshot;
      expect(e.apply(const ToggleStyle(Style.strong)), isFalse);
      expect(e.snapshot, same(before));
      expect(e.history.canUndo, isFalse);
    }
  });

  test('code and source mode availability and stale writes', () {
    final e = FlarkEditor(backend, text: '`literal`', caret: 3);
    expect(e.styleState(Style.strong).canToggle, isFalse);
    expect(e.styleState(Style.code).isOn, isTrue);
    expect(e.styleState(Style.code).canToggle, isTrue);
    expect(e.apply(const SetStyle(Style.strong, enabled: true)), isFalse);
    expect(e.source, '`literal`');
    e.setSourceMode(true);
    expect(e.styleState(Style.code).canToggle, isFalse);
    expect(e.apply(const SetStyle(Style.code, enabled: true)), isFalse);
    expect(e.styleState(12345).canToggle, isFalse);
    e.setSourceMode(false);
    expect(
      e.apply(const SetStyle(Style.code, enabled: true), expectedRevision: -1),
      isFalse,
    );
    expect(e.lastRejection, FlarkRejection.staleRevision);
  });

  test('unrenderable formatting is rejected atomically', () {
    final e = FlarkEditor(backend, text: 'a`b');
    e.apply(const SelectAll());
    expect(e.apply(const SetStyle(Style.code, enabled: true)), isFalse);
    expect(e.source, 'a`b');
    expect(e.history.canUndo, isFalse);
  });

  for (final prefix in ['# ', '- ', '> ']) {
    test('Select All formats content while retaining $prefix structure', () {
      final e = FlarkEditor(backend, text: '${prefix}hello');
      e.apply(const SelectAll());
      expect(e.apply(const SetStyle(Style.strong, enabled: true)), isTrue);
      expect(e.source, '$prefix**hello**');
      expect(e.styleState(Style.strong).isOn, isTrue);
      e.apply(const SetStyle(Style.strong, enabled: false));
      expect(e.source, '${prefix}hello');
    });
  }
}
